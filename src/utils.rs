use crate::consts::{APP_AUMID, APP_NAME, PNG_ICON_BYTES, PNG_ICON_FILE_NAME};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tauri_winrt_notification::Toast;
use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
use windows::core::{HSTRING, Result};
use windows_registry::CURRENT_USER;

pub fn get_executable_directory() -> PathBuf {
    std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

pub fn get_executable_path() -> PathBuf {
    std::env::current_exe().unwrap()
}

pub fn setup_app_aumid(executable_directory: &Path) -> Result<()> {
    // Create registry keys for the AppUserModelID
    let registry_path = format!(r"SOFTWARE\Classes\AppUserModelId\{APP_AUMID}");
    let _ = CURRENT_USER.remove_tree(registry_path.clone());
    let key = CURRENT_USER.create(registry_path.clone()).unwrap();
    let _ = key.set_string("DisplayName", APP_NAME);

    // Write the icon file to the executable directory and use it as the icon
    let png_path = executable_directory.join(PNG_ICON_FILE_NAME);
    if let Err(e) = fs::write(&png_path, PNG_ICON_BYTES) {
        log::warn!("Failed to write {PNG_ICON_FILE_NAME} icon: {e}");
        let _ = key.remove_value("IconUri");
    } else {
        let _ = key.set_hstring("IconUri", &png_path.as_path().into());
    }

    unsafe {
        let _ = SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(APP_AUMID));
    }

    Ok(())
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
        if let Err(e) = Toast::new(APP_AUMID).title(title).text1(message).show() {
            log::error!("Failed to show notification for {title}: {e}");
        }
        last_notification_times.insert(key.to_string(), now);
    }
}
