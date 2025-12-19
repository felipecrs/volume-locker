use std::ffi::c_void;
use windows::Win32::Media::Audio::{EDataFlow, ERole, eRender};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
use windows::core::{GUID, HRESULT, HSTRING, IInspectable, Interface, PCSTR, Result};

// Known GUIDs for IAudioPolicyConfig
const IIDS: &[GUID] = &[
    GUID::from_u128(0x2a59116d_6c4f_45e0_a74f_707e3fef9258), // Pre-H1H2
    GUID::from_u128(0xab3d4648_e242_459f_b02f_541c70306324), // H1H2/Win11
    GUID::from_u128(0x32aa8e18_6496_4e24_9f94_b800e7eccc45), // Windows 10.0.16299
];

const DEVINTERFACE_AUDIO_RENDER: &str = "#{e6327cad-dcec-4949-ae8a-991e976a79d2}";
const DEVINTERFACE_AUDIO_CAPTURE: &str = "#{2eef81be-33fa-4800-9670-1cd474972c3f}";
const MMDEVAPI_TOKEN: &str = r"\\?\SWD#MMDEVAPI#";

fn generate_device_id(device_id: &str, flow: EDataFlow) -> String {
    let suffix = if flow == eRender {
        DEVINTERFACE_AUDIO_RENDER
    } else {
        DEVINTERFACE_AUDIO_CAPTURE
    };
    format!("{}{}{}", MMDEVAPI_TOKEN, device_id, suffix)
}

fn unpack_device_id(device_id: &str) -> String {
    let mut id = device_id.to_string();
    if id.starts_with(MMDEVAPI_TOKEN) {
        id = id[MMDEVAPI_TOKEN.len()..].to_string();
    }
    if id.ends_with(DEVINTERFACE_AUDIO_RENDER) {
        id = id[..id.len() - DEVINTERFACE_AUDIO_RENDER.len()].to_string();
    } else if id.ends_with(DEVINTERFACE_AUDIO_CAPTURE) {
        id = id[..id.len() - DEVINTERFACE_AUDIO_CAPTURE.len()].to_string();
    }
    id
}

type DllGetActivationFactory = unsafe extern "system" fn(
    activatable_class_id: *mut c_void, // HSTRING
    factory: *mut *mut c_void,
) -> HRESULT;

pub struct AudioPolicyConfig {
    interface: IInspectable,
    vtable: *mut *mut c_void,
}

impl AudioPolicyConfig {
    pub fn new() -> Result<Self> {
        unsafe {
            let lib_name = "AudioSes.dll\0";
            let hmodule = LoadLibraryA(PCSTR(lib_name.as_ptr()))?;
            if hmodule.is_invalid() {
                return Err(windows::core::Error::from_win32());
            }

            let func_name = "DllGetActivationFactory\0";
            let func_ptr = GetProcAddress(hmodule, PCSTR(func_name.as_ptr()));
            let dll_get_activation_factory: DllGetActivationFactory =
                std::mem::transmute(func_ptr.ok_or(windows::core::Error::from_win32())?);

            let class_id = HSTRING::from("Windows.Media.Internal.AudioPolicyConfig");
            let mut factory_ptr: *mut c_void = std::ptr::null_mut();

            // HSTRING is ABI compatible with *mut c_void in this context
            (dll_get_activation_factory)(
                std::mem::transmute::<windows_core::HSTRING, *mut std::ffi::c_void>(class_id),
                &mut factory_ptr,
            )
            .ok()?;

            let factory: IInspectable = std::mem::transmute(factory_ptr);

            // Now we need to find the correct IID.
            for iid in IIDS {
                let mut result_ptr: *mut c_void = std::ptr::null_mut();
                if (factory.vtable().base.QueryInterface)(factory.as_raw(), iid, &mut result_ptr)
                    .is_ok()
                {
                    let interface: IInspectable = std::mem::transmute(result_ptr);
                    let vtable = *(interface.as_raw() as *mut *mut *mut c_void);
                    return Ok(Self { interface, vtable });
                }
            }

            Err(windows::core::Error::new(
                HRESULT(0x80004002u32 as i32),
                "Interface not found",
            ))
        }
    }

    pub fn set_persisted_default_audio_endpoint(
        &self,
        process_id: u32,
        flow: EDataFlow,
        role: ERole,
        device_id: &str,
    ) -> Result<()> {
        unsafe {
            let method_index = 25; // 25th method (0-indexed)
            let method_ptr = *self.vtable.add(method_index);
            let method: unsafe extern "system" fn(
                this: *mut c_void,
                process_id: u32,
                flow: EDataFlow,
                role: ERole,
                device_id: *mut c_void,
            ) -> HRESULT = std::mem::transmute(method_ptr);

            let full_device_id = generate_device_id(device_id, flow);
            let device_id_hstring = HSTRING::from(full_device_id);

            method(
                self.interface.as_raw(),
                process_id,
                flow,
                role,
                std::mem::transmute::<windows_core::HSTRING, *mut std::ffi::c_void>(
                    device_id_hstring,
                ),
            )
            .ok()
        }
    }

    pub fn get_persisted_default_audio_endpoint(
        &self,
        process_id: u32,
        flow: EDataFlow,
        role: ERole,
    ) -> Result<String> {
        unsafe {
            let method_index = 26; // 26th method
            let method_ptr = *self.vtable.add(method_index);
            let method: unsafe extern "system" fn(
                this: *mut c_void,
                process_id: u32,
                flow: EDataFlow,
                role: ERole,
                device_id: *mut *mut c_void,
            ) -> HRESULT = std::mem::transmute(method_ptr);

            let mut device_id_ptr: *mut c_void = std::ptr::null_mut();

            method(
                self.interface.as_raw(),
                process_id,
                flow,
                role,
                &mut device_id_ptr,
            )
            .ok()?;

            let hstring: HSTRING = std::mem::transmute(device_id_ptr);
            let full_id = hstring.to_string();
            Ok(unpack_device_id(&full_id))
        }
    }
}
