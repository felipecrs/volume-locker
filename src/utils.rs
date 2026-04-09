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
            log::error!("Failed to show notification for {title}: {e:#}");
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
pub fn open_path(path: &std::path::Path) -> anyhow::Result<()> {
    open::that_detached(path).context("failed to open path")
}

/// Open a URL in the default browser.
pub fn open_url(url: &str) -> anyhow::Result<()> {
    open::that_detached(url).context("failed to open URL")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_float_to_percent_zero() {
        assert_eq!(convert_float_to_percent(0.0), 0.0);
    }

    #[test]
    fn convert_float_to_percent_full() {
        assert_eq!(convert_float_to_percent(1.0), 100.0);
    }

    #[test]
    fn convert_float_to_percent_half() {
        assert_eq!(convert_float_to_percent(0.5), 50.0);
    }

    #[test]
    fn convert_float_to_percent_rounds() {
        assert_eq!(convert_float_to_percent(0.333), 33.0);
        assert_eq!(convert_float_to_percent(0.335), 34.0);
    }

    #[test]
    fn convert_percent_to_float_zero() {
        assert_eq!(convert_percent_to_float(0.0), 0.0);
    }

    #[test]
    fn convert_percent_to_float_full() {
        assert_eq!(convert_percent_to_float(100.0), 1.0);
    }

    #[test]
    fn convert_percent_to_float_half() {
        assert_eq!(convert_percent_to_float(50.0), 0.5);
    }

    #[test]
    fn roundtrip_float_percent() {
        let original = 0.75;
        let percent = convert_float_to_percent(original);
        let back = convert_percent_to_float(percent);
        assert_eq!(back, original);
    }

    #[test]
    fn convert_float_to_percent_over_100() {
        assert_eq!(convert_float_to_percent(1.5), 150.0);
    }

    #[test]
    fn convert_percent_to_float_over_100() {
        assert_eq!(convert_percent_to_float(200.0), 2.0);
    }

    #[test]
    fn convert_float_to_percent_negative() {
        assert_eq!(convert_float_to_percent(-0.1), -10.0);
    }
}
