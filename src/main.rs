#![cfg_attr(
    all(
      target_os = "windows",
      not(debug_assertions),
    ),
    windows_subsystem = "windows"
  )]

#![allow(unused)]

use tao::{
    event::Event,
    event_loop::{ControlFlow, EventLoopBuilder},
};
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIconBuilder, TrayIconEvent, MouseButton,
};
use single_instance::SingleInstance;
use std::{thread, time::Duration};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use windows::core::Result;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_ContainerId;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
	eCapture, eConsole, eRender, IMMDevice, IMMDeviceCollection, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, STGM_READ};
use auto_launch::AutoLaunchBuilder;

const APP_NAME: &str = "Volume Locker";
const APP_UID: &str = "25fc6555-723f-414b-9fa0-b4b658d85b43";
const STATE_FILE: &str = "VolumeLockerState.json";
const OUTPUT_DEVICES_LABEL: &str = "Output Devices";
const INPUT_DEVICES_LABEL: &str = "Input Devices";

#[derive(Debug, Serialize, Deserialize, Default)]
struct State {
    locked_output_devices: HashMap<String, f32>,
    locked_input_devices: HashMap<String, f32>,
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
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(5));
        proxy.send_event(UserEvent::Heartbeat);
    });

    let device_enumerator: IMMDeviceEnumerator = unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED);
        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER)
            .expect("CoCreateInstance failed")
    };

    let app_path = std::env::current_exe().unwrap().to_str().unwrap().to_string();
    let auto = AutoLaunchBuilder::new()
        .set_app_name(APP_NAME)
        .set_app_path(&app_path)
        .build()
        .unwrap();

    let auto_launch_enabled: bool = auto.is_enabled().unwrap_or(false);
    let auto_launch_i: CheckMenuItem = CheckMenuItem::new("Auto launch on startup", true, auto_launch_enabled, None);

    let quit_i = MenuItem::new("Quit", true, None);

    let tray_menu = Menu::new();
    tray_menu.append(&MenuItem::new("Loading...", false, None));
    tray_menu.append(&auto_launch_i);
    tray_menu.append(&quit_i);

    let mut tray_icon = None;

    let mut state = load_state();
    println!("Loaded: {:?}", state);

    MenuEvent::receiver();
    TrayIconEvent::receiver();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::NewEvents(tao::event::StartCause::Init) => {
                let icon: tray_icon::Icon = tray_icon::Icon::from_resource_name("app-icon", None).unwrap();
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
                if event.id == auto_launch_i.id() {
                    let checked = auto_launch_i.is_checked();
                    if checked {
                        let _ = auto.enable();
                    } else {
                        let _ = auto.disable();
                    }
                }

                if let Some(item) = tray_menu.items().iter().find(|i| i.id() == &event.id) {
                    if let Some(check_item) = item.as_check_menuitem() {
                        if let Some((name, volume)) = parse_label(&check_item.text()) {
                            let mut is_output = false;
                            let mut is_input = false;
                            let mut device_id = None;
                            for menu_item in tray_menu.items() {
                                if let Some(mi) = menu_item.as_menuitem() {
                                    match mi.text().as_str() {
                                        OUTPUT_DEVICES_LABEL => { is_output = true; is_input = false; }
                                        INPUT_DEVICES_LABEL => { is_output = false; is_input = true; }
                                        _ => {}
                                    }
                                }
                                if let Some(check) = menu_item.as_check_menuitem() {
                                    if check.text() == check_item.text() {
                                        device_id = find_device_id_by_name(&device_enumerator, is_output, &name);
                                        break;
                                    }
                                }
                                if menu_item.id() == item.id() {
                                    break;
                                }
                            }
                            if let Some(device_id) = device_id {
                                if is_output {
                                    if check_item.is_checked() {
                                        state.locked_output_devices.insert(device_id, volume);
                                    } else {
                                        state.locked_output_devices.remove(&device_id);
                                    }
                                } else if is_input {
                                    if check_item.is_checked() {
                                        state.locked_input_devices.insert(device_id, volume);
                                    } else {
                                        state.locked_input_devices.remove(&device_id);
                                    }
                                }
                                save_state(&state);
                                println!("Saved: {:?}", state);
                            }
                        }
                    }
                }

                if event.id == quit_i.id() {
                    tray_icon.take();
                    *control_flow = ControlFlow::Exit;
                }
            }

            // On right click of tray icon reload the menu
            Event::UserEvent(UserEvent::TrayIconEvent(event)) => {
                match event {
                    TrayIconEvent::Click { button, .. } => {
                        if button == MouseButton::Right || button == MouseButton::Left {
                            if let Some(tray_icon) = &tray_icon {
                                for i in 0..tray_menu.items().len() {
                                    tray_menu.remove_at(0);
                                }

                                tray_menu.append(&MenuItem::new(OUTPUT_DEVICES_LABEL, false, None));
                                let output_devices: IMMDeviceCollection = unsafe { device_enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE).unwrap() };
                                let output_count = unsafe { output_devices.GetCount().unwrap() };
                                for i in 0..output_count {
                                    let device = unsafe { output_devices.Item(i).unwrap() };
                                    let name = get_device_friendly_name(&device).unwrap();
                                    let device_id = get_device_id(&device).unwrap();
                                    let audio_endpoint = get_audio_endpoint(&device).unwrap();
                                    let volume = get_volume(&audio_endpoint).unwrap();
                                    let is_default = is_default_output_device(&device_enumerator, &device);
                                    let label = to_label(&name, volume, is_default);
                                    let checked = state.locked_output_devices.get(&device_id).is_some();
                                    tray_menu.append(&CheckMenuItem::new(&label, true, checked, None));
                                }
                                tray_menu.append(&PredefinedMenuItem::separator());

                                tray_menu.append(&MenuItem::new(INPUT_DEVICES_LABEL, false,None));
                                let input_devices: IMMDeviceCollection = unsafe { device_enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE).unwrap() };
                                let input_count = unsafe { input_devices.GetCount().unwrap() };
                                for i in 0..input_count {
                                    let device = unsafe { input_devices.Item(i).unwrap() };
                                    let name = get_device_friendly_name(&device).unwrap();
                                    let device_id = get_device_id(&device).unwrap();
                                    let audio_endpoint = get_audio_endpoint(&device).unwrap();
                                    let volume = get_volume(&audio_endpoint).unwrap();
                                    let is_default = is_default_input_device(&device_enumerator, &device);
                                    let label = to_label(&name, volume, is_default);
                                    let checked: bool = state.locked_input_devices.get(&device_id).is_some();
                                    tray_menu.append(&CheckMenuItem::new(&label, true, checked, None));
                                }
                                tray_menu.append(&PredefinedMenuItem::separator());

                                // Refresh the auto launch state
                                let auto_launch_enabled: bool = auto.is_enabled().unwrap();
                                auto_launch_i.set_checked(auto_launch_enabled);
                                tray_menu.append(&auto_launch_i);
                                tray_menu.append(&PredefinedMenuItem::separator());

                                tray_menu.append(&quit_i);
                            }
                        }
                    }
                    _ => {}
                }
            }

            Event::UserEvent(UserEvent::Heartbeat) => {
                // Adjust volume of locked devices
                for (device_id, volume) in &state.locked_output_devices {
                    // Search for the device id in the output devices
                    let output_devices: IMMDeviceCollection = unsafe { device_enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE).unwrap() };
                    let output_count = unsafe { output_devices.GetCount().unwrap() };
                    for i in 0..output_count {
                        let device = unsafe { output_devices.Item(i).unwrap() };
                        let id = get_device_id(&device).unwrap();
                        if id == *device_id {
                            adjust_volume(&device, *volume).unwrap();
                            break;
                        }
                    }
                }

                for (device_id, volume) in &state.locked_input_devices {
                    // Search for the device id in the input devices
                    let input_devices: IMMDeviceCollection = unsafe { device_enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE).unwrap() };
                    let input_count = unsafe { input_devices.GetCount().unwrap() };
                    for i in 0..input_count {
                        let device = unsafe { input_devices.Item(i).unwrap() };
                        let id = get_device_id(&device).unwrap();
                        if id == *device_id {
                            adjust_volume(&device, *volume).unwrap();
                            break;
                        }
                    }
                }
            }

            _ => {}
        }
    })
}

fn save_state(state: &State) {
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = fs::write(STATE_FILE, json);
    }
}

fn load_state() -> State {
    if Path::new(STATE_FILE).exists() {
        if let Ok(data) = fs::read_to_string(STATE_FILE) {
            if let Ok(state) = serde_json::from_str(&data) {
                return state;
            }
        }
    }
    State::default()
}

fn to_label(name: &str, volume: f32, is_default: bool) -> String {
    let percent = convert_float_to_percent(volume);
    let default_indicator = if is_default { " · ☆" } else { "" };
    format!("{}{} · {}%", name, default_indicator, percent)
}

fn parse_label(label: &str) -> Option<(String, f32)> {
    // Split by the dot separator
    let parts: Vec<&str> = label.split(" · ").collect();
    if parts.len() >= 2 {
        // First element is always the device name
        let name = parts[0].trim().to_string();

        // Last element is always the volume percentage
        let last_part = parts[parts.len() - 1];
        let volume_str = last_part.trim().trim_end_matches('%');
        if let Ok(volume) = volume_str.parse::<f32>() {
            return Some((name, volume));
        }
    }
    None
}

fn get_default_output_device(device_enumerator: &IMMDeviceEnumerator) -> Result<IMMDevice> {
    unsafe {
	    let default_device: IMMDevice = device_enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
        Ok(default_device)
    }
}

fn get_default_input_device(device_enumerator: &IMMDeviceEnumerator) -> Result<IMMDevice> {
	unsafe {
        let input_devices: IMMDeviceCollection = device_enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)?;
        let default_input_device = input_devices.Item(0)?;
        Ok(default_input_device)
    }
}

fn get_audio_endpoint(device: &IMMDevice) -> Result<IAudioEndpointVolume> {
	unsafe {
        let audio_endpoint: IAudioEndpointVolume = device.Activate(CLSCTX_INPROC_SERVER, None)?;
        Ok(audio_endpoint)
    }
}

fn get_device_friendly_name(device: &IMMDevice) -> Result<String> {
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
	unsafe {
        audio_endpoint.GetMasterVolumeLevelScalar()
    }
}

fn convert_float_to_percent(volume: f32) -> f32 {
    (volume * 100f32).round()
}

fn adjust_volume(device: &IMMDevice, new_volume_percent: f32) -> Result<()> {
	unsafe {
        let audio_endpoint: IAudioEndpointVolume = get_audio_endpoint(&device)?;
        let current_volume = get_volume(&audio_endpoint)?;
        let current_percent = convert_float_to_percent(current_volume);
        if current_percent != new_volume_percent {
            let new_volume = new_volume_percent / 100f32;
            audio_endpoint.SetMasterVolumeLevelScalar(new_volume, std::ptr::null());
            let name: String = get_device_friendly_name(device)?;
            println!("Adjusted volume of {name} from {current_percent}% to {new_volume_percent}%");
        }
        Ok(())
    }
}

fn find_device_id_by_name(
    device_enumerator: &IMMDeviceEnumerator,
    is_output: bool,
    name: &str,
) -> Option<String> {
    let endpoints = unsafe {
        device_enumerator.EnumAudioEndpoints(
            if is_output { eRender } else { eCapture },
            DEVICE_STATE_ACTIVE,
        )
        .ok()?
    };
    let count = unsafe { endpoints.GetCount().ok()? };
    for i in 0..count {
        let device = unsafe { endpoints.Item(i).ok()? };
        if get_device_friendly_name(&device).ok()? == name {
            return get_device_id(&device).ok();
        }
    }
    None
}

fn is_default_output_device(device_enumerator: &IMMDeviceEnumerator, device: &IMMDevice) -> bool {
    if let Ok(default_device) = get_default_output_device(device_enumerator) {
        if let (Ok(default_id), Ok(device_id)) = (get_device_id(&default_device), get_device_id(device)) {
            return default_id == device_id;
        }
    }
    false
}

fn is_default_input_device(device_enumerator: &IMMDeviceEnumerator, device: &IMMDevice) -> bool {
    if let Ok(default_device) = get_default_input_device(device_enumerator) {
        if let (Ok(default_id), Ok(device_id)) = (get_device_id(&default_device), get_device_id(device)) {
            return default_id == device_id;
        }
    }
    false
}
