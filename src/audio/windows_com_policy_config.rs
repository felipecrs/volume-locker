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
