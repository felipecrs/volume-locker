use crate::platform::{NotificationDuration, send_notification};
use anyhow::Context;
use std::path::PathBuf;

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

pub fn open_path(path: &std::path::Path) -> anyhow::Result<()> {
    open::that_detached(path).context("failed to open path")
}

pub fn open_url(url: &str) -> anyhow::Result<()> {
    open::that_detached(url).context("failed to open URL")
}
