pub enum NotificationDuration {
    Short,
    Long,
}

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
pub use self::windows::{
    ComToken, init_platform, open_device_properties, open_device_settings, open_devices_list,
    open_sound_settings, open_volume_mixer,
};

#[cfg(not(target_os = "windows"))]
pub struct ComToken(());

#[cfg(not(target_os = "windows"))]
pub fn init_platform(_executable_directory: &std::path::Path) -> anyhow::Result<ComToken> {
    Ok(ComToken(()))
}

pub fn send_notification(
    title: &str,
    message: &str,
    duration: NotificationDuration,
) -> anyhow::Result<()> {
    let timeout = match duration {
        NotificationDuration::Short => notify_rust::Timeout::Default,
        NotificationDuration::Long => notify_rust::Timeout::Milliseconds(25_000),
    };

    let mut notification = notify_rust::Notification::new();
    notification.summary(title).body(message).timeout(timeout);

    #[cfg(target_os = "windows")]
    notification.app_id(crate::consts::APP_AUMID);

    notification
        .show()
        .map_err(|e| anyhow::anyhow!("failed to show notification: {e:#}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::NotificationDuration;

    #[test]
    fn notification_duration_variants_exist() {
        let _short = NotificationDuration::Short;
        let _long = NotificationDuration::Long;
    }
}
