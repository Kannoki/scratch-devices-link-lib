//! Auto-start at system login feature.
//!
//! Auto-start is enabled by default (on first run, the app automatically registers
//! itself to run at system startup). A menu toggle allows users to change this setting.
//!
//! Platform implementations:
//! - Windows: HKCU\Software\Microsoft\Windows\CurrentVersion\Run registry
//! - macOS: ~/Library/LaunchAgents/ plist
//! - Linux: ~/.config/autostart/ .desktop file

use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Sentinel file
// ---------------------------------------------------------------------------

/// Path to the sentinel file that marks this app as having been launched before.
fn sentinel_path() -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("FutureAcademy").join(".launched")
}

// ---------------------------------------------------------------------------
// First-run detection
// ---------------------------------------------------------------------------

/// Returns true if this is the first time the app has ever been launched.
pub fn is_first_run() -> bool {
    !sentinel_path().exists()
}

/// Mark that the app has been launched at least once.
fn mark_launched() {
    if let Some(parent) = sentinel_path().parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&sentinel_path(), "");
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

/// Returns true if autostart is currently enabled.
pub fn is_autostart_enabled() -> bool {
    match get_autostart_path() {
        Some(path) => path.exists(),
        None => false,
    }
}

/// Returns the path where autostart configuration lives (platform-specific).
fn get_autostart_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        Some(PathBuf::from(r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run"))
    }
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

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let run_key = hkcu
            .open_subkey_with_flags(
                r"Software\Microsoft\Windows\CurrentVersion\Run",
                KEY_WRITE,
            )
            .map_err(|e| AutostartError::Registry(e.to_string()))?;

        run_key
            .set_value(APP_NAME, &exe.to_string_lossy().to_string())
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
Exec={}
Icon=futureacademy-link
Terminal=false
Categories=Development;Education;
"#,
            exe.to_string_lossy()
        );

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
    platform::enable_autostart_impl()
}

/// Disable auto-start at system login.
pub fn disable_autostart() -> Result<(), AutostartError> {
    platform::disable_autostart_impl()
}

/// Initialize autostart: enable it by default on first run.
pub fn init_autostart() {
    if is_first_run() {
        tracing::info!("[autostart] first run - enabling auto-start");
        if let Err(e) = enable_autostart() {
            tracing::warn!("[autostart] failed to enable autostart: {}", e);
        }
        mark_launched();
    } else {
        tracing::debug!("[autostart] not first run, skipping auto-enable");
    }
}
