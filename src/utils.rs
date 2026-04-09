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
    use crate::types::{VolumePercent, VolumeScalar};

    #[test]
    fn convert_float_to_percent_zero() {
        assert_eq!(VolumeScalar::from(0.0).to_percent().as_f32(), 0.0);
    }

    #[test]
    fn convert_float_to_percent_full() {
        assert_eq!(VolumeScalar::from(1.0).to_percent().as_f32(), 100.0);
    }

    #[test]
    fn convert_float_to_percent_half() {
        assert_eq!(VolumeScalar::from(0.5).to_percent().as_f32(), 50.0);
    }

    #[test]
    fn convert_float_to_percent_rounds() {
        assert_eq!(VolumeScalar::from(0.333).to_percent().as_f32(), 33.0);
        assert_eq!(VolumeScalar::from(0.335).to_percent().as_f32(), 34.0);
    }

    #[test]
    fn convert_percent_to_float_zero() {
        assert_eq!(VolumePercent::from(0.0).to_scalar().as_f32(), 0.0);
    }

    #[test]
    fn convert_percent_to_float_full() {
        assert_eq!(VolumePercent::from(100.0).to_scalar().as_f32(), 1.0);
    }

    #[test]
    fn convert_percent_to_float_half() {
        assert_eq!(VolumePercent::from(50.0).to_scalar().as_f32(), 0.5);
    }

    #[test]
    fn roundtrip_float_percent() {
        let original = 0.75;
        let percent = VolumeScalar::from(original).to_percent();
        let back = percent.to_scalar().as_f32();
        assert_eq!(back, original);
    }

    #[test]
    fn convert_float_to_percent_over_100() {
        assert_eq!(VolumeScalar::from(1.5).to_percent().as_f32(), 150.0);
    }

    #[test]
    fn convert_percent_to_float_over_100() {
        assert_eq!(VolumePercent::from(200.0).to_scalar().as_f32(), 2.0);
    }

    #[test]
    fn convert_float_to_percent_negative() {
        assert_eq!(VolumeScalar::from(-0.1).to_percent().as_f32(), -10.0);
    }
}
