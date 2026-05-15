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
#[derive(Default)]
pub struct NotificationThrottler {
    last_times: HashMap<String, Instant>,
}

impl NotificationThrottler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if the given key has not been seen within the cooldown period.
    pub fn should_notify(&mut self, key: &str) -> bool {
        let now = Instant::now();
        let allowed = match self.last_times.get(key) {
            Some(&last_time) => now.duration_since(last_time) > Duration::from_secs(5),
            None => true,
        };
        if allowed {
            self.last_times.insert(key.to_string(), now);
        }
        allowed
    }

    pub fn send_if_not_throttled(&mut self, key: &str, title: &str, message: &str) {
        if self.should_notify(key)
            && let Err(e) = send_notification(title, message, NotificationDuration::Short)
        {
            log::error!("Failed to show notification for {title}: {e:#}");
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
        assert!(throttler.should_notify("test_key"));
        assert!(throttler.last_times.contains_key("test_key"));
    }

    #[test]
    fn throttler_suppresses_within_cooldown() {
        let mut throttler = NotificationThrottler::new();
        throttler.last_times.insert(
            "test_key".to_string(),
            Instant::now() - Duration::from_secs(1),
        );
        let before = *throttler.last_times.get("test_key").unwrap();
        assert!(!throttler.should_notify("test_key"));
        assert_eq!(*throttler.last_times.get("test_key").unwrap(), before);
    }

    #[test]
    fn throttler_allows_after_cooldown_elapsed() {
        let mut throttler = NotificationThrottler::new();
        throttler.last_times.insert(
            "test_key".to_string(),
            Instant::now() - Duration::from_secs(10),
        );
        let before = *throttler.last_times.get("test_key").unwrap();
        assert!(throttler.should_notify("test_key"));
        assert_ne!(*throttler.last_times.get("test_key").unwrap(), before);
    }
}
