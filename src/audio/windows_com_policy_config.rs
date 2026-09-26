#![allow(dead_code)]
#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(clippy::missing_safety_doc)]
#![allow(unsafe_op_in_unsafe_fn)]

use std::ffi::c_void;
use windows::Devices::Custom::DeviceSharingMode;
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::{ERole, WAVEFORMATEX};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::core::imp::CanInto;
use windows::core::{GUID, HRESULT, IUnknown, Interface, PCWSTR, Param, Result};

pub const PolicyConfigClient: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

#[repr(transparent)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IPolicyConfig(IUnknown);

impl CanInto<IUnknown> for IPolicyConfig {}

impl IPolicyConfig {
    pub unsafe fn SetDefaultEndpoint(
        &self,
        device_name: impl Param<PCWSTR>,
        role: ERole,
    ) -> Result<()> {
        (Interface::vtable(self).SetDefaultEndpoint)(
            Interface::as_raw(self),
            device_name.param().abi(),
            role,
        )
        .ok()
    }
}

// SAFETY: IPolicyConfig vtable layout matches the COM ABI defined by Windows.
// IID is the documented interface GUID for IPolicyConfig.
unsafe impl Interface for IPolicyConfig {
    type Vtable = IPolicyConfig_Vtbl;
    const IID: GUID = GUID::from_u128(0xf8679f50_850a_41cf_9c72_430f290290c8);
}

// The vtable prefix must mirror the COM ABI: the offset of the method we call,
// SetDefaultEndpoint, depends on every preceding fn-pointer field being present
// and in order. Unused entries are kept (and dead_code allowed) for that reason.
#[repr(C)]
#[doc(hidden)]
pub struct IPolicyConfig_Vtbl {
    pub base__: ::windows::core::IUnknown_Vtbl,
    pub GetMixFormat:
        unsafe extern "system" fn(this: *mut c_void, PCWSTR, *mut *mut WAVEFORMATEX) -> HRESULT,
    pub GetDeviceFormat: unsafe extern "system" fn(
        this: *mut c_void,
        PCWSTR,
        i32,
        *mut *mut WAVEFORMATEX,
    ) -> HRESULT,
    pub ResetDeviceFormat: unsafe extern "system" fn(this: *mut c_void, PCWSTR) -> HRESULT,
    pub SetDeviceFormat: unsafe extern "system" fn(
        this: *mut c_void,
        PCWSTR,
        *mut WAVEFORMATEX,
        *mut WAVEFORMATEX,
    ) -> HRESULT,
    pub GetProcessingPeriod:
        unsafe extern "system" fn(this: *mut c_void, PCWSTR, i32, *mut i64, *mut i64) -> HRESULT,
    pub SetProcessingPeriod:
        unsafe extern "system" fn(this: *mut c_void, PCWSTR, *mut i64) -> HRESULT,
    pub GetShareMode:
        unsafe extern "system" fn(this: *mut c_void, PCWSTR, *mut DeviceSharingMode) -> HRESULT,
    pub SetShareMode:
        unsafe extern "system" fn(this: *mut c_void, PCWSTR, *mut DeviceSharingMode) -> HRESULT,
    pub GetPropertyValue: unsafe extern "system" fn(
        this: *mut c_void,
        PCWSTR,
        i32,
        *const PROPERTYKEY,
        *mut PROPVARIANT,
    ) -> HRESULT,
    pub SetPropertyValue: unsafe extern "system" fn(
        this: *mut c_void,
        PCWSTR,
        i32,
        *const PROPERTYKEY,
        *mut PROPVARIANT,
    ) -> HRESULT,
    pub SetDefaultEndpoint: unsafe extern "system" fn(this: *mut c_void, PCWSTR, ERole) -> HRESULT,
}
