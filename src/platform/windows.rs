use crate::consts::{APP_AUMID, APP_NAME, PNG_ICON_BYTES, PNG_ICON_FILE_NAME};
use crate::types::DeviceType;
use std::fs;
use std::path::Path;
use std::process::Command;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
use windows::core::{HSTRING, Result};
use windows_registry::CURRENT_USER;

/// Witness type proving COM has been initialized on this thread.
/// Only constructible via [`init_platform`], which calls `CoInitializeEx`.
pub struct ComToken(());

pub fn init_platform(executable_directory: &Path) -> anyhow::Result<ComToken> {
    // Initialize COM for the process. Must be called before any COM usage,
    // including WindowsAudioBackend::new().
    // SAFETY: CoInitializeEx is safe to call; first call on this thread.
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
    if let Err(e) = setup_app_aumid(executable_directory) {
        log::warn!("Failed to set up app AUMID: {e:#}");
    }
    Ok(ComToken(()))
}

fn setup_app_aumid(executable_directory: &Path) -> Result<()> {
    let registry_path = format!(r"SOFTWARE\Classes\AppUserModelId\{APP_AUMID}");
    let _ = CURRENT_USER.remove_tree(registry_path.clone());
    let key = CURRENT_USER.create(&registry_path)?;
    let _ = key.set_string("DisplayName", APP_NAME);

    // We need an icon file for the AUMID to work properly
    let png_path = executable_directory.join(PNG_ICON_FILE_NAME);
    if let Err(e) = fs::write(&png_path, PNG_ICON_BYTES) {
        log::warn!("Failed to write {PNG_ICON_FILE_NAME} icon: {e:#}");
        let _ = key.remove_value("IconUri");
    } else {
        let _ = key.set_hstring("IconUri", &png_path.as_path().into());
    }

    // SAFETY: APP_AUMID is a valid static string; setting the AUMID is a standard shell API call.
    unsafe {
        let _ = SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(APP_AUMID));
    }

    Ok(())
}

fn spawn_rundll32(dll: &str, function: &str, arg: &str, context: &str) -> anyhow::Result<()> {
    Command::new("rundll32.exe")
        .arg(format!("{dll},{function}"))
        .arg(arg)
        .spawn()
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!(e).context(format!("failed to {context}")))
}

pub fn open_devices_list(device_type: DeviceType) -> anyhow::Result<()> {
    let tab_index = match device_type {
        DeviceType::Output => "0",
        DeviceType::Input => "1",
    };

    spawn_rundll32(
        "shell32.dll",
        "Control_RunDLL",
        &format!("mmsys.cpl,,{tab_index}"),
        "open devices list",
    )
}

/// Opens the Sound control panel (mmsys.cpl). The `tab_selector` is passed as the
/// page argument (e.g. "0" for Playback, "1" for Recording). Non-numeric values
/// cause mmsys.cpl to open at the default tab.
pub fn open_device_properties(tab_selector: &str) -> anyhow::Result<()> {
    spawn_rundll32(
        "shell32.dll",
        "Control_RunDLL",
        &format!("mmsys.cpl,,{tab_selector}"),
        "open sound control panel",
    )
}

pub fn open_sound_settings() -> anyhow::Result<()> {
    spawn_rundll32(
        "url.dll",
        "FileProtocolHandler",
        "ms-settings:sound",
        "open sound settings",
    )
}

pub fn open_device_settings(device_id: &str) -> anyhow::Result<()> {
    spawn_rundll32(
        "url.dll",
        "FileProtocolHandler",
        &format!("ms-settings:sound-properties?endpointId={device_id}"),
        "open device settings",
    )
}

pub fn open_volume_mixer() -> anyhow::Result<()> {
    spawn_rundll32(
        "url.dll",
        "FileProtocolHandler",
        "ms-settings:apps-volume",
        "open volume mixer",
    )
}
