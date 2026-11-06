#![expect(non_snake_case, reason = "COM method names")]

use crate::bindings::{ERole, PROPERTYKEY, PROPVARIANT, WAVEFORMATEX};
use windows_core::{GUID, HRESULT, IUnknown, PCWSTR, interface};

pub const POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

// This interface is undocumented. Its method order is the COM vtable layout, so the methods we
// don't call must stay, in order.
#[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
pub unsafe trait IPolicyConfig: IUnknown {
    fn GetMixFormat(&self, device: PCWSTR, format: *mut *mut WAVEFORMATEX) -> HRESULT;
    fn GetDeviceFormat(
        &self,
        device: PCWSTR,
        default: i32,
        format: *mut *mut WAVEFORMATEX,
    ) -> HRESULT;
    fn ResetDeviceFormat(&self, device: PCWSTR) -> HRESULT;
    fn SetDeviceFormat(
        &self,
        device: PCWSTR,
        endpoint: *mut WAVEFORMATEX,
        mix: *mut WAVEFORMATEX,
    ) -> HRESULT;
    fn GetProcessingPeriod(
        &self,
        device: PCWSTR,
        default: i32,
        default_period: *mut i64,
        minimum_period: *mut i64,
    ) -> HRESULT;
    fn SetProcessingPeriod(&self, device: PCWSTR, period: *mut i64) -> HRESULT;
    fn GetShareMode(&self, device: PCWSTR, mode: *mut i32) -> HRESULT;
    fn SetShareMode(&self, device: PCWSTR, mode: *mut i32) -> HRESULT;
    fn GetPropertyValue(
        &self,
        device: PCWSTR,
        fx_store: i32,
        key: *const PROPERTYKEY,
        value: *mut PROPVARIANT,
    ) -> HRESULT;
    pub fn SetPropertyValue(
        &self,
        device: PCWSTR,
        fx_store: i32,
        key: *const PROPERTYKEY,
        value: *mut PROPVARIANT,
    ) -> HRESULT;
    pub fn SetDefaultEndpoint(&self, device: PCWSTR, role: ERole) -> HRESULT;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::windows::{read_prop_string, take_prop_string};
    use crate::bindings::{
        CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree, IMMDeviceEnumerator,
        MMDeviceEnumerator, PKEY_Device_DeviceDesc, STGM_READ, eConsole, eRender,
    };
    use std::mem::{offset_of, size_of};
    use windows_core::HSTRING;

    #[test]
    fn called_methods_sit_at_their_com_vtable_slots() {
        let slot = |offset: usize| offset / size_of::<usize>();
        assert_eq!(slot(offset_of!(IPolicyConfig_Vtbl, GetPropertyValue)), 11);
        assert_eq!(slot(offset_of!(IPolicyConfig_Vtbl, SetPropertyValue)), 12);
        assert_eq!(slot(offset_of!(IPolicyConfig_Vtbl, SetDefaultEndpoint)), 13);
    }

    fn policy_config() -> IPolicyConfig {
        windows_core::init_mta().expect("should initialize COM");
        // SAFETY: COM was just initialized on this thread.
        unsafe { CoCreateInstance(&POLICY_CONFIG_CLIENT, None, CLSCTX_INPROC_SERVER) }
            .expect("Windows should provide the class with this interface")
    }

    #[test]
    fn setting_the_default_of_an_unknown_device_fails_without_changing_anything() {
        let device = HSTRING::from("not-a-device");
        // SAFETY: the HSTRING is null-terminated and outlives this call.
        let result =
            unsafe { policy_config().SetDefaultEndpoint(PCWSTR(device.as_ptr()), eConsole) };
        assert!(result.is_err());
    }

    // Read-only, so it is safe on any machine. Machines without audio devices skip it.
    #[test]
    fn reads_the_same_endpoint_name_as_the_property_store() {
        let policy_config = policy_config();
        // SAFETY: COM was initialized by policy_config() and every pointer outlives its call.
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER)
                    .expect("should create enumerator");
            let Ok(device) = enumerator.GetDefaultAudioEndpoint(eRender, eConsole) else {
                return;
            };
            let raw_id = device.GetId().expect("should read device ID");
            let store = device
                .OpenPropertyStore(STGM_READ as u32)
                .expect("should open property store");
            let expected = read_prop_string(&store, &PKEY_Device_DeviceDesc);

            let mut value = PROPVARIANT::default();
            policy_config
                .GetPropertyValue(PCWSTR(raw_id.0), 0, &PKEY_Device_DeviceDesc, &mut value)
                .ok()
                .expect("should read the property");
            CoTaskMemFree(raw_id.0.cast());

            assert!(expected.is_some());
            assert_eq!(take_prop_string(&mut value), expected);
        }
    }
}
