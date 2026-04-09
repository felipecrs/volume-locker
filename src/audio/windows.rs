use super::{AudioBackend, AudioDevice, windows_com_policy_config};
use crate::types::{DeviceId, DeviceRole, DeviceType, VolumeScalar};
use regex_lite::Regex;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::OnceLock;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;

/// Encodes a string slice as a null-terminated UTF-16 wide string for Win32 APIs.
fn encode_wide_null(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE, DEVICE_STATE_ACTIVE, EDataFlow, ERole, IMMDevice,
    IMMDeviceCollection, IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl,
    MMDeviceEnumerator, eCapture, eCommunications, eConsole, eMultimedia, eRender,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, STGM_READ};
use windows::core::{PCWSTR, Result, implement};

pub struct WindowsAudioBackend {
    enumerator: IMMDeviceEnumerator,
    // Keep the callback alive
    #[allow(dead_code)]
    device_change_callback: Option<IMMNotificationClient>,
}

impl WindowsAudioBackend {
    pub fn new(_com_token: &crate::platform::ComToken) -> anyhow::Result<Self> {
        let enumerator = create_device_enumerator()?;
        Ok(Self {
            enumerator,
            device_change_callback: None,
        })
    }
}

pub struct WindowsAudioDevice {
    device: IMMDevice,
    endpoint: IAudioEndpointVolume,
    id: DeviceId,
    name: String,
    // Keep volume callback alive
    #[allow(dead_code)]
    volume_callback: Option<IAudioEndpointVolumeCallback>,
}

impl WindowsAudioDevice {
    pub fn new(device: IMMDevice) -> anyhow::Result<Self> {
        let endpoint = get_audio_endpoint(&device)?;
        let id = DeviceId::from(get_device_id(&device)?);
        let name = get_device_name(&device)?;
        Ok(Self {
            device,
            endpoint,
            id,
            name,
            volume_callback: None,
        })
    }
}

impl AudioBackend for WindowsAudioBackend {
    fn get_devices(&self, device_type: DeviceType) -> anyhow::Result<Vec<Box<dyn AudioDevice>>> {
        let endpoint_type = match device_type {
            DeviceType::Output => eRender,
            DeviceType::Input => eCapture,
        };
        let collection =
            enum_audio_endpoints(&self.enumerator, endpoint_type, DEVICE_STATE_ACTIVE)?;
        let count = get_device_count(&collection)?;
        let mut devices = Vec::new();
        for i in 0..count {
            let device = get_device_at_index(&collection, i)?;
            devices.push(Box::new(WindowsAudioDevice::new(device)?) as Box<dyn AudioDevice>);
        }
        Ok(devices)
    }

    fn get_device_by_id(&self, id: &DeviceId) -> anyhow::Result<Box<dyn AudioDevice>> {
        let device = get_device_by_id(&self.enumerator, id)?;
        Ok(Box::new(WindowsAudioDevice::new(device)?))
    }

    fn get_default_device(
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
        // enumerator is a valid COM pointer obtained from create_device_enumerator() in new().
        let device = unsafe { self.enumerator.GetDefaultAudioEndpoint(flow, role)? };
        Ok(Box::new(WindowsAudioDevice::new(device)?))
    }

    fn set_default_device(&self, device_id: &DeviceId, role: DeviceRole) -> anyhow::Result<()> {
        let role = match role {
            DeviceRole::Console => eConsole,
            DeviceRole::Multimedia => eMultimedia,
            DeviceRole::Communications => eCommunications,
        };
        set_default_device(device_id, role)?;
        Ok(())
    }

    fn register_device_change_callback(
        &mut self,
        callback: Box<dyn Fn() + Send + Sync>,
    ) -> anyhow::Result<()> {
        let cb: IMMNotificationClient = AudioDevicesChangedCallback { callback }.into();
        register_notification_callback(&self.enumerator, &cb)?;
        self.device_change_callback = Some(cb);
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
        Ok(VolumeScalar::from(get_volume(&self.endpoint)?))
    }

    fn set_volume(&self, volume: VolumeScalar) -> anyhow::Result<()> {
        Ok(set_volume(&self.endpoint, volume.as_f32())?)
    }

    fn is_muted(&self) -> anyhow::Result<bool> {
        Ok(get_mute(&self.endpoint)?)
    }

    fn set_mute(&self, muted: bool) -> anyhow::Result<()> {
        Ok(set_mute(&self.endpoint, muted)?)
    }

    fn is_active(&self) -> anyhow::Result<bool> {
        let state = get_device_state(&self.device)?;
        Ok(state == DEVICE_STATE_ACTIVE)
    }

    fn watch_volume(&self, callback: Box<dyn Fn(Option<VolumeScalar>) + Send + Sync>) -> anyhow::Result<()> {
        let cb: IAudioEndpointVolumeCallback = VolumeChangeCallback { callback }.into();
        register_control_change_notify(&self.endpoint, &cb)?;
        Ok(())
    }
}

pub(crate) fn create_device_enumerator() -> Result<IMMDeviceEnumerator> {
    // SAFETY: COM is initialized via CoInitializeEx (enforced by ComToken at construction);
    // MMDeviceEnumerator is a well-known COM CLSID that returns a valid interface pointer.
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER) }
}

pub(crate) fn register_notification_callback(
    enumerator: &IMMDeviceEnumerator,
    callback: &IMMNotificationClient,
) -> Result<()> {
    // SAFETY: Both pointers are valid: enumerator from CoCreateInstance, callback from
    // windows::core::implement. COM ref-counting keeps both alive for the registration duration.
    unsafe { enumerator.RegisterEndpointNotificationCallback(callback) }
}

pub(crate) fn get_device_state(device: &IMMDevice) -> Result<DEVICE_STATE> {
    // SAFETY: device obtained from IMMDeviceEnumerator methods which return valid COM pointers.
    unsafe { device.GetState() }
}

pub(crate) fn register_control_change_notify(
    endpoint: &IAudioEndpointVolume,
    callback: &IAudioEndpointVolumeCallback,
) -> Result<()> {
    // SAFETY: endpoint from IMMDevice::Activate, callback from windows::core::implement.
    // COM ref-counting manages lifetimes; registration persists until the endpoint is dropped.
    unsafe { endpoint.RegisterControlChangeNotify(callback) }
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
        let new_volume = unsafe { pnotify.as_ref().map(|p| VolumeScalar::from(p.fMasterVolume)) };
        (self.callback)(new_volume);
        Ok(())
    }
}

pub(crate) fn enum_audio_endpoints(
    enumerator: &IMMDeviceEnumerator,
    data_flow: EDataFlow,
    state_mask: DEVICE_STATE,
) -> Result<IMMDeviceCollection> {
    // SAFETY: enumerator obtained from CoCreateInstance; COM manages the returned collection lifetime.
    unsafe { enumerator.EnumAudioEndpoints(data_flow, state_mask) }
}

pub(crate) fn get_device_count(collection: &IMMDeviceCollection) -> Result<u32> {
    // SAFETY: collection obtained from EnumAudioEndpoints; COM manages lifetime.
    unsafe { collection.GetCount() }
}

pub(crate) fn get_device_at_index(
    collection: &IMMDeviceCollection,
    index: u32,
) -> Result<IMMDevice> {
    // SAFETY: index is within [0, GetCount()); caller is responsible for bounds checking.
    unsafe { collection.Item(index) }
}

pub(crate) fn get_audio_endpoint(device: &IMMDevice) -> Result<IAudioEndpointVolume> {
    // SAFETY: device from IMMDeviceEnumerator methods; Activate returns a COM interface pointer
    // that is ref-counted and valid for the lifetime of the returned wrapper.
    let endpoint: IAudioEndpointVolume = unsafe { device.Activate(CLSCTX_INPROC_SERVER, None)? };
    Ok(endpoint)
}

pub(crate) fn get_device_name(device: &IMMDevice) -> Result<String> {
    // SAFETY: device from IMMDeviceEnumerator; property store operations are standard COM calls.
    // PropVariantToStringAlloc returns an owned PWSTR that to_string()? converts and frees.
    let friendly_name = unsafe {
        let prop_store = device.OpenPropertyStore(STGM_READ)?;
        let friendly_name_prop = prop_store.GetValue(&PKEY_Device_FriendlyName)?;
        PropVariantToStringAlloc(&friendly_name_prop)?.to_string()?
    };
    Ok(clean_device_name(&friendly_name))
}

// Reimplemented from https://github.com/Belphemur/SoundSwitch/blob/50063dd35d3e648192cbcaa1f9a82a5856302562/SoundSwitch.Common/Framework/Audio/Device/DeviceInfo.cs#L33-L56
fn clean_device_name(name: &str) -> String {
    static NAME_SPLITTER: OnceLock<Regex> = OnceLock::new();
    static NAME_CLEANER: OnceLock<Regex> = OnceLock::new();

    let name_splitter = NAME_SPLITTER.get_or_init(|| {
        Regex::new(r"(?P<friendlyName>.+)\s\([\d\s\-|]*(?P<deviceName>.+)\)").unwrap()
    });

    let name_cleaner = NAME_CLEANER.get_or_init(|| Regex::new(r"\s?\(\d\)|^\d+\s?-\s?").unwrap());

    if let Some(captures) = name_splitter.captures(name) {
        let friendly_name = captures.name("friendlyName").map_or("", |m| m.as_str());
        let device_name = captures.name("deviceName").map_or("", |m| m.as_str());

        let cleaned_friendly = name_cleaner.replace_all(friendly_name, "");
        let cleaned_friendly = cleaned_friendly.trim();

        format!("{cleaned_friendly} ({device_name})")
    } else {
        // Old naming format, use as is
        name.to_string()
    }
}

pub(crate) fn get_device_id(device: &IMMDevice) -> Result<String> {
    // SAFETY: device from IMMDeviceEnumerator; GetId returns an owned PWSTR that to_string frees.
    let dev_id = unsafe { device.GetId()?.to_string()? };
    Ok(dev_id)
}

pub(crate) fn get_device_by_id(
    device_enumerator: &IMMDeviceEnumerator,
    device_id: &str,
) -> Result<IMMDevice> {
    let wide = encode_wide_null(device_id);
    // SAFETY: wide is a null-terminated UTF-16 string on the stack, valid for this call.
    // device_enumerator is a valid COM pointer from CoCreateInstance.
    let device = unsafe { device_enumerator.GetDevice(PCWSTR(wide.as_ptr()))? };
    Ok(device)
}

pub(crate) fn get_volume(endpoint: &IAudioEndpointVolume) -> Result<f32> {
    // SAFETY: endpoint obtained from IMMDevice::Activate; COM manages its lifetime.
    unsafe { endpoint.GetMasterVolumeLevelScalar() }
}

pub(crate) fn get_mute(endpoint: &IAudioEndpointVolume) -> Result<bool> {
    // SAFETY: endpoint obtained from IMMDevice::Activate; COM manages its lifetime.
    let muted = unsafe { endpoint.GetMute()? };
    Ok(muted.as_bool())
}

pub(crate) fn set_mute(endpoint: &IAudioEndpointVolume, muted: bool) -> Result<()> {
    // SAFETY: endpoint from IMMDevice::Activate; null event context means no specific caller.
    unsafe { endpoint.SetMute(muted, std::ptr::null()) }
}

pub(crate) fn set_volume(endpoint: &IAudioEndpointVolume, new_volume: f32) -> Result<()> {
    // SAFETY: endpoint from IMMDevice::Activate; null event context means no specific caller.
    unsafe { endpoint.SetMasterVolumeLevelScalar(new_volume, std::ptr::null()) }
}

fn set_default_device(device_id: &str, role: ERole) -> Result<()> {
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
    unsafe { policy_config.SetDefaultEndpoint(PCWSTR(wide.as_ptr()), role) }
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
