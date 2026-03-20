pub enum NotificationDuration {
    Short,
    Long,
}

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
pub use self::windows::*;

pub fn send_notification(
    title: &str,
    message: &str,
    duration: NotificationDuration,
) -> Result<(), String> {
    let timeout = match duration {
        NotificationDuration::Short => notify_rust::Timeout::Default,
        NotificationDuration::Long => notify_rust::Timeout::Milliseconds(25_000),
    };

    let mut notification = notify_rust::Notification::new();
    notification.summary(title).body(message).timeout(timeout);

    #[cfg(target_os = "windows")]
    notification.app_id(crate::consts::APP_AUMID);

    notification.show().map_err(|e| e.to_string())?;
    Ok(())
}
