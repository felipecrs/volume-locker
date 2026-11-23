//! Reads and changes Windows audio state the way other apps do, from outside the app under test.
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    DEVICE_STATE_ACTIVE, EDataFlow, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, eCapture,
    eConsole, eRender,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, STGM_READ,
};

/// Each test thread needs this once before using COM.
pub fn init_com() {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
        .ok()
        .expect("initialize COM");
}

fn enumerator() -> IMMDeviceEnumerator {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
        .expect("create the audio device enumerator")
}

fn friendly_name(device: &IMMDevice) -> windows::core::Result<String> {
    unsafe {
        let store = device.OpenPropertyStore(STGM_READ)?;
        Ok(store.GetValue(&PKEY_Device_FriendlyName)?.to_string())
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Flow {
    Output,
    Input,
}

impl Flow {
    fn data_flow(self) -> EDataFlow {
        match self {
            Flow::Output => eRender,
            Flow::Input => eCapture,
        }
    }
}

/// An active endpoint, looked up by a part of its name.
pub struct Endpoint(IMMDevice);

impl Endpoint {
    pub fn find(flow: Flow, name_part: &str) -> Option<Self> {
        let devices =
            unsafe { enumerator().EnumAudioEndpoints(flow.data_flow(), DEVICE_STATE_ACTIVE) }
                .ok()?;
        (0..unsafe { devices.GetCount() }.ok()?)
            .filter_map(|i| unsafe { devices.Item(i) }.ok())
            .find(|device| friendly_name(device).is_ok_and(|name| name.contains(name_part)))
            .map(Self)
    }

    pub fn name(&self) -> String {
        friendly_name(&self.0).expect("read the endpoint name")
    }

    pub fn id(&self) -> String {
        unsafe {
            let id = self.0.GetId().expect("read the endpoint ID");
            let text = id.to_string().expect("endpoint IDs are UTF-16");
            windows::Win32::System::Com::CoTaskMemFree(Some(id.0 as *const _));
            text
        }
    }

    fn volume_control(&self) -> IAudioEndpointVolume {
        unsafe { self.0.Activate(CLSCTX_ALL, None) }.expect("activate the endpoint volume")
    }

    pub fn volume(&self) -> f32 {
        unsafe { self.volume_control().GetMasterVolumeLevelScalar() }.expect("read the volume")
    }

    pub fn set_volume(&self, level: f32) {
        unsafe {
            self.volume_control()
                .SetMasterVolumeLevelScalar(level, std::ptr::null())
        }
        .expect("set the volume");
    }

    pub fn is_muted(&self) -> bool {
        unsafe { self.volume_control().GetMute() }
            .expect("read the mute state")
            .as_bool()
    }

    pub fn set_muted(&self, muted: bool) {
        unsafe { self.volume_control().SetMute(muted, std::ptr::null()) }.expect("set the mute");
    }
}

/// The friendly name of the default device for `flow`, if there is one.
pub fn default_name(flow: Flow) -> Option<String> {
    let device =
        unsafe { enumerator().GetDefaultAudioEndpoint(flow.data_flow(), eConsole) }.ok()?;
    friendly_name(&device).ok()
}
