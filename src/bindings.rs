#[inline]
pub unsafe fn CloseHandle(hobject: HANDLE) -> windows_core::BOOL {
    windows_core::link!("kernel32.dll" "system" fn CloseHandle(hobject : HANDLE) -> windows_core::BOOL);
    unsafe { CloseHandle(hobject) }
}
#[inline]
pub unsafe fn CoCreateInstance<P1, T>(
    rclsid: *const windows_core::GUID,
    punkouter: P1,
    dwclscontext: u32,
) -> windows_core::Result<T>
where
    P1: windows_core::Param<windows_core::IUnknown>,
    T: windows_core::Interface,
{
    windows_core::link!("ole32.dll" "system" fn CoCreateInstance(rclsid : *const windows_core::GUID, punkouter : *mut core::ffi::c_void, dwclscontext : u32, riid : *const windows_core::GUID, ppv : *mut *mut core::ffi::c_void) -> windows_core::HRESULT);
    let mut result__ = core::ptr::null_mut();
    unsafe {
        CoCreateInstance(
            rclsid,
            punkouter.param().abi(),
            dwclscontext,
            &T::IID,
            &mut result__,
        )
        .and_then(|| windows_core::imp::Type::from_abi(result__))
    }
}
#[inline]
pub unsafe fn CoInitializeEx(
    pvreserved: Option<*const core::ffi::c_void>,
    dwcoinit: u32,
) -> windows_core::HRESULT {
    windows_core::link!("ole32.dll" "system" fn CoInitializeEx(pvreserved : *const core::ffi::c_void, dwcoinit : u32) -> windows_core::HRESULT);
    unsafe { CoInitializeEx(pvreserved.unwrap_or(core::mem::zeroed()) as _, dwcoinit) }
}
#[inline]
pub unsafe fn CoTaskMemFree(pv: *mut core::ffi::c_void) {
    windows_core::link!("ole32.dll" "system" fn CoTaskMemFree(pv : *mut core::ffi::c_void));
    unsafe { CoTaskMemFree(pv as _) }
}
#[inline]
pub unsafe fn CreateMutexW<P2>(
    lpmutexattributes: Option<*const SECURITY_ATTRIBUTES>,
    binitialowner: bool,
    lpname: P2,
) -> HANDLE
where
    P2: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("kernel32.dll" "system" fn CreateMutexW(lpmutexattributes : *const SECURITY_ATTRIBUTES, binitialowner : windows_core::BOOL, lpname : windows_core::PCWSTR) -> HANDLE);
    unsafe {
        CreateMutexW(
            lpmutexattributes.unwrap_or(core::mem::zeroed()) as _,
            binitialowner.into(),
            lpname.param().abi(),
        )
    }
}
#[inline]
pub unsafe fn GetLastError() -> u32 {
    windows_core::link!("kernel32.dll" "system" fn GetLastError() -> u32);
    unsafe { GetLastError() }
}
#[inline]
pub unsafe fn PropVariantClear(pvar: *mut PROPVARIANT) -> windows_core::HRESULT {
    windows_core::link!("ole32.dll" "system" fn PropVariantClear(pvar : *mut PROPVARIANT) -> windows_core::HRESULT);
    unsafe { PropVariantClear(pvar) }
}
#[inline]
pub unsafe fn PropVariantToStringAlloc(
    propvar: *const PROPVARIANT,
) -> windows_core::Result<windows_core::PWSTR> {
    windows_core::link!("propsys.dll" "system" fn PropVariantToStringAlloc(propvar : *const PROPVARIANT, ppszout : *mut windows_core::PWSTR) -> windows_core::HRESULT);
    unsafe {
        let mut result__ = core::mem::zeroed();
        PropVariantToStringAlloc(propvar, &mut result__).map(|| result__)
    }
}
#[inline]
pub unsafe fn SetCurrentProcessExplicitAppUserModelID<P0>(appid: P0) -> windows_core::HRESULT
where
    P0: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("shell32.dll" "system" fn SetCurrentProcessExplicitAppUserModelID(appid : windows_core::PCWSTR) -> windows_core::HRESULT);
    unsafe { SetCurrentProcessExplicitAppUserModelID(appid.param().abi()) }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AUDIO_VOLUME_NOTIFICATION_DATA {
    pub guidEventContext: windows_core::GUID,
    pub bMuted: windows_core::BOOL,
    pub fMasterVolume: f32,
    pub nChannels: u32,
    pub afChannelVolumes: [f32; 1],
}
impl Default for AUDIO_VOLUME_NOTIFICATION_DATA {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BLOB {
    pub cbSize: u32,
    pub pBlobData: *mut u8,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BSTRBLOB {
    pub cbSize: u32,
    pub pData: *mut u8,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CABOOL {
    pub cElems: u32,
    pub pElems: *mut VARIANT_BOOL,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CABSTR {
    pub cElems: u32,
    pub pElems: *mut windows_core::BSTR,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CABSTRBLOB {
    pub cElems: u32,
    pub pElems: *mut BSTRBLOB,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CAC {
    pub cElems: u32,
    pub pElems: *mut i8,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CACLIPDATA {
    pub cElems: u32,
    pub pElems: *mut CLIPDATA,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CACLSID {
    pub cElems: u32,
    pub pElems: *mut windows_core::GUID,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CACY {
    pub cElems: u32,
    pub pElems: *mut CY,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CADATE {
    pub cElems: u32,
    pub pElems: *mut f64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CADBL {
    pub cElems: u32,
    pub pElems: *mut f64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CAFILETIME {
    pub cElems: u32,
    pub pElems: *mut FILETIME,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CAFLT {
    pub cElems: u32,
    pub pElems: *mut f32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CAH {
    pub cElems: u32,
    pub pElems: *mut i64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CAI {
    pub cElems: u32,
    pub pElems: *mut i16,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CAL {
    pub cElems: u32,
    pub pElems: *mut i32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CALPSTR {
    pub cElems: u32,
    pub pElems: *mut windows_core::PSTR,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CALPWSTR {
    pub cElems: u32,
    pub pElems: *mut windows_core::PWSTR,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CAPROPVARIANT {
    pub cElems: u32,
    pub pElems: *mut PROPVARIANT,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CASCODE {
    pub cElems: u32,
    pub pElems: *mut SCODE,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CAUB {
    pub cElems: u32,
    pub pElems: *mut u8,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CAUH {
    pub cElems: u32,
    pub pElems: *mut u64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CAUI {
    pub cElems: u32,
    pub pElems: *mut u16,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CAUL {
    pub cElems: u32,
    pub pElems: *mut u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CLIPDATA {
    pub cbSize: u32,
    pub ulClipFmt: i32,
    pub pClipData: *mut u8,
}
pub type CLSCTX = u32;
pub const CLSCTX_ALL: i32 = 23;
pub const CLSCTX_INPROC_SERVER: CLSCTX = 1;
#[repr(C)]
#[derive(Clone, Copy)]
pub union CY {
    pub Anonymous: CY_0,
    pub int64: i64,
}
impl Default for CY {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CY_0 {
    pub Lo: u32,
    pub Hi: i32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DECIMAL {
    pub wReserved: u16,
    pub Anonymous: DECIMAL_0,
    pub Hi32: u32,
    pub Anonymous2: DECIMAL_1,
}
impl Default for DECIMAL {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union DECIMAL_0 {
    pub Anonymous: DECIMAL_0_0,
    pub signscale: u16,
}
impl Default for DECIMAL_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DECIMAL_0_0 {
    pub scale: u8,
    pub sign: u8,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union DECIMAL_1 {
    pub Anonymous: DECIMAL_1_0,
    pub Lo64: u64,
}
impl Default for DECIMAL_1 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DECIMAL_1_0 {
    pub Lo32: u32,
    pub Mid32: u32,
}
pub const DEVICE_STATE_ACTIVE: i32 = 1;
pub const DEVICE_STATE_DISABLED: i32 = 2;
pub const DEVICE_STATE_NOTPRESENT: i32 = 4;
pub const DEVICE_STATE_UNPLUGGED: i32 = 8;
pub type EDataFlow = i32;
pub const EDataFlow_enum_count: EDataFlow = 3;
pub const ERROR_ALREADY_EXISTS: i32 = 183;
pub type ERole = i32;
pub const ERole_enum_count: ERole = 3;
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FILETIME {
    pub dwLowDateTime: u32,
    pub dwHighDateTime: u32,
}
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct HANDLE(pub *mut core::ffi::c_void);
windows_core::imp::define_interface!(
    IAudioEndpointVolume,
    IAudioEndpointVolume_Vtbl,
    0x5cdf2c82_841e_4546_9722_0cf74078229a
);
windows_core::imp::interface_hierarchy!(IAudioEndpointVolume, windows_core::IUnknown);
impl IAudioEndpointVolume {
    pub unsafe fn RegisterControlChangeNotify<P0>(&self, pnotify: P0) -> windows_core::HRESULT
    where
        P0: windows_core::Param<IAudioEndpointVolumeCallback>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).RegisterControlChangeNotify)(
                windows_core::Interface::as_raw(self),
                pnotify.param().abi(),
            )
        }
    }
    pub unsafe fn UnregisterControlChangeNotify<P0>(&self, pnotify: P0) -> windows_core::HRESULT
    where
        P0: windows_core::Param<IAudioEndpointVolumeCallback>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).UnregisterControlChangeNotify)(
                windows_core::Interface::as_raw(self),
                pnotify.param().abi(),
            )
        }
    }
    pub unsafe fn GetChannelCount(&self) -> windows_core::Result<u32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetChannelCount)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn SetMasterVolumeLevel(
        &self,
        fleveldb: f32,
        pguideventcontext: *const windows_core::GUID,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).SetMasterVolumeLevel)(
                windows_core::Interface::as_raw(self),
                fleveldb,
                pguideventcontext,
            )
        }
    }
    pub unsafe fn SetMasterVolumeLevelScalar(
        &self,
        flevel: f32,
        pguideventcontext: *const windows_core::GUID,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).SetMasterVolumeLevelScalar)(
                windows_core::Interface::as_raw(self),
                flevel,
                pguideventcontext,
            )
        }
    }
    pub unsafe fn GetMasterVolumeLevel(&self) -> windows_core::Result<f32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetMasterVolumeLevel)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn GetMasterVolumeLevelScalar(&self) -> windows_core::Result<f32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetMasterVolumeLevelScalar)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn SetChannelVolumeLevel(
        &self,
        nchannel: u32,
        fleveldb: f32,
        pguideventcontext: *const windows_core::GUID,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).SetChannelVolumeLevel)(
                windows_core::Interface::as_raw(self),
                nchannel,
                fleveldb,
                pguideventcontext,
            )
        }
    }
    pub unsafe fn SetChannelVolumeLevelScalar(
        &self,
        nchannel: u32,
        flevel: f32,
        pguideventcontext: *const windows_core::GUID,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).SetChannelVolumeLevelScalar)(
                windows_core::Interface::as_raw(self),
                nchannel,
                flevel,
                pguideventcontext,
            )
        }
    }
    pub unsafe fn GetChannelVolumeLevel(&self, nchannel: u32) -> windows_core::Result<f32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetChannelVolumeLevel)(
                windows_core::Interface::as_raw(self),
                nchannel,
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn GetChannelVolumeLevelScalar(&self, nchannel: u32) -> windows_core::Result<f32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetChannelVolumeLevelScalar)(
                windows_core::Interface::as_raw(self),
                nchannel,
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn SetMute(
        &self,
        bmute: bool,
        pguideventcontext: *const windows_core::GUID,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).SetMute)(
                windows_core::Interface::as_raw(self),
                bmute.into(),
                pguideventcontext,
            )
        }
    }
    pub unsafe fn GetMute(&self) -> windows_core::Result<windows_core::BOOL> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetMute)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn GetVolumeStepInfo(
        &self,
        pnstep: *mut u32,
        pnstepcount: *mut u32,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).GetVolumeStepInfo)(
                windows_core::Interface::as_raw(self),
                pnstep as _,
                pnstepcount as _,
            )
        }
    }
    pub unsafe fn VolumeStepUp(
        &self,
        pguideventcontext: *const windows_core::GUID,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).VolumeStepUp)(
                windows_core::Interface::as_raw(self),
                pguideventcontext,
            )
        }
    }
    pub unsafe fn VolumeStepDown(
        &self,
        pguideventcontext: *const windows_core::GUID,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).VolumeStepDown)(
                windows_core::Interface::as_raw(self),
                pguideventcontext,
            )
        }
    }
    pub unsafe fn QueryHardwareSupport(&self) -> windows_core::Result<u32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).QueryHardwareSupport)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn GetVolumeRange(
        &self,
        pflvolumemindb: *mut f32,
        pflvolumemaxdb: *mut f32,
        pflvolumeincrementdb: *mut f32,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).GetVolumeRange)(
                windows_core::Interface::as_raw(self),
                pflvolumemindb as _,
                pflvolumemaxdb as _,
                pflvolumeincrementdb as _,
            )
        }
    }
}
#[repr(C)]
pub struct IAudioEndpointVolume_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    pub RegisterControlChangeNotify: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub UnregisterControlChangeNotify: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetChannelCount:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut u32) -> windows_core::HRESULT,
    pub SetMasterVolumeLevel: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        f32,
        *const windows_core::GUID,
    ) -> windows_core::HRESULT,
    pub SetMasterVolumeLevelScalar: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        f32,
        *const windows_core::GUID,
    ) -> windows_core::HRESULT,
    pub GetMasterVolumeLevel:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut f32) -> windows_core::HRESULT,
    pub GetMasterVolumeLevelScalar:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut f32) -> windows_core::HRESULT,
    pub SetChannelVolumeLevel: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        u32,
        f32,
        *const windows_core::GUID,
    ) -> windows_core::HRESULT,
    pub SetChannelVolumeLevelScalar: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        u32,
        f32,
        *const windows_core::GUID,
    ) -> windows_core::HRESULT,
    pub GetChannelVolumeLevel:
        unsafe extern "system" fn(*mut core::ffi::c_void, u32, *mut f32) -> windows_core::HRESULT,
    pub GetChannelVolumeLevelScalar:
        unsafe extern "system" fn(*mut core::ffi::c_void, u32, *mut f32) -> windows_core::HRESULT,
    pub SetMute: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::BOOL,
        *const windows_core::GUID,
    ) -> windows_core::HRESULT,
    pub GetMute: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::BOOL,
    ) -> windows_core::HRESULT,
    pub GetVolumeStepInfo: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut u32,
        *mut u32,
    ) -> windows_core::HRESULT,
    pub VolumeStepUp: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const windows_core::GUID,
    ) -> windows_core::HRESULT,
    pub VolumeStepDown: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const windows_core::GUID,
    ) -> windows_core::HRESULT,
    pub QueryHardwareSupport:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut u32) -> windows_core::HRESULT,
    pub GetVolumeRange: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut f32,
        *mut f32,
        *mut f32,
    ) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IAudioEndpointVolumeCallback,
    IAudioEndpointVolumeCallback_Vtbl,
    0x657804fa_d6ad_4496_8a60_352752af4f89
);
windows_core::imp::interface_hierarchy!(IAudioEndpointVolumeCallback, windows_core::IUnknown);
impl IAudioEndpointVolumeCallback {
    pub unsafe fn OnNotify(
        &self,
        pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).OnNotify)(
                windows_core::Interface::as_raw(self),
                pnotify as _,
            )
        }
    }
}
#[repr(C)]
pub struct IAudioEndpointVolumeCallback_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    pub OnNotify: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut AUDIO_VOLUME_NOTIFICATION_DATA,
    ) -> windows_core::HRESULT,
}
pub trait IAudioEndpointVolumeCallback_Impl: windows_core::IUnknownImpl {
    fn OnNotify(&self, pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows_core::Result<()>;
}
impl IAudioEndpointVolumeCallback_Vtbl {
    pub const fn new<Identity: IAudioEndpointVolumeCallback_Impl, const OFFSET: isize>() -> Self {
        unsafe extern "system" fn OnNotify<
            Identity: IAudioEndpointVolumeCallback_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IAudioEndpointVolumeCallback_Impl::OnNotify(
                    this,
                    core::mem::transmute_copy(&pnotify),
                )
                .into()
            }
        }
        Self {
            base__: windows_core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
            OnNotify: OnNotify::<Identity, OFFSET>,
        }
    }
    pub fn matches(iid: &windows_core::GUID) -> bool {
        iid == &<IAudioEndpointVolumeCallback as windows_core::Interface>::IID
    }
}
impl windows_core::RuntimeName for IAudioEndpointVolumeCallback {}
windows_core::imp::define_interface!(
    IDispatch,
    IDispatch_Vtbl,
    0x00020400_0000_0000_c000_000000000046
);
windows_core::imp::interface_hierarchy!(IDispatch, windows_core::IUnknown);
#[repr(C)]
pub struct IDispatch_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    GetTypeInfoCount: usize,
    GetTypeInfo: usize,
    GetIDsOfNames: usize,
    Invoke: usize,
}
windows_core::imp::define_interface!(
    IMMDevice,
    IMMDevice_Vtbl,
    0xd666063f_1587_4e43_81f1_b948e807363f
);
windows_core::imp::interface_hierarchy!(IMMDevice, windows_core::IUnknown);
impl IMMDevice {
    pub unsafe fn Activate<T>(
        &self,
        dwclsctx: u32,
        pactivationparams: Option<*const PROPVARIANT>,
    ) -> windows_core::Result<T>
    where
        T: windows_core::Interface,
    {
        let mut result__ = core::ptr::null_mut();
        unsafe {
            (windows_core::Interface::vtable(self).Activate)(
                windows_core::Interface::as_raw(self),
                &T::IID,
                dwclsctx,
                pactivationparams.unwrap_or(core::mem::zeroed()) as _,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn OpenPropertyStore(
        &self,
        stgmaccess: u32,
    ) -> windows_core::Result<IPropertyStore> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).OpenPropertyStore)(
                windows_core::Interface::as_raw(self),
                stgmaccess,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn GetId(&self) -> windows_core::Result<windows_core::PWSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetId)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn GetState(&self) -> windows_core::Result<u32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetState)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
}
#[repr(C)]
pub struct IMMDevice_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    pub Activate: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const windows_core::GUID,
        u32,
        *const PROPVARIANT,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub OpenPropertyStore: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        u32,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetId: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::PWSTR,
    ) -> windows_core::HRESULT,
    pub GetState:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut u32) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IMMDeviceCollection,
    IMMDeviceCollection_Vtbl,
    0x0bd7a1be_7a1a_44db_8397_cc5392387b5e
);
windows_core::imp::interface_hierarchy!(IMMDeviceCollection, windows_core::IUnknown);
impl IMMDeviceCollection {
    pub unsafe fn GetCount(&self) -> windows_core::Result<u32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetCount)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn Item(&self, ndevice: u32) -> windows_core::Result<IMMDevice> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).Item)(
                windows_core::Interface::as_raw(self),
                ndevice,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
}
#[repr(C)]
pub struct IMMDeviceCollection_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    pub GetCount:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut u32) -> windows_core::HRESULT,
    pub Item: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        u32,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IMMDeviceEnumerator,
    IMMDeviceEnumerator_Vtbl,
    0xa95664d2_9614_4f35_a746_de8db63617e6
);
windows_core::imp::interface_hierarchy!(IMMDeviceEnumerator, windows_core::IUnknown);
impl IMMDeviceEnumerator {
    pub unsafe fn EnumAudioEndpoints(
        &self,
        dataflow: EDataFlow,
        dwstatemask: u32,
    ) -> windows_core::Result<IMMDeviceCollection> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).EnumAudioEndpoints)(
                windows_core::Interface::as_raw(self),
                dataflow,
                dwstatemask,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn GetDefaultAudioEndpoint(
        &self,
        dataflow: EDataFlow,
        role: ERole,
    ) -> windows_core::Result<IMMDevice> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetDefaultAudioEndpoint)(
                windows_core::Interface::as_raw(self),
                dataflow,
                role,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn GetDevice<P0>(&self, pwstrid: P0) -> windows_core::Result<IMMDevice>
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetDevice)(
                windows_core::Interface::as_raw(self),
                pwstrid.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn RegisterEndpointNotificationCallback<P0>(
        &self,
        pclient: P0,
    ) -> windows_core::HRESULT
    where
        P0: windows_core::Param<IMMNotificationClient>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).RegisterEndpointNotificationCallback)(
                windows_core::Interface::as_raw(self),
                pclient.param().abi(),
            )
        }
    }
    pub unsafe fn UnregisterEndpointNotificationCallback<P0>(
        &self,
        pclient: P0,
    ) -> windows_core::HRESULT
    where
        P0: windows_core::Param<IMMNotificationClient>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).UnregisterEndpointNotificationCallback)(
                windows_core::Interface::as_raw(self),
                pclient.param().abi(),
            )
        }
    }
}
#[repr(C)]
pub struct IMMDeviceEnumerator_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    pub EnumAudioEndpoints: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        EDataFlow,
        u32,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetDefaultAudioEndpoint: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        EDataFlow,
        ERole,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetDevice: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub RegisterEndpointNotificationCallback: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    )
        -> windows_core::HRESULT,
    pub UnregisterEndpointNotificationCallback: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    )
        -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IMMNotificationClient,
    IMMNotificationClient_Vtbl,
    0x7991eec9_7e89_4d85_8390_6c703cec60c0
);
windows_core::imp::interface_hierarchy!(IMMNotificationClient, windows_core::IUnknown);
impl IMMNotificationClient {
    pub unsafe fn OnDeviceStateChanged<P0>(
        &self,
        pwstrdeviceid: P0,
        dwnewstate: u32,
    ) -> windows_core::HRESULT
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).OnDeviceStateChanged)(
                windows_core::Interface::as_raw(self),
                pwstrdeviceid.param().abi(),
                dwnewstate,
            )
        }
    }
    pub unsafe fn OnDeviceAdded<P0>(&self, pwstrdeviceid: P0) -> windows_core::HRESULT
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).OnDeviceAdded)(
                windows_core::Interface::as_raw(self),
                pwstrdeviceid.param().abi(),
            )
        }
    }
    pub unsafe fn OnDeviceRemoved<P0>(&self, pwstrdeviceid: P0) -> windows_core::HRESULT
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).OnDeviceRemoved)(
                windows_core::Interface::as_raw(self),
                pwstrdeviceid.param().abi(),
            )
        }
    }
    pub unsafe fn OnDefaultDeviceChanged<P2>(
        &self,
        flow: EDataFlow,
        role: ERole,
        pwstrdefaultdeviceid: P2,
    ) -> windows_core::HRESULT
    where
        P2: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).OnDefaultDeviceChanged)(
                windows_core::Interface::as_raw(self),
                flow,
                role,
                pwstrdefaultdeviceid.param().abi(),
            )
        }
    }
    pub unsafe fn OnPropertyValueChanged<P0>(
        &self,
        pwstrdeviceid: P0,
        key: PROPERTYKEY,
    ) -> windows_core::HRESULT
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).OnPropertyValueChanged)(
                windows_core::Interface::as_raw(self),
                pwstrdeviceid.param().abi(),
                key,
            )
        }
    }
}
#[repr(C)]
pub struct IMMNotificationClient_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    pub OnDeviceStateChanged: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
        u32,
    ) -> windows_core::HRESULT,
    pub OnDeviceAdded: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
    ) -> windows_core::HRESULT,
    pub OnDeviceRemoved: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
    ) -> windows_core::HRESULT,
    pub OnDefaultDeviceChanged: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        EDataFlow,
        ERole,
        windows_core::PCWSTR,
    ) -> windows_core::HRESULT,
    pub OnPropertyValueChanged: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
        PROPERTYKEY,
    ) -> windows_core::HRESULT,
}
pub trait IMMNotificationClient_Impl: windows_core::IUnknownImpl {
    fn OnDeviceStateChanged(
        &self,
        pwstrdeviceid: &windows_core::PCWSTR,
        dwnewstate: u32,
    ) -> windows_core::Result<()>;
    fn OnDeviceAdded(&self, pwstrdeviceid: &windows_core::PCWSTR) -> windows_core::Result<()>;
    fn OnDeviceRemoved(&self, pwstrdeviceid: &windows_core::PCWSTR) -> windows_core::Result<()>;
    fn OnDefaultDeviceChanged(
        &self,
        flow: EDataFlow,
        role: ERole,
        pwstrdefaultdeviceid: &windows_core::PCWSTR,
    ) -> windows_core::Result<()>;
    fn OnPropertyValueChanged(
        &self,
        pwstrdeviceid: &windows_core::PCWSTR,
        key: &PROPERTYKEY,
    ) -> windows_core::Result<()>;
}
impl IMMNotificationClient_Vtbl {
    pub const fn new<Identity: IMMNotificationClient_Impl, const OFFSET: isize>() -> Self {
        unsafe extern "system" fn OnDeviceStateChanged<
            Identity: IMMNotificationClient_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            pwstrdeviceid: windows_core::PCWSTR,
            dwnewstate: u32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IMMNotificationClient_Impl::OnDeviceStateChanged(
                    this,
                    core::mem::transmute(&pwstrdeviceid),
                    core::mem::transmute_copy(&dwnewstate),
                )
                .into()
            }
        }
        unsafe extern "system" fn OnDeviceAdded<
            Identity: IMMNotificationClient_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            pwstrdeviceid: windows_core::PCWSTR,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IMMNotificationClient_Impl::OnDeviceAdded(
                    this,
                    core::mem::transmute(&pwstrdeviceid),
                )
                .into()
            }
        }
        unsafe extern "system" fn OnDeviceRemoved<
            Identity: IMMNotificationClient_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            pwstrdeviceid: windows_core::PCWSTR,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IMMNotificationClient_Impl::OnDeviceRemoved(
                    this,
                    core::mem::transmute(&pwstrdeviceid),
                )
                .into()
            }
        }
        unsafe extern "system" fn OnDefaultDeviceChanged<
            Identity: IMMNotificationClient_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            flow: EDataFlow,
            role: ERole,
            pwstrdefaultdeviceid: windows_core::PCWSTR,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IMMNotificationClient_Impl::OnDefaultDeviceChanged(
                    this,
                    core::mem::transmute_copy(&flow),
                    core::mem::transmute_copy(&role),
                    core::mem::transmute(&pwstrdefaultdeviceid),
                )
                .into()
            }
        }
        unsafe extern "system" fn OnPropertyValueChanged<
            Identity: IMMNotificationClient_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            pwstrdeviceid: windows_core::PCWSTR,
            key: PROPERTYKEY,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IMMNotificationClient_Impl::OnPropertyValueChanged(
                    this,
                    core::mem::transmute(&pwstrdeviceid),
                    core::mem::transmute(&key),
                )
                .into()
            }
        }
        Self {
            base__: windows_core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
            OnDeviceStateChanged: OnDeviceStateChanged::<Identity, OFFSET>,
            OnDeviceAdded: OnDeviceAdded::<Identity, OFFSET>,
            OnDeviceRemoved: OnDeviceRemoved::<Identity, OFFSET>,
            OnDefaultDeviceChanged: OnDefaultDeviceChanged::<Identity, OFFSET>,
            OnPropertyValueChanged: OnPropertyValueChanged::<Identity, OFFSET>,
        }
    }
    pub fn matches(iid: &windows_core::GUID) -> bool {
        iid == &<IMMNotificationClient as windows_core::Interface>::IID
    }
}
impl windows_core::RuntimeName for IMMNotificationClient {}
windows_core::imp::define_interface!(
    IPropertyStore,
    IPropertyStore_Vtbl,
    0x886d8eeb_8cf2_4446_8d02_cdba1dbdcf99
);
windows_core::imp::interface_hierarchy!(IPropertyStore, windows_core::IUnknown);
impl IPropertyStore {
    pub unsafe fn GetCount(&self) -> windows_core::Result<u32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetCount)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn GetAt(&self, iprop: u32, pkey: *mut PROPERTYKEY) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).GetAt)(
                windows_core::Interface::as_raw(self),
                iprop,
                pkey as _,
            )
        }
    }
    pub unsafe fn GetValue(&self, key: *const PROPERTYKEY) -> windows_core::Result<PROPVARIANT> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetValue)(
                windows_core::Interface::as_raw(self),
                key,
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn SetValue(
        &self,
        key: *const PROPERTYKEY,
        propvar: *const PROPVARIANT,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).SetValue)(
                windows_core::Interface::as_raw(self),
                key,
                propvar,
            )
        }
    }
    pub unsafe fn Commit(&self) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).Commit)(windows_core::Interface::as_raw(self))
        }
    }
}
#[repr(C)]
pub struct IPropertyStore_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    pub GetCount:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut u32) -> windows_core::HRESULT,
    pub GetAt: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        u32,
        *mut PROPERTYKEY,
    ) -> windows_core::HRESULT,
    pub GetValue: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const PROPERTYKEY,
        *mut PROPVARIANT,
    ) -> windows_core::HRESULT,
    pub SetValue: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const PROPERTYKEY,
        *const PROPVARIANT,
    ) -> windows_core::HRESULT,
    pub Commit: unsafe extern "system" fn(*mut core::ffi::c_void) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    ISequentialStream,
    ISequentialStream_Vtbl,
    0x0c733a30_2a1c_11ce_ade5_00aa0044773d
);
windows_core::imp::interface_hierarchy!(ISequentialStream, windows_core::IUnknown);
#[repr(C)]
pub struct ISequentialStream_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    Read: usize,
    Write: usize,
}
windows_core::imp::define_interface!(
    IStorage,
    IStorage_Vtbl,
    0x0000000b_0000_0000_c000_000000000046
);
windows_core::imp::interface_hierarchy!(IStorage, windows_core::IUnknown);
#[repr(C)]
pub struct IStorage_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    CreateStream: usize,
    OpenStream: usize,
    CreateStorage: usize,
    OpenStorage: usize,
    CopyTo: usize,
    MoveElementTo: usize,
    Commit: usize,
    Revert: usize,
    EnumElements: usize,
    DestroyElement: usize,
    RenameElement: usize,
    SetElementTimes: usize,
    SetClass: usize,
    SetStateBits: usize,
    Stat: usize,
}
windows_core::imp::define_interface!(
    IStream,
    IStream_Vtbl,
    0x0000000c_0000_0000_c000_000000000046
);
impl core::ops::Deref for IStream {
    type Target = ISequentialStream;
    fn deref(&self) -> &Self::Target {
        unsafe { core::mem::transmute(self) }
    }
}
windows_core::imp::interface_hierarchy!(IStream, windows_core::IUnknown, ISequentialStream);
#[repr(C)]
pub struct IStream_Vtbl {
    pub base__: ISequentialStream_Vtbl,
    Seek: usize,
    SetSize: usize,
    CopyTo: usize,
    Commit: usize,
    Revert: usize,
    LockRegion: usize,
    UnlockRegion: usize,
    Stat: usize,
    Clone: usize,
}
pub type LPSAFEARRAY = *mut SAFEARRAY;
pub type LPVERSIONEDSTREAM = *mut VERSIONEDSTREAM;
pub const MMDeviceEnumerator: windows_core::GUID =
    windows_core::GUID::from_u128(0xbcde0395_e52f_467c_8e3d_c4579291692e);
pub const PKEY_Device_FriendlyName: PROPERTYKEY = PROPERTYKEY {
    fmtid: windows_core::GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
    pid: 14,
};
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PROPERTYKEY {
    pub fmtid: windows_core::GUID,
    pub pid: u32,
}
#[repr(C)]
pub struct PROPVARIANT {
    pub Anonymous: PROPVARIANT_0,
}
impl Clone for PROPVARIANT {
    fn clone(&self) -> Self {
        unsafe { core::mem::transmute_copy(self) }
    }
}
impl Default for PROPVARIANT {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
pub union PROPVARIANT_0 {
    pub Anonymous: core::mem::ManuallyDrop<PROPVARIANT_0_0>,
    pub decVal: DECIMAL,
}
impl Clone for PROPVARIANT_0 {
    fn clone(&self) -> Self {
        unsafe { core::mem::transmute_copy(self) }
    }
}
impl Default for PROPVARIANT_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
pub struct PROPVARIANT_0_0 {
    pub vt: VARTYPE,
    pub wReserved1: PROPVAR_PAD1,
    pub wReserved2: PROPVAR_PAD2,
    pub wReserved3: PROPVAR_PAD3,
    pub Anonymous: PROPVARIANT_0_0_0,
}
impl Clone for PROPVARIANT_0_0 {
    fn clone(&self) -> Self {
        unsafe { core::mem::transmute_copy(self) }
    }
}
impl Default for PROPVARIANT_0_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
pub union PROPVARIANT_0_0_0 {
    pub cVal: i8,
    pub bVal: u8,
    pub iVal: i16,
    pub uiVal: u16,
    pub lVal: i32,
    pub ulVal: u32,
    pub intVal: i32,
    pub uintVal: u32,
    pub hVal: i64,
    pub uhVal: u64,
    pub fltVal: f32,
    pub dblVal: f64,
    pub boolVal: VARIANT_BOOL,
    pub __OBSOLETE__VARIANT_BOOL: VARIANT_BOOL,
    pub scode: SCODE,
    pub cyVal: CY,
    pub date: f64,
    pub filetime: FILETIME,
    pub puuid: *mut windows_core::GUID,
    pub pclipdata: *mut CLIPDATA,
    pub bstrVal: core::mem::ManuallyDrop<windows_core::BSTR>,
    pub bstrblobVal: BSTRBLOB,
    pub blob: BLOB,
    pub pszVal: windows_core::PSTR,
    pub pwszVal: windows_core::PWSTR,
    pub punkVal: core::mem::ManuallyDrop<Option<windows_core::IUnknown>>,
    pub pdispVal: core::mem::ManuallyDrop<Option<IDispatch>>,
    pub pStream: core::mem::ManuallyDrop<Option<IStream>>,
    pub pStorage: core::mem::ManuallyDrop<Option<IStorage>>,
    pub pVersionedStream: LPVERSIONEDSTREAM,
    pub parray: LPSAFEARRAY,
    pub cac: CAC,
    pub caub: CAUB,
    pub cai: CAI,
    pub caui: CAUI,
    pub cal: CAL,
    pub caul: CAUL,
    pub cah: CAH,
    pub cauh: CAUH,
    pub caflt: CAFLT,
    pub cadbl: CADBL,
    pub cabool: CABOOL,
    pub cascode: CASCODE,
    pub cacy: CACY,
    pub cadate: CADATE,
    pub cafiletime: CAFILETIME,
    pub cauuid: CACLSID,
    pub caclipdata: CACLIPDATA,
    pub cabstr: CABSTR,
    pub cabstrblob: CABSTRBLOB,
    pub calpstr: CALPSTR,
    pub calpwstr: CALPWSTR,
    pub capropvar: CAPROPVARIANT,
    pub pcVal: *mut i8,
    pub pbVal: *mut u8,
    pub piVal: *mut i16,
    pub puiVal: *mut u16,
    pub plVal: *mut i32,
    pub pulVal: *mut u32,
    pub pintVal: *mut i32,
    pub puintVal: *mut u32,
    pub pfltVal: *mut f32,
    pub pdblVal: *mut f64,
    pub pboolVal: *mut VARIANT_BOOL,
    pub pdecVal: *mut DECIMAL,
    pub pscode: *mut SCODE,
    pub pcyVal: *mut CY,
    pub pdate: *mut f64,
    pub pbstrVal: *mut windows_core::BSTR,
    pub ppunkVal: *mut Option<windows_core::IUnknown>,
    pub ppdispVal: *mut Option<IDispatch>,
    pub pparray: *mut LPSAFEARRAY,
    pub pvarVal: *mut PROPVARIANT,
}
impl Clone for PROPVARIANT_0_0_0 {
    fn clone(&self) -> Self {
        unsafe { core::mem::transmute_copy(self) }
    }
}
impl Default for PROPVARIANT_0_0_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PROPVAR_PAD1(pub u16);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PROPVAR_PAD2(pub u16);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PROPVAR_PAD3(pub u16);
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SAFEARRAY {
    pub cDims: u16,
    pub fFeatures: u16,
    pub cbElements: u32,
    pub cLocks: u32,
    pub pvData: *mut core::ffi::c_void,
    pub rgsabound: [SAFEARRAYBOUND; 1],
}
impl Default for SAFEARRAY {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SAFEARRAYBOUND {
    pub cElements: u32,
    pub lLbound: i32,
}
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SCODE(pub i32);
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SECURITY_ATTRIBUTES {
    pub nLength: u32,
    pub lpSecurityDescriptor: *mut core::ffi::c_void,
    pub bInheritHandle: windows_core::BOOL,
}
pub const STGM_READ: i32 = 0;
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct VARIANT_BOOL(pub i16);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct VARTYPE(pub u16);
#[repr(C)]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VERSIONEDSTREAM {
    pub guidVersion: windows_core::GUID,
    pub pStream: core::mem::ManuallyDrop<Option<IStream>>,
}
#[repr(C, packed(1))]
#[derive(Clone, Copy, Default)]
pub struct WAVEFORMATEX {
    pub wFormatTag: u16,
    pub nChannels: u16,
    pub nSamplesPerSec: u32,
    pub nAvgBytesPerSec: u32,
    pub nBlockAlign: u16,
    pub wBitsPerSample: u16,
    pub cbSize: u16,
}
pub const eAll: EDataFlow = 2;
pub const eCapture: EDataFlow = 1;
pub const eCommunications: ERole = 2;
pub const eConsole: ERole = 0;
pub const eMultimedia: ERole = 1;
pub const eRender: EDataFlow = 0;
