#![cfg_attr(
    all(target_os = "windows", not(debug_assertions),),
    windows_subsystem = "windows"
)]

use serde::{Deserialize, Serialize};
use simplelog::*;
use single_instance::SingleInstance;
use std::fs;
use std::fs::File;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::{collections::HashMap, ffi::OsStr};
use tao::{
    event::Event,
    event_loop::{ControlFlow, EventLoopBuilder},
};
use tauri_winrt_notification::Toast;
use tray_icon::{
    MouseButton, TrayIconBuilder, TrayIconEvent,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem},
};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows_core::PCWSTR;

use auto_launch::AutoLaunchBuilder;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE, DEVICE_STATE_ACTIVE, EDataFlow, ERole, IMMDevice,
    IMMDeviceCollection, IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl,
    MMDeviceEnumerator, eCapture, eConsole, eRender,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, STGM_READ,
};
use windows::core::{Result, implement};

const APP_NAME: &str = "Volume Locker";
const APP_UID: &str = "25fc6555-723f-414b-9fa0-b4b658d85b43";
const STATE_FILE_NAME: &str = "VolumeLockerState.json";
const LOG_FILE_NAME: &str = "VolumeLocker.log";

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

#[derive(Debug, Default, Serialize, Deserialize)]
struct PersistentState {
    locked_devices: HashMap<String, DeviceLockedInfo>,
    #[serde(default)]
    notify_on_volume_restored: bool,
}

#[derive(Debug)]
struct MenuItemDeviceInfo {
    device_id: String,
    volume_percent: f32,
    device_type: DeviceType,
    name: String,
}

enum UserEvent {
    TrayIcon(tray_icon::TrayIconEvent),
    Menu(tray_icon::menu::MenuEvent),
    ConfigurationChanged,
    WatchedDevicesShouldReload,
}

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeChangeCallback {
    device: IMMDevice,
    target_volume_percent: f32,
    notify_on_volume_restored: bool,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeChangeCallback_Impl {
    fn OnNotify(
        &self,
        pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA,
    ) -> ::windows::core::Result<()> {
        let new_volume = unsafe { (*pnotify).fMasterVolume };
        let new_volume_percent = convert_float_to_percent(new_volume);
        if new_volume_percent != self.target_volume_percent {
            let target_volume = convert_percent_to_float(self.target_volume_percent);
            let endpoint = get_audio_endpoint(&self.device)?;
            set_volume(&endpoint, target_volume)?;
            let device_name = get_device_name(&self.device)?;
            log::info!(
                "Restored volume of {} from {}% to {}%",
                device_name,
                new_volume_percent,
                self.target_volume_percent
            );
            if self.notify_on_volume_restored {
                Toast::new(Toast::POWERSHELL_APP_ID)
                    .title("Volume Restored")
                    .text1(&format!(
                        "The volume of {} has been restored from {}% to {}%.",
                        device_name, new_volume_percent, self.target_volume_percent
                    ))
                    .show()
                    .unwrap();
            }
        }
        Ok(())
    }
}

#[implement(IMMNotificationClient)]
struct AudioDevicesChangedCallback {
    proxy: tao::event_loop::EventLoopProxy<UserEvent>,
}

impl IMMNotificationClient_Impl for AudioDevicesChangedCallback_Impl {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> windows::core::Result<()> {
        log::info!("Some device state changed");
        let _ = self.proxy.send_event(UserEvent::WatchedDevicesShouldReload);
        Ok(())
    }

    fn OnDeviceAdded(&self, _: &PCWSTR) -> windows::core::Result<()> {
        log::info!("Some device was added");
        let _ = self.proxy.send_event(UserEvent::WatchedDevicesShouldReload);
        Ok(())
    }

    fn OnDeviceRemoved(&self, _: &PCWSTR) -> windows::core::Result<()> {
        log::info!("Some device was removed");
        let _ = self.proxy.send_event(UserEvent::WatchedDevicesShouldReload);
        Ok(())
    }

    fn OnDefaultDeviceChanged(
        &self,
        _: EDataFlow,
        _: ERole,
        _: &PCWSTR,
    ) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> windows::core::Result<()> {
        Ok(())
    }
}

fn main() {
    let log_path = get_executable_directory().join(LOG_FILE_NAME);
    let loggers: Vec<Box<dyn SharedLogger>> = vec![
        WriteLogger::new(
            LevelFilter::Info,
            Config::default(),
            File::create(&log_path).unwrap(),
        ),
        #[cfg(debug_assertions)]
        TermLogger::new(
            LevelFilter::Info,
            Config::default(),
            TerminalMode::Stderr,
            ColorChoice::Auto,
        ),
    ];

    CombinedLogger::init(loggers).unwrap();

    // Only allow one instance of the application to run at a time
    let instance = SingleInstance::new(APP_UID).expect("Failed to create single instance");
    if !instance.is_single() {
        log::error!("Another instance is already running.");
        std::process::exit(1);
    }

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();

    let proxy = event_loop.create_proxy();
    TrayIconEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::TrayIcon(event));
    }));
    TrayIconEvent::receiver();

    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::Menu(event));
    }));
    MenuEvent::receiver();

    let app_path = get_executable_path().to_str().unwrap().to_string();
    let auto_launch = AutoLaunchBuilder::new()
        .set_app_name(APP_NAME)
        .set_app_path(&app_path)
        .build()
        .unwrap();

    let output_devices_heading_item = MenuItem::new("Output devices", false, None);
    let input_devices_heading_item = MenuItem::new("Input devices", false, None);
    let notify_check_item: CheckMenuItem =
        CheckMenuItem::new("Notify on volume restored", true, false, None);
    let auto_launch_check_item: CheckMenuItem =
        CheckMenuItem::new("Auto launch on startup", true, false, None);
    let quit_item = MenuItem::new("Quit", true, None);

    let tray_menu = Menu::new();
    // At least one item must be added to the menu on initialization, otherwise
    // the menu will not be shown on first click
    tray_menu.append(&quit_item).unwrap();

    let mut tray_icon = None;

    let unlocked_icon = tray_icon::Icon::from_resource_name("volume-unlocked-icon", None).unwrap();
    let locked_icon = tray_icon::Icon::from_resource_name("volume-locked-icon", None).unwrap();

    let mut menu_id_to_device: HashMap<MenuId, MenuItemDeviceInfo> = HashMap::new();

    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).unwrap() };
    let device_enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER).unwrap() };

    let devices_changed_callback: IMMNotificationClient = AudioDevicesChangedCallback {
        proxy: event_loop.create_proxy(),
    }
    .into();
    unsafe {
        device_enumerator
            .RegisterEndpointNotificationCallback(&devices_changed_callback)
            .unwrap();
    }

    let mut watched_endpoints: Vec<IAudioEndpointVolume> = Vec::new();

    let main_proxy = event_loop.create_proxy();

    let mut persistent_state = load_state();
    log::info!("Loaded: {:?}", persistent_state);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::NewEvents(tao::event::StartCause::Init) => {
                let tooltip = format!("Volume Locker v{}", env!("CARGO_PKG_VERSION"));
                tray_icon = Some(
                    TrayIconBuilder::new()
                        .with_menu(Box::new(tray_menu.clone()))
                        .with_tooltip(&tooltip)
                        .with_icon(unlocked_icon.clone())
                        .with_id(APP_UID)
                        .build()
                        .unwrap(),
                );
                let _ = main_proxy.send_event(UserEvent::WatchedDevicesShouldReload);
            }

            Event::UserEvent(UserEvent::Menu(event)) => {
                if event.id == notify_check_item.id() {
                    persistent_state.notify_on_volume_restored = notify_check_item.is_checked();
                    let _ = main_proxy.send_event(UserEvent::ConfigurationChanged);
                } else if event.id == auto_launch_check_item.id() {
                    let checked = auto_launch_check_item.is_checked();
                    if checked {
                        auto_launch.enable().unwrap();
                    } else {
                        auto_launch.disable().unwrap();
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
                            let _ = main_proxy.send_event(UserEvent::ConfigurationChanged);
                        }
                    }
                }
            }

            // On right or left click of tray icon: reload the menu
            Event::UserEvent(UserEvent::TrayIcon(TrayIconEvent::Click { button, .. }))
                if button == MouseButton::Right || button == MouseButton::Left =>
            {
                // Clear the menu
                for _ in 0..tray_menu.items().len() {
                    tray_menu.remove_at(0);
                }
                menu_id_to_device.clear();

                for (heading_item, device_type) in [
                    (&output_devices_heading_item, DeviceType::Output),
                    (&input_devices_heading_item, DeviceType::Input),
                ] {
                    tray_menu.append(heading_item).unwrap();
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
                        let endpoint = get_audio_endpoint(&device).unwrap();
                        let volume = get_volume(&endpoint).unwrap();
                        let volume_percent = convert_float_to_percent(volume);
                        let is_default =
                            is_default_device(&device_enumerator, &device, device_type);
                        let label = to_label(&name, volume_percent, is_default);
                        let checked = persistent_state
                            .locked_devices
                            .get(&device_id)
                            .is_some_and(|info| info.device_type == device_type);
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
                        tray_menu.append(&menu_item).unwrap();
                    }
                    tray_menu.append(&PredefinedMenuItem::separator()).unwrap();
                }

                // Refresh check items
                notify_check_item.set_checked(persistent_state.notify_on_volume_restored);
                tray_menu.append(&notify_check_item).unwrap();
                let auto_launch_enabled = auto_launch.is_enabled().unwrap();
                auto_launch_check_item.set_checked(auto_launch_enabled);
                tray_menu.append(&auto_launch_check_item).unwrap();
                tray_menu.append(&PredefinedMenuItem::separator()).unwrap();
                tray_menu.append(&quit_item).unwrap();
            }

            Event::UserEvent(UserEvent::WatchedDevicesShouldReload) => {
                log::info!("Reloading list of watched devices...");

                watched_endpoints.clear();
                let mut some_locked = false;
                for (device_id, device_info) in persistent_state.locked_devices.iter() {
                    let device = match get_device_by_id(&device_enumerator, device_id) {
                        Ok(device) => device,
                        Err(e) => {
                            log::warn!(
                                "Not watching volume of {} because of error: {}",
                                device_info.name,
                                e
                            );
                            continue;
                        }
                    };

                    let device_state = unsafe { device.GetState().unwrap() };
                    if device_state != DEVICE_STATE_ACTIVE {
                        log::info!(
                            "Not watching volume of {} because it is not enabled",
                            device_info.name
                        );
                        continue;
                    }

                    let endpoint = get_audio_endpoint(&device).unwrap();
                    let volume_callback: IAudioEndpointVolumeCallback = VolumeChangeCallback {
                        device,
                        target_volume_percent: device_info.volume_percent,
                        notify_on_volume_restored: persistent_state.notify_on_volume_restored,
                    }
                    .into();
                    unsafe {
                        endpoint
                            .RegisterControlChangeNotify(&volume_callback)
                            .unwrap()
                    };
                    watched_endpoints.push(endpoint);
                    log::info!(
                        "Watching volume of {} for when it changes from {}%",
                        device_info.name,
                        device_info.volume_percent
                    );
                    some_locked = true;
                }

                if let Some(tray_icon) = &tray_icon {
                    if some_locked {
                        tray_icon.set_icon(Some(locked_icon.clone())).unwrap();
                    } else {
                        tray_icon.set_icon(Some(unlocked_icon.clone())).unwrap();
                    }
                }
            }

            Event::UserEvent(UserEvent::ConfigurationChanged) => {
                save_state(&persistent_state);
                log::info!("Saved: {:?}", persistent_state);
                let _ = main_proxy.send_event(UserEvent::WatchedDevicesShouldReload);
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
        let endpoint: IAudioEndpointVolume = device.Activate(CLSCTX_INPROC_SERVER, None)?;
        Ok(endpoint)
    }
}

fn get_device_name(device: &IMMDevice) -> Result<String> {
    unsafe {
        let prop_store = device.OpenPropertyStore(STGM_READ)?;
        let friendly_name_prop = prop_store.GetValue(&PKEY_Device_FriendlyName)?;
        let friendly_name = PropVariantToStringAlloc(&friendly_name_prop)?.to_string()?;
        Ok(friendly_name)
    }
}

fn get_device_id(device: &IMMDevice) -> Result<String> {
    unsafe {
        let dev_id = device.GetId()?.to_string()?;
        Ok(dev_id)
    }
}

fn get_device_by_id(device_enumerator: &IMMDeviceEnumerator, device_id: &str) -> Result<IMMDevice> {
    let wide: Vec<u16> = OsStr::new(device_id)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let device = device_enumerator.GetDevice(PCWSTR(wide.as_ptr()))?;
        Ok(device)
    }
}

fn get_volume(endpoint: &IAudioEndpointVolume) -> Result<f32> {
    unsafe { endpoint.GetMasterVolumeLevelScalar() }
}

fn convert_float_to_percent(volume: f32) -> f32 {
    (volume * 100f32).round()
}

fn convert_percent_to_float(volume: f32) -> f32 {
    volume / 100f32
}

fn set_volume(endpoint: &IAudioEndpointVolume, new_volume: f32) -> Result<()> {
    unsafe {
        endpoint.SetMasterVolumeLevelScalar(new_volume, std::ptr::null())?;
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
