use crate::consts::{APP_AUMID, APP_NAME, PNG_ICON_BYTES, PNG_ICON_FILE_NAME};
use crate::types::{DeviceId, DeviceType};
use std::fs;
use std::path::Path;
use std::process::Command;
use windows::Win32::Foundation::ERROR_ALREADY_EXISTS;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::Win32::System::Threading::CreateMutexW;
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
    if let Err(e) = key.set_string("DisplayName", APP_NAME) {
        log::warn!("Failed to set AUMID DisplayName: {e:#}");
    }

    // We need an icon file for the AUMID to work properly
    let png_path = executable_directory.join(PNG_ICON_FILE_NAME);
    if let Err(e) = fs::write(&png_path, PNG_ICON_BYTES) {
        log::warn!("Failed to write {PNG_ICON_FILE_NAME} icon: {e:#}");
        let _ = key.remove_value("IconUri");
    } else if let Err(e) = key.set_hstring("IconUri", &png_path.as_path().into()) {
        log::warn!("Failed to set AUMID IconUri: {e:#}");
    }

    // SAFETY: APP_AUMID is a valid static string; setting the AUMID is a standard shell API call.
    unsafe {
        if let Err(e) = SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(APP_AUMID)) {
            log::warn!("Failed to set explicit AppUserModelID: {e:#}");
        }
    }

    Ok(())
}

/// RAII guard that holds a named mutex for single-instance enforcement.
/// The mutex is released when this struct is dropped.
pub struct SingleInstanceGuard {
    _handle: windows::Win32::Foundation::HANDLE,
}

impl SingleInstanceGuard {
    /// Creates a named mutex. Returns `Ok(guard)` if this is the only instance,
    /// or `Err` if another instance already holds the mutex.
    pub fn acquire(name: &str) -> anyhow::Result<Self> {
        let wide_name = HSTRING::from(name);
        // SAFETY: CreateMutexW with no security attributes and no initial ownership
        // is a standard Win32 call. The wide_name lives on the stack for the call duration.
        let handle = unsafe { CreateMutexW(None, false, &wide_name)? };
        // SAFETY: GetLastError retrieves the thread-local error code set by CreateMutexW.
        let last_error = unsafe { windows::Win32::Foundation::GetLastError() };
        if last_error == ERROR_ALREADY_EXISTS {
            anyhow::bail!("Another instance is already running.");
        }
        Ok(Self { _handle: handle })
    }
}

/// Checks if a directory is writable by attempting to create and delete a temp file.
pub fn is_directory_writable(dir: &Path) -> bool {
    let test_path = dir.join(".volume_locker_write_test");
    match fs::write(&test_path, b"") {
        Ok(()) => {
            let _ = fs::remove_file(&test_path);
            true
        }
        Err(_) => false,
    }
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
pub fn open_sound_control_panel(tab_selector: &str) -> anyhow::Result<()> {
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

pub fn open_device_settings(device_id: &DeviceId) -> anyhow::Result<()> {
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
