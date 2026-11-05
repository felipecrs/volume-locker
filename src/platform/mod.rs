#[derive(Clone, Copy)]
pub enum NotificationDuration {
    Short,
    Long,
}

mod windows;

pub use self::windows::{
    ComToken, SingleInstanceGuard, init_platform, is_directory_writable, open_device_properties,
    open_device_settings, open_sound_control_panel, open_sound_settings, open_volume_mixer,
    preferred_ui_languages,
};

pub fn send_notification(
    title: &str,
    message: &str,
    duration: NotificationDuration,
) -> anyhow::Result<()> {
    // Tests exercise notifying code paths; don't spam the desktop of whoever runs them.
    if cfg!(test) {
        return Ok(());
    }

    let timeout = match duration {
        NotificationDuration::Short => notify_rust::Timeout::Default,
        NotificationDuration::Long => notify_rust::Timeout::Milliseconds(25_000),
    };

    let mut notification = notify_rust::Notification::new();
    notification.summary(title).body(message).timeout(timeout);

    notification.app_id(crate::consts::APP_AUMID);

    notification
        .show()
        .map_err(|e| anyhow::anyhow!("failed to show notification: {e:#}"))?;
    Ok(())
}
