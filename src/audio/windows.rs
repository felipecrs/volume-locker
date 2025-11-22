use super::{AudioBackend, AudioDevice, AudioResult};
use crate::types::{DeviceRole, DeviceType};
use regex_lite::Regex;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
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
    pub fn new() -> AudioResult<Self> {
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
    id: String,
    name: String,
    // Keep volume callback alive
    #[allow(dead_code)]
    volume_callback: Option<IAudioEndpointVolumeCallback>,
}

impl WindowsAudioDevice {
    pub fn new(device: IMMDevice) -> AudioResult<Self> {
        let endpoint = get_audio_endpoint(&device)?;
        let id = get_device_id(&device)?;
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
    fn get_devices(&self, device_type: DeviceType) -> AudioResult<Vec<Box<dyn AudioDevice>>> {
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

    fn get_device_by_id(&self, id: &str) -> AudioResult<Box<dyn AudioDevice>> {
        let device = get_device_by_id(&self.enumerator, id)?;
        Ok(Box::new(WindowsAudioDevice::new(device)?))
    }

    fn get_default_device(
        &self,
        device_type: DeviceType,
        role: DeviceRole,
    ) -> AudioResult<Box<dyn AudioDevice>> {
        let flow = match device_type {
            DeviceType::Output => eRender,
            DeviceType::Input => eCapture,
        };
        let role = match role {
            DeviceRole::Console => eConsole,
            DeviceRole::Multimedia => eMultimedia,
            DeviceRole::Communications => eCommunications,
        };
        let device = unsafe { self.enumerator.GetDefaultAudioEndpoint(flow, role)? };
        Ok(Box::new(WindowsAudioDevice::new(device)?))
    }

    fn set_default_device(&self, device_id: &str, role: DeviceRole) -> AudioResult<()> {
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
    ) -> AudioResult<()> {
        let cb: IMMNotificationClient = AudioDevicesChangedCallback { callback }.into();
        register_notification_callback(&self.enumerator, &cb)?;
        self.device_change_callback = Some(cb);
        Ok(())
    }
}

impl AudioDevice for WindowsAudioDevice {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn volume(&self) -> AudioResult<f32> {
        Ok(get_volume(&self.endpoint)?)
    }

    fn set_volume(&self, volume: f32) -> AudioResult<()> {
        Ok(set_volume(&self.endpoint, volume)?)
    }

    fn is_muted(&self) -> AudioResult<bool> {
        Ok(get_mute(&self.endpoint)?)
    }

    fn set_mute(&self, muted: bool) -> AudioResult<()> {
        Ok(set_mute(&self.endpoint, muted)?)
    }

    fn is_active(&self) -> AudioResult<bool> {
        let state = get_device_state(&self.device)?;
        Ok(state == DEVICE_STATE_ACTIVE)
    }

    fn watch_volume(&self, callback: Box<dyn Fn(Option<f32>) + Send + Sync>) -> AudioResult<()> {
        let cb: IAudioEndpointVolumeCallback = VolumeChangeCallback { callback }.into();
        register_control_change_notify(&self.endpoint, &cb)?;
        Ok(())
    }
}

pub fn create_device_enumerator() -> Result<IMMDeviceEnumerator> {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER) }
}

pub fn register_notification_callback(
    enumerator: &IMMDeviceEnumerator,
    callback: &IMMNotificationClient,
) -> Result<()> {
    unsafe { enumerator.RegisterEndpointNotificationCallback(callback) }
}

pub fn get_device_state(device: &IMMDevice) -> Result<DEVICE_STATE> {
    unsafe { device.GetState() }
}

pub fn register_control_change_notify(
    endpoint: &IAudioEndpointVolume,
    callback: &IAudioEndpointVolumeCallback,
) -> Result<()> {
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
    pub callback: Box<dyn Fn(Option<f32>) + Send + Sync>,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeChangeCallback_Impl {
    fn OnNotify(
        &self,
        pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA,
    ) -> ::windows::core::Result<()> {
        let new_volume = unsafe { pnotify.as_ref().map(|p| p.fMasterVolume) };
        (self.callback)(new_volume);
        Ok(())
    }
}

pub fn enum_audio_endpoints(
    enumerator: &IMMDeviceEnumerator,
    data_flow: EDataFlow,
    state_mask: DEVICE_STATE,
) -> Result<IMMDeviceCollection> {
    unsafe { enumerator.EnumAudioEndpoints(data_flow, state_mask) }
}

pub fn get_device_count(collection: &IMMDeviceCollection) -> Result<u32> {
    unsafe { collection.GetCount() }
}

pub fn get_device_at_index(collection: &IMMDeviceCollection, index: u32) -> Result<IMMDevice> {
    unsafe { collection.Item(index) }
}

pub fn get_audio_endpoint(device: &IMMDevice) -> Result<IAudioEndpointVolume> {
    let endpoint: IAudioEndpointVolume = unsafe { device.Activate(CLSCTX_INPROC_SERVER, None)? };
    Ok(endpoint)
}

pub fn get_device_name(device: &IMMDevice) -> Result<String> {
    let friendly_name = unsafe {
        let prop_store = device.OpenPropertyStore(STGM_READ)?;
        let friendly_name_prop = prop_store.GetValue(&PKEY_Device_FriendlyName)?;
        PropVariantToStringAlloc(&friendly_name_prop)?.to_string()?
    };
    Ok(clean_device_name(&friendly_name))
}

// Reimplemented from https://github.com/Belphemur/SoundSwitch/blob/50063dd35d3e648192cbcaa1f9a82a5856302562/SoundSwitch.Common/Framework/Audio/Device/DeviceInfo.cs#L33-L56
fn clean_device_name(name: &str) -> String {
    let name_splitter = match Regex::new(r"(?P<friendlyName>.+)\s\([\d\s\-|]*(?P<deviceName>.+)\)")
    {
        Ok(regex) => regex,
        Err(_) => return name.to_string(),
    };

    let name_cleaner = match Regex::new(r"\s?\(\d\)|^\d+\s?-\s?") {
        Ok(regex) => regex,
        Err(_) => return name.to_string(),
    };

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

pub fn get_device_id(device: &IMMDevice) -> Result<String> {
    let dev_id = unsafe { device.GetId()?.to_string()? };
    Ok(dev_id)
}

pub fn get_device_by_id(
    device_enumerator: &IMMDeviceEnumerator,
    device_id: &str,
) -> Result<IMMDevice> {
    let wide: Vec<u16> = OsStr::new(device_id)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let device = unsafe { device_enumerator.GetDevice(PCWSTR(wide.as_ptr()))? };
    Ok(device)
}

pub fn get_volume(endpoint: &IAudioEndpointVolume) -> Result<f32> {
    unsafe { endpoint.GetMasterVolumeLevelScalar() }
}

pub fn get_mute(endpoint: &IAudioEndpointVolume) -> Result<bool> {
    let muted = unsafe { endpoint.GetMute()? };
    Ok(muted.as_bool())
}

pub fn set_mute(endpoint: &IAudioEndpointVolume, muted: bool) -> Result<()> {
    unsafe { endpoint.SetMute(muted, std::ptr::null()) }
}

pub fn set_volume(endpoint: &IAudioEndpointVolume, new_volume: f32) -> Result<()> {
    unsafe { endpoint.SetMasterVolumeLevelScalar(new_volume, std::ptr::null()) }
}

fn set_default_device(device_id: &str, role: ERole) -> Result<()> {
    let policy_config: com_policy_config::IPolicyConfig = unsafe {
        CoCreateInstance(
            &com_policy_config::PolicyConfigClient,
            None,
            CLSCTX_INPROC_SERVER,
        )?
    };
    let wide: Vec<u16> = OsStr::new(device_id)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe { policy_config.SetDefaultEndpoint(PCWSTR(wide.as_ptr()), role) }
}
