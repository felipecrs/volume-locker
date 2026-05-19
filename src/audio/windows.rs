use super::{AudioBackend, AudioDevice, windows_com_policy_config};
use crate::types::{DeviceId, DeviceRole, DeviceType, VolumeScalar};
use regex_lite::Regex;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::{LazyLock, Mutex};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE, DEVICE_STATE_ACTIVE, EDataFlow, ERole, IMMDevice,
    IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl, MMDeviceEnumerator,
    eCapture, eCommunications, eConsole, eMultimedia, eRender,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, STGM_READ};
use windows::core::{PCWSTR, implement};

/// Encodes a string slice as a null-terminated UTF-16 wide string for Win32 APIs.
fn encode_wide_null(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

pub struct WindowsAudioBackend {
    enumerator: IMMDeviceEnumerator,
    /// Prevents the COM callback from dropping — the field is written to in
    /// `register_device_change_callback` and must remain alive for the COM callback.
    device_change_callback: Mutex<Option<IMMNotificationClient>>,
}

impl WindowsAudioBackend {
    pub fn new(_com_token: &crate::platform::ComToken) -> anyhow::Result<Self> {
        let enumerator: IMMDeviceEnumerator =
            // SAFETY: COM is initialized via CoInitializeEx (enforced by ComToken at construction);
            // MMDeviceEnumerator is a well-known COM CLSID that returns a valid interface pointer.
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER)? };
        Ok(Self {
            enumerator,
            device_change_callback: Mutex::new(None),
        })
    }
}

pub struct WindowsAudioDevice {
    device: IMMDevice,
    endpoint: IAudioEndpointVolume,
    id: DeviceId,
    name: String,
}

impl WindowsAudioDevice {
    pub fn new(device: IMMDevice) -> anyhow::Result<Self> {
        let endpoint: IAudioEndpointVolume =
            // SAFETY: device from IMMDeviceEnumerator methods; Activate returns a COM interface pointer
            // that is ref-counted and valid for the lifetime of the returned wrapper.
            unsafe { device.Activate(CLSCTX_INPROC_SERVER, None)? };
        // SAFETY: device from IMMDeviceEnumerator; GetId returns an owned PWSTR that to_string frees.
        let id = DeviceId::from(unsafe { device.GetId()?.to_string()? });
        let name = get_device_name(&device)?;
        Ok(Self {
            device,
            endpoint,
            id,
            name,
        })
    }
}

impl AudioBackend for WindowsAudioBackend {
    fn devices(&self, device_type: DeviceType) -> anyhow::Result<Vec<Box<dyn AudioDevice>>> {
        let endpoint_type = match device_type {
            DeviceType::Output => eRender,
            DeviceType::Input => eCapture,
        };
        // SAFETY: enumerator obtained from CoCreateInstance; COM manages the returned collection.
        let collection = unsafe {
            self.enumerator
                .EnumAudioEndpoints(endpoint_type, DEVICE_STATE_ACTIVE)?
        };
        // SAFETY: collection is a valid COM pointer from EnumAudioEndpoints above.
        let count = unsafe { collection.GetCount()? };
        let mut devices = Vec::new();
        for i in 0..count {
            // SAFETY: index is within [0, GetCount()); COM manages the returned device.
            let device = unsafe { collection.Item(i)? };
            devices.push(Box::new(WindowsAudioDevice::new(device)?) as Box<dyn AudioDevice>);
        }
        Ok(devices)
    }

    fn device_by_id(&self, id: &DeviceId) -> anyhow::Result<Box<dyn AudioDevice>> {
        let wide = encode_wide_null(id);
        // SAFETY: wide is a null-terminated UTF-16 string on the stack, valid for this call.
        let device = unsafe { self.enumerator.GetDevice(PCWSTR(wide.as_ptr()))? };
        Ok(Box::new(WindowsAudioDevice::new(device)?))
    }

    fn default_device(
        &self,
        device_type: DeviceType,
        role: DeviceRole,
    ) -> anyhow::Result<Box<dyn AudioDevice>> {
        let flow = match device_type {
            DeviceType::Output => eRender,
            DeviceType::Input => eCapture,
        };
        let role = match role {
            DeviceRole::Console => eConsole,
            DeviceRole::Multimedia => eMultimedia,
            DeviceRole::Communications => eCommunications,
        };
        // SAFETY: COM was initialized via CoInitializeEx (guaranteed by ComToken);
        // enumerator is a valid COM pointer obtained from CoCreateInstance in new().
        let device = unsafe { self.enumerator.GetDefaultAudioEndpoint(flow, role)? };
        Ok(Box::new(WindowsAudioDevice::new(device)?))
    }

    fn set_default_device(&self, device_id: &DeviceId, role: DeviceRole) -> anyhow::Result<()> {
        let role = match role {
            DeviceRole::Console => eConsole,
            DeviceRole::Multimedia => eMultimedia,
            DeviceRole::Communications => eCommunications,
        };
        // SAFETY: COM is initialized (enforced by ComToken); PolicyConfigClient is an
        // undocumented but widely-used COM class for changing default audio endpoints.
        let policy_config: windows_com_policy_config::IPolicyConfig = unsafe {
            CoCreateInstance(
                &windows_com_policy_config::PolicyConfigClient,
                None,
                CLSCTX_INPROC_SERVER,
            )?
        };
        let wide = encode_wide_null(device_id);
        // SAFETY: wide is a null-terminated UTF-16 string on the stack, valid for this call.
        unsafe { policy_config.SetDefaultEndpoint(PCWSTR(wide.as_ptr()), role)? };
        Ok(())
    }

    fn register_device_change_callback(
        &self,
        callback: Box<dyn Fn() + Send + Sync>,
    ) -> anyhow::Result<()> {
        let cb: IMMNotificationClient = AudioDevicesChangedCallback { callback }.into();
        // SAFETY: Both pointers are valid: enumerator from CoCreateInstance, callback from
        // windows::core::implement. COM ref-counting keeps both alive for the registration duration.
        unsafe { self.enumerator.RegisterEndpointNotificationCallback(&cb)? };
        // Recover from mutex poisoning — the callback must be stored regardless.
        let mut guard = match self.device_change_callback.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        *guard = Some(cb);
        Ok(())
    }
}

impl AudioDevice for WindowsAudioDevice {
    fn id(&self) -> &DeviceId {
        &self.id
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn volume(&self) -> anyhow::Result<VolumeScalar> {
        // SAFETY: endpoint obtained from IMMDevice::Activate; COM manages its lifetime.
        Ok(VolumeScalar::from(unsafe {
            self.endpoint.GetMasterVolumeLevelScalar()?
        }))
    }

    fn set_volume(&self, volume: VolumeScalar) -> anyhow::Result<()> {
        // SAFETY: endpoint from IMMDevice::Activate; null event context means no specific caller.
        unsafe {
            self.endpoint
                .SetMasterVolumeLevelScalar(volume.as_f32(), std::ptr::null())?;
        }
        Ok(())
    }

    fn is_muted(&self) -> anyhow::Result<bool> {
        // SAFETY: endpoint obtained from IMMDevice::Activate; COM manages its lifetime.
        Ok(unsafe { self.endpoint.GetMute()?.as_bool() })
    }

    fn set_mute(&self, muted: bool) -> anyhow::Result<()> {
        // SAFETY: endpoint from IMMDevice::Activate; null event context means no specific caller.
        unsafe { self.endpoint.SetMute(muted, std::ptr::null())? };
        Ok(())
    }

    fn is_active(&self) -> anyhow::Result<bool> {
        // SAFETY: device obtained from IMMDeviceEnumerator methods which return valid COM pointers.
        let state = unsafe { self.device.GetState()? };
        Ok(state == DEVICE_STATE_ACTIVE)
    }

    fn watch_volume(
        &self,
        callback: Box<dyn Fn(Option<VolumeScalar>) + Send + Sync>,
    ) -> anyhow::Result<()> {
        let cb: IAudioEndpointVolumeCallback = VolumeChangeCallback { callback }.into();
        // SAFETY: endpoint from IMMDevice::Activate, callback from windows::core::implement.
        // COM ref-counting manages lifetimes; registration persists until the endpoint is dropped.
        unsafe { self.endpoint.RegisterControlChangeNotify(&cb)? };
        Ok(())
    }
}

#[implement(IMMNotificationClient)]
pub struct AudioDevicesChangedCallback {
    pub callback: Box<dyn Fn() + Send + Sync>,
}

impl IMMNotificationClient_Impl for AudioDevicesChangedCallback_Impl {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> windows::core::Result<()> {
        (self.callback)();
        Ok(())
    }

    fn OnDeviceAdded(&self, _: &PCWSTR) -> windows::core::Result<()> {
        (self.callback)();
        Ok(())
    }

    fn OnDeviceRemoved(&self, _: &PCWSTR) -> windows::core::Result<()> {
        (self.callback)();
        Ok(())
    }

    fn OnDefaultDeviceChanged(
        &self,
        _: EDataFlow,
        _: ERole,
        _: &PCWSTR,
    ) -> windows::core::Result<()> {
        (self.callback)();
        Ok(())
    }

    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> windows::core::Result<()> {
        Ok(())
    }
}

#[implement(IAudioEndpointVolumeCallback)]
pub struct VolumeChangeCallback {
    pub callback: Box<dyn Fn(Option<VolumeScalar>) + Send + Sync>,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeChangeCallback_Impl {
    fn OnNotify(
        &self,
        pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA,
    ) -> ::windows::core::Result<()> {
        // SAFETY: pnotify is provided by the COM runtime and points to a valid
        // AUDIO_VOLUME_NOTIFICATION_DATA for the duration of this callback invocation.
        let new_volume = unsafe {
            pnotify
                .as_ref()
                .map(|p| VolumeScalar::from(p.fMasterVolume))
        };
        (self.callback)(new_volume);
        Ok(())
    }
}

fn get_device_name(device: &IMMDevice) -> windows::core::Result<String> {
    // SAFETY: device from IMMDeviceEnumerator; property store operations are standard COM calls.
    // PropVariantToStringAlloc returns an owned PWSTR that to_string()? converts and frees.
    let friendly_name = unsafe {
        let prop_store = device.OpenPropertyStore(STGM_READ)?;
        let friendly_name_prop = prop_store.GetValue(&PKEY_Device_FriendlyName)?;
        PropVariantToStringAlloc(&raw const friendly_name_prop)?.to_string()?
    };
    Ok(clean_device_name(&friendly_name))
}

// Reimplemented from https://github.com/Belphemur/SoundSwitch/blob/50063dd35d3e648192cbcaa1f9a82a5856302562/SoundSwitch.Common/Framework/Audio/Device/DeviceInfo.cs#L33-L56
fn clean_device_name(name: &str) -> String {
    // SAFETY: These patterns are compile-time constants — Regex::new cannot fail.
    static NAME_SPLITTER: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?P<friendlyName>.+)\s\([\d\s\-|]*(?P<deviceName>.+)\)")
            .unwrap_or_else(|_| unreachable!("constant regex pattern"))
    });
    static NAME_CLEANER: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\s?\(\d\)|^\d+\s?-\s?")
            .unwrap_or_else(|_| unreachable!("constant regex pattern"))
    });

    if let Some(captures) = NAME_SPLITTER.captures(name) {
        let friendly_name = captures.name("friendlyName").map_or("", |m| m.as_str());
        let device_name = captures.name("deviceName").map_or("", |m| m.as_str());

        let cleaned_friendly = NAME_CLEANER.replace_all(friendly_name, "");
        let cleaned_friendly = cleaned_friendly.trim();

        format!("{cleaned_friendly} ({device_name})")
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::clean_device_name;

    #[test]
    fn clean_device_name_standard_format() {
        let result = clean_device_name("Speakers (Realtek High Definition Audio)");
        assert_eq!(result, "Speakers (Realtek High Definition Audio)");
    }

    #[test]
    fn clean_device_name_with_device_number() {
        let result = clean_device_name("Speakers (2) (Realtek High Definition Audio)");
        assert_eq!(result, "Speakers (Realtek High Definition Audio)");
    }

    #[test]
    fn clean_device_name_with_numbered_prefix() {
        let result = clean_device_name("2 - Speakers (Realtek Audio)");
        assert_eq!(result, "Speakers (Realtek Audio)");
    }

    #[test]
    fn clean_device_name_old_format_passthrough() {
        let result = clean_device_name("My Audio Device");
        assert_eq!(result, "My Audio Device");
    }

    #[test]
    fn clean_device_name_with_port_numbers() {
        let result = clean_device_name("Headphones (2- USB Audio Device)");
        assert_eq!(result, "Headphones (USB Audio Device)");
    }
}
