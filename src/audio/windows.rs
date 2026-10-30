use super::migration::{build_fingerprint, clean_device_name};
use super::presence::{A50X_PROFILE, G533_PROFILE, HeadsetPresence, HidPresenceMonitor};
use super::{AudioBackend, AudioDevice, windows_com_policy_config};
use crate::bindings::{
    AUDIO_VOLUME_NOTIFICATION_DATA, CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree,
    DEVICE_STATE_ACTIVE, EDataFlow, ERole, IAudioEndpointVolume, IAudioEndpointVolumeCallback,
    IAudioEndpointVolumeCallback_Impl, IMMDevice, IMMDeviceEnumerator, IMMNotificationClient,
    IMMNotificationClient_Impl, MMDeviceEnumerator, PKEY_AudioEndpoint_FormFactor,
    PKEY_Device_ContainerId, PKEY_Device_DeviceDesc, PKEY_Device_EnumeratorName,
    PKEY_Device_FriendlyName, PKEY_DeviceInterface_FriendlyName, PROPERTYKEY, PropVariantClear,
    PropVariantToStringAlloc, PropVariantToUInt32, STGM_READ, eCapture, eCommunications, eConsole,
    eMultimedia, eRender,
};
use crate::types::{
    DeviceChangeSource, DeviceFingerprint, DeviceId, DeviceRole, DeviceType, VolumeScalar,
};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::{Arc, Mutex};
use windows_core::{PCWSTR, implement};

const PKEY_DEVICE_INTERFACE_PATH: PROPERTYKEY = PROPERTYKEY {
    fmtid: windows_core::GUID::from_u128(0xb3f8fa53_0004_438e_9003_51a46e139bfc),
    pid: 2,
};

/// Encodes a string slice as a null-terminated UTF-16 wide string for Win32 APIs.
fn encode_wide_null(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn to_erole(role: DeviceRole) -> ERole {
    match role {
        DeviceRole::Console => eConsole,
        DeviceRole::Multimedia => eMultimedia,
        DeviceRole::Communications => eCommunications,
    }
}

#[repr(C)]
struct DeviceNotifySubscribeParameters {
    callback: unsafe extern "system" fn(*mut std::ffi::c_void, u32, *mut std::ffi::c_void) -> u32,
    context: *mut std::ffi::c_void,
}

const DEVICE_NOTIFY_CALLBACK: u32 = 2;
// In Win32, PBT_APMRESUMEAUTOMATIC (0x0012) is automatic resume, and PBT_APMRESUMESUSPEND (0x0007)
// is user-initiated resume. We ignore PBT_APMSUSPEND (0x0004) since the system is suspending anyway.
const PBT_APMRESUMEAUTOMATIC: u32 = 0x0012;
const PBT_APMRESUMESUSPEND: u32 = 0x0007;

unsafe extern "system" fn power_suspend_resume_callback(
    context: *mut std::ffi::c_void,
    event_type: u32,
    _setting: *mut std::ffi::c_void,
) -> u32 {
    // Only react on resume from sleep, never on suspend
    if event_type == PBT_APMRESUMEAUTOMATIC || event_type == PBT_APMRESUMESUSPEND {
        let callback =
            unsafe { &*(context as *const Arc<dyn Fn(DeviceChangeSource) + Send + Sync>) };
        callback(DeviceChangeSource::PowerResume);
    }
    0
}

windows_core::link!("user32.dll" "system" fn RegisterSuspendResumeNotification(hrecipient: *const DeviceNotifySubscribeParameters, flags: u32) -> *mut std::ffi::c_void);
windows_core::link!("user32.dll" "system" fn UnregisterSuspendResumeNotification(handle: *mut std::ffi::c_void) -> windows_core::BOOL);

struct PowerNotifyGuard {
    handle: *mut std::ffi::c_void,
    _params: Box<DeviceNotifySubscribeParameters>,
    _context: Box<Arc<dyn Fn(DeviceChangeSource) + Send + Sync>>,
}

// SAFETY: handle is an opaque OS registration handle, _params is heap-allocated and immovable, and context is Send + Sync.
unsafe impl Send for PowerNotifyGuard {}
unsafe impl Sync for PowerNotifyGuard {}

impl Drop for PowerNotifyGuard {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: handle was obtained from RegisterSuspendResumeNotification.
            // Unregister before _params and _context are freed.
            unsafe {
                let _ = UnregisterSuspendResumeNotification(self.handle);
            }
        }
    }
}

pub struct WindowsAudioBackend {
    enumerator: IMMDeviceEnumerator,
    /// Prevents the COM callback from dropping — the field is written to in
    /// `register_device_change_callback` and must remain alive for the COM callback.
    device_change_callback: Mutex<Option<IMMNotificationClient>>,
    presence_callback: Mutex<Option<Arc<dyn Fn(DeviceChangeSource) + Send + Sync>>>,
    a50x_presence_monitor: Mutex<Option<HidPresenceMonitor>>,
    g533_presence_monitor: Mutex<Option<HidPresenceMonitor>>,
    power_notify_guard: Mutex<Option<PowerNotifyGuard>>,
}

impl WindowsAudioBackend {
    pub fn new(_com_token: &crate::platform::ComToken) -> anyhow::Result<Self> {
        let enumerator: IMMDeviceEnumerator =
            // SAFETY: COM is initialized via CoInitializeEx/init_mta (enforced by ComToken at construction);
            // MMDeviceEnumerator is a well-known COM CLSID that returns a valid interface pointer.
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER)? };
        Ok(Self {
            enumerator,
            device_change_callback: Mutex::new(None),
            presence_callback: Mutex::new(None),
            a50x_presence_monitor: Mutex::new(None),
            g533_presence_monitor: Mutex::new(None),
            power_notify_guard: Mutex::new(None),
        })
    }

    fn configure_presence_monitor(
        &self,
        profile: super::presence::HidPresenceProfile,
        enabled: bool,
        monitor: &Mutex<Option<HidPresenceMonitor>>,
    ) {
        let mut monitor = monitor
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !enabled {
            monitor.take();
            return;
        }
        if monitor.is_some() {
            return;
        }
        let callback = self
            .presence_callback
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(callback) = callback {
            let on_presence_change = Arc::new(move || {
                callback(DeviceChangeSource::HeadsetPresenceChanged);
            });
            match HidPresenceMonitor::start(profile, on_presence_change) {
                Ok(started) => *monitor = Some(started),
                Err(error) => {
                    log::error!("Cannot start {} presence monitor: {error}", profile.name)
                }
            }
        }
    }
}

pub struct WindowsAudioDevice {
    device: IMMDevice,
    endpoint: IAudioEndpointVolume,
    id: DeviceId,
    name: String,
    fingerprint: Option<DeviceFingerprint>,
}

impl WindowsAudioDevice {
    pub fn new(device: IMMDevice) -> anyhow::Result<Self> {
        let endpoint: IAudioEndpointVolume =
            // SAFETY: device from IMMDeviceEnumerator methods; Activate returns a COM interface pointer
            // that is ref-counted and valid for the lifetime of the returned wrapper.
            unsafe { device.Activate(CLSCTX_INPROC_SERVER, None)? };
        // SAFETY: device from IMMDeviceEnumerator; GetId returns an allocated PWSTR freed via CoTaskMemFree.
        let raw_id = unsafe { device.GetId()? };
        let id_result = unsafe { raw_id.to_string() }
            .map_err(|e| anyhow::anyhow!("Invalid UTF-16 in device ID: {e}"));
        unsafe { CoTaskMemFree(raw_id.0 as _) };
        let id = DeviceId::from(id_result?);
        let name = get_device_name(&device)?;
        let fingerprint = get_device_fingerprint(&device);
        Ok(Self {
            device,
            endpoint,
            id,
            name,
            fingerprint,
        })
    }
}

impl AudioBackend for WindowsAudioBackend {
    fn configure_presence_monitoring(&self, enabled: bool) {
        self.configure_presence_monitor(A50X_PROFILE, enabled, &self.a50x_presence_monitor);
        self.configure_presence_monitor(G533_PROFILE, enabled, &self.g533_presence_monitor);
    }

    fn headset_presence(&self, usb_vid_pid: Option<(u16, u16)>) -> HeadsetPresence {
        let Some((vendor_id, product_id)) = usb_vid_pid else {
            return HeadsetPresence::Unknown;
        };
        let monitor = if A50X_PROFILE.matches_usb_device(vendor_id, product_id) {
            &self.a50x_presence_monitor
        } else if G533_PROFILE.matches_usb_device(vendor_id, product_id) {
            &self.g533_presence_monitor
        } else {
            return HeadsetPresence::Unknown;
        };
        monitor
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .map_or(HeadsetPresence::Unknown, HidPresenceMonitor::presence)
    }

    fn devices(&self, device_type: DeviceType) -> anyhow::Result<Vec<Box<dyn AudioDevice>>> {
        let endpoint_type = match device_type {
            DeviceType::Output => eRender,
            DeviceType::Input => eCapture,
        };
        // SAFETY: enumerator obtained from CoCreateInstance; COM manages the returned collection.
        let collection = unsafe {
            self.enumerator
                .EnumAudioEndpoints(endpoint_type, DEVICE_STATE_ACTIVE as u32)?
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
        let role = to_erole(role);
        // SAFETY: COM was initialized via CoInitializeEx (guaranteed by ComToken);
        // enumerator is a valid COM pointer obtained from CoCreateInstance in new().
        let device = unsafe { self.enumerator.GetDefaultAudioEndpoint(flow, role)? };
        Ok(Box::new(WindowsAudioDevice::new(device)?))
    }

    fn set_default_device(&self, device_id: &DeviceId, role: DeviceRole) -> anyhow::Result<()> {
        let role = to_erole(role);
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
        callback: Box<dyn Fn(DeviceChangeSource) + Send + Sync>,
    ) -> anyhow::Result<()> {
        let callback: Arc<dyn Fn(DeviceChangeSource) + Send + Sync> = callback.into();
        *self
            .presence_callback
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Arc::clone(&callback));

        let cb: IMMNotificationClient = AudioDevicesChangedCallback {
            callback: Arc::clone(&callback),
        }
        .into();

        // SAFETY: Both pointers are valid: enumerator from CoCreateInstance, callback from
        // windows::core::implement. COM ref-counting keeps both alive for the registration duration.
        unsafe {
            self.enumerator
                .RegisterEndpointNotificationCallback(&cb)
                .ok()?
        };

        // Register for Windows system resume (sleep/wake) events
        let context = Box::new(callback);
        let params = Box::new(DeviceNotifySubscribeParameters {
            callback: power_suspend_resume_callback,
            context: &*context as *const Arc<dyn Fn(DeviceChangeSource) + Send + Sync>
                as *mut std::ffi::c_void,
        });
        // SAFETY: params is heap-allocated and kept alive in PowerNotifyGuard until UnregisterSuspendResumeNotification.
        let handle = unsafe { RegisterSuspendResumeNotification(&*params, DEVICE_NOTIFY_CALLBACK) };
        let power_guard = if !handle.is_null() {
            Some(PowerNotifyGuard {
                handle,
                _params: params,
                _context: context,
            })
        } else {
            log::warn!("Failed to register suspend/resume notification");
            None
        };

        // Recover from mutex poisoning — the callback must be stored regardless.
        let mut guard = match self.device_change_callback.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        *guard = Some(cb);

        let mut power_lock = match self.power_notify_guard.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        *power_lock = power_guard;

        Ok(())
    }

    fn device_fingerprint(&self, id: &DeviceId) -> Option<DeviceFingerprint> {
        let wide = encode_wide_null(id);
        // SAFETY: wide is a null-terminated UTF-16 string on the stack, valid for this call.
        let device = unsafe { self.enumerator.GetDevice(PCWSTR(wide.as_ptr())).ok()? };
        get_device_fingerprint(&device)
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
                .SetMasterVolumeLevelScalar(volume.as_f32(), std::ptr::null())
                .ok()?;
        }
        Ok(())
    }

    fn is_muted(&self) -> anyhow::Result<bool> {
        // SAFETY: endpoint obtained from IMMDevice::Activate; COM manages its lifetime.
        Ok(unsafe { self.endpoint.GetMute()?.as_bool() })
    }

    fn set_mute(&self, muted: bool) -> anyhow::Result<()> {
        // SAFETY: endpoint from IMMDevice::Activate; null event context means no specific caller.
        unsafe { self.endpoint.SetMute(muted, std::ptr::null()).ok()? };
        Ok(())
    }

    fn is_active(&self) -> anyhow::Result<bool> {
        // SAFETY: device obtained from IMMDeviceEnumerator methods which return valid COM pointers.
        let state = unsafe { self.device.GetState()? };
        Ok(state == DEVICE_STATE_ACTIVE as u32)
    }

    fn watch_volume(
        &self,
        callback: Box<dyn Fn(Option<VolumeScalar>) + Send + Sync>,
    ) -> anyhow::Result<()> {
        let cb: IAudioEndpointVolumeCallback = VolumeChangeCallback { callback }.into();
        // SAFETY: endpoint from IMMDevice::Activate, callback from windows::core::implement.
        // COM ref-counting manages lifetimes; registration persists until the endpoint is dropped.
        unsafe { self.endpoint.RegisterControlChangeNotify(&cb).ok()? };
        Ok(())
    }

    fn fingerprint(&self) -> Option<DeviceFingerprint> {
        self.fingerprint.clone()
    }
}

fn pcwstr_to_device_id(dev_id: &PCWSTR) -> DeviceId {
    if dev_id.is_null() {
        DeviceId::default()
    } else {
        unsafe { dev_id.to_string().map(DeviceId::from).unwrap_or_default() }
    }
}

#[implement(IMMNotificationClient)]
pub struct AudioDevicesChangedCallback {
    pub callback: Arc<dyn Fn(DeviceChangeSource) + Send + Sync>,
}

impl IMMNotificationClient_Impl for AudioDevicesChangedCallback_Impl {
    fn OnDeviceStateChanged(&self, dev_id: &PCWSTR, state: u32) -> windows_core::Result<()> {
        let id = pcwstr_to_device_id(dev_id);
        (self.callback)(DeviceChangeSource::DeviceStateChanged(id, state));
        Ok(())
    }

    fn OnDeviceAdded(&self, dev_id: &PCWSTR) -> windows_core::Result<()> {
        let id = pcwstr_to_device_id(dev_id);
        (self.callback)(DeviceChangeSource::DeviceAdded(id));
        Ok(())
    }

    fn OnDeviceRemoved(&self, dev_id: &PCWSTR) -> windows_core::Result<()> {
        let id = pcwstr_to_device_id(dev_id);
        (self.callback)(DeviceChangeSource::DeviceRemoved(id));
        Ok(())
    }

    fn OnDefaultDeviceChanged(
        &self,
        flow: EDataFlow,
        role: ERole,
        dev_id: &PCWSTR,
    ) -> windows_core::Result<()> {
        let id = pcwstr_to_device_id(dev_id);
        let flow_str = match flow {
            f if f == eRender => "Output",
            f if f == eCapture => "Input",
            _ => "Audio",
        };
        let role_str = match role {
            r if r == eConsole => "Main",
            r if r == eMultimedia => "Multimedia",
            r if r == eCommunications => "Communication",
            _ => "Default",
        };
        (self.callback)(DeviceChangeSource::DefaultDeviceChanged(
            id,
            flow_str.to_string(),
            role_str.to_string(),
        ));
        Ok(())
    }

    fn OnPropertyValueChanged(
        &self,
        dev_id: &PCWSTR,
        key: &PROPERTYKEY,
    ) -> windows_core::Result<()> {
        // Only react when an endpoint's friendly name changes (e.g. user renamed the device
        // in Windows Sound Settings). Other property updates (internal routing, formats,
        // volume, Bluetooth battery/telemetry) fire continuously and must be ignored.
        if *key == PKEY_Device_FriendlyName {
            let id = pcwstr_to_device_id(dev_id);
            if !id.is_empty() {
                (self.callback)(DeviceChangeSource::DeviceRenamed(id));
            }
        }
        Ok(())
    }
}

#[implement(IAudioEndpointVolumeCallback)]
pub struct VolumeChangeCallback {
    pub callback: Box<dyn Fn(Option<VolumeScalar>) + Send + Sync>,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeChangeCallback_Impl {
    fn OnNotify(&self, pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows_core::Result<()> {
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

unsafe fn read_prop_string(
    prop_store: &crate::bindings::IPropertyStore,
    key: &PROPERTYKEY,
) -> Option<String> {
    // SAFETY: prop_store is a valid COM interface pointer; key is a valid PROPERTYKEY.
    let mut prop = unsafe { prop_store.GetValue(key).ok()? };
    let str_result = match unsafe { PropVariantToStringAlloc(&raw const prop) } {
        Ok(pwstr) => {
            let s = unsafe { pwstr.to_string().ok() };
            unsafe { CoTaskMemFree(pwstr.0 as _) };
            s
        }
        Err(_) => None,
    };
    let _ = unsafe { PropVariantClear(&mut prop) };
    str_result
}

unsafe fn read_prop_u32(
    prop_store: &crate::bindings::IPropertyStore,
    key: &PROPERTYKEY,
) -> Option<u32> {
    // SAFETY: prop_store is a valid COM interface pointer; key is a valid PROPERTYKEY.
    let mut prop = unsafe { prop_store.GetValue(key).ok()? };
    let val = unsafe { PropVariantToUInt32(&raw const prop) }.ok();
    let _ = unsafe { PropVariantClear(&mut prop) };
    val
}

fn get_device_name(device: &IMMDevice) -> windows_core::Result<String> {
    // SAFETY: device from IMMDeviceEnumerator; property store operations are standard COM calls.
    let friendly_name = unsafe {
        let prop_store = device.OpenPropertyStore(STGM_READ as u32)?;
        read_prop_string(&prop_store, &PKEY_Device_FriendlyName)
            .ok_or_else(|| windows_core::Error::from_hresult(windows_core::HRESULT(-1)))?
    };
    Ok(clean_device_name(&friendly_name))
}

fn get_device_fingerprint(device: &IMMDevice) -> Option<DeviceFingerprint> {
    unsafe {
        let prop_store = device.OpenPropertyStore(STGM_READ as u32).ok()?;
        let friendly_name = read_prop_string(&prop_store, &PKEY_Device_FriendlyName);
        let hardware_desc = read_prop_string(&prop_store, &PKEY_Device_DeviceDesc);
        let interface_name = read_prop_string(&prop_store, &PKEY_DeviceInterface_FriendlyName);
        let enumerator_name = read_prop_string(&prop_store, &PKEY_Device_EnumeratorName);
        let container_id = read_prop_string(&prop_store, &PKEY_Device_ContainerId);
        let form_factor = read_prop_u32(&prop_store, &PKEY_AudioEndpoint_FormFactor);
        let pnp_interface = read_prop_string(&prop_store, &PKEY_DEVICE_INTERFACE_PATH);

        Some(build_fingerprint(
            friendly_name,
            hardware_desc,
            interface_name,
            enumerator_name,
            container_id,
            form_factor,
            pnp_interface,
        ))
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
