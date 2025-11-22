use crate::config::PersistentState;
use crate::types::{DeviceSettings, DeviceType, UserEvent, VolumeChangedEvent};
use crate::utils::send_notification_debounced;
use regex_lite::Regex;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::time::Instant;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE, DEVICE_STATE_ACTIVE, EDataFlow, ERole, IMMDevice,
    IMMDeviceCollection, IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl,
    MMDeviceEnumerator, eCapture, eCommunications, eConsole, eRender,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, STGM_READ};
use windows::core::{PCWSTR, Result, implement};

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
    pub proxy: tao::event_loop::EventLoopProxy<UserEvent>,
}

impl IMMNotificationClient_Impl for AudioDevicesChangedCallback_Impl {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> windows::core::Result<()> {
        log::info!("Some device state changed");
        let _ = self.proxy.send_event(UserEvent::DevicesChanged);
        Ok(())
    }

    fn OnDeviceAdded(&self, _: &PCWSTR) -> windows::core::Result<()> {
        log::info!("Some device was added");
        let _ = self.proxy.send_event(UserEvent::DevicesChanged);
        Ok(())
    }

    fn OnDeviceRemoved(&self, _: &PCWSTR) -> windows::core::Result<()> {
        log::info!("Some device was removed");
        let _ = self.proxy.send_event(UserEvent::DevicesChanged);
        Ok(())
    }

    fn OnDefaultDeviceChanged(
        &self,
        _: EDataFlow,
        _: ERole,
        _: &PCWSTR,
    ) -> windows::core::Result<()> {
        let _ = self.proxy.send_event(UserEvent::DevicesChanged);
        Ok(())
    }

    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> windows::core::Result<()> {
        Ok(())
    }
}

#[implement(IAudioEndpointVolumeCallback)]
pub struct VolumeChangeCallback {
    pub proxy: tao::event_loop::EventLoopProxy<UserEvent>,
    pub device_id: String,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeChangeCallback_Impl {
    fn OnNotify(
        &self,
        pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA,
    ) -> ::windows::core::Result<()> {
        let new_volume = unsafe { pnotify.as_ref().map(|p| p.fMasterVolume) };
        let _ = self
            .proxy
            .send_event(UserEvent::VolumeChanged(VolumeChangedEvent {
                device_id: self.device_id.clone(),
                new_volume,
            }));
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

pub fn convert_float_to_percent(volume: f32) -> f32 {
    (volume * 100f32).round()
}

pub fn convert_percent_to_float(volume: f32) -> f32 {
    volume / 100f32
}

pub fn set_volume(endpoint: &IAudioEndpointVolume, new_volume: f32) -> Result<()> {
    unsafe { endpoint.SetMasterVolumeLevelScalar(new_volume, std::ptr::null()) }
}

pub fn get_default_output_device(device_enumerator: &IMMDeviceEnumerator) -> Result<IMMDevice> {
    let default_device: IMMDevice =
        unsafe { device_enumerator.GetDefaultAudioEndpoint(eRender, eConsole)? };
    Ok(default_device)
}

pub fn get_default_input_device(device_enumerator: &IMMDeviceEnumerator) -> Result<IMMDevice> {
    let default_device: IMMDevice =
        unsafe { device_enumerator.GetDefaultAudioEndpoint(eCapture, eConsole)? };
    Ok(default_device)
}

pub fn is_default_device(
    device_enumerator: &IMMDeviceEnumerator,
    device: &IMMDevice,
    device_type: DeviceType,
) -> bool {
    let default_device = match device_type {
        DeviceType::Output => get_default_output_device(device_enumerator),
        DeviceType::Input => get_default_input_device(device_enumerator),
    };
    if let Ok(default_device) = default_device
        && let (Ok(default_id), Ok(device_id)) =
            (get_device_id(&default_device), get_device_id(device))
    {
        return default_id == device_id;
    }
    false
}

pub fn migrate_device_ids(
    device_enumerator: &IMMDeviceEnumerator,
    persistent_state: &mut PersistentState,
) {
    let mut devices_to_migrate: Vec<(String, DeviceSettings)> = Vec::new();
    let mut devices_to_update: Vec<(String, DeviceSettings)> = Vec::new();

    // Check which devices need migration
    for (device_id, device_settings) in persistent_state.devices.iter() {
        if let Ok(device) = get_device_by_id(device_enumerator, device_id) {
            // Device exists, check if name has changed
            if let Ok(current_name) = get_device_name(&device)
                && current_name != device_settings.name
            {
                log::info!(
                    "Device {} with ID {} had the name changed to {}",
                    device_settings.name,
                    device_id,
                    current_name,
                );
                let mut updated_settings = device_settings.clone();
                updated_settings.name = current_name;
                devices_to_update.push((device_id.clone(), updated_settings));
            }
        } else {
            devices_to_migrate.push((device_id.clone(), device_settings.clone()));
        }
    }

    // Apply the name updates
    for (device_id, updated_settings) in devices_to_update {
        persistent_state.devices.insert(device_id, updated_settings);
    }

    // Attempt to migrate each device
    for (old_device_id, device_settings) in devices_to_migrate {
        let device_name = device_settings.name.clone();
        if let Ok(new_device_id) = find_device_by_name_and_type(
            device_enumerator,
            &device_name,
            device_settings.device_type,
        ) {
            // Swap the old device with the new one
            persistent_state.devices.remove(&old_device_id);
            persistent_state
                .devices
                .insert(new_device_id.clone(), device_settings.clone());

            // Update priority lists
            let priority_list = match device_settings.device_type {
                DeviceType::Output => &mut persistent_state.output_priority_list,
                DeviceType::Input => &mut persistent_state.input_priority_list,
            };

            if let Some(pos) = priority_list.iter().position(|id| id == &old_device_id) {
                priority_list[pos] = new_device_id.clone();
            }

            log::info!("Migrated device {device_name} from ID {old_device_id} to {new_device_id}");
        } else {
            log::warn!(
                "Device {device_name} with ID {old_device_id} could not be found, keeping it in case it returns"
            );
        }
    }
}

fn find_device_by_name_and_type(
    device_enumerator: &IMMDeviceEnumerator,
    target_name: &str,
    device_type: DeviceType,
) -> Result<String> {
    let endpoint_type = match device_type {
        DeviceType::Output => eRender,
        DeviceType::Input => eCapture,
    };

    let devices: IMMDeviceCollection =
        unsafe { device_enumerator.EnumAudioEndpoints(endpoint_type, DEVICE_STATE_ACTIVE)? };

    let count = unsafe { devices.GetCount()? };
    for i in 0..count {
        let device = unsafe { devices.Item(i)? };
        let device_name = get_device_name(&device)?;

        if device_name == target_name {
            return get_device_id(&device);
        }
    }

    Err(windows::core::Error::empty())
}

pub fn check_and_unmute_device(
    device_enumerator: &IMMDeviceEnumerator,
    device_id: &str,
    device_name: &str,
    notify: bool,
    notification_title: &str,
    notification_message_suffix: &str,
    last_notification_times: &mut HashMap<String, Instant>,
) {
    let device = match get_device_by_id(device_enumerator, device_id) {
        Ok(d) => d,
        Err(_) => return,
    };
    let endpoint = match get_audio_endpoint(&device) {
        Ok(ep) => ep,
        Err(_) => return,
    };

    if let Ok(true) = get_mute(&endpoint) {
        if let Err(e) = set_mute(&endpoint, false) {
            log::error!("Failed to unmute {device_name}: {e}");
        } else {
            log::info!("Unmuted {device_name} due to lock settings");
            if notify {
                let message = format!("{device_name} {notification_message_suffix}");
                send_notification_debounced(
                    &format!("unmute_{}", device_id),
                    notification_title,
                    &message,
                    last_notification_times,
                );
            }
        }
    }
}

pub fn get_unmute_notification_details(device_type: DeviceType) -> (&'static str, &'static str) {
    match device_type {
        DeviceType::Input => (
            "Input Device Unmuted",
            "was unmuted due to Keep unmuted setting.",
        ),
        DeviceType::Output => (
            "Output Device Unmuted",
            "was unmuted due to Keep unmuted setting.",
        ),
    }
}

pub fn enforce_priorities(
    device_enumerator: &IMMDeviceEnumerator,
    state: &PersistentState,
    last_notification_times: &mut HashMap<String, Instant>,
    temporary_priority_output: &Option<String>,
    temporary_priority_input: &Option<String>,
) {
    // Check Output Priorities
    let mut output_priority_list = state.output_priority_list.clone();
    if let Some(temp_id) = temporary_priority_output {
        output_priority_list.insert(0, temp_id.clone());
    }

    if let Some(target_id) =
        find_highest_priority_active_device(device_enumerator, &output_priority_list)
    {
        let mut switched = false;

        // Check Console/Multimedia
        let is_console_correct = if let Ok(default_device) =
            get_default_output_device(device_enumerator)
            && let Ok(default_id) = get_device_id(&default_device)
        {
            default_id == target_id
        } else {
            false
        };

        if !is_console_correct {
            log::info!("Enforcing output priority: Switching to {}", target_id);
            let _ = set_default_device(&target_id, eConsole);
            let _ = set_default_device(&target_id, windows::Win32::Media::Audio::eMultimedia);
            switched = true;
        }

        // Check Communications
        if state.switch_communication_device_output {
            let is_comm_correct = if let Ok(default_device) =
                unsafe { device_enumerator.GetDefaultAudioEndpoint(eRender, eCommunications) }
                && let Ok(default_id) = get_device_id(&default_device)
            {
                default_id == target_id
            } else {
                false
            };

            if !is_comm_correct {
                log::info!(
                    "Enforcing output priority (Communication): Switching to {}",
                    target_id
                );
                let _ = set_default_device(&target_id, eCommunications);
                switched = true;
            }
        }

        if switched && state.notify_on_priority_restore_output {
            let device_name = match get_device_by_id(device_enumerator, &target_id) {
                Ok(d) => get_device_name(&d).unwrap_or_else(|_| "Unknown Device".to_string()),
                Err(_) => "Unknown Device".to_string(),
            };
            send_notification_debounced(
                &format!("priority_restore_{}", target_id),
                "Default Output Device Restored",
                &format!("Switched to {} based on priority list.", device_name),
                last_notification_times,
            );
        }
    }

    // Check Input Priorities
    let mut input_priority_list = state.input_priority_list.clone();
    if let Some(temp_id) = temporary_priority_input {
        input_priority_list.insert(0, temp_id.clone());
    }

    if let Some(target_id) =
        find_highest_priority_active_device(device_enumerator, &input_priority_list)
    {
        let mut switched = false;

        // Check Console/Multimedia
        let is_console_correct = if let Ok(default_device) =
            get_default_input_device(device_enumerator)
            && let Ok(default_id) = get_device_id(&default_device)
        {
            default_id == target_id
        } else {
            false
        };

        if !is_console_correct {
            log::info!("Enforcing input priority: Switching to {}", target_id);
            let _ = set_default_device(&target_id, eConsole);
            let _ = set_default_device(&target_id, windows::Win32::Media::Audio::eMultimedia);
            switched = true;
        }

        // Check Communications
        if state.switch_communication_device_input {
            let is_comm_correct = if let Ok(default_device) =
                unsafe { device_enumerator.GetDefaultAudioEndpoint(eCapture, eCommunications) }
                && let Ok(default_id) = get_device_id(&default_device)
            {
                default_id == target_id
            } else {
                false
            };

            if !is_comm_correct {
                log::info!(
                    "Enforcing input priority (Communication): Switching to {}",
                    target_id
                );
                let _ = set_default_device(&target_id, eCommunications);
                switched = true;
            }
        }

        if switched && state.notify_on_priority_restore_input {
            let device_name = match get_device_by_id(device_enumerator, &target_id) {
                Ok(d) => get_device_name(&d).unwrap_or_else(|_| "Unknown Device".to_string()),
                Err(_) => "Unknown Device".to_string(),
            };
            send_notification_debounced(
                &format!("priority_restore_{}", target_id),
                "Default Input Device Restored",
                &format!("Switched to {} based on priority list.", device_name),
                last_notification_times,
            );
        }
    }
}

fn find_highest_priority_active_device(
    device_enumerator: &IMMDeviceEnumerator,
    priority_list: &[String],
) -> Option<String> {
    for device_id in priority_list {
        if let Ok(device) = get_device_by_id(device_enumerator, device_id)
            && let Ok(state) = unsafe { device.GetState() }
            && state == DEVICE_STATE_ACTIVE
        {
            return Some(device_id.clone());
        }
    }
    None
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
