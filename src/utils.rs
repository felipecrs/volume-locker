use crate::platform::{NotificationDuration, send_notification};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub fn get_executable_path() -> PathBuf {
    let exe_path = std::env::current_exe().expect("failed to determine current executable path");
    // Resolves symbolic links (e.g., when installed via winget)
    std::fs::canonicalize(&exe_path).unwrap_or(exe_path)
}

pub fn get_executable_directory() -> PathBuf {
    get_executable_path()
        .parent()
        .expect("executable path has no parent directory")
        .to_path_buf()
}

pub fn get_executable_path_str() -> String {
    get_executable_path()
        .to_str()
        .expect("executable path is not valid UTF-8")
        .to_string()
}

pub fn log_and_notify_error(title: &str, message: &str) {
    log::error!("{message}");
    let _ = send_notification(title, message, NotificationDuration::Long);
}

pub fn send_notification_debounced(
    key: &str,
    title: &str,
    message: &str,
    last_notification_times: &mut HashMap<String, Instant>,
) {
    let now = Instant::now();
    let should_notify = match last_notification_times.get(key) {
        Some(&last_time) => now.duration_since(last_time) > Duration::from_secs(5),
        None => true,
    };
    if should_notify {
        if let Err(e) = send_notification(title, message, NotificationDuration::Short) {
            log::error!("Failed to show notification for {title}: {e}");
        }
        last_notification_times.insert(key.to_string(), now);
    }
}

pub fn convert_float_to_percent(volume: f32) -> f32 {
    (volume * 100.0).round()
}

pub fn convert_percent_to_float(volume: f32) -> f32 {
    volume / 100.0
}

/// Open a path in the system file explorer.
pub fn open_path(path: &std::path::Path) {
    let _ = open::that_detached(path);
}

/// Open a URL in the default browser.
pub fn open_url(url: &str) {
    let _ = open::that_detached(url);
}
