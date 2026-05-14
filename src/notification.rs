use crate::platform::{NotificationDuration, send_notification};
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub fn log_and_notify_error(title: &str, message: &str) {
    log::error!("{message}");
    if let Err(e) = send_notification(title, message, NotificationDuration::Long) {
        log::error!("Failed to send error notification: {e:#}");
    }
}

/// Manages debounced notifications, preventing repeated notifications within a cooldown period.
pub struct NotificationThrottler {
    pub(crate) last_times: HashMap<String, Instant>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throttler_records_key_on_first_send() {
        let mut throttler = NotificationThrottler::new();
        assert!(!throttler.last_times.contains_key("test_key"));
        throttler.send_if_not_throttled("test_key", "Title", "Message");
        assert!(throttler.last_times.contains_key("test_key"));
    }

    #[test]
    fn throttler_suppresses_within_cooldown() {
        let mut throttler = NotificationThrottler::new();
        throttler
            .last_times
            .insert("test_key".to_string(), Instant::now() - Duration::from_secs(1));
        let before = *throttler.last_times.get("test_key").unwrap();
        throttler.send_if_not_throttled("test_key", "Title", "Message");
        assert_eq!(*throttler.last_times.get("test_key").unwrap(), before);
    }

    #[test]
    fn throttler_allows_after_cooldown_elapsed() {
        let mut throttler = NotificationThrottler::new();
        throttler
            .last_times
            .insert("test_key".to_string(), Instant::now() - Duration::from_secs(10));
        let before = *throttler.last_times.get("test_key").unwrap();
        throttler.send_if_not_throttled("test_key", "Title", "Message");
        assert_ne!(*throttler.last_times.get("test_key").unwrap(), before);
    }
}
