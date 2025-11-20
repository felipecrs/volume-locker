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
    menu::{
        CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu,
    },
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
struct DeviceSettings {
    #[serde(default)]
    is_volume_locked: bool,
    #[serde(default)]
    volume_percent: f32,
    #[serde(default)]
    notify_on_volume_lock: bool,
    #[serde(default)]
    is_unmute_locked: bool,
    #[serde(default)]
    notify_on_unmute_lock: bool,
    device_type: DeviceType,
    name: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PersistentState {
    #[serde(default)]
    devices: HashMap<String, DeviceSettings>,
}

#[derive(Debug)]
enum DeviceSettingType {
    VolumeLock,
    VolumeLockNotify,
    UnmuteLock,
    UnmuteLockNotify,
}

#[derive(Debug)]
struct MenuItemDeviceInfo {
    device_id: String,
    setting_type: DeviceSettingType,
    name: String,
    device_type: DeviceType,
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
                if event.id == auto_launch_check_item.id() {
                    let checked = auto_launch_check_item.is_checked();
                    if checked {
                        auto_launch.enable().unwrap();
                    } else {
                        auto_launch.disable().unwrap();
                    }
                } else if event.id == quit_item.id() {
                    tray_icon.take();
                    *control_flow = ControlFlow::Exit;
                } else if let Some(menu_info) = menu_id_to_device.get(&event.id) {
                    // Check if the menu item is checked
                    if let Some(item) = find_menu_item(&tray_menu, &event.id)
                        && let Some(check_item) = item.as_check_menuitem()
                    {
                        let is_checked = check_item.is_checked();
                        let mut should_remove = false;

                        {
                            let device_settings = persistent_state
                                .devices
                                .entry(menu_info.device_id.clone())
                                .or_insert_with(|| DeviceSettings {
                                    is_volume_locked: false,
                                    volume_percent: 0.0,
                                    notify_on_volume_lock: false,
                                    is_unmute_locked: false,
                                    notify_on_unmute_lock: false,
                                    device_type: menu_info.device_type,
                                    name: menu_info.name.clone(),
                                });

                            match menu_info.setting_type {
                                DeviceSettingType::VolumeLock => {
                                    device_settings.is_volume_locked = is_checked;
                                    // Update target volume to current volume when locking
                                    if is_checked
                                        && let Ok(device) = get_device_by_id(
                                            &device_enumerator,
                                            &menu_info.device_id,
                                        )
                                        && let Ok(endpoint) = get_audio_endpoint(&device)
                                        && let Ok(vol) = get_volume(&endpoint)
                                        {
                                            device_settings.volume_percent =
                                                convert_float_to_percent(vol);
                                        }
                                }
                                DeviceSettingType::VolumeLockNotify => {
                                    device_settings.notify_on_volume_lock = is_checked;
                                }
                                DeviceSettingType::UnmuteLock => {
                                    device_settings.is_unmute_locked = is_checked;
                                }
                                DeviceSettingType::UnmuteLockNotify => {
                                    device_settings.notify_on_unmute_lock = is_checked;
                                }
                            }

                            if !device_settings.is_volume_locked
                                && !device_settings.is_unmute_locked
                                && !device_settings.notify_on_volume_lock
                                && !device_settings.notify_on_unmute_lock
                            {
                                should_remove = true;
                            }
                        }

                        if should_remove {
                            persistent_state.devices.remove(&menu_info.device_id);
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
                        let is_muted = get_mute(&endpoint).unwrap_or(false);
                        let is_default =
                            is_default_device(&device_enumerator, &device, device_type);

                        let (is_volume_locked, notify_on_volume_lock, is_unmute_locked, notify_on_unmute_lock) =
                            if let Some(settings) = persistent_state.devices.get(&device_id) {
                                (
                                    settings.is_volume_locked,
                                    settings.notify_on_volume_lock,
                                    settings.is_unmute_locked,
                                    settings.notify_on_unmute_lock,
                                )
                            } else {
                                (false, false, false, false)
                            };

                        let is_locked = is_volume_locked || is_unmute_locked;
                        let label = to_label(&name, volume_percent, is_default, is_locked, is_muted);

                        let submenu = Submenu::new(&label, true);

                        let volume_lock_item = CheckMenuItem::new(
                            "Keep volume locked",
                            true,
                            is_volume_locked,
                            None,
                        );
                        let volume_notify_item = CheckMenuItem::new(
                            "Notify on volume restore",
                            is_volume_locked,
                            notify_on_volume_lock,
                            None,
                        );
                        let unmute_lock_item = CheckMenuItem::new(
                            "Keep unmuted",
                            true,
                            is_unmute_locked,
                            None,
                        );
                        let unmute_notify_item = CheckMenuItem::new(
                            "Notify on unmute",
                            is_unmute_locked,
                            notify_on_unmute_lock,
                            None,
                        );

                        menu_id_to_device.insert(
                            volume_lock_item.id().clone(),
                            MenuItemDeviceInfo {
                                device_id: device_id.clone(),
                                setting_type: DeviceSettingType::VolumeLock,
                                name: name.clone(),
                                device_type,
                            },
                        );
                        menu_id_to_device.insert(
                            volume_notify_item.id().clone(),
                            MenuItemDeviceInfo {
                                device_id: device_id.clone(),
                                setting_type: DeviceSettingType::VolumeLockNotify,
                                name: name.clone(),
                                device_type,
                            },
                        );
                        menu_id_to_device.insert(
                            unmute_lock_item.id().clone(),
                            MenuItemDeviceInfo {
                                device_id: device_id.clone(),
                                setting_type: DeviceSettingType::UnmuteLock,
                                name: name.clone(),
                                device_type,
                            },
                        );
                        menu_id_to_device.insert(
                            unmute_notify_item.id().clone(),
                            MenuItemDeviceInfo {
                                device_id: device_id.clone(),
                                setting_type: DeviceSettingType::UnmuteLockNotify,
                                name: name.clone(),
                                device_type,
                            },
                        );

                        // Ensure device exists in persistent state to facilitate updates
                        if let Some(settings) = persistent_state.devices.get_mut(&device_id) {
                            settings.name = name.clone();
                            settings.device_type = device_type;
                        }

                        submenu.append(&volume_lock_item).unwrap();
                        submenu.append(&unmute_lock_item).unwrap();
                        submenu.append(&PredefinedMenuItem::separator()).unwrap();
                        submenu.append(&volume_notify_item).unwrap();
                        submenu.append(&unmute_notify_item).unwrap();
                        tray_menu.append(&submenu).unwrap();
                    }
                    tray_menu.append(&PredefinedMenuItem::separator()).unwrap();
                }

                // Refresh check items
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

                // We need to check if the device is in our managed list
                if let Some(device_settings) = persistent_state.devices.get_mut(&device_id) {
                    // Check volume lock
                    if device_settings.is_volume_locked {
                        let target_volume_percent = device_settings.volume_percent;
                        if new_volume_percent != target_volume_percent {
                            let target_volume = convert_percent_to_float(target_volume_percent);
                            let device = match get_device_by_id(&device_enumerator, &device_id) {
                                Ok(d) => d,
                                Err(e) => {
                                    log::error!(
                                        "Failed to get device by id for {}: {}",
                                        device_settings.name,
                                        e
                                    );
                                    return;
                                }
                            };
                            let device_name =
                                get_device_name(&device).unwrap_or_else(|_| device_settings.name.clone());
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
                            if device_settings.notify_on_volume_lock {
                                send_notification_debounced(
                                    &device_id,
                                    "Volume Restored",
                                    &format!(
                                        "The volume of {device_name} has been restored from {new_volume_percent}% to {target_volume_percent}%."
                                    ),
                                    &mut last_notification_times,
                                );
                            }
                        }
                    }

                    // Check unmute lock
                    if device_settings.is_unmute_locked {
                        let device_name = device_settings.name.clone();
                        let notification_title = match device_settings.device_type {
                            DeviceType::Input => "Input Device Unmuted",
                            DeviceType::Output => "Output Device Unmuted",
                        };
                        let notification_suffix = match device_settings.device_type {
                            DeviceType::Input => "was unmuted due to Keep unmuted setting.",
                            DeviceType::Output => "was unmuted due to Keep unmuted setting.",
                        };

                        check_and_unmute_device(
                            &device_enumerator,
                            &device_id,
                            &device_name,
                            device_settings.notify_on_unmute_lock,
                            notification_title,
                            notification_suffix,
                            &mut last_notification_times,
                        );
                    }
                }
            }

            Event::UserEvent(UserEvent::DevicesChanged) => {
                log::info!("Reloading list of watched devices...");

                watched_endpoints.clear();
                let mut some_locked = false;

                for (device_id, device_settings) in persistent_state.devices.iter() {
                    // Only watch if at least one setting is enabled
                    if !device_settings.is_volume_locked && !device_settings.is_unmute_locked {
                        continue;
                    }

                    let device = match get_device_by_id(&device_enumerator, device_id) {
                        Ok(device) => device,
                        Err(e) => {
                            log::warn!(
                                "Not watching volume of {} as failed to get its device by id: {}",
                                device_settings.name,
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
                                device_settings.name,
                                e
                            );
                            continue;
                        }
                    };
                    if device_state != DEVICE_STATE_ACTIVE {
                        log::info!(
                            "Not watching volume of {} as it is not active",
                            device_settings.name
                        );
                        continue;
                    }

                    let endpoint = match get_audio_endpoint(&device) {
                        Ok(ep) => ep,
                        Err(e) => {
                            log::warn!(
                                "Not watching volume of {} as failed to get its endpoint: {}",
                                device_settings.name,
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
                            device_settings.name,
                            e
                        );
                        continue;
                    }
                    watched_endpoints.push(endpoint.clone());
                    log::info!(
                        "Watching volume of {} (Locked: {}, Unmute: {})",
                        device_settings.name,
                        device_settings.is_volume_locked,
                        device_settings.is_unmute_locked
                    );

                    let _ = main_proxy.send_event(UserEvent::VolumeChanged(
                        VolumeChangedEvent {
                            device_id: device_id.clone(),
                            new_volume: None,
                        },
                    ));

                    // Enforce unmute on refresh if enabled
                    if device_settings.is_unmute_locked {
                        let notification_title = match device_settings.device_type {
                            DeviceType::Input => "Input Device Unmuted",
                            DeviceType::Output => "Output Device Unmuted",
                        };
                        let notification_suffix = match device_settings.device_type {
                            DeviceType::Input => "was unmuted due to Keep unmuted setting.",
                            DeviceType::Output => "was unmuted due to Keep unmuted setting.",
                        };

                        check_and_unmute_device(
                            &device_enumerator,
                            device_id,
                            &device_settings.name,
                            device_settings.notify_on_unmute_lock,
                            notification_title,
                            notification_suffix,
                            &mut last_notification_times,
                        );
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

fn to_label(
    name: &str,
    volume_percent: f32,
    is_default: bool,
    is_locked: bool,
    is_muted: bool,
) -> String {
    let default_indicator = if is_default { " · ☆" } else { "" };
    let locked_indicator = if is_locked { " · 🔒" } else { "" };
    let muted_indicator = if is_muted { " 🚫" } else { "" };
    format!("{name}{default_indicator} · {volume_percent}%{muted_indicator}{locked_indicator}")
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
    let mut devices_to_migrate: Vec<(String, DeviceSettings)> = Vec::new();
    let mut devices_to_update: Vec<(String, DeviceSettings)> = Vec::new();

    // Check which devices need migration
    for (device_id, device_settings) in persistent_state.devices.iter() {
        if let Ok(device) = get_device_by_id(device_enumerator, device_id) {
            // Device exists, check if name has changed
            if let Ok(current_name) = get_device_name(&device)
                && current_name != device_settings.name
            {
                log::info!(
                    "Device {} with ID {} had the name changed to {}",
                    device_settings.name,
                    device_id,
                    current_name,
                );
                let mut updated_settings = device_settings.clone();
                updated_settings.name = current_name;
                devices_to_update.push((device_id.clone(), updated_settings));
            }
        } else {
            devices_to_migrate.push((device_id.clone(), device_settings.clone()));
        }
    }

    // Apply the name updates
    for (device_id, updated_settings) in devices_to_update {
        persistent_state.devices.insert(device_id, updated_settings);
    }

    // Attempt to migrate each device
    for (old_device_id, device_settings) in devices_to_migrate {
        let device_name = device_settings.name.clone();
        if let Ok(new_device_id) = find_device_by_name_and_type(
            device_enumerator,
            &device_name,
            device_settings.device_type,
        ) {
            // Swap the old device with the new one
            persistent_state.devices.remove(&old_device_id);
            persistent_state
                .devices
                .insert(new_device_id.clone(), device_settings);
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

fn send_notification_debounced(
    device_id: &str,
    title: &str,
    message: &str,
    last_notification_times: &mut HashMap<String, Instant>,
) {
    let now = Instant::now();
    let should_notify = match last_notification_times.get(device_id) {
        Some(&last_time) => now.duration_since(last_time) > Duration::from_secs(5),
        None => true,
    };
    if should_notify {
        if let Err(e) = Toast::new(APP_AUMID).title(title).text1(message).show() {
            log::error!("Failed to show notification for {title}: {e}");
        }
        last_notification_times.insert(device_id.to_string(), now);
    }
}

fn check_and_unmute_device(
    device_enumerator: &IMMDeviceEnumerator,
    device_id: &str,
    device_name: &str,
    notify: bool,
    notification_title: &str,
    notification_message_suffix: &str,
    last_notification_times: &mut HashMap<String, Instant>,
) {
    let device = match get_device_by_id(device_enumerator, device_id) {
        Ok(d) => d,
        Err(_) => return,
    };
    let endpoint = match get_audio_endpoint(&device) {
        Ok(ep) => ep,
        Err(_) => return,
    };

    if let Ok(true) = get_mute(&endpoint) {
        if let Err(e) = set_mute(&endpoint, false) {
            log::error!("Failed to unmute {device_name}: {e}");
        } else {
            log::info!("Unmuted {device_name} due to lock settings");
            if notify {
                let message = format!("{device_name} {notification_message_suffix}");
                send_notification_debounced(
                    device_id,
                    notification_title,
                    &message,
                    last_notification_times,
                );
            }
        }
    }
}

fn find_menu_item(menu: &Menu, id: &MenuId) -> Option<MenuItemKind> {
    find_in_items(&menu.items(), id)
}

fn find_in_items(items: &[MenuItemKind], id: &MenuId) -> Option<MenuItemKind> {
    for item in items {
        if item.id() == id {
            return Some(item.clone());
        }
        if let Some(submenu) = item.as_submenu()
            && let Some(sub_item) = find_in_items(&submenu.items(), id)
        {
            return Some(sub_item);
        }
    }
    None
}
