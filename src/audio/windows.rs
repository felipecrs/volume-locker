use super::presence::{HeadsetPresence, HidPresenceMonitor, devices::PROFILES};
use super::transfer::{build_fingerprint, compose_device_name};
use super::{AudioBackend, AudioDevice, windows_com_policy_config};
use crate::bindings::{
    AUDIO_VOLUME_NOTIFICATION_DATA, CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree,
    DEVICE_NOTIFY_CALLBACK, DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, DEVICE_STATE_ACTIVE, EDataFlow,
    ERole, HANDLE, HPOWERNOTIFY, IAudioEndpointVolume, IAudioEndpointVolumeCallback,
    IAudioEndpointVolumeCallback_Impl, IMMDevice, IMMDeviceEnumerator, IMMNotificationClient,
    IMMNotificationClient_Impl, MMDeviceEnumerator, PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND,
    PKEY_AudioEndpoint_FormFactor, PKEY_Device_ContainerId, PKEY_Device_DeviceDesc,
    PKEY_Device_EnumeratorName, PKEY_DeviceInterface_FriendlyName, PROPERTYKEY, PROPVAR_PAD1,
    PROPVAR_PAD2, PROPVAR_PAD3, PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
    PropVariantClear, PropVariantToStringAlloc, PropVariantToUInt32,
    RegisterSuspendResumeNotification, STGM_READ, UnregisterSuspendResumeNotification, VARTYPE,
    VT_LPWSTR, eCapture, eCommunications, eConsole, eMultimedia, eRender,
};
use crate::types::{
    DeviceChangeSource, DeviceFingerprint, DeviceId, DeviceRole, DeviceType, VolumeScalar,
};
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex};
use windows_core::{HSTRING, PCWSTR, implement};

const PKEY_DEVICE_INTERFACE_PATH: PROPERTYKEY = PROPERTYKEY {
    fmtid: windows_core::GUID::from_u128(0xb3f8fa53_0004_438e_9003_51a46e139bfc),
    pid: 2,
};

/// PKEY_DeviceClass_IconPath: the endpoint icon chosen in Sound settings (a `.ico` path or a
/// `module.dll,index` resource reference). Endpoints store it here, not in the logo property.
const PKEY_ENDPOINT_ICON_PATH: PROPERTYKEY = PROPERTYKEY {
    fmtid: windows_core::GUID::from_u128(0x259abffc_50a7_47ce_af08_68c9a7d73366),
    pid: 12,
};

fn to_erole(role: DeviceRole) -> ERole {
    match role {
        DeviceRole::Console => eConsole,
        DeviceRole::Multimedia => eMultimedia,
        DeviceRole::Communications => eCommunications,
    }
}

type DeviceChangeCallback = Arc<dyn Fn(DeviceChangeSource) + Send + Sync>;

unsafe extern "system" fn power_suspend_resume_callback(
    context: *const std::ffi::c_void,
    event_type: u32,
    _setting: *const std::ffi::c_void,
) -> u32 {
    // Only react on resume from sleep, never on suspend
    if event_type == PBT_APMRESUMEAUTOMATIC as u32 || event_type == PBT_APMRESUMESUSPEND as u32 {
        let callback = unsafe { &*context.cast::<DeviceChangeCallback>() };
        callback(DeviceChangeSource::PowerResume);
    }
    0
}

struct PowerNotifyGuard {
    handle: HPOWERNOTIFY,
    _params: Box<DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS>,
    _context: Box<DeviceChangeCallback>,
}

// SAFETY: handle is an opaque OS registration handle, _params is heap-allocated and immovable, and context is Send + Sync.
unsafe impl Send for PowerNotifyGuard {}
unsafe impl Sync for PowerNotifyGuard {}

impl Drop for PowerNotifyGuard {
    fn drop(&mut self) {
        // SAFETY: handle was obtained from RegisterSuspendResumeNotification.
        // Unregister before _params and _context are freed.
        unsafe {
            let _ = UnregisterSuspendResumeNotification(self.handle);
        }
    }
}

/// Registers for Windows system resume (sleep/wake) events.
fn register_power_notification(callback: DeviceChangeCallback) -> Option<PowerNotifyGuard> {
    let context = Box::new(callback);
    let params = Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
        Callback: Some(power_suspend_resume_callback),
        Context: (&raw const *context).cast_mut().cast(),
    });
    // SAFETY: params is heap-allocated and kept alive in PowerNotifyGuard until UnregisterSuspendResumeNotification.
    let handle = unsafe {
        RegisterSuspendResumeNotification(
            HANDLE((&raw const *params).cast_mut().cast()),
            DEVICE_NOTIFY_CALLBACK as u32,
        )
    };
    if handle.0.is_null() {
        log::warn!("Failed to register suspend/resume notification");
        return None;
    }
    Some(PowerNotifyGuard {
        handle,
        _params: params,
        _context: context,
    })
}

pub struct WindowsAudioBackend {
    enumerator: IMMDeviceEnumerator,
    /// Prevents the COM callback from dropping — the field is written to in
    /// `register_device_change_callback` and must remain alive for the COM callback.
    device_change_callback: Mutex<Option<IMMNotificationClient>>,
    presence_callback: Mutex<Option<DeviceChangeCallback>>,
    presence_monitors: Mutex<HashMap<(u16, u16), HidPresenceMonitor>>,
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
            presence_monitors: Mutex::new(HashMap::new()),
            power_notify_guard: Mutex::new(None),
        })
    }

    fn start_presence_monitor(
        &self,
        profile: super::presence::HidPresenceProfile,
    ) -> Option<HidPresenceMonitor> {
        let callback = self
            .presence_callback
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()?;
        let on_presence_change = Arc::new(move || {
            callback(DeviceChangeSource::HeadsetPresenceChanged);
        });
        match HidPresenceMonitor::start(profile, on_presence_change) {
            Ok(started) => Some(started),
            Err(error) => {
                log::error!("Cannot start {} presence monitor: {error}", profile.model);
                None
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
    fn configure_presence_monitoring(&self, usb_devices: &[(u16, u16)]) {
        let mut monitors = self
            .presence_monitors
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        monitors.retain(|identity, _| usb_devices.contains(identity));
        for profile in PROFILES {
            let identity = (profile.vendor_id, profile.product_id);
            if usb_devices.contains(&identity)
                && !monitors.contains_key(&identity)
                && let Some(monitor) = self.start_presence_monitor(*profile)
            {
                monitors.insert(identity, monitor);
            }
        }
    }

    fn headset_presence(&self, usb_vid_pid: (u16, u16)) -> HeadsetPresence {
        self.presence_monitors
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&usb_vid_pid)
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
        // SAFETY: the temporary HSTRING is null-terminated and outlives this call.
        let device = unsafe { self.enumerator.GetDevice(&HSTRING::from(&**id))? };
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
                &windows_com_policy_config::POLICY_CONFIG_CLIENT,
                None,
                CLSCTX_INPROC_SERVER,
            )?
        };
        let wide = HSTRING::from(&**device_id);
        // SAFETY: wide is null-terminated and outlives this call.
        unsafe {
            policy_config
                .SetDefaultEndpoint(PCWSTR(wide.as_ptr()), role)
                .ok()?
        };
        Ok(())
    }

    fn register_device_change_callback(
        &self,
        callback: Box<dyn Fn(DeviceChangeSource) + Send + Sync>,
    ) -> anyhow::Result<()> {
        let callback: DeviceChangeCallback = callback.into();
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

        let power_guard = register_power_notification(callback);

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
        // SAFETY: the temporary HSTRING is null-terminated and outlives this call.
        let device = unsafe { self.enumerator.GetDevice(&HSTRING::from(&**id)).ok()? };
        get_device_fingerprint(&device)
    }

    fn restore_device_properties(
        &self,
        id: &DeviceId,
        endpoint_name: Option<&str>,
        icon_path: Option<&str>,
    ) -> anyhow::Result<()> {
        if endpoint_name.is_none() && icon_path.is_none() {
            return Ok(());
        }

        // SAFETY: PolicyConfigClient is the existing Windows endpoint configuration COM class.
        let policy_config: windows_com_policy_config::IPolicyConfig = unsafe {
            CoCreateInstance(
                &windows_com_policy_config::POLICY_CONFIG_CLIENT,
                None,
                CLSCTX_INPROC_SERVER,
            )?
        };
        let wide_id = HSTRING::from(&**id);
        let mut errors = Vec::new();

        if let Some(name) = endpoint_name
            && let Err(e) =
                set_policy_config_string(&policy_config, &wide_id, &PKEY_Device_DeviceDesc, name)
        {
            errors.push(format!("endpoint name: {e:#}"));
        }
        if let Some(path) = icon_path
            && let Err(e) =
                set_policy_config_string(&policy_config, &wide_id, &PKEY_ENDPOINT_ICON_PATH, path)
        {
            errors.push(format!("icon path: {e:#}"));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            anyhow::bail!(
                "failed to restore endpoint properties ({})",
                errors.join("; ")
            )
        }
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
    pub callback: DeviceChangeCallback,
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
        // Only react when an endpoint's name or icon changes (e.g. user edited the device in
        // Windows Sound Settings; a rename edits PKEY_Device_DeviceDesc). Other property updates
        // (internal routing, formats, volume, Bluetooth battery/telemetry) fire continuously and
        // must be ignored.
        let source = if *key == PKEY_Device_DeviceDesc {
            DeviceChangeSource::DeviceRenamed
        } else if *key == PKEY_ENDPOINT_ICON_PATH {
            DeviceChangeSource::DeviceIconChanged
        } else {
            return Ok(());
        };
        let id = pcwstr_to_device_id(dev_id);
        if !id.is_empty() {
            (self.callback)(source(id));
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

pub(super) unsafe fn read_prop_string(
    prop_store: &crate::bindings::IPropertyStore,
    key: &PROPERTYKEY,
) -> Option<String> {
    // SAFETY: prop_store is a valid COM interface pointer; key is a valid PROPERTYKEY.
    let mut prop = unsafe { prop_store.GetValue(key).ok()? };
    unsafe { take_prop_string(&mut prop) }
}

/// Converts `prop` to a string and clears it.
pub(super) unsafe fn take_prop_string(prop: &mut PROPVARIANT) -> Option<String> {
    let str_result = match unsafe { PropVariantToStringAlloc(&raw const *prop) } {
        Ok(pwstr) => {
            let s = unsafe { pwstr.to_string().ok() };
            unsafe { CoTaskMemFree(pwstr.0 as _) };
            s
        }
        Err(_) => None,
    };
    let _ = unsafe { PropVariantClear(prop) };
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
    let (desc, interface) = unsafe {
        let prop_store = device.OpenPropertyStore(STGM_READ as u32)?;
        (
            read_prop_string(&prop_store, &PKEY_Device_DeviceDesc),
            read_prop_string(&prop_store, &PKEY_DeviceInterface_FriendlyName),
        )
    };
    compose_device_name(desc.as_deref(), interface.as_deref())
        .ok_or_else(|| windows_core::Error::from_hresult(windows_core::HRESULT(-1)))
}

fn get_device_fingerprint(device: &IMMDevice) -> Option<DeviceFingerprint> {
    unsafe {
        let prop_store = device.OpenPropertyStore(STGM_READ as u32).ok()?;
        let device_desc = read_prop_string(&prop_store, &PKEY_Device_DeviceDesc);
        let interface_name = read_prop_string(&prop_store, &PKEY_DeviceInterface_FriendlyName);
        let enumerator_name = read_prop_string(&prop_store, &PKEY_Device_EnumeratorName);
        let container_id = read_prop_string(&prop_store, &PKEY_Device_ContainerId);
        let form_factor = read_prop_u32(&prop_store, &PKEY_AudioEndpoint_FormFactor);
        let pnp_interface = read_prop_string(&prop_store, &PKEY_DEVICE_INTERFACE_PATH);
        let icon_path = read_prop_string(&prop_store, &PKEY_ENDPOINT_ICON_PATH);

        let mut fingerprint = build_fingerprint(
            device_desc,
            interface_name,
            enumerator_name,
            container_id,
            form_factor,
            pnp_interface,
        );
        fingerprint.icon_path = icon_path;
        Some(fingerprint)
    }
}

fn set_policy_config_string(
    policy_config: &windows_com_policy_config::IPolicyConfig,
    device_id: &HSTRING,
    key: &PROPERTYKEY,
    value: &str,
) -> anyhow::Result<()> {
    let wide_value = HSTRING::from(value);
    let mut prop = PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VARTYPE(VT_LPWSTR as u16),
                wReserved1: PROPVAR_PAD1(0),
                wReserved2: PROPVAR_PAD2(0),
                wReserved3: PROPVAR_PAD3(0),
                Anonymous: PROPVARIANT_0_0_0 {
                    pwszVal: windows_core::PWSTR(wide_value.as_ptr().cast_mut()),
                },
            }),
        },
    };
    // SAFETY: the UTF-16 buffers and PROPVARIANT remain alive through this COM call.
    unsafe {
        policy_config
            .SetPropertyValue(
                PCWSTR(device_id.as_ptr()),
                0, // bFxStore = false selects the endpoint's regular property store.
                key,
                &mut prop,
            )
            .ok()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn counting_callback() -> (Arc<AtomicUsize>, DeviceChangeCallback) {
        let count = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&count);
        let callback: DeviceChangeCallback = Arc::new(move |source| {
            if matches!(source, DeviceChangeSource::PowerResume) {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        });
        (count, callback)
    }

    #[test]
    fn power_callback_fires_only_on_resume() {
        const PBT_APMSUSPEND: u32 = 0x0004;
        let (count, callback) = counting_callback();
        let context = (&raw const callback).cast();

        for (event, expected) in [
            (PBT_APMSUSPEND, 0),
            (PBT_APMRESUMEAUTOMATIC as u32, 1),
            (PBT_APMRESUMESUSPEND as u32, 2),
        ] {
            // SAFETY: context points to `callback`, which outlives this call.
            unsafe { power_suspend_resume_callback(context, event, std::ptr::null()) };
            assert_eq!(count.load(Ordering::SeqCst), expected);
        }
    }

    #[test]
    fn power_notification_registers_and_unregisters() {
        let (_count, callback) = counting_callback();
        assert!(register_power_notification(callback).is_some());
    }
}
