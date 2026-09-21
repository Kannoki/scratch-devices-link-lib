#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

// Future Academy Link — single-binary tray shell + local hardware link server.
//
// The tray event loop owns the main thread (tao requirement). A tokio runtime
// runs on a separate thread and hosts: the axum link server, the background
// toolchain-setup task, and the per-connection serial sessions. The tray polls
// /status every 2s (synchronously, via ureq, on the poll thread) to drive the
// menu. There is NO Node runtime — the Rust binary IS the server.

mod ansi;
mod autostart;
mod download;
mod instance;
mod library_sync;
mod notification;
mod paths;
mod progress;
mod serial;
mod server;
mod toolchain;
mod update;
mod upload;
mod usb_id;
mod ws;

use std::io::IsTerminal;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use serde::Deserialize;
use tao::event::Event;
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::{TrayIconBuilder, TrayIconEvent};

pub use server::AppState;

const ICON_PNG: &[u8] = include_bytes!("../../assets/logo.png");
const STATUS_URL: &str = "http://127.0.0.1:11337/status";
const SCRATCH_URL: &str = "https://stem.windify.edu.vn/";
const POLL_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Deserialize)]
struct Device {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct StatusResponse {
    #[serde(default)]
    ready: bool,
    #[serde(default)]
    devices: Vec<Device>,
    #[serde(rename = "setupPhase", default)]
    setup_phase: Option<String>,
    #[serde(rename = "setupProgress", default)]
    setup_progress: u8,
    #[serde(default)]
    host: String,
    #[serde(default)]
    port: u16,
}

#[derive(Debug, Clone)]
struct TrayState {
    status_label: String,
    devices: Vec<String>,
}

impl TrayState {
    fn from_response(resp: &StatusResponse) -> Self {
        let status_label = if let Some(phase) = resp.setup_phase.as_deref().filter(|p| *p != "done")
        {
            let label = match phase {
                "downloading-cli" => "Downloading arduino-cli",
                "extracting" => "Extracting tools",
                "configuring" => "Configuring",
                "updating-index" => "Updating package index",
                "installing-core" => "Installing ESP32 core",
                "downloading-platform" => "Downloading ESP32 core",
                "downloading-tools" => "Downloading toolchain",
                "pruning" => "Cleaning up unused tools",
                "error" => "Setup failed \u{2014} restart app",
                _ => "Setting up tools",
            };
            format!("{} ({}%)", label, resp.setup_progress)
        } else if resp.ready {
            let host = if resp.host.is_empty() {
                "127.0.0.1"
            } else {
                &resp.host
            };
            let port = if resp.port == 0 { 11337 } else { resp.port };
            format!("Running on http://{}:{}", host, port)
        } else {
            "Starting\u{2026}".to_string()
        };
        Self {
            status_label,
            devices: resp.devices.iter().map(|d| d.name.clone()).collect(),
        }
    }

    fn starting() -> Self {
        Self {
            status_label: "Starting\u{2026}".to_string(),
            devices: Vec::new(),
        }
    }
}

#[derive(Debug)]
enum UserEvent {
    Status(TrayState),
    UpdateCheck(update::UpdateCheck),
    UpdateProgress {
        received: u64,
        total: u64,
    },
    UpdatePrepared {
        version_label: String,
        result: Result<update::PreparedUpdate, String>,
    },
}

#[cfg(target_os = "windows")]
mod console {
    use std::sync::atomic::{AtomicIsize, Ordering};

    static CONSOLE_HWND: AtomicIsize = AtomicIsize::new(0);

    const SW_HIDE: i32 = 0;
    const SW_SHOWNORMAL: i32 = 1;
    const SW_SHOW: i32 = 5;
    const SC_CLOSE: u32 = 0xF060;
    const MF_BYCOMMAND: u32 = 0x00000000;

    extern "system" {
        fn AllocConsole() -> i32;
        fn AttachConsole(dwProcessId: u32) -> i32;
        fn GetConsoleWindow() -> isize;
        fn GetSystemMenu(hWnd: isize, bRevert: i32) -> isize;
        fn DeleteMenu(hMenu: isize, uPosition: u32, uFlags: u32) -> i32;
        fn ShowWindow(hWnd: isize, nCmdShow: i32) -> i32;
        fn IsWindowVisible(hWnd: isize) -> i32;
        fn IsIconic(hWnd: isize) -> i32;
        fn SetForegroundWindow(hWnd: isize) -> i32;
        fn SetConsoleTitleW(lpConsoleTitle: *const u16) -> i32;
        fn SetConsoleCtrlHandler(
            HandlerRoutine: Option<unsafe extern "system" fn(u32) -> i32>,
            Add: i32,
        ) -> i32;
    }

    unsafe extern "system" fn console_ctrl_handler(ctrl_type: u32) -> i32 {
        // 0 = CTRL_C_EVENT, 1 = CTRL_BREAK_EVENT
        // If user presses Ctrl+C or Ctrl+Break, hide the console window instead of terminating the app!
        if ctrl_type == 0 || ctrl_type == 1 {
            let hwnd = CONSOLE_HWND.load(Ordering::Relaxed);
            if hwnd != 0 {
                ShowWindow(hwnd, SW_HIDE);
            }
            return 1; // Handled, prevent termination!
        }
        0
    }

    pub fn init_console() {
        unsafe {
            let mut hwnd = GetConsoleWindow();
            if hwnd == 0 {
                // Attach to parent console if started from terminal (CMD/PowerShell)
                if AttachConsole(!0u32) == 0 {
                    // Otherwise allocate a dedicated console
                    AllocConsole();
                }
                hwnd = GetConsoleWindow();
            }

            if hwnd != 0 {
                CONSOLE_HWND.store(hwnd, Ordering::Relaxed);

                // Set console window title
                let title: Vec<u16> = "Future Academy Link (Minimize to hide to tray)\0"
                    .encode_utf16()
                    .collect();
                SetConsoleTitleW(title.as_ptr());

                // Disable SC_CLOSE on the console system menu so clicking 'X'
                // cannot terminate the background process!
                let hmenu = GetSystemMenu(hwnd, 0);
                if hmenu != 0 {
                    DeleteMenu(hmenu, SC_CLOSE, MF_BYCOMMAND);
                }

                // Register Ctrl+C / Ctrl+Break handler to hide window instead of terminating
                SetConsoleCtrlHandler(Some(console_ctrl_handler), 1);

                // Print friendly console guidance
                println!("============================================================");
                println!(" Future Academy Link");
                println!(" Running in background.");
                println!(" - To hide console: Minimize window, press Ctrl+C, or use tray menu.");
                println!(" - To quit app:     Use 'Quit' in the system tray menu.");
                println!("============================================================");

                // Watcher thread: when the user minimizes the console window, hide it completely to the tray
                std::thread::spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    let hwnd = CONSOLE_HWND.load(Ordering::Relaxed);
                    if hwnd != 0 && IsWindowVisible(hwnd) != 0 && IsIconic(hwnd) != 0 {
                        ShowWindow(hwnd, SW_HIDE);
                    }
                });

                // Stdin thread: if user types exit / hide / quit / close, hide the console
                std::thread::spawn(move || {
                    use std::io::BufRead;
                    let stdin = std::io::stdin();
                    for line in stdin.lock().lines() {
                        if let Ok(cmd) = line {
                            let cmd = cmd.trim().to_lowercase();
                            if cmd == "hide" || cmd == "exit" || cmd == "quit" || cmd == "close" {
                                println!("[link] Console hidden. Future Academy Link is still running in background.");
                                hide_console();
                            }
                        }
                    }
                });
            }
        }
    }

    pub fn is_console_visible() -> bool {
        let hwnd = CONSOLE_HWND.load(Ordering::Relaxed);
        if hwnd != 0 {
            unsafe { IsWindowVisible(hwnd) != 0 }
        } else {
            false
        }
    }

    pub fn show_console() {
        let hwnd = CONSOLE_HWND.load(Ordering::Relaxed);
        if hwnd != 0 {
            unsafe {
                ShowWindow(hwnd, SW_SHOW);
                ShowWindow(hwnd, SW_SHOWNORMAL);
                SetForegroundWindow(hwnd);
            }
        }
    }

    pub fn hide_console() {
        let hwnd = CONSOLE_HWND.load(Ordering::Relaxed);
        if hwnd != 0 {
            unsafe {
                ShowWindow(hwnd, SW_HIDE);
            }
        }
    }

    pub fn toggle_console() -> bool {
        if is_console_visible() {
            hide_console();
            false
        } else {
            show_console();
            true
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod console {
    pub fn init_console() {}
    pub fn is_console_visible() -> bool { false }
    pub fn show_console() {}
    pub fn hide_console() {}
    pub fn toggle_console() -> bool { false }
}

fn open_url(url: &str) {
    let _ = open::that_detached(url);
}

/// Global handle to the main tokio runtime, set once by start_runtime() and
/// read by OTA spawn sites on the main thread.
static RT_HANDLE: OnceLock<tokio::runtime::Handle> = OnceLock::new();

/// Spawn the tokio runtime on a background thread and start the link server +
/// toolchain setup. Returns immediately; the runtime thread runs forever.
fn start_runtime() {
    thread::spawn(|| {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("failed to build tokio runtime");
        RT_HANDLE.set(rt.handle().clone()).ok();
        rt.block_on(async {
            // Path resolution (port of start-link-server.js).
            let base_dir = paths::resolve_runtime_base_dir();
            let user_data_path = paths::resolve_user_data_path(&base_dir);
            let tools_path = paths::resolve_tools_path(&base_dir);

            tracing::info!("[link] runtime base: {}", base_dir.display());
            tracing::info!("[link] tools path: {}", tools_path.display());
            tracing::info!("[link] user data: {}", user_data_path.display());

            let app = Arc::new(AppState::new(user_data_path, tools_path.clone()));

            // Background toolchain check/setup → updates /status.
            let (ok, _cli) = toolchain::check_toolchain(&tools_path);
            if ok {
                let layout = paths::validate_tools_layout(&tools_path);
                if !layout.ok {
                    for m in &layout.missing {
                        tracing::error!("[link] some tools are missing: {}", m);
                    }
                }
            } else {
                tracing::info!("[link] downloading toolchain in background…");

                // indicatif progress bar + a background print thread so the bar
                // updates on a stable terminal line even when tokio yields.
                let pb = ProgressBar::new(100);
                pb.set_style(
                    ProgressStyle::with_template(
                        "{spinner:.cyan} [{bar:40}] {msg:.dim} {percent:>3}%",
                    )
                    .unwrap()
                    .progress_chars("█▉▊▋▌▍▎▏  "),
                );
                pb.set_message("downloading-cli");
                let pb = Arc::new(Mutex::new(Some(pb)));
                let (tx, rx) = channel::<(String, u8)>();
                let pb_for_print = pb.clone();
                let _print_thread = thread::spawn(move || {
                    // Drain the channel and update the bar from a single OS thread,
                    // keeping the cursor in one place so the bar redraws cleanly.
                    while let Ok((phase, pct)) = rx.recv() {
                        let label = match phase.as_str() {
                            "downloading-cli" => "Downloading CLI",
                            "extracting" => "Extracting",
                            "configuring" => "Configuring",
                            "updating-index" => "Updating index",
                            "downloading-platform" => "Downloading ESP32 core",
                            "downloading-tools" => "Downloading toolchain",
                            "pruning" => "Cleaning up",
                            "done" => "Done",
                            "error" => "Error",
                            _ => "Setup",
                        };
                        if let Some(pb) = pb_for_print.lock().unwrap().as_ref() {
                            pb.set_message(label);
                            pb.set_position(pct as u64);
                            if pct >= 100 {
                                pb.finish();
                            }
                        }
                    }
                });

                app.set_setup_phase(Some("downloading-cli".to_string()));
                app.set_setup_progress(0);
                let app_setup = app.clone();
                let tools_setup = tools_path.clone();
                let tx_for_setup = tx.clone();
                tokio::spawn(async move {
                    let app_for_cb = app_setup.clone();
                    let tx_clone = tx_for_setup.clone();
                    let report_fn: toolchain::ProgressFn =
                        Arc::new(move |p: toolchain::SetupProgress| {
                            app_for_cb.set_setup_phase(if p.phase == "done" {
                                None
                            } else {
                                Some(p.phase.clone())
                            });
                            app_for_cb.set_setup_progress(p.progress);
                            let _ = tx_clone.send((p.phase.clone(), p.progress));
                        });
                    let res = toolchain::setup_toolchain(&tools_setup, report_fn).await;
                    // Signal the print thread to drain then exit.
                    let _ = tx_for_setup.send(("done".to_string(), 100));
                    drop(tx_for_setup);
                    if let Err(e) = res {
                        let _ = tx.send(("error".to_string(), 0));
                        tracing::error!("[link] toolchain setup failed: {}", e);
                        app_setup.set_setup_phase(Some("error".to_string()));
                    } else {
                        // CLI environment init after successful toolchain setup.
                        // Runs on the blocking thread pool so the tokio worker
                        // is not held hostage by arduino-cli shell-outs.
                        let tools_clone = tools_setup.clone();
                        let user_data_clone = app_setup.user_data_path.clone();
                        if let Err(e) = tokio::task::spawn_blocking(move || {
                            upload::arduino::init_cli_environment(
                                &tools_clone,
                                &user_data_clone,
                            );
                        })
                        .await
                        {
                            tracing::error!("[link] init_cli_environment panicked: {}", e);
                        }
                    }
                });
            }

            // Initialize CLI environment at startup when tools already exist.
            // Runs on the blocking thread pool so the tokio worker is free
            // and the server can start serving immediately in parallel.
            if ok {
                let tools_clone = tools_path.clone();
                let user_data_clone = app.user_data_path.clone();
                let cli_init = tokio::task::spawn_blocking(move || {
                    upload::arduino::init_cli_environment(
                        &tools_clone,
                        &user_data_clone,
                    );
                });
                // Fire-and-forget: server starts now, CLI init happens in background.
                // Errors are logged inside init_cli_environment via tracing.
                tokio::spawn(async move {
                    if let Err(e) = cli_init.await {
                        tracing::error!("[link] init_cli_environment panicked: {}", e);
                    }
                });
            }

            // Serve forever (with EADDRINUSE same-server retry).
            if let Err(e) = server::start(app).await {
                tracing::error!("[link] server error: {}", e);
            }
        });
    });
}

fn main() {
    // Initialize or attach console on Windows before tracing initialization.
    // Disables the Close ('X') button so closing the console does not terminate
    // the application, enables minimize-to-tray, and provides tray show/hide.
    console::init_console();

    // Subscribe tracing to stderr. Colors stay on for interactive terminals
    // and are stripped automatically when stderr is redirected (a file or
    // pipe). The default formatter emits one line per event with a timestamp
    // and target, which is what operators expect.
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .try_init();

    // Acquire before starting the runtime or tool installer. A second launch
    // exits quietly and leaves the existing tray process untouched.
    let _instance_guard = match instance::acquire() {
        Ok(instance::AcquireOutcome::Acquired(guard)) => Some(guard),
        Ok(instance::AcquireOutcome::AlreadyRunning) => {
            tracing::info!("[link] another Future Academy Link instance is already running");
            return;
        }
        Err(error) => {
            // Do not make an unusual temp-directory policy fatal. The server
            // port guard and installer lock remain as secondary protection.
            tracing::warn!("[link] single-instance guard unavailable: {error}");
            None
        }
    };

    // Auto-start at system login (enabled by default on first run).
    autostart::init_autostart();

    // --headless: run without tray icon, just the server. Use Ctrl+C to stop.
    let headless = std::env::args().any(|a| a == "--headless");
    if headless {
        progress::set_headless(true);
        tracing::info!("[link] starting in headless mode (no tray icon)");
    }

    // Test notifications (for debugging).
    if std::env::args().any(|a| a == "--test-notification") {
        tracing::info!("[link] showing test notification");
        let _ = notification::show_test_notification();
    }
    if std::env::args().any(|a| a == "--test-upload-notification") {
        tracing::info!("[link] showing test upload notification");
        let _ = notification::show_test_upload_notification();
    }
    if std::env::args().any(|a| a == "--test-download-notification") {
        tracing::info!("[link] showing test download notification");
        let _ = notification::show_test_download_notification();
    }
    if std::env::args().any(|a| a == "--test-update-notification") {
        tracing::info!("[link] showing test update notification");
        let _ = notification::show_test_update_notification();
    }

    // Start the embedded link server on its own runtime thread (no Node spawn).
    start_runtime();

    if headless {
        tracing::info!("[link] server running — press Ctrl+C to stop");
        // Block forever; the runtime thread lives until the process is killed.
        loop {
            thread::park();
        }
    }

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    TrayIconEvent::set_event_handler(Some(|_| {}));
    let menu_receiver = MenuEvent::receiver();

    {
        let proxy = proxy.clone();
        thread::spawn(move || loop {
            let state = match ureq::get(STATUS_URL).call() {
                Ok(resp) => match resp.into_json::<StatusResponse>() {
                    Ok(s) => TrayState::from_response(&s),
                    Err(_) => TrayState::starting(),
                },
                Err(_) => TrayState::starting(),
            };
            let _ = proxy.send_event(UserEvent::Status(state));
            thread::sleep(POLL_INTERVAL);
        });
    }

    // ── Background OTA update check (5 s after startup, then every 4 h) ────
    let proxy_upd = proxy.clone();
    {
        let proxy = proxy.clone();
        thread::spawn(move || {
            // Wait for the main runtime handle to become available.
            while RT_HANDLE.get().is_none() {
                thread::sleep(Duration::from_millis(50));
            }
            let handle = RT_HANDLE.get().unwrap();

            let client = reqwest::Client::builder()
                .user_agent("FutureAcademyLink/2.0")
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(30))
                .build()
                .expect("update check client");

            thread::sleep(Duration::from_secs(2));

            loop {
                let result = handle.block_on(update::check_for_update(&client));
                let _ = proxy.send_event(UserEvent::UpdateCheck(result));
                thread::sleep(Duration::from_secs(4 * 60 * 60));
            }
        });
    }

    // ── Static menu items (never removed) ───────────────────────────────────
    let menu = Menu::new();
    let title_item = MenuItem::new("Future Academy Link", false, None);
    let status_item = MenuItem::new("Starting\u{2026}", false, None);
    let sep1 = PredefinedMenuItem::separator();
    let devices_header = MenuItem::new("Devices", false, None);
    let sep2 = PredefinedMenuItem::separator();
    let open_website = MenuItem::new("Open Website", true, None);
    let console_toggle = MenuItem::new(
        if console::is_console_visible() {
            "Hide Console"
        } else {
            "Show Console"
        },
        true,
        None,
    );
    let sep3 = PredefinedMenuItem::separator();
    let update_check_item = MenuItem::new("Check for Updates\u{2026}", true, None);
    let sep_upd = PredefinedMenuItem::separator();
    let autostart_enabled = autostart::is_autostart_enabled();
    let autostart_toggle = MenuItem::new(
        if autostart_enabled {
            "Run at Startup \u{2713}"
        } else {
            "Run at Startup"
        },
        true,
        None,
    );
    let sep_final = PredefinedMenuItem::separator();
    let quit_accel = if cfg!(target_os = "macos") {
        Accelerator::new(Some(Modifiers::META), Code::KeyQ)
    } else {
        Accelerator::new(Some(Modifiers::ALT), Code::F4)
    };
    let quit_item = MenuItem::new("Quit", true, Some(quit_accel));

    menu.append(&title_item).ok();
    menu.append(&status_item).ok();
    menu.append(&sep1).ok();
    menu.append(&devices_header).ok();
    menu.append(&sep2).ok();
    menu.append(&open_website).ok();
    menu.append(&console_toggle).ok();
    menu.append(&sep3).ok();
    menu.append(&update_check_item).ok();
    menu.append(&sep_upd).ok();
    menu.append(&autostart_toggle).ok();
    menu.append(&sep_final).ok();
    menu.append(&quit_item).ok();

    let open_website_id = open_website.id().clone();
    let console_toggle_id = console_toggle.id().clone();
    let quit_id = quit_item.id().clone();
    let update_check_id = update_check_item.id().clone();
    let autostart_toggle_id = autostart_toggle.id().clone();

    let icon = {
        // Load logo.png and resize to 44×44 px (22 pt @2× Retina).
        const SIZE: u32 = 44;
        let img = image::load_from_memory(ICON_PNG).expect("invalid icon PNG");
        let img = img.resize_exact(SIZE, SIZE, image::imageops::FilterType::Lanczos3);
        let rgba = img.into_rgba8().into_raw();
        tray_icon::Icon::from_rgba(rgba, SIZE, SIZE).expect("invalid icon")
    };

    let _tray = TrayIconBuilder::new()
        .with_tooltip("Future Academy Link")
        .with_icon(icon)
        .with_menu(Box::new(menu.clone()))
        .build()
        .expect("failed to build tray icon");

    // Devices section starts right after devices_header (index 4 in the menu).
    let mut current_device_items: Vec<MenuItem> = Vec::new();
    let mut update_check_in_progress = false;
    let mut pending_update: Option<update::UpdateInfo> = None;
    let mut prepared_update: Option<update::PreparedUpdate> = None;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        while let Ok(ev) = menu_receiver.try_recv() {
            if ev.id == open_website_id {
                open_url(SCRATCH_URL);
            } else if ev.id == console_toggle_id {
                let visible = console::toggle_console();
                let _ = console_toggle.set_text(if visible {
                    "Hide Console"
                } else {
                    "Show Console"
                });
            } else if ev.id == autostart_toggle_id {
                let current = autostart::is_autostart_enabled();
                if current {
                    if let Err(e) = autostart::disable_autostart() {
                        tracing::warn!("[autostart] failed to disable: {}", e);
                    } else {
                        let _ = autostart_toggle.set_text("Run at Startup");
                    }
                } else {
                    if let Err(e) = autostart::enable_autostart() {
                        tracing::warn!("[autostart] failed to enable: {}", e);
                    } else {
                        let _ = autostart_toggle.set_text("Run at Startup \u{2713}");
                    }
                }
            } else if ev.id == quit_id {
                *control_flow = ControlFlow::Exit;
            } else if ev.id == update_check_id {
                if update_check_in_progress {
                    // Already checking — ignore.
                } else if let Some(prepared) = prepared_update.take() {
                    match update::install_prepared_update(prepared) {
                        update::ApplyOutcome::RestartRequired => {
                            update_check_item.set_text("Restarting to finish update\u{2026}");
                            update_check_item.set_enabled(false);
                            *control_flow = ControlFlow::Exit;
                        }
                        update::ApplyOutcome::Failed(error) => {
                            update_check_item.set_text(&format!("Update failed: {error}"));
                            update_check_item.set_enabled(true);
                        }
                    }
                } else if let Some(info) = pending_update.take() {
                    let version_label = info.version_label.clone();
                    update_check_item.set_text(&format!("Downloading {}\u{2026}", version_label));
                    update_check_item.set_enabled(false);
                    update_check_in_progress = true;

                    let proxy = proxy_upd.clone();
                    thread::spawn(move || {
                        let handle = RT_HANDLE.get().expect("runtime handle not set");
                        let client = reqwest::Client::builder()
                            .user_agent("FutureAcademyLink/2.0")
                            .connect_timeout(Duration::from_secs(10))
                            .timeout(Duration::from_secs(30 * 60))
                            .build()
                            .expect("update download client");
                        let progress_proxy = proxy.clone();
                        let progress: Arc<dyn Fn(u64, u64) + Send + Sync> =
                            Arc::new(move |received, total| {
                                let _ = progress_proxy
                                    .send_event(UserEvent::UpdateProgress { received, total });
                            });

                        let result = match handle.block_on(update::download_update(
                            &client,
                            &info,
                            Some(progress),
                        )) {
                            update::DownloadOutcome::Downloaded(bytes) => {
                                update::prepare_update(&bytes)
                            }
                            update::DownloadOutcome::Failed(error) => Err(error),
                        };
                        let _ = proxy.send_event(UserEvent::UpdatePrepared {
                            version_label,
                            result,
                        });
                    });
                } else {
                    // Manual trigger.
                    update_check_item.set_text("Checking for updates\u{2026}");
                    update_check_item.set_enabled(false);
                    update_check_in_progress = true;
                    let proxy = proxy_upd.clone();
                    thread::spawn(move || {
                        let handle = RT_HANDLE.get().expect("runtime handle not set");
                        let client = reqwest::Client::builder()
                            .user_agent("FutureAcademyLink/2.0")
                            .connect_timeout(Duration::from_secs(10))
                            .timeout(Duration::from_secs(30))
                            .build()
                            .expect("manual check client");
                        let result = handle.block_on(update::check_for_update(&client));
                        let _ = proxy.send_event(UserEvent::UpdateCheck(result));
                    });
                }
            }
        }

        if let Event::UserEvent(UserEvent::Status(state)) = event {
            status_item.set_text(&state.status_label);

            let is_visible = console::is_console_visible();
            let expected_text = if is_visible {
                "Hide Console"
            } else {
                "Show Console"
            };
            if console_toggle.text() != expected_text {
                let _ = console_toggle.set_text(expected_text);
            }

            let current_names: Vec<String> =
                current_device_items.iter().map(|i| i.text()).collect();
            let new_names: Vec<&str> = state.devices.iter().map(|s| s.as_str()).collect();
            let current_refs: Vec<&str> = current_names.iter().map(|s| s.as_str()).collect();

            if current_refs != new_names {
                for item in &current_device_items {
                    menu.remove(item).ok();
                }
                current_device_items.clear();

                if state.devices.is_empty() {
                    let item = MenuItem::new("No devices", false, None);
                    menu.insert(&item, 4).ok();
                    current_device_items.push(item);
                } else {
                    for (i, name) in state.devices.iter().enumerate() {
                        let item = MenuItem::new(name.as_str(), false, None);
                        menu.insert(&item, 4 + i).ok();
                        current_device_items.push(item);
                    }
                }
            }
        } else if let Event::UserEvent(UserEvent::UpdateCheck(result)) = event {
            update_check_in_progress = false;
            match result {
                update::UpdateCheck::UpToDate => {
                    update_check_item.set_text("Up to date");
                    update_check_item.set_enabled(true);
                    pending_update = None;
                    prepared_update = None;
                }
                update::UpdateCheck::Available(info) => {
                    let version_label = info.version_label.clone();
                    tracing::info!("[update] new update available: {}", version_label);

                    // Show notification on Windows
                    #[cfg(windows)]
                    {
                        notification::notify_update_available(&version_label);
                    }

                    #[cfg(not(windows))]
                    let _ = proxy_upd;

                    update_check_item.set_text(&format!("Update to {} \u{2192}", version_label));
                    update_check_item.set_enabled(true);
                    pending_update = Some(info);
                    prepared_update = None;
                }
                update::UpdateCheck::Error(e) => {
                    update_check_item.set_text(&format!("Update failed: {e}"));
                    update_check_item.set_enabled(true);
                    pending_update = None;
                }
            }
        } else if let Event::UserEvent(UserEvent::UpdateProgress { received, total }) = event {
            if total > 0 {
                let percent = received.saturating_mul(100) / total;
                update_check_item.set_text(&format!("Downloading update\u{2026} {}%", percent));
            } else {
                update_check_item.set_text(&format!(
                    "Downloading update\u{2026} {} KB",
                    received / 1024
                ));
            }
        } else if let Event::UserEvent(UserEvent::UpdatePrepared {
            version_label,
            result,
        }) = event
        {
            update_check_in_progress = false;
            match result {
                Ok(prepared) => {
                    prepared_update = Some(prepared);
                    update_check_item
                        .set_text(&format!("Restart to install {} \u{2192}", version_label));
                    update_check_item.set_enabled(true);
                }
                Err(error) => {
                    update_check_item.set_text(&format!("Update failed: {error}"));
                    update_check_item.set_enabled(true);
                }
            }
        }
    });
}
