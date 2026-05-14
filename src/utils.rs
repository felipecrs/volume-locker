use crate::platform::{NotificationDuration, send_notification};
use anyhow::Context;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub fn get_executable_path() -> anyhow::Result<PathBuf> {
    let exe_path =
        std::env::current_exe().context("failed to determine current executable path")?;
    // Resolves symbolic links (e.g., when installed via winget)
    Ok(dunce::canonicalize(&exe_path).unwrap_or(exe_path))
}

pub fn get_executable_directory() -> anyhow::Result<PathBuf> {
    Ok(get_executable_path()?
        .parent()
        .context("executable path has no parent directory")?
        .to_path_buf())
}

pub fn get_executable_path_str() -> anyhow::Result<String> {
    Ok(get_executable_path()?
        .to_str()
        .context("executable path is not valid UTF-8")?
        .to_string())
}

pub fn log_and_notify_error(title: &str, message: &str) {
    log::error!("{message}");
    if let Err(e) = send_notification(title, message, NotificationDuration::Long) {
        log::error!("Failed to send error notification: {e:#}");
    }
}

/// Manages debounced notifications, preventing repeated notifications within a cooldown period.
pub struct NotificationThrottler {
    last_times: HashMap<String, Instant>,
}

impl NotificationThrottler {
    pub fn new() -> Self {
        Self {
            last_times: HashMap::new(),
        }
    }

    pub fn send_if_not_throttled(&mut self, key: &str, title: &str, message: &str) {
        let now = Instant::now();
        let should_notify = match self.last_times.get(key) {
            Some(&last_time) => now.duration_since(last_time) > Duration::from_secs(5),
            None => true,
        };
        if should_notify {
            if let Err(e) = send_notification(title, message, NotificationDuration::Short) {
                log::error!("Failed to show notification for {title}: {e:#}");
            }
            self.last_times.insert(key.to_string(), now);
        }
    }
}

pub fn open_path(path: &std::path::Path) -> anyhow::Result<()> {
    open::that_detached(path).context("failed to open path")
}

pub fn open_url(url: &str) -> anyhow::Result<()> {
    open::that_detached(url).context("failed to open URL")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn throttler_records_key_on_first_send() {
        let mut throttler = NotificationThrottler::new();
        // Key should not exist before first send
        assert!(!throttler.last_times.contains_key("test_key"));
        throttler.send_if_not_throttled("test_key", "Title", "Message");
        // Key should be recorded after first send
        assert!(throttler.last_times.contains_key("test_key"));
    }

    #[test]
    fn throttler_suppresses_within_cooldown() {
        let mut throttler = NotificationThrottler::new();
        // Pre-populate with a very recent timestamp (1 second ago)
        throttler
            .last_times
            .insert("test_key".to_string(), Instant::now() - Duration::from_secs(1));
        let before = *throttler.last_times.get("test_key").unwrap();
        throttler.send_if_not_throttled("test_key", "Title", "Message");
        // Timestamp should NOT be updated since we are within cooldown
        assert_eq!(*throttler.last_times.get("test_key").unwrap(), before);
    }

    #[test]
    fn throttler_allows_after_cooldown_elapsed() {
        let mut throttler = NotificationThrottler::new();
        // Pre-populate with an old timestamp (10 seconds ago, past 5s cooldown)
        throttler
            .last_times
            .insert("test_key".to_string(), Instant::now() - Duration::from_secs(10));
        let before = *throttler.last_times.get("test_key").unwrap();
        throttler.send_if_not_throttled("test_key", "Title", "Message");
        // Timestamp SHOULD be updated since cooldown has elapsed
        assert_ne!(*throttler.last_times.get("test_key").unwrap(), before);
    }
}
