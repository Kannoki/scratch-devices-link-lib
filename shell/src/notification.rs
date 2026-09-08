//! Windows toast notifications + indicatif progress wrappers.
//!
//! Two responsibilities live here:
//!
//! 1. Native Windows 10/11 toast notifications (via `winrt-notification`) shown
//!    when an update is available, plus a click-detection callback mechanism.
//! 2. A `Notification` struct that owns an `indicatif` bar/spinner (from
//!    `progress`) so download / update / upload progress can be reported on
//!    stderr with a single consistent API. When stderr is not a TTY
//!    (`--headless`, tray mode, service), the bars are silently replaced by
//!    `tracing` log lines so logs stay readable.

use crate::progress::{DownloadBar, Spinner};

#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(windows)]
use winrt_notification::{Toast, Sound};

#[cfg(windows)]
static NOTIFICATION_CLICKED: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
const APP_ID: &str = "Future Academy Link";

// ── Toast notifications ────────────────────────────────────────────────────

/// Show a toast notification for an available update.
/// When the user clicks the notification, `on_click` will be invoked.
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

        let result = Toast::new(Toast::POWERSHELL_APP_ID)
            .title(APP_ID)
            .text1(&format!("Version {} is available", version_owned))
            .text2("Click to download and install the update.")
            .sound(Some(Sound::Default))
            .show();

        if let Err(e) = result {
            tracing::warn!("[notification] failed to show toast: {}", e);
        }

        std::thread::sleep(std::time::Duration::from_secs(3));

        if NOTIFICATION_CLICKED.load(Ordering::SeqCst) {
            if let Some(callback) = on_click.lock().unwrap().take() {
                callback();
            }
        }
    });

    Ok(())
}

/// Check if a notification was clicked and invoke the callback if so.
/// Call this periodically or when ready to process the update.
#[cfg(windows)]
pub fn check_and_process_notification_click<F>(on_click: F)
where
    F: Fn() + Send + 'static,
{
    if NOTIFICATION_CLICKED.swap(false, Ordering::SeqCst) {
        on_click();
    }
}

/// Show a simple test notification (for debugging).
#[cfg(windows)]
pub fn show_test_notification() -> Result<(), String> {
    Toast::new(Toast::POWERSHELL_APP_ID)
        .title(APP_ID)
        .text1("Test Notification")
        .text2("This is a test update notification.")
        .sound(Some(Sound::Default))
        .show()
        .map_err(|e| format!("Failed to show notification: {}", e))
}

/// Non-Windows stubs
#[cfg(not(windows))]
pub fn show_update_notification_with_callback<F>(_: &str, _: F) -> Result<(), String>
where
    F: Fn() + Send + 'static,
{
    Ok(())
}

#[cfg(not(windows))]
pub fn check_and_process_notification_click<F>(_: F)
where
    F: Fn() + Send + 'static,
{
}

#[cfg(not(windows))]
pub fn show_test_notification() -> Result<(), String> {
    Ok(())
}

// ── Progress notifications (indicatif wrapper) ─────────────────────────────

/// Internal state for the progress notification. Holds either a spinner
/// (indeterminate) or a `DownloadBar` (bytes progress), or nothing for
/// fire-and-forget messages.
enum ProgressState {
    /// No persistent indicator — used for `send_message` only.
    None,
    /// Indeterminate spinner.
    Spinner(Spinner),
    /// Bytes-progress bar (download / update / upload with known total).
    DownloadBar(DownloadBar),
}

/// A unified progress notification that wraps `indicatif` bars/spinners from
/// [`crate::progress`] and provides a single API for download, update, and
/// upload progress.
///
/// Construct with one of the factory methods and call [`set_progress`] /
/// [`finish_ok`] / [`finish_err`] as work proceeds. The underlying bar is
/// automatically finished when the value is dropped, so callers can rely on
/// RAII to clean up the indicator on early-return or panic.
pub struct Notification {
    label: String,
    state: ProgressState,
}

impl Notification {
    /// Construct a notification backed by an indeterminate spinner.
    /// Use this for upload phases where progress is reported as a percentage
    /// without a known byte total (compile, erase, verify, …).
    pub fn spinner(label: &str) -> Self {
        Self {
            label: label.to_string(),
            state: ProgressState::Spinner(Spinner::new(label)),
        }
    }

    /// Construct a notification backed by a bytes-progress bar. Use this for
    /// downloads (toolchain setup, OTA update) and uploads with a known total
    /// payload size.
    pub fn download_progress(label: &str, total_bytes: u64) -> Self {
        Self {
            label: label.to_string(),
            state: ProgressState::DownloadBar(DownloadBar::new(label, total_bytes)),
        }
    }

    /// Alias for [`download_progress`] — semantic sugar for upload callers.
    pub fn upload_progress(label: &str, total_bytes: u64) -> Self {
        Self::download_progress(label, total_bytes)
    }

    /// Update the indicator's display message. Safe to call on any state.
    pub fn set_message(&self, msg: &str) {
        match &self.state {
            ProgressState::Spinner(s) => s.set_message(msg),
            ProgressState::DownloadBar(b) => b.set_message(msg),
            ProgressState::None => {
                tracing::info!("[{}] {}", self.label, msg);
            }
        }
    }

    /// Advance the indicator to `current` of `total`. If `total` is zero the
    /// call is a no-op (we cannot compute a fraction).
    pub fn set_progress(&self, current: u64, total: u64) {
        if total == 0 {
            return;
        }
        match &self.state {
            ProgressState::DownloadBar(b) => {
                // indicatif's bar already represents `total`; setting position
                // to `current` is the most accurate representation of bytes
                // received (it tolerates out-of-range values).
                b.set_position(current);
            }
            ProgressState::Spinner(s) => {
                let pct = (current as f64 / total as f64) * 100.0;
                s.set_message(&format!("{} ({:.0}%)", self.label, pct));
            }
            ProgressState::None => {
                let pct = (current as f64 / total as f64) * 100.0;
                tracing::info!(
                    "[{}] {}/{} ({:.0}%)",
                    self.label,
                    current,
                    total,
                    pct
                );
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
            ProgressState::None => {
                let pct = (fraction * 100.0).clamp(0.0, 100.0);
                tracing::info!("[{}] {:.0}%", self.label, pct);
            }
        }
    }

    /// Mark the operation as successfully finished and clear the indicator.
    pub fn finish_ok(&self, msg: &str) {
        match &self.state {
            ProgressState::Spinner(s) => s.finish_ok(msg),
            ProgressState::DownloadBar(b) => b.finish(msg),
            ProgressState::None => tracing::info!("[{}] ✓ {}", self.label, msg),
        }
    }

    /// Mark the operation as finished with a warning and clear the indicator.
    pub fn finish_warn(&self, msg: &str) {
        match &self.state {
            ProgressState::Spinner(s) => s.finish_warn(msg),
            ProgressState::DownloadBar(b) => {
                b.finish(&format!("⚠ {}", msg));
            }
            ProgressState::None => tracing::warn!("[{}] ⚠ {}", self.label, msg),
        }
    }

    /// Mark the operation as failed and leave the bar visible with an error
    /// style.
    pub fn finish_err(&self, msg: &str) {
        match &self.state {
            ProgressState::Spinner(s) => s.finish_err(msg),
            ProgressState::DownloadBar(b) => b.abandon(msg),
            ProgressState::None => tracing::error!("[{}] ✗ {}", self.label, msg),
        }
    }

    /// Borrow the underlying `DownloadBar` so callers can drive it directly
    /// (e.g. `inc(n)` per chunk). Returns `None` if this notification is not
    /// backed by a `DownloadBar`.
    pub fn download_bar(&self) -> Option<&DownloadBar> {
        match &self.state {
            ProgressState::DownloadBar(b) => Some(b),
            _ => None,
        }
    }

    /// Borrow the underlying `Spinner` so callers can call `set_message`
    /// directly. Returns `None` if this notification is not backed by a
    /// `Spinner`.
    pub fn spinner_ref(&self) -> Option<&Spinner> {
        match &self.state {
            ProgressState::Spinner(s) => Some(s),
            _ => None,
        }
    }
}

impl Drop for Notification {
    fn drop(&mut self) {
        // Only finish the bar if it hasn't been closed yet. The wrapped types
        // handle this themselves, but DownloadBar has no is_finished guard so
        // we need to avoid re-calling finish after finish_ok/finish_err/finish_warn
        // already closed it.
        if let ProgressState::DownloadBar(b) = &self.state {
            b.finish_if_not_finished();
        }
    }
}
