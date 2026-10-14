//! Auto-start at system login feature.
//!
//! Auto-start is enabled by default. A menu toggle allows users to disable or enable
//! this setting. If the user explicitly disables autostart, an opt-out marker is stored
//! so the app respects their choice across launches.
//! When autostart is active (the default), `init_autostart()` ensures the startup entry
//! is registered and points to the current binary, auto-recovering from binary renames
//! (e.g., FutureAcademyTray -> FutureAcademy) or directory moves.
//!
//! Platform implementations:
//! - Windows: HKCU\Software\Microsoft\Windows\CurrentVersion\Run registry
//! - macOS: ~/Library/LaunchAgents/ plist
//! - Linux: ~/.config/autostart/ .desktop file

use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Sentinel & Preference files
// ---------------------------------------------------------------------------

/// Path to the sentinel file that marks this app as having been launched before.
fn sentinel_path() -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("FutureAcademy").join(".launched")
}

/// Path to the opt-out marker file indicating the user explicitly disabled autostart.
fn disabled_marker_path() -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("FutureAcademy").join(".autostart_disabled")
}

// ---------------------------------------------------------------------------
// First-run & Preference detection
// ---------------------------------------------------------------------------

/// Returns true if this is the first time the app has ever been launched.
#[allow(dead_code)]
pub fn is_first_run() -> bool {
    !sentinel_path().exists()
}

/// Mark that the app has been launched at least once.
fn mark_launched() {
    let path = sentinel_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, "");
}

/// Returns true if the user explicitly opted out of autostart.
pub fn is_autostart_disabled() -> bool {
    disabled_marker_path().exists()
}

/// Record user's explicit autostart preference.
fn set_autostart_disabled(disabled: bool) {
    let path = disabled_marker_path();
    if disabled {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, "");
    } else {
        let _ = std::fs::remove_file(&path);
    }
}

// ---------------------------------------------------------------------------
// Cross-platform autostart registration
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum AutostartError {
    Registry(String),
    Io(String),
}

impl std::fmt::Display for AutostartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AutostartError::Registry(s) => write!(f, "Registry error: {s}"),
            AutostartError::Io(s) => write!(f, "IO error: {s}"),
        }
    }
}

impl std::error::Error for AutostartError {}

/// Parse the executable path from a startup command string (handles quotes and arguments).
pub fn parse_command_executable(cmd: &str) -> Option<PathBuf> {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix('"') {
        let end = rest.find('"')?;
        Some(PathBuf::from(&rest[..end]))
    } else {
        let end = trimmed.find(' ').unwrap_or(trimmed.len());
        Some(PathBuf::from(&trimmed[..end]))
    }
}

/// Returns true if autostart is currently enabled.
///
/// On Windows the autostart entry lives in the registry, not on the
/// filesystem, so we read the registry value directly.  On macOS and Linux
/// the entry is a real file (plist / .desktop) so the path-exists check is
/// correct.
#[cfg(target_os = "windows")]
pub fn is_autostart_enabled() -> bool {
    if is_autostart_disabled() {
        return false;
    }
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let Ok(key) = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run") else {
        return false;
    };
    match key.get_value::<String, _>("FutureAcademyLink") {
        Ok(val) => {
            if let Some(path) = parse_command_executable(&val) {
                path.exists()
            } else {
                !val.trim().is_empty()
            }
        }
        Err(_) => false,
    }
}

#[cfg(not(target_os = "windows"))]
pub fn is_autostart_enabled() -> bool {
    if is_autostart_disabled() {
        return false;
    }
    match get_autostart_path() {
        Some(path) => path.exists(),
        None => false,
    }
}

/// Returns the path where autostart configuration lives (platform-specific).
#[cfg(not(target_os = "windows"))]
fn get_autostart_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|h| {
            h.join("Library")
                .join("LaunchAgents")
                .join("edu.futureacademy.FutureAcademyLink.plist")
        })
    }
    #[cfg(target_os = "linux")]
    {
        dirs::config_dir().map(|c| c.join("autostart").join("futureacademy-link.desktop"))
    }
}

/// Returns the app's executable path.
fn get_exe_path() -> Option<PathBuf> {
    std::env::current_exe().ok()
}

// ---------------------------------------------------------------------------
// Platform-specific implementation
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use winreg::enums::*;
    use winreg::RegKey;

    const APP_NAME: &str = "FutureAcademyLink";

    pub fn enable_autostart_impl() -> Result<(), super::AutostartError> {
        let exe = get_exe_path().ok_or_else(|| {
            AutostartError::Io("Could not determine executable path".to_string())
        })?;

        let expected_cmd = format!("\"{}\" --silent", exe.to_string_lossy());

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (run_key, _) = hkcu
            .create_subkey_with_flags(
                r"Software\Microsoft\Windows\CurrentVersion\Run",
                KEY_READ | KEY_WRITE,
            )
            .map_err(|e| AutostartError::Registry(e.to_string()))?;

        // If current value already matches expected command, nothing to do
        if let Ok(current_val) = run_key.get_value::<String, _>(APP_NAME) {
            if current_val == expected_cmd {
                return Ok(());
            }
        }

        run_key
            .set_value(APP_NAME, &expected_cmd)
            .map_err(|e| AutostartError::Registry(e.to_string()))?;

        Ok(())
    }

    pub fn disable_autostart_impl() -> Result<(), super::AutostartError> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let run_key = hkcu
            .open_subkey_with_flags(
                r"Software\Microsoft\Windows\CurrentVersion\Run",
                KEY_WRITE,
            )
            .map_err(|e| AutostartError::Registry(e.to_string()))?;

        // Ignore error if key doesn't exist
        let _ = run_key.delete_value(APP_NAME);

        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;

    pub fn enable_autostart_impl() -> Result<(), AutostartError> {
        let exe = get_exe_path().ok_or_else(|| {
            AutostartError::Io("Could not determine executable path".to_string())
        })?;

        let plist_path = dirs::home_dir()
            .ok_or_else(|| AutostartError::Io("Could not determine home directory".to_string()))?
            .join("Library")
            .join("LaunchAgents")
            .join("edu.futureacademy.FutureAcademyLink.plist");

        // Create LaunchAgents directory if needed
        if let Some(parent) = plist_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AutostartError::Io(e.to_string()))?;
        }

        let plist_content = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>edu.futureacademy.FutureAcademyLink</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>--silent</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>LaunchOnlyOnce</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
</dict>
</plist>
"#,
            exe.to_string_lossy()
        );

        if let Ok(existing) = std::fs::read_to_string(&plist_path) {
            if existing == plist_content {
                return Ok(());
            }
        }

        std::fs::write(&plist_path, plist_content)
            .map_err(|e| AutostartError::Io(e.to_string()))?;

        Ok(())
    }

    pub fn disable_autostart_impl() -> Result<(), AutostartError> {
        let plist_path = dirs::home_dir()
            .ok_or_else(|| AutostartError::Io("Could not determine home directory".to_string()))?
            .join("Library")
            .join("LaunchAgents")
            .join("edu.futureacademy.FutureAcademyLink.plist");

        // Ignore error if file doesn't exist
        let _ = std::fs::remove_file(&plist_path);

        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;

    pub fn enable_autostart_impl() -> Result<(), AutostartError> {
        let exe = get_exe_path().ok_or_else(|| {
            AutostartError::Io("Could not determine executable path".to_string())
        })?;

        let desktop_path = dirs::config_dir()
            .ok_or_else(|| AutostartError::Io("Could not determine config directory".to_string()))?
            .join("autostart")
            .join("futureacademy-link.desktop");

        // Create autostart directory if needed
        if let Some(parent) = desktop_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AutostartError::Io(e.to_string()))?;
        }

        let desktop_content = format!(
            r#"[Desktop Entry]
Type=Application
Name=Future Academy Link
Exec="{}" --silent
Icon=futureacademy-link
Terminal=false
Categories=Development;Education;
"#,
            exe.to_string_lossy()
        );

        if let Ok(existing) = std::fs::read_to_string(&desktop_path) {
            if existing == desktop_content {
                return Ok(());
            }
        }

        std::fs::write(&desktop_path, desktop_content)
            .map_err(|e| AutostartError::Io(e.to_string()))?;

        Ok(())
    }

    pub fn disable_autostart_impl() -> Result<(), AutostartError> {
        let desktop_path = dirs::config_dir()
            .ok_or_else(|| AutostartError::Io("Could not determine config directory".to_string()))?
            .join("autostart")
            .join("futureacademy-link.desktop");

        // Ignore error if file doesn't exist
        let _ = std::fs::remove_file(&desktop_path);

        Ok(())
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
mod platform {
    use super::*;

    pub fn enable_autostart_impl() -> Result<(), AutostartError> {
        Err(AutostartError::Io(
            "Autostart not supported on this platform".to_string(),
        ))
    }

    pub fn disable_autostart_impl() -> Result<(), AutostartError> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Enable auto-start at system login.
pub fn enable_autostart() -> Result<(), AutostartError> {
    set_autostart_disabled(false);
    platform::enable_autostart_impl()
}

/// Disable auto-start at system login.
pub fn disable_autostart() -> Result<(), AutostartError> {
    set_autostart_disabled(true);
    platform::disable_autostart_impl()
}

/// Initialize autostart: ensure autostart is enabled by default.
/// If the user explicitly disabled it, keep it disabled.
/// If autostart should be active, verify and register/update the current binary path.
pub fn init_autostart() {
    mark_launched();

    if is_autostart_disabled() {
        tracing::debug!("[autostart] autostart is explicitly disabled by user preference");
        let _ = platform::disable_autostart_impl();
        return;
    }

    tracing::info!("[autostart] ensuring autostart registration (default: enabled)");
    if let Err(e) = platform::enable_autostart_impl() {
        tracing::warn!("[autostart] failed to register autostart: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_autostart_enabled_does_not_panic() {
        // Must return a boolean without panicking on any platform
        let _enabled = is_autostart_enabled();
    }

    #[test]
    fn parse_command_executable_handles_formats() {
        let quoted = r#""C:\Program Files\Future Academy\FutureAcademy.exe" --silent"#;
        assert_eq!(
            parse_command_executable(quoted),
            Some(PathBuf::from(r"C:\Program Files\Future Academy\FutureAcademy.exe"))
        );

        let unquoted_with_arg = r#"C:\FutureAcademy\FutureAcademy.exe --silent"#;
        assert_eq!(
            parse_command_executable(unquoted_with_arg),
            Some(PathBuf::from(r"C:\FutureAcademy\FutureAcademy.exe"))
        );

        let simple = r#"FutureAcademy.exe"#;
        assert_eq!(
            parse_command_executable(simple),
            Some(PathBuf::from("FutureAcademy.exe"))
        );

        assert_eq!(parse_command_executable(""), None);
        assert_eq!(parse_command_executable("   "), None);
    }

    #[test]
    fn disabled_marker_lifecycle() {
        let initial = is_autostart_disabled();
        set_autostart_disabled(true);
        assert!(is_autostart_disabled());
        set_autostart_disabled(false);
        assert!(!is_autostart_disabled());
        if initial {
            set_autostart_disabled(true);
        }
    }
}

