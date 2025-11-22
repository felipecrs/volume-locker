pub enum NotificationDuration {
    Short,
    Long,
}

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
pub use self::windows::*;
