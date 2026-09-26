//! Validates that windows ffi bindings do not require updating.
//!
//! Based upon the approach used in `chrono` and `redbook`.

#[test]
fn gen_bindings() {
    let existing = std::fs::read_to_string("src/bindings.rs").unwrap_or_default();
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_file = temp_dir.path().join("bindings.rs");

    windows_bindgen::builder()
        .output(&temp_file)
        .flat()
        .filters([
            "Windows.Win32.AUDIO_VOLUME_NOTIFICATION_DATA",
            "Windows.Win32.CloseHandle",
            "Windows.Win32.CLSCTX_ALL",
            "Windows.Win32.CLSCTX_INPROC_SERVER",
            "Windows.Win32.CoCreateInstance",
            "Windows.Win32.CoInitializeEx",
            "Windows.Win32.CoTaskMemFree",
            "Windows.Win32.CreateMutexW",
            "Windows.Win32.DEVICE_STATE_ACTIVE",
            "Windows.Win32.DEVICE_STATE_DISABLED",
            "Windows.Win32.DEVICE_STATE_NOTPRESENT",
            "Windows.Win32.DEVICE_STATE_UNPLUGGED",
            "Windows.Win32.EDataFlow",
            "Windows.Win32.ERole",
            "Windows.Win32.ERROR_ALREADY_EXISTS",
            "Windows.Win32.GetLastError",
            "Windows.Win32.IAudioEndpointVolume",
            "Windows.Win32.IAudioEndpointVolumeCallback",
            "Windows.Win32.IMMDevice",
            "Windows.Win32.IMMDeviceCollection",
            "Windows.Win32.IMMDeviceEnumerator",
            "Windows.Win32.IMMNotificationClient",
            "Windows.Win32.IPropertyStore",
            "Windows.Win32.MMDeviceEnumerator",
            "Windows.Win32.PKEY_Device_FriendlyName",
            "Windows.Win32.PROPERTYKEY",
            "Windows.Win32.PROPVARIANT",
            "Windows.Win32.PropVariantClear",
            "Windows.Win32.PropVariantToStringAlloc",
            "Windows.Win32.SetCurrentProcessExplicitAppUserModelID",
            "Windows.Win32.STGM_READ",
            "Windows.Win32.WAVEFORMATEX",
        ])
        .implements([
            "Windows.Win32.IMMNotificationClient",
            "Windows.Win32.IAudioEndpointVolumeCallback",
        ])
        .write();

    let new = std::fs::read_to_string(&temp_file).unwrap();
    if !new.lines().eq(existing.lines()) {
        if std::env::var("UPDATE_BINDINGS").is_ok() || existing.is_empty() {
            std::fs::write("src/bindings.rs", &new).unwrap();
        } else {
            panic!("`src/bindings.rs` is out of date. Run with UPDATE_BINDINGS=1 to update.");
        }
    }
}
