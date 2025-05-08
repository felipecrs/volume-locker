#![cfg_attr(
    all(target_os = "windows", not(debug_assertions),),
    windows_subsystem = "windows"
)]
#![allow(unused)]

use serde::{Deserialize, Serialize};
use single_instance::SingleInstance;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::{thread, time::Duration};
use tao::{
    event::Event,
    event_loop::{ControlFlow, EventLoopBuilder},
};
use tray_icon::{
    MouseButton, TrayIconBuilder, TrayIconEvent,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem},
};

use auto_launch::AutoLaunchBuilder;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_ContainerId;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    DEVICE_STATE_ACTIVE, IMMDevice, IMMDeviceCollection, IMMDeviceEnumerator, MMDeviceEnumerator,
    eCapture, eConsole, eRender,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, STGM_READ,
};
use windows::core::Result;

const APP_NAME: &str = "Volume Locker";
const APP_UID: &str = "25fc6555-723f-414b-9fa0-b4b658d85b43";
const STATE_FILE_NAME: &str = "VolumeLockerState.json";

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
enum DeviceType {
    Input,
    Output,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeviceLockedInfo {
    volume_percent: f32,
    device_type: DeviceType,
    name: String, // Store device name
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistentState {
    locked_devices: HashMap<String, DeviceLockedInfo>,
}

impl Default for PersistentState {
    fn default() -> Self {
        PersistentState {
            locked_devices: HashMap::new(),
        }
    }
}

#[derive(Debug)]
struct MenuItemDeviceInfo {
    device_id: String,
    volume_percent: f32,
    device_type: DeviceType,
    name: String,
}

enum UserEvent {
    TrayIconEvent(tray_icon::TrayIconEvent),
    MenuEvent(tray_icon::menu::MenuEvent),
    Heartbeat,
}

fn main() {
    // Only allow one instance of the application to run at a time
    let instance = SingleInstance::new(APP_UID).expect("Failed to create single instance");
    if !instance.is_single() {
        println!("Another instance is already running.");
        std::process::exit(1);
    }

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();

    let proxy = event_loop.create_proxy();
    TrayIconEvent::set_event_handler(Some(move |event| {
        proxy.send_event(UserEvent::TrayIconEvent(event));
    }));

    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        proxy.send_event(UserEvent::MenuEvent(event));
    }));

    let proxy = event_loop.create_proxy();
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(5));
            proxy.send_event(UserEvent::Heartbeat);
        }
    });

    let device_enumerator: IMMDeviceEnumerator = unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED);
        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER)
            .expect("CoCreateInstance failed")
    };

    let app_path = get_executable_path().to_str().unwrap().to_string();
    let auto = AutoLaunchBuilder::new()
        .set_app_name(APP_NAME)
        .set_app_path(&app_path)
        .build()
        .unwrap();

    let output_devices_heading_item = MenuItem::new("Output devices", false, None);
    let input_devices_heading_item = MenuItem::new("Input devices", false, None);

    let auto_launch_enabled: bool = auto.is_enabled().unwrap_or(false);
    let auto_launch_check_item: CheckMenuItem =
        CheckMenuItem::new("Auto launch on startup", true, auto_launch_enabled, None);

    let quit_item = MenuItem::new("Quit", true, None);

    let tray_menu = Menu::new();
    tray_menu.append(&MenuItem::new("Loading...", false, None));
    tray_menu.append(&PredefinedMenuItem::separator());
    tray_menu.append(&auto_launch_check_item);
    tray_menu.append(&PredefinedMenuItem::separator());
    tray_menu.append(&quit_item);

    let mut tray_icon = None;

    let mut persistent_state = load_state();
    println!("Loaded: {:?}", persistent_state);

    // Map menu item ids to device information
    let mut menu_id_to_device: HashMap<MenuId, MenuItemDeviceInfo> = HashMap::new();

    MenuEvent::receiver();
    TrayIconEvent::receiver();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::NewEvents(tao::event::StartCause::Init) => {
                let icon: tray_icon::Icon =
                    tray_icon::Icon::from_resource_name("app-icon", None).unwrap();
                let tooltip = format!("Volume Locker v{}", env!("CARGO_PKG_VERSION"));
                tray_icon = Some(
                    TrayIconBuilder::new()
                        .with_menu(Box::new(tray_menu.clone()))
                        .with_tooltip(&tooltip)
                        .with_icon(icon)
                        .with_id(APP_UID)
                        .build()
                        .unwrap(),
                );
            }

            Event::UserEvent(UserEvent::MenuEvent(event)) => {
                if event.id == auto_launch_check_item.id() {
                    let checked = auto_launch_check_item.is_checked();
                    if checked {
                        let _ = auto.enable();
                    } else {
                        let _ = auto.disable();
                    }
                } else if event.id == quit_item.id() {
                    tray_icon.take();
                    *control_flow = ControlFlow::Exit;
                } else if let Some(device_info) = menu_id_to_device.get(&event.id) {
                    // Check if the menu item is checked
                    if let Some(item) = tray_menu.items().iter().find(|i| i.id() == &event.id) {
                        if let Some(check_item) = item.as_check_menuitem() {
                            if check_item.is_checked() {
                                persistent_state.locked_devices.insert(
                                    device_info.device_id.clone(),
                                    DeviceLockedInfo {
                                        volume_percent: device_info.volume_percent,
                                        device_type: device_info.device_type,
                                        name: device_info.name.clone(),
                                    },
                                );
                            } else {
                                persistent_state
                                    .locked_devices
                                    .remove(&device_info.device_id);
                            }
                            save_state(&persistent_state);
                            println!("Saved: {:?}", persistent_state);
                        }
                    }
                }
            }

            // On right click of tray icon: reload the menu
            Event::UserEvent(UserEvent::TrayIconEvent(event)) => {
                match event {
                    TrayIconEvent::Click { button, .. } => {
                        if button == MouseButton::Right || button == MouseButton::Left {
                            if let Some(tray_icon) = &tray_icon {
                                // Clear the menu
                                for _ in 0..tray_menu.items().len() {
                                    tray_menu.remove_at(0);
                                }
                                menu_id_to_device.clear();

                                populate_device_menu_items(
                                    &tray_menu,
                                    &output_devices_heading_item,
                                    &device_enumerator,
                                    &persistent_state,
                                    &mut menu_id_to_device,
                                    DeviceType::Output,
                                );

                                populate_device_menu_items(
                                    &tray_menu,
                                    &input_devices_heading_item,
                                    &device_enumerator,
                                    &persistent_state,
                                    &mut menu_id_to_device,
                                    DeviceType::Input,
                                );

                                // Refresh the auto launch state
                                let auto_launch_enabled: bool = auto.is_enabled().unwrap();
                                auto_launch_check_item.set_checked(auto_launch_enabled);
                                tray_menu.append(&auto_launch_check_item);
                                tray_menu.append(&PredefinedMenuItem::separator());

                                tray_menu.append(&quit_item);
                            }
                        }
                    }
                    _ => {}
                }
            }

            Event::UserEvent(UserEvent::Heartbeat) => {
                // Adjust volume of locked devices
                for (device_id, info) in &persistent_state.locked_devices {
                    let (endpoint_type, is_output) = match info.device_type {
                        DeviceType::Output => (eRender, true),
                        DeviceType::Input => (eCapture, false),
                    };
                    let devices: IMMDeviceCollection = unsafe {
                        device_enumerator
                            .EnumAudioEndpoints(endpoint_type, DEVICE_STATE_ACTIVE)
                            .unwrap()
                    };
                    let count = unsafe { devices.GetCount().unwrap() };
                    for i in 0..count {
                        let device = unsafe { devices.Item(i).unwrap() };
                        let id = get_device_id(&device).unwrap();
                        if id == *device_id {
                            set_volume(&device, info.volume_percent).unwrap();
                            break;
                        }
                    }
                }
            }

            _ => {}
        }
    })
}

fn get_executable_directory() -> PathBuf {
    std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn get_executable_path() -> PathBuf {
    std::env::current_exe().unwrap()
}

fn get_state_file_path() -> PathBuf {
    get_executable_directory().join(STATE_FILE_NAME)
}

fn save_state(state: &PersistentState) {
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = fs::write(get_state_file_path(), json);
    }
}

fn load_state() -> PersistentState {
    let state_path = get_state_file_path();
    if state_path.exists() {
        if let Ok(data) = fs::read_to_string(state_path) {
            if let Ok(state) = serde_json::from_str(&data) {
                return state;
            }
        }
    }
    PersistentState::default()
}

fn to_label(name: &str, volume_percent: f32, is_default: bool) -> String {
    let default_indicator = if is_default { " · ☆" } else { "" };
    format!("{}{} · {}%", name, default_indicator, volume_percent)
}

fn get_audio_endpoint(device: &IMMDevice) -> Result<IAudioEndpointVolume> {
    unsafe {
        let audio_endpoint: IAudioEndpointVolume = device.Activate(CLSCTX_INPROC_SERVER, None)?;
        Ok(audio_endpoint)
    }
}

fn get_device_name(device: &IMMDevice) -> Result<String> {
    unsafe {
        let prop_store = device.OpenPropertyStore(STGM_READ)?;
        let friendly_name_prop = prop_store.GetValue(&PKEY_Device_FriendlyName)?;
        let friendly_name = PropVariantToStringAlloc(&friendly_name_prop)?;
        Ok(friendly_name.to_string()?)
    }
}

fn get_device_id(device: &IMMDevice) -> Result<String> {
    unsafe {
        let prop_store = device.OpenPropertyStore(STGM_READ)?;
        let device_id_prop = prop_store.GetValue(&PKEY_Device_ContainerId)?;
        let device_id = PropVariantToStringAlloc(&device_id_prop)?;
        Ok(device_id.to_string()?)
    }
}

fn get_volume(audio_endpoint: &IAudioEndpointVolume) -> Result<f32> {
    unsafe { audio_endpoint.GetMasterVolumeLevelScalar() }
}

fn convert_float_to_percent(volume: f32) -> f32 {
    (volume * 100f32).round()
}

fn set_volume(device: &IMMDevice, new_volume_percent: f32) -> Result<()> {
    unsafe {
        let audio_endpoint: IAudioEndpointVolume = get_audio_endpoint(&device)?;
        let current_volume = get_volume(&audio_endpoint)?;
        let current_percent = convert_float_to_percent(current_volume);
        if current_percent != new_volume_percent {
            let new_volume = new_volume_percent / 100f32;
            audio_endpoint.SetMasterVolumeLevelScalar(new_volume, std::ptr::null());
            let name: String = get_device_name(device)?;
            println!("Adjusted volume of {name} from {current_percent}% to {new_volume_percent}%");
        }
        Ok(())
    }
}

fn get_default_output_device(device_enumerator: &IMMDeviceEnumerator) -> Result<IMMDevice> {
    unsafe {
        let default_device: IMMDevice =
            device_enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
        Ok(default_device)
    }
}

fn get_default_input_device(device_enumerator: &IMMDeviceEnumerator) -> Result<IMMDevice> {
    unsafe {
        let input_devices: IMMDeviceCollection =
            device_enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)?;
        let default_input_device = input_devices.Item(0)?;
        Ok(default_input_device)
    }
}

fn is_default_device(
    device_enumerator: &IMMDeviceEnumerator,
    device: &IMMDevice,
    device_type: DeviceType,
) -> bool {
    let default_device = match device_type {
        DeviceType::Output => get_default_output_device(device_enumerator),
        DeviceType::Input => get_default_input_device(device_enumerator),
    };
    if let Ok(default_device) = default_device {
        if let (Ok(default_id), Ok(device_id)) =
            (get_device_id(&default_device), get_device_id(device))
        {
            return default_id == device_id;
        }
    }
    false
}

fn populate_device_menu_items(
    tray_menu: &Menu,
    heading_item: &MenuItem,
    device_enumerator: &IMMDeviceEnumerator,
    persistent_state: &PersistentState,
    menu_id_to_device: &mut HashMap<MenuId, MenuItemDeviceInfo>,
    device_type: DeviceType,
) {
    tray_menu.append(heading_item);
    let endpoint_type = match device_type {
        DeviceType::Output => eRender,
        DeviceType::Input => eCapture,
    };
    let devices: IMMDeviceCollection = unsafe {
        device_enumerator
            .EnumAudioEndpoints(endpoint_type, DEVICE_STATE_ACTIVE)
            .unwrap()
    };
    let count = unsafe { devices.GetCount().unwrap() };
    for i in 0..count {
        let device = unsafe { devices.Item(i).unwrap() };
        let name = get_device_name(&device).unwrap();
        let device_id = get_device_id(&device).unwrap();
        let audio_endpoint = get_audio_endpoint(&device).unwrap();
        let volume = get_volume(&audio_endpoint).unwrap();
        let volume_percent = convert_float_to_percent(volume);
        let is_default = is_default_device(device_enumerator, &device, device_type);
        let label = to_label(&name, volume_percent, is_default);
        let checked = persistent_state
            .locked_devices
            .get(&device_id)
            .map_or(false, |info| info.device_type == device_type);
        let menu_item = CheckMenuItem::new(&label, true, checked, None);
        menu_id_to_device.insert(
            menu_item.id().clone(),
            MenuItemDeviceInfo {
                device_id,
                name,
                volume_percent,
                device_type,
            },
        );
        tray_menu.append(&menu_item);
    }
    tray_menu.append(&PredefinedMenuItem::separator());
}
