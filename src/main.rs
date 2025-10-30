#![cfg_attr(
    all(target_os = "windows", not(debug_assertions),),
    windows_subsystem = "windows"
)]

use faccess::PathExt;
use regex_lite::Regex;
use serde::{Deserialize, Serialize};
use simplelog::*;
use single_instance::SingleInstance;
use std::fs;
use std::fs::File;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
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
use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
use windows::core::{HSTRING, PCWSTR};
use windows_registry::CURRENT_USER;

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
const APP_AUMID: &str = "FelipeSantos.VolumeLocker";
const APP_UID: &str = "25fc6555-723f-414b-9fa0-b4b658d85b43";
const STATE_FILE_NAME: &str = "VolumeLockerState.json";
const LOG_FILE_NAME: &str = "VolumeLocker.log";
const PNG_ICON_BYTES: &[u8] = include_bytes!("../icons/volume-locked.png");
const PNG_ICON_FILE_NAME: &str = "VolumeLocker.png";

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
enum DeviceType {
    Input,
    Output,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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
    #[serde(default = "default_true")]
    keep_selected_inputs_fixed: bool,
    #[serde(default = "default_true")]
    keep_selected_outputs_fixed: bool,
    #[serde(default)]
    keep_selected_mics_unmuted: bool,
    #[serde(default)]
    #[allow(dead_code)]
    notify_on_mic_unmute_restore: bool,
    #[serde(default)]
    keep_selected_outputs_unmuted: bool,
    #[serde(default)]
    #[allow(dead_code)]
    notify_on_output_unmute_restore: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug)]
struct MenuItemDeviceInfo {
    device_id: String,
    volume_percent: f32,
    device_type: DeviceType,
    name: String,
}

struct VolumeChangedEvent {
    device_id: String,
    new_volume: Option<f32>,
}

enum UserEvent {
    TrayIcon(tray_icon::TrayIconEvent),
    Menu(tray_icon::menu::MenuEvent),
    VolumeChanged(VolumeChangedEvent),
    DevicesChanged,
    ConfigurationChanged,
}

#[implement(IMMNotificationClient)]
struct AudioDevicesChangedCallback {
    proxy: tao::event_loop::EventLoopProxy<UserEvent>,
}

impl IMMNotificationClient_Impl for AudioDevicesChangedCallback_Impl {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> windows::core::Result<()> {
        log::info!("Some device state changed");
        let _ = self.proxy.send_event(UserEvent::DevicesChanged);
        Ok(())
    }

    fn OnDeviceAdded(&self, _: &PCWSTR) -> windows::core::Result<()> {
        log::info!("Some device was added");
        let _ = self.proxy.send_event(UserEvent::DevicesChanged);
        Ok(())
    }

    fn OnDeviceRemoved(&self, _: &PCWSTR) -> windows::core::Result<()> {
        log::info!("Some device was removed");
        let _ = self.proxy.send_event(UserEvent::DevicesChanged);
        Ok(())
    }

    fn OnDefaultDeviceChanged(
        &self,
        _: EDataFlow,
        _: ERole,
        _: &PCWSTR,
    ) -> windows::core::Result<()> {
        let _ = self.proxy.send_event(UserEvent::DevicesChanged);
        Ok(())
    }

    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> windows::core::Result<()> {
        Ok(())
    }
}

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeChangeCallback {
    proxy: tao::event_loop::EventLoopProxy<UserEvent>,
    device_id: String,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeChangeCallback_Impl {
    fn OnNotify(
        &self,
        pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA,
    ) -> ::windows::core::Result<()> {
        let new_volume = unsafe { pnotify.as_ref().map(|p| p.fMasterVolume) };
        let _ = self
            .proxy
            .send_event(UserEvent::VolumeChanged(VolumeChangedEvent {
                device_id: self.device_id.clone(),
                new_volume,
            }));
        Ok(())
    }
}

fn main() {
    let executable_directory = get_executable_directory();

    if !executable_directory.writable() {
        let error_title = "Volume Locker Directory Not Writable";
        let error_message = format!(
            "Please move Volume Locker to a directory that is writable or fix the permissions of '{}'.",
            executable_directory.display(),
        );

        eprintln!("{error_title}: {error_message}");

        if let Err(e) = Toast::new(APP_AUMID)
            .title(error_title)
            .text1(&error_message)
            .duration(tauri_winrt_notification::Duration::Long)
            .show()
        {
            eprintln!("Failed to show {error_title} notification: {e}");
        }

        std::process::exit(1);
    }

    let log_path = executable_directory.join(LOG_FILE_NAME);
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

    // Set panic hook to log panic info before exiting
    std::panic::set_hook(Box::new(|panic_info| {
        log::error!("Panic occurred: {panic_info}");
    }));

    // Only allow one instance of the application to run at a time
    let instance = SingleInstance::new(APP_UID).expect("Failed to create single instance");
    if !instance.is_single() {
        log::error!("Another instance is already running.");
        std::process::exit(1);
    }

    // Set AppUserModelID so toast notifications show correct app name and icon
    let _ = setup_app_aumid(&executable_directory);

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
        CheckMenuItem::new("Show notifications", true, false, None);
    let keep_mics_unmuted_check_item: CheckMenuItem =
        CheckMenuItem::new("Keep selected microphones unmuted", true, false, None);
    let keep_outputs_unmuted_check_item: CheckMenuItem =
        CheckMenuItem::new("Keep selected speakers unmuted", true, false, None);
    let keep_inputs_fixed_check_item: CheckMenuItem =
        CheckMenuItem::new("Keep selected microphone volumes fixed", true, false, None);
    let keep_outputs_fixed_check_item: CheckMenuItem =
        CheckMenuItem::new("Keep selected speaker volumes fixed", true, false, None);
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

    let mut last_notification_times: HashMap<String, Instant> = HashMap::new();

    let main_proxy = event_loop.create_proxy();

    let mut persistent_state = load_state();
    log::info!("Loaded: {persistent_state:?}");

    // Migrate device IDs if they have changed
    migrate_device_ids(&device_enumerator, &mut persistent_state);

    // Save the state if any migrations occurred
    save_state(&persistent_state);

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
                let _ = main_proxy.send_event(UserEvent::DevicesChanged);
            }

            Event::UserEvent(UserEvent::Menu(event)) => {
                if event.id == notify_check_item.id() {
                    persistent_state.notify_on_volume_restored = notify_check_item.is_checked();
                    let _ = main_proxy.send_event(UserEvent::ConfigurationChanged);
                } else if event.id == keep_mics_unmuted_check_item.id() {
                    persistent_state.keep_selected_mics_unmuted =
                        keep_mics_unmuted_check_item.is_checked();
                    let _ = main_proxy.send_event(UserEvent::ConfigurationChanged);
                } else if event.id == keep_outputs_unmuted_check_item.id() {
                    persistent_state.keep_selected_outputs_unmuted =
                        keep_outputs_unmuted_check_item.is_checked();
                    let _ = main_proxy.send_event(UserEvent::ConfigurationChanged);
                } else if event.id == keep_inputs_fixed_check_item.id() {
                    persistent_state.keep_selected_inputs_fixed =
                        keep_inputs_fixed_check_item.is_checked();
                    let _ = main_proxy.send_event(UserEvent::ConfigurationChanged);
                } else if event.id == keep_outputs_fixed_check_item.id() {
                    persistent_state.keep_selected_outputs_fixed =
                        keep_outputs_fixed_check_item.is_checked();
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
                    if let Some(item) = tray_menu.items().iter().find(|i| i.id() == &event.id)
                        && let Some(check_item) = item.as_check_menuitem() {
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
                keep_mics_unmuted_check_item
                    .set_checked(persistent_state.keep_selected_mics_unmuted);
                tray_menu.append(&keep_mics_unmuted_check_item).unwrap();
                keep_outputs_unmuted_check_item
                    .set_checked(persistent_state.keep_selected_outputs_unmuted);
                tray_menu.append(&keep_outputs_unmuted_check_item).unwrap();
                keep_inputs_fixed_check_item
                    .set_checked(persistent_state.keep_selected_inputs_fixed);
                tray_menu.append(&keep_inputs_fixed_check_item).unwrap();
                keep_outputs_fixed_check_item
                    .set_checked(persistent_state.keep_selected_outputs_fixed);
                tray_menu.append(&keep_outputs_fixed_check_item).unwrap();
                let auto_launch_enabled = auto_launch.is_enabled().unwrap();
                auto_launch_check_item.set_checked(auto_launch_enabled);
                tray_menu.append(&auto_launch_check_item).unwrap();
                tray_menu.append(&PredefinedMenuItem::separator()).unwrap();
                tray_menu.append(&quit_item).unwrap();
            }

            Event::UserEvent(UserEvent::VolumeChanged(event)) => {
                let VolumeChangedEvent {
                    device_id,
                    new_volume,
                } = event;
                let new_volume = match new_volume {
                    Some(v) => v,
                    None => {
                        let device = match get_device_by_id(&device_enumerator, &device_id) {
                            Ok(d) => d,
                            Err(e) => {
                                log::error!(
                                    "Failed to get device by id for {device_id}: {e}"
                                );
                                return;
                            }
                        };
                        let endpoint = match get_audio_endpoint(&device) {
                            Ok(ep) => ep,
                            Err(e) => {
                                log::error!("Failed to get endpoint for {device_id}: {e}");
                                return;
                            }
                        };
                        match get_volume(&endpoint) {
                            Ok(v) => v,
                            Err(e) => {
                                log::error!("Failed to get volume for {device_id}: {e}");
                                return;
                            }
                        }
                    }
                };
                let new_volume_percent = convert_float_to_percent(new_volume);
                let device_info = persistent_state.locked_devices.get(&device_id).unwrap();
                match device_info.device_type {
                    DeviceType::Input => {
                        if !persistent_state.keep_selected_inputs_fixed {
                            return;
                        }
                    }
                    DeviceType::Output => {
                        if !persistent_state.keep_selected_outputs_fixed {
                            return;
                        }
                    }
                }
                let target_volume_percent = device_info.volume_percent;
                if new_volume_percent != target_volume_percent {
                    let target_volume = convert_percent_to_float(target_volume_percent);
                    let device = match get_device_by_id(&device_enumerator, &device_id) {
                        Ok(d) => d,
                        Err(e) => {
                            log::error!(
                                "Failed to get device by id for {}: {}",
                                device_info.name,
                                e
                            );
                            return;
                        }
                    };
                    let device_name =
                        get_device_name(&device).unwrap_or_else(|_| device_info.name.clone());
                    let endpoint = match get_audio_endpoint(&device) {
                        Ok(ep) => ep,
                        Err(e) => {
                            log::error!("Failed to get endpoint for {device_name}: {e}");
                            return;
                        }
                    };
                    if let Err(e) = set_volume(&endpoint, target_volume) {
                        log::error!(
                            "Failed to set volume of {device_name} to {target_volume_percent}%: {e}"
                        );
                        return;
                    }
                    log::info!(
                        "Restored volume of {device_name} from {new_volume_percent}% to {target_volume_percent}%"
                    );
                    if persistent_state.notify_on_volume_restored {
                        let now = Instant::now();
                        let should_notify = match last_notification_times.get(device_id.as_str()) {
                            Some(&last_time) => {
                                now.duration_since(last_time) > Duration::from_secs(5)
                            }
                            None => true,
                        };
                        if should_notify {
                            if let Err(e) = Toast::new(APP_AUMID)
                                .title("Volume Restored")
                                .text1(&format!(
                                    "The volume of {device_name} has been restored from {new_volume_percent}% to {target_volume_percent}%."
                                ))
                                .show()
                            {
                                log::error!(
                                    "Failed to show volume restored notification for {device_name}: {e}"
                                );
                            }
                            last_notification_times.insert(device_id.clone(), now);
                        }
                    }
                }

                // Enforce unmute for locked input devices when enabled
                if persistent_state.keep_selected_mics_unmuted
                    && device_info.device_type == DeviceType::Input
                {
                    let device = match get_device_by_id(&device_enumerator, &device_id) {
                        Ok(d) => d,
                        Err(_) => return,
                    };
                    let device_name =
                        get_device_name(&device).unwrap_or_else(|_| device_info.name.clone());
                    let endpoint = match get_audio_endpoint(&device) {
                        Ok(ep) => ep,
                        Err(_) => return,
                    };
                    if let Ok(true) = get_mute(&endpoint) {
                        if let Err(e) = set_mute(&endpoint, false) {
                            log::error!("Failed to unmute {device_name}: {e}");
                        } else {
                            log::info!(
                                "Unmuted {device_name} due to keep-selected-mics-unmuted"
                            );
                            if persistent_state.notify_on_volume_restored {
                                let now = Instant::now();
                        let should_notify = match last_notification_times.get(device_id.as_str()) {
                                    Some(&last_time) => now.duration_since(last_time) > Duration::from_secs(5),
                                    None => true,
                                };
                                if should_notify {
                                    if let Err(e) = Toast::new(APP_AUMID)
                                        .title("Microphone Unmuted")
                                        .text1(&format!(
                                            "{device_name} was unmuted due to Keep selected microphones unmuted."
                                        ))
                                        .show()
                                    {
                                        log::error!(
                                            "Failed to show mic unmute restore notification for {device_name}: {e}"
                                        );
                                    }
                                    last_notification_times.insert(device_id.clone(), now);
                                }
                            }
                        }
                    }
                }

                // Enforce unmute for locked output devices when enabled
                if persistent_state.keep_selected_outputs_unmuted
                    && device_info.device_type == DeviceType::Output
                {
                    let device = match get_device_by_id(&device_enumerator, &device_id) {
                        Ok(d) => d,
                        Err(_) => return,
                    };
                    let device_name =
                        get_device_name(&device).unwrap_or_else(|_| device_info.name.clone());
                    let endpoint = match get_audio_endpoint(&device) {
                        Ok(ep) => ep,
                        Err(_) => return,
                    };
                    if let Ok(true) = get_mute(&endpoint) {
                        if let Err(e) = set_mute(&endpoint, false) {
                            log::error!("Failed to unmute {device_name}: {e}");
                        } else {
                            log::info!(
                                "Unmuted {device_name} due to keep-selected-outputs-unmuted"
                            );
                            if persistent_state.notify_on_volume_restored {
                                let now = Instant::now();
                                let should_notify = match last_notification_times.get(device_id.as_str()) {
                                    Some(&last_time) => now.duration_since(last_time) > Duration::from_secs(5),
                                    None => true,
                                };
                                if should_notify {
                                    if let Err(e) = Toast::new(APP_AUMID)
                                        .title("Speaker Unmuted")
                                        .text1(&format!(
                                            "{device_name} was unmuted due to Keep selected outputs unmuted."
                                        ))
                                        .show()
                                    {
                                        log::error!(
                                            "Failed to show output unmute restore notification for {device_name}: {e}"
                                        );
                                    }
                                    last_notification_times.insert(device_id.clone(), now);
                                }
                            }
                        }
                    }
                }
            }

            Event::UserEvent(UserEvent::DevicesChanged) => {
                log::info!("Reloading list of watched devices...");

                watched_endpoints.clear();
                let mut some_locked = false;
                for (device_id, device_info) in persistent_state.locked_devices.iter() {
                    let device = match get_device_by_id(&device_enumerator, device_id) {
                        Ok(device) => device,
                        Err(e) => {
                            log::warn!(
                                "Not watching volume of {} as failed to get its device by id: {}",
                                device_info.name,
                                e
                            );
                            continue;
                        }
                    };

                    let device_state = match unsafe { device.GetState() } {
                        Ok(state) => state,
                        Err(e) => {
                            log::warn!(
                                "Not watching volume of {} as failed to get its state: {}",
                                device_info.name,
                                e
                            );
                            continue;
                        }
                    };
                    if device_state != DEVICE_STATE_ACTIVE {
                        log::info!(
                            "Not watching volume of {} as it is not active",
                            device_info.name
                        );
                        continue;
                    }

                    let endpoint = match get_audio_endpoint(&device) {
                        Ok(ep) => ep,
                        Err(e) => {
                            log::warn!(
                                "Not watching volume of {} as failed to get its endpoint: {}",
                                device_info.name,
                                e
                            );
                            continue;
                        }
                    };
                    let volume_callback: IAudioEndpointVolumeCallback = VolumeChangeCallback {
                        proxy: main_proxy.clone(),
                        device_id: device_id.clone(),
                    }
                    .into();
                    if let Err(e) =
                        unsafe { endpoint.RegisterControlChangeNotify(&volume_callback) }
                    {
                        log::warn!(
                            "Not watching volume of {} as failed to register for volume changes: {}",
                            device_info.name,
                            e
                        );
                        continue;
                    }
                    watched_endpoints.push(endpoint.clone());
                    log::info!(
                        "Watching volume of {} for when it changes from {}%",
                        device_info.name,
                        device_info.volume_percent
                    );

                    let _ = main_proxy.send_event(UserEvent::VolumeChanged(
                        VolumeChangedEvent {
                            device_id: device_id.clone(),
                            new_volume: None,
                        },
                    ));

                    // Enforce unmute for locked input devices when enabled on refresh
                    if persistent_state.keep_selected_mics_unmuted
                        && device_info.device_type == DeviceType::Input
                        && let Ok(true) = get_mute(&endpoint) {
                            if let Err(e) = set_mute(&endpoint, false) {
                                log::warn!(
                                    "Failed to unmute {} on refresh: {}",
                                    device_info.name, e
                                );
                            } else {
                                log::info!(
                                    "Unmuted {} on refresh due to keep-selected-mics-unmuted",
                                    device_info.name
                                );
                                if persistent_state.notify_on_volume_restored {
                                    let now = Instant::now();
                                    let should_notify = match last_notification_times.get(device_id.as_str()) {
                                        Some(&last_time) => now.duration_since(last_time) > Duration::from_secs(5),
                                        None => true,
                                    };
                                    if should_notify {
                                        if let Err(e) = Toast::new(APP_AUMID)
                                            .title("Microphone Unmuted")
                                            .text1(&format!(
                                                "{} was unmuted due to Keep selected microphones unmuted.",
                                                device_info.name
                                            ))
                                            .show()
                                        {
                                            log::error!(
                                                "Failed to show mic unmute restore notification for {}: {}",
                                                device_info.name, e
                                            );
                                        }
                                        last_notification_times.insert(device_id.clone(), now);
                                    }
                                }
                            }
                        }

                    // Enforce unmute for locked output devices when enabled on refresh
                    if persistent_state.keep_selected_outputs_unmuted
                        && device_info.device_type == DeviceType::Output
                        && let Ok(true) = get_mute(&endpoint) {
                            if let Err(e) = set_mute(&endpoint, false) {
                                log::warn!(
                                    "Failed to unmute {} on refresh: {}",
                                    device_info.name, e
                                );
                            } else {
                                log::info!(
                                    "Unmuted {} on refresh due to keep-selected-outputs-unmuted",
                                    device_info.name
                                );
                                if persistent_state.notify_on_volume_restored {
                                    let now = Instant::now();
                                    let should_notify = match last_notification_times.get(device_id.as_str()) {
                                        Some(&last_time) => now.duration_since(last_time) > Duration::from_secs(5),
                                        None => true,
                                    };
                                    if should_notify {
                                        if let Err(e) = Toast::new(APP_AUMID)
                                            .title("Speaker Unmuted")
                                            .text1(&format!(
                                                "{} was unmuted due to Keep selected outputs unmuted.",
                                                device_info.name
                                            ))
                                            .show()
                                        {
                                            log::error!(
                                                "Failed to show output unmute restore notification for {}: {}",
                                                device_info.name, e
                                            );
                                        }
                                        last_notification_times.insert(device_id.clone(), now);
                                    }
                                }
                            }
                        }

                    some_locked = true;
                }

                if let Some(tray_icon) = &tray_icon {
                    if some_locked {
                        if let Err(e) = tray_icon.set_icon(Some(locked_icon.clone())) {
                            log::error!("Failed to update tray icon to locked: {e}");
                        }
                    } else if let Err(e) = tray_icon.set_icon(Some(unlocked_icon.clone())) {
                        log::error!("Failed to update tray icon to unlocked: {e}");
                    }
                }

            }

            Event::UserEvent(UserEvent::ConfigurationChanged) => {
                save_state(&persistent_state);
                log::info!("Saved: {persistent_state:?}");
                let _ = main_proxy.send_event(UserEvent::DevicesChanged);
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

fn setup_app_aumid(executable_directory: &Path) -> Result<()> {
    // Create registry keys for the AppUserModelID
    let registry_path = format!(r"SOFTWARE\Classes\AppUserModelId\{APP_AUMID}");
    let _ = CURRENT_USER.remove_tree(registry_path.clone());
    let key = CURRENT_USER.create(registry_path.clone()).unwrap();
    let _ = key.set_string("DisplayName", APP_NAME);

    // Write the icon file to the executable directory and use it as the icon
    let png_path = executable_directory.join(PNG_ICON_FILE_NAME);
    if let Err(e) = fs::write(&png_path, PNG_ICON_BYTES) {
        log::warn!("Failed to write {PNG_ICON_FILE_NAME} icon: {e}");
        let _ = key.remove_value("IconUri");
    } else {
        let _ = key.set_hstring("IconUri", &png_path.as_path().into());
    }

    unsafe {
        let _ = SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(APP_AUMID));
    }

    Ok(())
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
    if state_path.exists()
        && let Ok(data) = fs::read_to_string(state_path)
        && let Ok(state) = serde_json::from_str(&data)
    {
        return state;
    }
    PersistentState::default()
}

fn to_label(name: &str, volume_percent: f32, is_default: bool) -> String {
    let default_indicator = if is_default { " · ☆" } else { "" };
    format!("{name}{default_indicator} · {volume_percent}%")
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
        let name_clean = clean_device_name(&friendly_name);
        Ok(name_clean)
    }
}

// Reimplemented from https://github.com/Belphemur/SoundSwitch/blob/50063dd35d3e648192cbcaa1f9a82a5856302562/SoundSwitch.Common/Framework/Audio/Device/DeviceInfo.cs#L33-L56
fn clean_device_name(name: &str) -> String {
    let name_splitter = match Regex::new(r"(?P<friendlyName>.+)\s\([\d\s\-|]*(?P<deviceName>.+)\)")
    {
        Ok(regex) => regex,
        Err(_) => return name.to_string(),
    };

    let name_cleaner = match Regex::new(r"\s?\(\d\)|^\d+\s?-\s?") {
        Ok(regex) => regex,
        Err(_) => return name.to_string(),
    };

    if let Some(captures) = name_splitter.captures(name) {
        let friendly_name = captures.name("friendlyName").map_or("", |m| m.as_str());
        let device_name = captures.name("deviceName").map_or("", |m| m.as_str());

        let cleaned_friendly = name_cleaner.replace_all(friendly_name, "");
        let cleaned_friendly = cleaned_friendly.trim();

        format!("{cleaned_friendly} ({device_name})")
    } else {
        // Old naming format, use as is
        name.to_string()
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

fn get_mute(endpoint: &IAudioEndpointVolume) -> Result<bool> {
    unsafe { endpoint.GetMute().map(|b| b.as_bool()) }
}

fn set_mute(endpoint: &IAudioEndpointVolume, muted: bool) -> Result<()> {
    unsafe { endpoint.SetMute(muted, std::ptr::null()).map(|_| ()) }
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
        let default_device: IMMDevice =
            device_enumerator.GetDefaultAudioEndpoint(eCapture, eConsole)?;
        Ok(default_device)
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
    if let Ok(default_device) = default_device
        && let (Ok(default_id), Ok(device_id)) =
            (get_device_id(&default_device), get_device_id(device))
    {
        return default_id == device_id;
    }
    false
}

fn migrate_device_ids(
    device_enumerator: &IMMDeviceEnumerator,
    persistent_state: &mut PersistentState,
) {
    let mut devices_to_migrate: Vec<(String, DeviceLockedInfo)> = Vec::new();
    let mut devices_to_update: Vec<(String, DeviceLockedInfo)> = Vec::new();

    // Check which devices need migration
    for (device_id, device_info) in persistent_state.locked_devices.iter() {
        if let Ok(device) = get_device_by_id(device_enumerator, device_id) {
            // Device exists, check if name has changed
            if let Ok(current_name) = get_device_name(&device)
                && current_name != device_info.name
            {
                log::info!(
                    "Device {} with ID {} had the name changed to {}",
                    device_info.name,
                    device_id,
                    current_name,
                );
                let updated_info = DeviceLockedInfo {
                    name: current_name,
                    volume_percent: device_info.volume_percent,
                    device_type: device_info.device_type,
                };
                devices_to_update.push((device_id.clone(), updated_info));
            }
        } else {
            devices_to_migrate.push((device_id.clone(), device_info.clone()));
        }
    }

    // Apply the name updates
    for (device_id, updated_info) in devices_to_update {
        persistent_state
            .locked_devices
            .insert(device_id, updated_info);
    }

    // Attempt to migrate each device
    for (old_device_id, device_info) in devices_to_migrate {
        let device_name = device_info.name.clone();
        if let Ok(new_device_id) =
            find_device_by_name_and_type(device_enumerator, &device_name, device_info.device_type)
        {
            // Swap the old device with the new one
            persistent_state.locked_devices.remove(&old_device_id);
            persistent_state
                .locked_devices
                .insert(new_device_id.clone(), device_info);
            log::info!("Migrated device {device_name} from ID {old_device_id} to {new_device_id}");
        } else {
            log::warn!(
                "Device {device_name} with ID {old_device_id} could not be found, keeping it in case it returns"
            );
        }
    }
}

fn find_device_by_name_and_type(
    device_enumerator: &IMMDeviceEnumerator,
    target_name: &str,
    device_type: DeviceType,
) -> Result<String> {
    let endpoint_type = match device_type {
        DeviceType::Output => eRender,
        DeviceType::Input => eCapture,
    };

    let devices: IMMDeviceCollection =
        unsafe { device_enumerator.EnumAudioEndpoints(endpoint_type, DEVICE_STATE_ACTIVE)? };

    let count = unsafe { devices.GetCount()? };
    for i in 0..count {
        let device = unsafe { devices.Item(i)? };
        let device_name = get_device_name(&device)?;

        if device_name == target_name {
            return get_device_id(&device);
        }
    }

    Err(windows::core::Error::empty())
}
