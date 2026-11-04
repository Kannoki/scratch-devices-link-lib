//! Runtime toolchain setup and validation.
//!
//! The desktop app installs one platform-specific tools archive. Keeping this
//! module focused on that package avoids drifting between an archive installer
//! and a second per-component installer with different versions and layouts.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::download;
use crate::notification;

#[cfg(windows)]
pub const CLI_FILE: &str = "arduino-cli.exe";
#[cfg(not(windows))]
pub const CLI_FILE: &str = "arduino-cli";

#[allow(dead_code)]
pub const ESP32_INDEX_URL: &str =
    "https://raw.githubusercontent.com/espressif/arduino-esp32/gh-pages/package_esp32_index.json";

/// Setup-phase + progress payload.
#[derive(Debug, Clone)]
pub struct SetupProgress {
    pub phase: String,
    pub progress: u8,
}

/// Shared progress sink. Cloneable so it can be captured by background tasks.
pub type ProgressFn = Arc<dyn Fn(SetupProgress) + Send + Sync + 'static>;

/// Full-package validation result. A CLI by itself is not a usable toolchain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolchainValidation {
    pub missing: Vec<String>,
}

impl ToolchainValidation {
    pub fn is_ready(&self) -> bool {
        self.missing.is_empty()
    }
}

fn expected_binary(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn find_file_named(root: &Path, file_name: &str) -> Option<PathBuf> {
    if !root.exists() {
        return None;
    }

    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if entry.file_name() == file_name {
                return Some(path);
            }
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// Validate every package component required for ESP32-S3 compilation/upload.
///
/// The paths below intentionally allow arbitrary version directory names.
/// Tool releases can therefore update versions without changing the app.
pub fn validate_toolchain(tools_path: &Path) -> ToolchainValidation {
    let arduino = tools_path.join("Arduino");
    let cli = arduino.join(CLI_FILE);
    let esp32_hardware = arduino
        .join("packages")
        .join("esp32")
        .join("hardware")
        .join("esp32");
    let esp32_tools = arduino.join("packages").join("esp32").join("tools");
    let builtin_tools = arduino.join("packages").join("builtin").join("tools");
    let libraries = arduino.join("libraries");

    let mut missing = Vec::new();
    if !is_executable(&cli) {
        missing.push(format!("executable Arduino CLI ({})", cli.display()));
    }
    if find_file_named(&esp32_hardware, "platform.txt").is_none() {
        missing.push(format!(
            "ESP32 platform (platform.txt under {})",
            esp32_hardware.display()
        ));
    }

    let esptool_name = expected_binary("esptool");
    if find_file_named(&esp32_tools.join("esptool_py"), &esptool_name)
        .filter(|path| is_executable(path))
        .is_none()
    {
        missing.push(format!("ESP32 uploader ({esptool_name})"));
    }

    let compiler_name = expected_binary("xtensa-esp32s3-elf-g++");
    if find_file_named(&esp32_tools, &compiler_name)
        .filter(|path| is_executable(path))
        .is_none()
    {
        missing.push(format!("ESP32-S3 compiler ({compiler_name})"));
    }
    let frontend_name = expected_binary("cc1plus");
    if find_file_named(&esp32_tools.join("esp-x32"), &frontend_name)
        .filter(|path| is_executable(path))
        .is_none()
    {
        missing.push(format!("ESP32-S3 C++ frontend ({frontend_name})"));
    }
    let ctags_name = expected_binary("ctags");
    if find_file_named(&builtin_tools.join("ctags"), &ctags_name)
        .filter(|path| is_executable(path))
        .is_none()
    {
        missing.push(format!("Arduino source indexer ({ctags_name})"));
    }

    if !libraries.is_dir() {
        let _ = fs::create_dir_all(&libraries);
    }
    if !libraries.is_dir() {
        missing.push(format!("Arduino libraries ({})", libraries.display()));
    }

    ToolchainValidation { missing }
}

/// Compatibility wrapper used by startup.
pub fn check_toolchain(tools_path: &Path) -> (bool, PathBuf) {
    let cli = tools_path.join("Arduino").join(CLI_FILE);
    if let Err(error) = repair_executable_permissions(tools_path) {
        tracing::warn!("[tools] startup permission repair failed: {error}");
    }
    (validate_toolchain(tools_path).is_ready(), cli)
}

/// Restore execute bits that the 7z decoder does not preserve on Unix.
///
/// Arduino packages contain executable payloads in many layouts: `bin/`,
/// `libexec/`, and version roots such as `builtin/tools/ctags/5.8/ctags`.
/// The 7z decoder does not expose enough Unix mode metadata to distinguish
/// binaries from support files reliably, so every regular file inside a
/// package `tools` payload receives execute bits. Executing a support file is
/// harmless, while missing one executable breaks builds at an arbitrary stage.
#[cfg(unix)]
pub fn repair_executable_permissions(tools_path: &Path) -> Result<usize, String> {
    use std::os::unix::fs::PermissionsExt;

    let arduino = tools_path.join("Arduino");
    if !arduino.exists() {
        return Ok(0);
    }

    let known_launchers = [
        "arduino-cli",
        "esptool",
        "esptool.py",
        "espota.py",
        "gen_esp32part.py",
        "mklittlefs",
        "mkspiffs",
    ];
    let mut repaired = 0;
    let mut pending = vec![arduino.clone()];
    while let Some(dir) = pending.pop() {
        let entries = fs::read_dir(&dir)
            .map_err(|error| format!("read tool directory {}: {error}", dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("read tool entry: {error}"))?;
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if !path.is_file() {
                continue;
            }

            let relative = path.strip_prefix(&arduino).unwrap_or(&path);
            let mut inside_packages = false;
            let in_package_tools = relative.components().any(|component| {
                let name = component.as_os_str();
                if name == "packages" {
                    inside_packages = true;
                    false
                } else {
                    inside_packages && name == "tools"
                }
            });
            let known_launcher = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| known_launchers.contains(&name));
            if !in_package_tools && !known_launcher {
                continue;
            }

            let metadata = fs::metadata(&path)
                .map_err(|error| format!("read permissions for {}: {error}", path.display()))?;
            let old_mode = metadata.permissions().mode();
            let new_mode = old_mode | 0o111;
            if new_mode != old_mode {
                let mut permissions = metadata.permissions();
                permissions.set_mode(new_mode);
                fs::set_permissions(&path, permissions).map_err(|error| {
                    format!("set executable permission on {}: {error}", path.display())
                })?;
                repaired += 1;
            }
        }
    }
    Ok(repaired)
}

#[cfg(not(unix))]
pub fn repair_executable_permissions(_tools_path: &Path) -> Result<usize, String> {
    Ok(0)
}

pub const BOARD_MANAGER_URLS: &[&str] = &[
    "https://raw.githubusercontent.com/espressif/arduino-esp32/gh-pages/package_esp32_index.json",
    "https://espressif.github.io/arduino-esp32/package_esp32_index.json",
    "https://arduino.esp8266.com/stable/package_esp8266com_index.json",
    "https://raw.githubusercontent.com/sparkfun/Arduino_Boards/master/IDE_Board_Manager/package_sparkfun_index.json",
    "https://github.com/earlephilhower/arduino-pico/releases/download/global/package_rp2040_index.json",
];

#[derive(Serialize)]
struct ArduinoConfig<'a> {
    board_manager: BoardManager<'a>,
    directories: ArduinoDirectories<'a>,
}

#[derive(Serialize)]
struct BoardManager<'a> {
    additional_urls: &'a [&'a str],
}

#[derive(Serialize)]
struct ArduinoDirectories<'a> {
    data: &'a str,
    downloads: &'a str,
    user: &'a str,
}

/// Write a deterministic, YAML-escaped Arduino configuration.
///
/// Serializing the paths is important on Windows: hand-written YAML can
/// misinterpret drive-letter colons, backslashes, spaces, and non-ASCII user
/// names. Process arguments continue to use native `Path`/`OsStr` values.
pub fn write_arduino_config(config_path: &Path, arduino_dir: &Path) -> Result<(), String> {
    let staging = arduino_dir.join("staging");
    fs::create_dir_all(&staging)
        .map_err(|error| format!("create Arduino staging directory: {error}"))?;
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create Arduino config directory: {error}"))?;
    }

    let data = arduino_dir.to_str().ok_or_else(|| {
        format!(
            "Arduino path is not valid Unicode: {}",
            arduino_dir.display()
        )
    })?;
    let downloads = staging
        .to_str()
        .ok_or_else(|| format!("staging path is not valid Unicode: {}", staging.display()))?;
    let config = ArduinoConfig {
        board_manager: BoardManager {
            additional_urls: BOARD_MANAGER_URLS,
        },
        directories: ArduinoDirectories {
            data,
            downloads,
            user: data,
        },
    };
    let yaml = serde_yaml::to_string(&config)
        .map_err(|error| format!("serialize Arduino config: {error}"))?;
    fs::write(config_path, yaml)
        .map_err(|error| format!("write Arduino config {}: {error}", config_path.display()))
}

/// Install or repair the one packaged toolchain archive.
pub async fn setup_toolchain(tools_path: &Path, report: ProgressFn) -> Result<(), String> {
    report(SetupProgress {
        phase: "downloading-tools".to_string(),
        progress: 0,
    });
    notification::notify_download_start("Bộ công cụ Windify");

    let progress_report = report.clone();
    let download_progress: download::ProgressFn = Arc::new(move |progress| {
        progress_report(SetupProgress {
            phase: "downloading-tools".to_string(),
            progress,
        });
    });
    let tools = tools_path.to_path_buf();
    let status =
        tokio::task::spawn_blocking(move || download::ensure_tools(&tools, download_progress))
            .await
            .map_err(|error| format!("tool package task panicked: {error}"))?;

    match status {
        download::ToolsStatus::Present => {
            tracing::info!("[tools] validated existing tool package");
        }
        download::ToolsStatus::Downloaded => {
            tracing::info!("[tools] downloaded and validated tool package");
            notification::notify_download_success("Bộ công cụ Windify");
        }
        download::ToolsStatus::Failed(error) => {
            notification::notify_download_error("Bộ công cụ Windify", &error);
            return Err(error);
        }
    }

    report(SetupProgress {
        phase: "done".to_string(),
        progress: 100,
    });
    Ok(())
}

// ── Launch diagnostics: Windows Application Control ──────────────────────────
//
// The toolchain is third-party and mostly unsigned (132 of 140 binaries in the
// shipped 3.1.3 platform); only the esptool family carries an Authenticode
// signature. On a machine where Application Control enforces - Smart App
// Control on Windows 11, WDAC, AppLocker, or Defender ISG - `CreateProcess`
// fails with `ERROR_APPCONTROL_BLOCKED` (4551) and the raw OS string reached the
// user as `failed to start ... (os error 4551)`, which reads like a hardware or
// cable fault when it is a machine policy. Nothing in the build path is at
// fault, so the failure must name the real cause and the way out.
//
// See docs/windows-app-control.md and issue #25.

/// Win32 `ERROR_APPCONTROL_BLOCKED`.
pub const ERROR_APPCONTROL_BLOCKED: i32 = 4551;

/// Why the OS refused to start one of our toolchain binaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchBlock {
    /// Application Control refused the image (4551).
    AppControl,
    /// Policy denial reported as `ERROR_ACCESS_DISABLED_BY_POLICY` (0x800704EC).
    DeniedByPolicy,
    /// Plain `ERROR_ACCESS_DENIED` (os error 5).
    AccessDenied,
    /// Image absent (os error 2 or 3): the toolchain is incomplete.
    Missing,
    /// Anything else; the caller keeps the raw OS text.
    Other,
}

impl LaunchBlock {
    /// True when the executable exists but a machine policy refuses to run it.
    pub fn is_policy_block(self) -> bool {
        matches!(self, LaunchBlock::AppControl | LaunchBlock::DeniedByPolicy)
    }
}

/// Classify a `CreateProcess` failure.
///
/// Pure and platform-independent: the numeric code is checked first so the
/// classification does not depend on the localized OS message.
pub fn classify_launch_error(error: &std::io::Error) -> LaunchBlock {
    let code = error.raw_os_error().unwrap_or_default();
    let text = error.to_string().to_lowercase();
    if code == ERROR_APPCONTROL_BLOCKED {
        return LaunchBlock::AppControl;
    }
    if text.contains("application control") || text.contains("4551") || text.contains("0x11c7") {
        return LaunchBlock::AppControl;
    }
    if text.contains("0x800704ec")
        || text.contains("disabled by policy")
        || text.contains("blocked by policy")
    {
        return LaunchBlock::DeniedByPolicy;
    }
    match code {
        2 | 3 => LaunchBlock::Missing,
        5 => LaunchBlock::AccessDenied,
        _ => LaunchBlock::Other,
    }
}

fn app_control_message() -> String {
    "Windows Application Control đã chặn toolchain của Arduino (os error 4551). \
Đây là chính sách bảo mật của Windows, KHÔNG phải lỗi cáp USB hay mạch nạp. \
Binary arduino-cli / trình biên dịch không có chữ ký số nên Smart App Control (Windows 11), \
WDAC hoặc AppLocker đã từ chối khởi chạy. \
Cách xử lý: nhờ IT cho phép thư mục tools của FutureAcademy (AppLocker path rule hoặc \
Defender exclusion), hoặc chuyển sang phương án biên dịch trên máy chủ. \
Xem docs/windows-app-control.md."
        .to_string()
}

/// Turn a failed toolchain launch into a message that names the cause and the fix.
pub fn launch_failure_message(program: &Path, error: &std::io::Error) -> String {
    let path = program.display();
    match classify_launch_error(error) {
        LaunchBlock::AppControl | LaunchBlock::DeniedByPolicy => format!(
            "{path}: blocked by Windows Application Control ({error})\n{}",
            app_control_message()
        ),
        LaunchBlock::Missing => format!(
            "{path}: not found ({error}). Toolchain chưa đầy đủ - khởi động lại FutureAcademy để tải lại gói tools."
        ),
        LaunchBlock::AccessDenied => format!(
            "{path}: access denied ({error}). Một tiến trình khác hoặc antivirus đang giữ file; \
đóng các cửa sổ Arduino/IDE khác rồi thử lại."
        ),
        LaunchBlock::Other => format!("failed to start {path}: {error}"),
    }
}

/// Phrases that mean "an unsigned toolchain binary was blocked", as they appear
/// in captured output. The elevated upload path runs through `cmd.exe` with the
/// output redirected to a log file, so the refusal arrives as text rather than
/// as a spawn error.
const APP_CONTROL_NEEDLES: [&str; 4] = [
    "application control policy",
    "application control",
    "os error 4551",
    "0x11c7",
];

/// Detect an Application Control refusal inside captured command output.
pub fn app_control_hint(output: &str) -> Option<String> {
    let lower = output.to_lowercase();
    if !APP_CONTROL_NEEDLES
        .iter()
        .any(|needle| lower.contains(needle))
    {
        return None;
    }
    Some(format!(
        "Windows Application Control đã chặn toolchain (os error 4551).\n{}",
        app_control_message()
    ))
}

/// Size of the PE certificate table (Authenticode), or `None` when `bytes` is
/// not a readable PE image. `Some(0)` means "no signature at all".
///
/// A present table does not prove the signature is valid, but an absent one
/// proves the image is unsigned - which is the precondition for Application
/// Control to refuse it.
fn pe_certificate_table_size(bytes: &[u8]) -> Option<u32> {
    fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
        bytes
            .get(offset..offset + 2)
            .map(|slice| u16::from_le_bytes([slice[0], slice[1]]))
    }
    fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
        bytes
            .get(offset..offset + 4)
            .map(|slice| u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
    }

    if bytes.get(0..2)? != b"MZ" {
        return None;
    }
    let pe_offset = u32_at(bytes, 0x3c)? as usize;
    if bytes.get(pe_offset..pe_offset + 4)? != b"PE\0\0" {
        return None;
    }
    // Optional header follows the 24-byte PE signature + COFF header.
    let optional = pe_offset + 24;
    let (dir_count, dir_start) = match u16_at(bytes, optional)? {
        0x20b => (optional + 108, optional + 112), // PE32+
        0x10b => (optional + 92, optional + 96),   // PE32
        _ => return None,
    };
    if u32_at(bytes, dir_count)? <= 4 {
        return Some(0);
    }
    // Data directory entry 4 is { VirtualAddress, Size }.
    u32_at(bytes, dir_start + 4 * 8 + 4)
}

/// True when the PE file carries an Authenticode certificate table.
/// `None` when the file cannot be read or is not a PE image.
pub fn pe_has_signature(path: &Path) -> Option<bool> {
    let bytes = fs::read(path).ok()?;
    pe_certificate_table_size(&bytes).map(|size| size > 0)
}

/// Log a warning when a toolchain launcher carries no Authenticode signature.
pub fn warn_if_unsigned(tools_path: &Path) -> Option<bool> {
    let cli = tools_path.join("Arduino").join(CLI_FILE);
    let signed = pe_has_signature(&cli)?;
    if !signed {
        tracing::warn!(
            "[tools] {} carries no Authenticode signature; Windows Application Control \
(Smart App Control / WDAC / AppLocker) may refuse to run it - see docs/windows-app-control.md",
            cli.display()
        );
    }
    Some(signed)
}

/// Startup preflight timeout for `arduino-cli version`.
pub const CLI_PROBE_TIMEOUT_MS: u64 = 3000;

/// Result of probing the CLI binary.
#[derive(Debug, Clone)]
pub struct CliProbe {
    pub ok: bool,
    pub block: LaunchBlock,
    /// Human-readable cause; empty when the probe succeeded.
    pub detail: String,
}

/// Start `arduino-cli version` to prove the OS will actually run the toolchain.
///
/// This is the only reliable way to detect an Application Control block: the
/// policy lives in the OS, not in the file, so it cannot be read off the image.
pub fn probe_cli(tools_path: &Path) -> CliProbe {
    let cli = tools_path.join("Arduino").join(CLI_FILE);
    if !cli.is_file() {
        return CliProbe {
            ok: false,
            block: LaunchBlock::Missing,
            detail: launch_failure_message(&cli, &std::io::Error::from_raw_os_error(2)),
        };
    }

    let spawned = Command::new(&cli)
        .arg("version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => {
            return CliProbe {
                ok: false,
                block: classify_launch_error(&error),
                detail: launch_failure_message(&cli, &error),
            };
        }
    };

    let deadline = Instant::now() + Duration::from_millis(CLI_PROBE_TIMEOUT_MS);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return CliProbe {
                    ok: status.success(),
                    block: LaunchBlock::Other,
                    detail: if status.success() {
                        String::new()
                    } else {
                        format!("{} exited with {status}", cli.display())
                    },
                };
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return CliProbe {
                    ok: false,
                    block: LaunchBlock::Other,
                    detail: format!(
                        "{} did not answer within {CLI_PROBE_TIMEOUT_MS}ms",
                        cli.display()
                    ),
                };
            }
            Err(error) => {
                return CliProbe {
                    ok: false,
                    block: classify_launch_error(&error),
                    detail: launch_failure_message(&cli, &error),
                };
            }
        }
    }
}

/// Some(probe) when the toolchain exists but a machine policy refuses to run it.
pub fn toolchain_launch_blocker(tools_path: &Path) -> Option<CliProbe> {
    let probe = probe_cli(tools_path);
    if probe.ok || !probe.block.is_policy_block() {
        return None;
    }
    Some(probe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn test_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{label}-{}", Uuid::new_v4()))
    }

    fn write_file(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, b"test").unwrap();
    }

    fn create_minimal_toolchain(root: &Path) {
        write_file(&root.join("Arduino").join(CLI_FILE));
        write_file(&root.join("Arduino/packages/esp32/hardware/esp32/3.1.3/platform.txt"));
        write_file(
            &root
                .join("Arduino/packages/esp32/tools/esptool_py/4.9")
                .join(expected_binary("esptool")),
        );
        write_file(
            &root
                .join("Arduino/packages/esp32/tools/xtensa/14.2/bin")
                .join(expected_binary("xtensa-esp32s3-elf-g++")),
        );
        write_file(
            &root
                .join("Arduino/packages/esp32/tools/esp-x32/2405/libexec/gcc/xtensa-esp-elf/13.2.0")
                .join(expected_binary("cc1plus")),
        );
        write_file(
            &root
                .join("Arduino/packages/builtin/tools/ctags/5.8-arduino11")
                .join(expected_binary("ctags")),
        );
        fs::create_dir_all(root.join("Arduino/libraries/Windify")).unwrap();
        repair_executable_permissions(root).unwrap();
    }

    #[test]
    fn validates_versioned_layout_under_unicode_path() {
        let root = test_root("Thiết-bị-học-sinh");
        create_minimal_toolchain(&root);
        assert!(validate_toolchain(&root).is_ready());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validates_futureacademy_tools_if_present() {
        let p = Path::new(r"C:\futureacademy\tools");
        if p.exists() {
            let res = validate_toolchain(p);
            assert!(res.is_ready(), "missing: {:?}", res.missing);
        }
    }

    #[test]
    fn reports_missing_uploader_instead_of_accepting_cli_only() {
        let root = test_root("toolchain-incomplete");
        create_minimal_toolchain(&root);
        fs::remove_file(
            root.join("Arduino/packages/esp32/tools/esptool_py/4.9")
                .join(expected_binary("esptool")),
        )
        .unwrap();
        let result = validate_toolchain(&root);
        assert!(!result.is_ready());
        assert!(result.missing.iter().any(|item| item.contains("uploader")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn serializes_unicode_paths_as_yaml_strings() {
        let root = test_root("Nguyễn Văn A");
        let arduino = root.join("Người dùng").join("Học sinh").join("Arduino");
        let config = root.join("arduino-cli.yaml");
        write_arduino_config(&config, &arduino).unwrap();
        let parsed: serde_yaml::Value =
            serde_yaml::from_slice(&fs::read(&config).unwrap()).unwrap();
        assert_eq!(parsed["directories"]["data"].as_str(), arduino.to_str());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn repairs_nested_tool_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let root = test_root("tool-permissions");
        create_minimal_toolchain(&root);
        let esptool = root.join("Arduino/packages/esp32/tools/esptool_py/4.9/esptool");
        let cc1plus = root.join(
            "Arduino/packages/esp32/tools/esp-x32/2405/libexec/gcc/xtensa-esp-elf/13.2.0/cc1plus",
        );
        let ctags = root
            .join("Arduino/packages/builtin/tools/ctags/5.8-arduino11")
            .join(expected_binary("ctags"));
        let library_source = root.join("Arduino/libraries/Windify/Windify.cpp");
        write_file(&library_source);
        fs::set_permissions(&esptool, fs::Permissions::from_mode(0o644)).unwrap();
        fs::set_permissions(&cc1plus, fs::Permissions::from_mode(0o644)).unwrap();
        fs::set_permissions(&ctags, fs::Permissions::from_mode(0o644)).unwrap();
        fs::set_permissions(&library_source, fs::Permissions::from_mode(0o644)).unwrap();
        let repaired = repair_executable_permissions(&root).unwrap();
        assert!(repaired >= 3);
        assert_ne!(
            fs::metadata(esptool).unwrap().permissions().mode() & 0o111,
            0
        );
        assert_ne!(
            fs::metadata(cc1plus).unwrap().permissions().mode() & 0o111,
            0
        );
        assert_ne!(fs::metadata(ctags).unwrap().permissions().mode() & 0o111, 0);
        assert_eq!(
            fs::metadata(library_source).unwrap().permissions().mode() & 0o111,
            0
        );
        fs::remove_dir_all(root).unwrap();
    }

    // ── Application Control launch diagnostics (#25) ─────────────────────

    fn pe_image(cert_table_size: u32) -> Vec<u8> {
        let mut bytes = vec![0u8; 512];
        bytes[0..2].copy_from_slice(b"MZ");
        let pe_offset = 0x40usize;
        bytes[0x3c..0x40].copy_from_slice(&(pe_offset as u32).to_le_bytes());
        bytes[pe_offset..pe_offset + 4].copy_from_slice(b"PE\0\0");
        let optional = pe_offset + 24;
        bytes[optional..optional + 2].copy_from_slice(&0x20bu16.to_le_bytes()); // PE32+
        bytes[optional + 108..optional + 112].copy_from_slice(&16u32.to_le_bytes());
        let size_offset = optional + 112 + 4 * 8 + 4;
        bytes[size_offset..size_offset + 4].copy_from_slice(&cert_table_size.to_le_bytes());
        bytes
    }

    #[test]
    fn classifies_app_control_block_by_raw_os_error() {
        let error = std::io::Error::from_raw_os_error(ERROR_APPCONTROL_BLOCKED);
        assert_eq!(classify_launch_error(&error), LaunchBlock::AppControl);
        assert!(classify_launch_error(&error).is_policy_block());
    }

    #[test]
    fn classifies_app_control_block_by_message_text() {
        // Non-elevated reports carry the code; the elevated path only carries text.
        let error = std::io::Error::new(
            std::io::ErrorKind::Other,
            "An Application Control policy has blocked this file.",
        );
        assert_eq!(classify_launch_error(&error), LaunchBlock::AppControl);
    }

    #[test]
    fn classifies_missing_and_access_denied_and_other() {
        assert_eq!(
            classify_launch_error(&std::io::Error::from_raw_os_error(2)),
            LaunchBlock::Missing
        );
        assert_eq!(
            classify_launch_error(&std::io::Error::from_raw_os_error(5)),
            LaunchBlock::AccessDenied
        );
        assert_eq!(
            classify_launch_error(&std::io::Error::from_raw_os_error(110)),
            LaunchBlock::Other
        );
        assert!(!classify_launch_error(&std::io::Error::from_raw_os_error(110)).is_policy_block());
    }

    #[test]
    fn launch_failure_message_names_the_policy_and_the_binary() {
        let error = std::io::Error::from_raw_os_error(ERROR_APPCONTROL_BLOCKED);
        let message = launch_failure_message(Path::new("C:\\tools\\Arduino\\arduino-cli.exe"), &error);
        assert!(message.contains("Application Control"));
        assert!(message.contains("arduino-cli.exe"));
        assert!(message.contains("windows-app-control.md"));
    }

    #[test]
    fn detects_app_control_refusal_in_captured_output() {
        let blocked = "failed to start C:\\futureacademy\\tools\\Arduino/arduino-cli.exe: \
An Application Control policy has blocked this file. (os error 4551)";
        assert!(app_control_hint(blocked).is_some());
        assert!(app_control_hint("A fatal error occurred: Failed to connect to ESP32-S3").is_none());
        assert!(app_control_hint("Hash of data verified.").is_none());
    }

    #[test]
    fn parsed_pe_certificate_table_reports_signed_and_unsigned() {
        assert_eq!(pe_certificate_table_size(&pe_image(0x0)), Some(0));
        assert_eq!(pe_certificate_table_size(&pe_image(0x200)), Some(0x200));
        assert_eq!(pe_certificate_table_size(b"not a portable executable"), None);
    }

    #[test]
    fn pe_has_signature_reads_the_image_from_disk() {
        let unsigned = std::env::temp_dir().join(format!("dsh-pe-{}.bin", Uuid::new_v4()));
        fs::write(&unsigned, pe_image(0)).unwrap();
        assert_eq!(pe_has_signature(&unsigned), Some(false));
        fs::write(&unsigned, pe_image(0x180)).unwrap();
        assert_eq!(pe_has_signature(&unsigned), Some(true));
        let _ = fs::remove_file(&unsigned);
        assert_eq!(pe_has_signature(Path::new("C:\\definitely\\missing.exe")), None);
    }
}
