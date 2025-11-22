use crate::platform::{NotificationDuration, send_notification};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub fn get_executable_directory() -> PathBuf {
    std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

pub fn get_executable_path() -> PathBuf {
    std::env::current_exe().unwrap()
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
