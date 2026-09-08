//! Windows toast notifications + indicatif progress wrappers.
//!
//! Two responsibilities live here:
//!
//! 1. Native Windows 10/11 toast notifications (via `winrt-notification`) shown
//!    for upload (start / success / failure / abort), download (toolchain & app updates),
//!    and update availability with click handling.
//! 2. A `Notification` struct that owns an `indicatif` bar/spinner (from
//!    `progress`) so download / update / upload progress can be reported on
//!    stderr when run in a console, while simultaneously offering native desktop
//!    toasts for GUI/tray mode where stderr is detached.

use crate::progress::{DownloadBar, Spinner};

#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(windows)]
use winrt_notification::{Sound, Toast};

#[cfg(windows)]
static NOTIFICATION_CLICKED: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
const APP_ID: &str = "Future Academy Link";

// ── Native Toast notifications ─────────────────────────────────────────────

/// Show a native Windows toast notification with title, up to two text lines,
/// and optional sound. Run asynchronously on a background thread so WinRT COM
/// calls do not block the tokio runtime or event loop.
#[cfg(windows)]
pub fn show_toast(title: &str, text1: &str, text2: &str, sound: Option<Sound>) -> Result<(), String> {
    let title_owned = title.to_string();
    let text1_owned = text1.to_string();
    let text2_owned = text2.to_string();

    std::thread::spawn(move || {
        let mut toast = Toast::new(Toast::POWERSHELL_APP_ID);
        toast = toast.title(&title_owned);
        if !text1_owned.is_empty() {
            toast = toast.text1(&text1_owned);
        }
        if !text2_owned.is_empty() {
            toast = toast.text2(&text2_owned);
        }
        toast = toast.sound(sound);

        if let Err(e) = toast.show() {
            tracing::warn!("[notification] failed to show toast '{}': {}", title_owned, e);
        }
    });

    Ok(())
}

#[cfg(not(windows))]
pub fn show_toast(_title: &str, _text1: &str, _text2: &str, _sound: Option<()>) -> Result<(), String> {
    Ok(())
}

// ── Upload notifications ───────────────────────────────────────────────────

/// Notify that an upload (compilation / flash) to the specified target has started.
/// Displays a silent toast so the user is informed without an intrusive chime.
#[cfg(windows)]
pub fn notify_upload_start(target: &str, details: &str) {
    let target_label = if target.is_empty() { "thiết bị" } else { target };
    let _ = show_toast(
        APP_ID,
        &format!("Đang nạp cho {}...", target_label),
        details,
        None,
    );
}

#[cfg(not(windows))]
pub fn notify_upload_start(_target: &str, _details: &str) {}

/// Notify that an upload completed successfully.
/// Plays the standard notification chime.
#[cfg(windows)]
pub fn notify_upload_success(target: &str) {
    let target_label = if target.is_empty() { "thiết bị" } else { target };
    let _ = show_toast(
        APP_ID,
        "Nạp thành công \u{2713}",
        &format!("Đã nạp phần mềm thành công vào {}", target_label),
        Some(Sound::Default),
    );
}

#[cfg(not(windows))]
pub fn notify_upload_success(_target: &str) {}

/// Notify that an upload failed.
/// Plays the warning chime and shows a summarized error message.
#[cfg(windows)]
pub fn notify_upload_error(target: &str, error: &str) {
    let target_label = if target.is_empty() { "thiết bị" } else { target };
    let clean_err = error.lines().next().unwrap_or(error).trim();
    let display_err = if clean_err.len() > 120 {
        format!("{}...", &clean_err[..117])
    } else {
        clean_err.to_string()
    };
    let _ = show_toast(
        APP_ID,
        "Nạp thất bại \u{2717}",
        &format!("{}: {}", target_label, display_err),
        Some(Sound::Default),
    );
}

#[cfg(not(windows))]
pub fn notify_upload_error(_target: &str, _error: &str) {}

/// Notify that an upload was aborted by user request.
#[cfg(windows)]
pub fn notify_upload_aborted(target: &str) {
    let target_label = if target.is_empty() { "thiết bị" } else { target };
    let _ = show_toast(
        APP_ID,
        "Đã hủy nạp",
        &format!("Quá trình nạp cho {} đã bị hủy.", target_label),
        None,
    );
}

#[cfg(not(windows))]
pub fn notify_upload_aborted(_target: &str) {}

// ── Download notifications ─────────────────────────────────────────────────

/// Notify that a download operation has started (silent banner).
#[cfg(windows)]
pub fn notify_download_start(item: &str) {
    let _ = show_toast(
        APP_ID,
        &format!("Đang tải {}...", item),
        "Vui lòng chờ trong khi bộ công cụ đang được tải xuống.",
        None,
    );
}

#[cfg(not(windows))]
pub fn notify_download_start(_item: &str) {}

/// Notify that a download and setup operation finished successfully.
#[cfg(windows)]
pub fn notify_download_success(item: &str) {
    let _ = show_toast(
        APP_ID,
        &format!("{} đã sẵn sàng \u{2713}", item),
        "Tải xuống và cài đặt bộ công cụ thành công.",
        Some(Sound::Default),
    );
}

#[cfg(not(windows))]
pub fn notify_download_success(_item: &str) {}

/// Notify that a download failed.
#[cfg(windows)]
pub fn notify_download_error(item: &str, error: &str) {
    let clean_err = error.lines().next().unwrap_or(error).trim();
    let display_err = if clean_err.len() > 120 {
        format!("{}...", &clean_err[..117])
    } else {
        clean_err.to_string()
    };
    let _ = show_toast(
        APP_ID,
        &format!("Tải {} thất bại \u{2717}", item),
        &display_err,
        Some(Sound::Default),
    );
}

#[cfg(not(windows))]
pub fn notify_download_error(_item: &str, _error: &str) {}

// ── App Update Notification ────────────────────────────────────────────────

/// Notify that a new version of the app is available.
#[cfg(windows)]
pub fn notify_update_available(version: &str) {
    let _ = show_toast(
        APP_ID,
        &format!("Có bản cập nhật mới: {}", version),
        "Nhấn vào menu khay hệ thống để tải và cài đặt.",
        Some(Sound::Default),
    );
}

#[cfg(not(windows))]
pub fn notify_update_available(_version: &str) {}

/// Show a toast notification for an available update.
/// When the user clicks the notification, `on_click` will be invoked.
#[allow(dead_code)]
#[cfg(windows)]
pub fn show_update_notification_with_callback<F>(version: &str, on_click: F) -> Result<(), String>
where
    F: Fn() + Send + 'static,
{
    NOTIFICATION_CLICKED.store(false, Ordering::SeqCst);

    let version_owned = version.to_string();
    let on_click = std::sync::Mutex::new(Some(on_click));

    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));

        let _ = show_toast(
            APP_ID,
            &format!("Có bản cập nhật mới: {}", version_owned),
            "Nhấn vào menu khay hệ thống để tải và cài đặt.",
            Some(Sound::Default),
        );

        std::thread::sleep(std::time::Duration::from_secs(3));

        if NOTIFICATION_CLICKED.load(Ordering::SeqCst) {
            if let Some(callback) = on_click.lock().unwrap().take() {
                callback();
            }
        }
    });

    Ok(())
}

#[cfg(not(windows))]
pub fn show_update_notification_with_callback<F>(_: &str, _: F) -> Result<(), String>
where
    F: Fn() + Send + 'static,
{
    Ok(())
}

/// Check if a notification was clicked and invoke the callback if so.
#[allow(dead_code)]
#[cfg(windows)]
pub fn check_and_process_notification_click<F>(on_click: F)
where
    F: Fn() + Send + 'static,
{
    if NOTIFICATION_CLICKED.swap(false, Ordering::SeqCst) {
        on_click();
    }
}

#[allow(dead_code)]
#[cfg(not(windows))]
pub fn check_and_process_notification_click<F>(_: F)
where
    F: Fn() + Send + 'static,
{
}

// ── Test notifications ─────────────────────────────────────────────────────

/// Show a simple test notification (for debugging via `--test-notification`).
#[cfg(windows)]
pub fn show_test_notification() -> Result<(), String> {
    Toast::new(Toast::POWERSHELL_APP_ID)
        .title(APP_ID)
        .text1("Thông báo thử nghiệm")
        .text2("Đây là thông báo thử nghiệm từ Future Academy Link.")
        .sound(Some(Sound::Default))
        .show()
        .map_err(|e| format!("Failed to show notification: {}", e))
}

#[cfg(not(windows))]
pub fn show_test_notification() -> Result<(), String> {
    tracing::info!("[notification] test notification invoked (non-Windows)");
    Ok(())
}

/// Show test upload start and success notifications.
pub fn show_test_upload_notification() -> Result<(), String> {
    notify_upload_start("ESP32-S3 (COM3)", "Đang biên dịch mã và nạp firmware...");
    std::thread::sleep(std::time::Duration::from_secs(2));
    notify_upload_success("ESP32-S3 (COM3)");
    Ok(())
}

/// Show test download start and success notifications.
pub fn show_test_download_notification() -> Result<(), String> {
    notify_download_start("Bộ công cụ Arduino");
    std::thread::sleep(std::time::Duration::from_secs(2));
    notify_download_success("Bộ công cụ Arduino");
    Ok(())
}

/// Show test update available notification.
pub fn show_test_update_notification() -> Result<(), String> {
    notify_update_available("v2.1.21");
    Ok(())
}

// ── Progress notifications (indicatif wrapper + desktop toast hook) ────────

enum ProgressState {
    Spinner(Spinner),
    DownloadBar(DownloadBar),
}

/// Desktop notification mode for a `Notification` instance.
#[derive(Debug, Clone)]
pub enum DesktopToastMode {
    None,
    Upload { target: String },
    Download { item: String },
}

/// A unified progress notification that wraps `indicatif` bars/spinners from
/// [`crate::progress`] and provides a single API for download, update, and
/// upload progress. Optionally delivers desktop toasts upon start and finish.
pub struct Notification {
    label: String,
    state: ProgressState,
    toast_mode: DesktopToastMode,
}

impl Notification {
    /// Construct a notification backed by an indeterminate spinner.
    pub fn spinner(label: &str) -> Self {
        Self {
            label: label.to_string(),
            state: ProgressState::Spinner(Spinner::new(label)),
            toast_mode: DesktopToastMode::None,
        }
    }

    /// Construct a notification backed by a bytes-progress bar.
    pub fn download_progress(label: &str, total_bytes: u64) -> Self {
        Self {
            label: label.to_string(),
            state: ProgressState::DownloadBar(DownloadBar::new(label, total_bytes)),
            toast_mode: DesktopToastMode::None,
        }
    }

    /// Alias for [`download_progress`].
    #[allow(dead_code)]
    pub fn upload_progress(label: &str, total_bytes: u64) -> Self {
        Self::download_progress(label, total_bytes)
    }

    /// Attach a desktop toast mode to this notification so start / finish
    /// automatically fire native Windows toasts.
    pub fn with_desktop_toast(mut self, mode: DesktopToastMode) -> Self {
        match &mode {
            DesktopToastMode::Upload { target } => {
                notify_upload_start(target, &self.label);
            }
            DesktopToastMode::Download { item } => {
                notify_download_start(item);
            }
            DesktopToastMode::None => {}
        }
        self.toast_mode = mode;
        self
    }

    /// Convenience factory for upload operations with desktop toast integration.
    #[allow(dead_code)]
    pub fn upload_task(target: &str, initial_label: &str) -> Self {
        Self::spinner(initial_label).with_desktop_toast(DesktopToastMode::Upload {
            target: target.to_string(),
        })
    }

    /// Convenience factory for download operations with desktop toast integration.
    #[allow(dead_code)]
    pub fn download_task(item: &str, total_bytes: Option<u64>) -> Self {
        let base = if let Some(bytes) = total_bytes {
            Self::download_progress(&format!("Đang tải {}", item), bytes)
        } else {
            Self::spinner(&format!("Đang tải {}", item))
        };
        base.with_desktop_toast(DesktopToastMode::Download {
            item: item.to_string(),
        })
    }

    /// Update the indicator's display message.
    pub fn set_message(&self, msg: &str) {
        match &self.state {
            ProgressState::Spinner(s) => s.set_message(msg),
            ProgressState::DownloadBar(b) => b.set_message(msg),
        }
    }

    /// Advance the indicator to `current` of `total`.
    pub fn set_progress(&self, current: u64, total: u64) {
        if total == 0 {
            return;
        }
        match &self.state {
            ProgressState::DownloadBar(b) => {
                b.set_position(current);
            }
            ProgressState::Spinner(s) => {
                let pct = (current as f64 / total as f64) * 100.0;
                s.set_message(&format!("{} ({:.0}%)", self.label, pct));
            }
        }
    }

    /// Convenience helper for percentage-based callers (0.0–1.0).
    pub fn set_fraction(&self, fraction: f64) {
        match &self.state {
            ProgressState::Spinner(s) => {
                let pct = (fraction * 100.0).clamp(0.0, 100.0);
                s.set_message(&format!("{} ({:.0}%)", self.label, pct));
            }
            ProgressState::DownloadBar(b) => {
                let total = b.len().unwrap_or(0);
                if total > 0 {
                    b.set_position((total as f64 * fraction).round() as u64);
                }
            }
        }
    }

    /// Mark the operation as successfully finished.
    pub fn finish_ok(&self, msg: &str) {
        match &self.state {
            ProgressState::Spinner(s) => s.finish_ok(msg),
            ProgressState::DownloadBar(b) => b.finish(msg),
        }
        match &self.toast_mode {
            DesktopToastMode::Upload { target } => notify_upload_success(target),
            DesktopToastMode::Download { item } => notify_download_success(item),
            DesktopToastMode::None => {}
        }
    }

    /// Mark the operation as finished with a warning / abort.
    pub fn finish_warn(&self, msg: &str) {
        match &self.state {
            ProgressState::Spinner(s) => s.finish_warn(msg),
            ProgressState::DownloadBar(b) => {
                b.finish(&format!("⚠ {}", msg));
            }
        }
        match &self.toast_mode {
            DesktopToastMode::Upload { target } => notify_upload_aborted(target),
            DesktopToastMode::Download { item } => notify_download_error(item, msg),
            DesktopToastMode::None => {}
        }
    }

    /// Mark the operation as failed.
    pub fn finish_err(&self, msg: &str) {
        match &self.state {
            ProgressState::Spinner(s) => s.finish_err(msg),
            ProgressState::DownloadBar(b) => b.abandon(msg),
        }
        match &self.toast_mode {
            DesktopToastMode::Upload { target } => notify_upload_error(target, msg),
            DesktopToastMode::Download { item } => notify_download_error(item, msg),
            DesktopToastMode::None => {}
        }
    }

    #[allow(dead_code)]
    pub fn download_bar(&self) -> Option<&DownloadBar> {
        match &self.state {
            ProgressState::DownloadBar(b) => Some(b),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn spinner_ref(&self) -> Option<&Spinner> {
        match &self.state {
            ProgressState::Spinner(s) => Some(s),
            _ => None,
        }
    }
}

impl Drop for Notification {
    fn drop(&mut self) {
        if let ProgressState::DownloadBar(b) = &self.state {
            b.finish_if_not_finished();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spinner_lifecycle() {
        let notif = Notification::spinner("Test Spinner");
        notif.set_message("Working...");
        notif.set_fraction(0.5);
        notif.finish_ok("Done");
    }

    #[test]
    fn download_progress_lifecycle() {
        let notif = Notification::download_progress("Test Download", 1000);
        notif.set_progress(500, 1000);
        notif.finish_ok("Done");
    }

    #[test]
    fn upload_task_lifecycle() {
        let notif = Notification::upload_task("ESP32", "Uploading");
        notif.set_fraction(0.75);
        notif.finish_ok("Uploaded successfully");
    }

    #[test]
    fn download_task_lifecycle() {
        let notif = Notification::download_task("Tools", Some(5000));
        notif.set_progress(2500, 5000);
        notif.finish_err("Network error");
    }
}

