//! Slint-based GUI for Future Academy Link.
//!
//! This module provides the UI implementation using Slint framework,
//! matching the Figma design specifications.

use crate::gui::screens::Screen as AppScreen;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Instant;

// Import the generated Slint module
slint::include_modules!();

/// Thread-safe state wrapper
pub type SharedState = Arc<RwLock<AppState>>;

/// Application state shared between GUI and backend
#[derive(Clone)]
pub struct AppState {
    pub screen: AppScreen,
    pub last_update: Instant,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            screen: AppScreen::Starting,
            last_update: Instant::now(),
        }
    }
}

/// Platform-specific location of the link.log file. Kept in sync with
/// `log_path()` in main.rs so the GUI opens the same file the runtime writes.
pub fn log_file_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let base = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(base).join("Library/Logs/FutureAcademy/link.log")
    }
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| "C:\\Temp".to_string());
        PathBuf::from(base).join("FutureAcademy").join("link.log")
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        PathBuf::from("/tmp/future-academy-link.log")
    }
}

/// Open link.log in a platform-appropriate console/tail viewer. This mirrors
/// `show_console_log` from main.rs but is self-contained so the GUI doesn't
/// need a reference back into main.
pub fn open_log_file() {
    let log = log_file_path();
    if !log.exists() {
        tracing::warn!(
            "[gui] link.log not found at {} — opening its parent folder instead",
            log.display()
        );
        if let Some(parent) = log.parent() {
            let _ = std::fs::create_dir_all(parent);
            #[cfg(target_os = "macos")]
            {
                let _ = std::process::Command::new("open").arg(parent).spawn();
            }
            #[cfg(target_os = "windows")]
            {
                let _ = std::process::Command::new("explorer").arg(parent).spawn();
            }
            #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
            {
                let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
            }
        }
        return;
    }

    let s = log.to_string_lossy().to_string();
    tracing::info!("[gui] opening log: {}", s);

    #[cfg(target_os = "macos")]
    {
        let ok = std::process::Command::new("open")
            .args(["-a", "Console", &s])
            .status()
            .map(|st| st.success())
            .unwrap_or(false);
        if !ok {
            let script = format!(
                "tell application \"Terminal\" to do script \"tail -f '{}'\"",
                s
            );
            let _ = std::process::Command::new("osascript")
                .args(["-e", &script])
                .spawn();
        }
    }
    #[cfg(target_os = "windows")]
    {
        // PowerShell Get-Content -Wait streams updates as they are appended.
        let cmd = format!(
            "powershell -NoExit -Command \"Get-Content '{}' -Wait\"",
            s.replace('\'', "''")
        );
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", "cmd", "/k", &cmd])
            .spawn();
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let _ = std::process::Command::new("xterm")
            .args(["-e", &format!("tail -f {}", s)])
            .spawn();
    }
}

/// Start the Slint UI and run the event loop
pub fn run_ui(shared_state: SharedState) {
    // Create the Slint app
    let app = AppWindow::new().expect("Failed to create Slint app");
    
    // Clone for callbacks
    let state_for_callbacks = shared_state.clone();
    
    // Set up callbacks
    app.on_refresh_clicked(move || {
        tracing::info!("[gui] Refresh clicked");
    });
    
    app.on_website_clicked(move || {
        tracing::info!("[gui] Website clicked");
        let _ = open::that("https://futureacademy.edu.vn");
    });
    
    app.on_console_clicked(move || {
        tracing::info!("[gui] Console clicked — opening link.log");
        open_log_file();
    });
    
    // Clone for state sync
    let state_clone = shared_state.clone();
    
    // Use Slint timer for periodic state sync
    let mut timer = slint::Timer::default();
    let app_handle = app.as_weak();
    timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(100), move || {
        if let Some(app) = app_handle.upgrade() {
            let state = state_clone.read().unwrap();
            
            // Update rotation for spinner animation
            let elapsed = state.last_update.elapsed().as_secs_f32();
            let rotation = (elapsed * 100.0) as i32 % 360;
            
            // Convert screen to Slint enum
            let slint_screen = match &state.screen {
                AppScreen::Starting => Screen::Starting,
                AppScreen::Downloading { .. } => Screen::Downloading,
                AppScreen::Extracting { .. } => Screen::Extracting,
                AppScreen::Ready { .. } => Screen::Ready,
            };
            
            // Get progress from screen
            let progress = match &state.screen {
                AppScreen::Downloading { progress } => *progress,
                AppScreen::Extracting { progress } => *progress,
                _ => 0.0,
            };
            
            // Get devices from screen
            let devices: Vec<DeviceInfo> = match &state.screen {
                AppScreen::Ready { devices } => devices
                    .iter()
                    .map(|d| DeviceInfo {
                        name: d.name.clone().into(),
                        port: d.port.clone().into(),
                        pid: d.pid.clone().into(),
                        vid: d.vid.clone().into(),
                    })
                    .collect(),
                _ => Vec::new(),
            };
            
            // Update Slint properties
            app.set_current_screen(slint_screen);
            app.set_progress(progress);
            app.set_devices(slint::ModelRc::new(slint::VecModel::from(devices)));
        }
    });
    
    tracing::info!("[gui] Starting Slint event loop...");
    
    // Run the Slint event loop
    app.run().expect("Failed to run Slint event loop");
    
    tracing::info!("[gui] Slint event loop exited");
}
