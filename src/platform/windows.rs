use crate::consts::{APP_AUMID, APP_NAME, PNG_ICON_BYTES, PNG_ICON_FILE_NAME};
use crate::platform::NotificationDuration;
use crate::types::DeviceType;
use std::fs;
use std::path::Path;
use std::process::Command;
use tauri_winrt_notification::Toast;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
use windows::core::{HSTRING, Result};
use windows_registry::CURRENT_USER;

pub fn init_platform(executable_directory: &Path) {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).unwrap() };
    // Set AppUserModelID so toast notifications show correct app name and icon
    let _ = setup_app_aumid(executable_directory);
}

pub fn send_notification(
    title: &str,
    message: &str,
    duration: NotificationDuration,
) -> std::result::Result<(), String> {
    let duration = match duration {
        NotificationDuration::Short => tauri_winrt_notification::Duration::Short,
        NotificationDuration::Long => tauri_winrt_notification::Duration::Long,
    };

    Toast::new(APP_AUMID)
        .title(title)
        .text1(message)
        .duration(duration)
        .show()
        .map_err(|e| e.to_string())
}

fn setup_app_aumid(executable_directory: &Path) -> Result<()> {
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

pub fn open_devices_list(device_type: DeviceType) {
    let tab_index = match device_type {
        DeviceType::Output => "0",
        DeviceType::Input => "1",
    };

    let _ = Command::new("rundll32.exe")
        .arg("shell32.dll,Control_RunDLL")
        .arg(format!("mmsys.cpl,,{}", tab_index))
        .spawn();
}

pub fn open_device_properties(device_id: &str) {
    let _ = Command::new("rundll32.exe")
        .arg("shell32.dll,Control_RunDLL")
        .arg(format!("mmsys.cpl,,{}", device_id))
        .spawn();
}

pub fn open_sound_settings() {
    let _ = Command::new("rundll32.exe")
        .arg("url.dll,FileProtocolHandler")
        .arg("ms-settings:sound")
        .spawn();
}

pub fn open_device_settings(device_id: &str) {
    let _ = Command::new("rundll32.exe")
        .arg("url.dll,FileProtocolHandler")
        .arg(format!(
            "ms-settings:sound-properties?endpointId={}",
            device_id
        ))
        .spawn();
}

pub fn open_volume_mixer() {
    let _ = Command::new("rundll32.exe")
        .arg("url.dll,FileProtocolHandler")
        .arg("ms-settings:apps-volume")
        .spawn();
}
