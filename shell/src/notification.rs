//! Windows toast notifications for update alerts.
//!
//! Uses the `winrt-notification` crate to show native Windows 10/11
//! toast notifications when an update is available.

#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(windows)]
use winrt_notification::{Toast, Sound};

#[cfg(windows)]
static NOTIFICATION_CLICKED: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
const APP_ID: &str = "Future Academy Link";

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
