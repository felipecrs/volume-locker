#![allow(dead_code)]
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(clippy::missing_safety_doc)]
#![allow(unsafe_op_in_unsafe_fn)]

use std::ffi::c_void;
use std::mem::zeroed;
use windows::Devices::Custom::DeviceSharingMode;
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::{ERole, WAVEFORMATEX};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::core::imp::CanInto;
use windows::core::{BOOL, GUID, HRESULT, IUnknown, Interface, PCWSTR, Param, Result};

pub const PolicyConfigClient: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

#[repr(transparent)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IPolicyConfig(IUnknown);

impl CanInto<IUnknown> for IPolicyConfig {}

impl IPolicyConfig {
    pub unsafe fn GetMixFormat(
        &self,
        device_name: impl Param<PCWSTR>,
    ) -> Result<*mut WAVEFORMATEX> {
        let mut result__ = zeroed::<*mut WAVEFORMATEX>();
        (Interface::vtable(self).GetMixFormat)(
            Interface::as_raw(self),
            device_name.param().abi(),
            &mut result__,
        )
        .map(|| result__)
    }

    pub unsafe fn GetDeviceFormat(
        &self,
        device_name: impl Param<PCWSTR>,
        default: impl Into<BOOL>,
    ) -> Result<*mut WAVEFORMATEX> {
        let mut result__ = zeroed::<*mut WAVEFORMATEX>();
        (Interface::vtable(self).GetDeviceFormat)(
            Interface::as_raw(self),
            device_name.param().abi(),
            default.into().0,
            &mut result__,
        )
        .map(|| result__)
    }

    pub unsafe fn ResetDeviceFormat(&self, device_name: impl Param<PCWSTR>) -> Result<()> {
        (Interface::vtable(self).ResetDeviceFormat)(
            Interface::as_raw(self),
            device_name.param().abi(),
        )
        .ok()
    }

    pub unsafe fn SetDeviceFormat(
        &self,
        device_name: impl Param<PCWSTR>,
        mut endpoint_format: WAVEFORMATEX,
        mut mix_format: WAVEFORMATEX,
    ) -> Result<()> {
        (Interface::vtable(self).SetDeviceFormat)(
            Interface::as_raw(self),
            device_name.param().abi(),
            &mut endpoint_format,
            &mut mix_format,
        )
        .ok()
    }

    pub unsafe fn GetProcessingPeriod(
        &self,
        device_name: impl Param<PCWSTR>,
        default: impl Into<BOOL>,
        default_period: *mut i64,
        min_period: *mut i64,
    ) -> Result<()> {
        (Interface::vtable(self).GetProcessingPeriod)(
            Interface::as_raw(self),
            device_name.param().abi(),
            default.into().0,
            default_period,
            min_period,
        )
        .ok()
    }

    pub unsafe fn SetProcessingPeriod(
        &self,
        device_name: impl Param<PCWSTR>,
        period: *mut i64,
    ) -> Result<()> {
        (Interface::vtable(self).SetProcessingPeriod)(
            Interface::as_raw(self),
            device_name.param().abi(),
            period,
        )
        .ok()
    }

    pub unsafe fn GetShareMode(
        &self,
        device_name: impl Param<PCWSTR>,
    ) -> Result<DeviceSharingMode> {
        let mut result__ = zeroed::<DeviceSharingMode>();
        (Interface::vtable(self).GetShareMode)(
            Interface::as_raw(self),
            device_name.param().abi(),
            &mut result__,
        )
        .map(|| result__)
    }

    pub unsafe fn SetShareMode(
        &self,
        device_name: impl Param<PCWSTR>,
        mut mode: DeviceSharingMode,
    ) -> Result<()> {
        (Interface::vtable(self).SetShareMode)(
            Interface::as_raw(self),
            device_name.param().abi(),
            &mut mode,
        )
        .ok()
    }

    pub unsafe fn GetPropertyValue(
        &self,
        device_name: impl Param<PCWSTR>,
        bFxStore: impl Into<BOOL>,
        key: *const PROPERTYKEY,
    ) -> Result<PROPVARIANT> {
        let mut result__ = zeroed::<PROPVARIANT>();
        (Interface::vtable(self).GetPropertyValue)(
            Interface::as_raw(self),
            device_name.param().abi(),
            bFxStore.into().0,
            key,
            &mut result__,
        )
        .map(|| result__)
    }

    pub unsafe fn SetPropertyValue(
        &self,
        device_name: impl Param<PCWSTR>,
        bFxStore: impl Into<BOOL>,
        key: *const PROPERTYKEY,
        propvar: *mut PROPVARIANT,
    ) -> Result<()> {
        (Interface::vtable(self).SetPropertyValue)(
            Interface::as_raw(self),
            device_name.param().abi(),
            bFxStore.into().0,
            key,
            propvar,
        )
        .ok()
    }

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

    pub unsafe fn SetEndpointVisibility(
        &self,
        device_name: impl Param<PCWSTR>,
        visible: impl Into<BOOL>,
    ) -> Result<()> {
        (Interface::vtable(self).SetEndpointVisibility)(
            Interface::as_raw(self),
            device_name.param().abi(),
            visible.into().0,
        )
        .ok()
    }
}

unsafe impl Interface for IPolicyConfig {
    type Vtable = IPolicyConfig_Vtbl;
    const IID: GUID = GUID::from_u128(0xf8679f50_850a_41cf_9c72_430f290290c8);
}

#[repr(C)]
#[doc(hidden)]
#[allow(non_camel_case_types)]
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
    pub SetEndpointVisibility: unsafe extern "system" fn(this: *mut c_void, PCWSTR, i32) -> HRESULT,
}
