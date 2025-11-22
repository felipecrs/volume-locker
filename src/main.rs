#![cfg_attr(
    all(target_os = "windows", not(debug_assertions),),
    windows_subsystem = "windows"
)]

mod audio;
mod config;
mod consts;
mod types;
mod ui;
mod utils;

use crate::audio::{
    AudioDevicesChangedCallback, VolumeChangeCallback, check_and_unmute_device,
    convert_float_to_percent, convert_percent_to_float, create_device_enumerator,
    enforce_priorities, get_audio_endpoint, get_device_by_id, get_device_name, get_device_state,
    get_unmute_notification_details, get_volume, migrate_device_ids,
    register_control_change_notify, register_notification_callback, set_volume,
};
use crate::config::{load_state, save_state};
use crate::consts::{APP_AUMID, APP_NAME, APP_UID, LOG_FILE_NAME};
use crate::types::{
    DeviceSettingType, DeviceSettings, DeviceType, MenuItemDeviceInfo, UserEvent,
    VolumeChangedEvent,
};
use crate::ui::{find_menu_item, rebuild_tray_menu};
use crate::utils::{
    get_executable_directory, get_executable_path, send_notification_debounced, setup_app_aumid,
};
use auto_launch::AutoLaunchBuilder;
use faccess::PathExt;
use simplelog::*;
use single_instance::SingleInstance;
use std::collections::HashMap;
use std::fs::File;
use std::time::Instant;
use tao::{
    event::Event,
    event_loop::{ControlFlow, EventLoopBuilder},
};
use tauri_winrt_notification::Toast;
use tray_icon::{
    MouseButton, TrayIconBuilder, TrayIconEvent,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem},
};
use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolume, IAudioEndpointVolumeCallback};
use windows::Win32::Media::Audio::{DEVICE_STATE_ACTIVE, IMMNotificationClient};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

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
    let device_enumerator = create_device_enumerator().unwrap();

    let devices_changed_callback: IMMNotificationClient = AudioDevicesChangedCallback {
        proxy: event_loop.create_proxy(),
    }
    .into();
    register_notification_callback(&device_enumerator, &devices_changed_callback).unwrap();

    let mut watched_endpoints: Vec<IAudioEndpointVolume> = Vec::new();

    let mut last_notification_times: HashMap<String, Instant> = HashMap::new();

    let mut temporary_priority_output: Option<String> = None;
    let mut temporary_priority_input: Option<String> = None;

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
                    let mut should_save = false;

                    match menu_info.setting_type {
                        DeviceSettingType::VolumeLock
                        | DeviceSettingType::VolumeLockNotify
                        | DeviceSettingType::UnmuteLock
                        | DeviceSettingType::UnmuteLockNotify => {
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
                                            if is_checked {
                                                if let Ok(device) = get_device_by_id(
                                                    &device_enumerator,
                                                    &menu_info.device_id,
                                                )
                                                && let Ok(endpoint) = get_audio_endpoint(&device)
                                                && let Ok(vol) = get_volume(&endpoint)
                                                {
                                                    device_settings.volume_percent =
                                                        convert_float_to_percent(vol);
                                                    device_settings.is_volume_locked = true;
                                                } else {
                                                    log::error!(
                                                        "Failed to get volume for device {}, cannot lock.",
                                                        menu_info.name
                                                    );
                                                    device_settings.is_volume_locked = false;
                                                }
                                            } else {
                                                device_settings.is_volume_locked = false;
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
                                        _ => {}
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
                                    let is_in_priority = persistent_state
                                        .output_priority_list
                                        .contains(&menu_info.device_id)
                                        || persistent_state
                                            .input_priority_list
                                            .contains(&menu_info.device_id);

                                    if !is_in_priority {
                                        persistent_state.devices.remove(&menu_info.device_id);
                                    }
                                }
                                should_save = true;
                            }
                        }
                        DeviceSettingType::AddToPriority => {
                            let list = match menu_info.device_type {
                                DeviceType::Output => &mut persistent_state.output_priority_list,
                                DeviceType::Input => &mut persistent_state.input_priority_list,
                            };
                            if !list.contains(&menu_info.device_id) {
                                list.push(menu_info.device_id.clone());

                                persistent_state
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

                                should_save = true;
                            }
                        }
                        DeviceSettingType::RemoveFromPriority => {
                            let list = match menu_info.device_type {
                                DeviceType::Output => &mut persistent_state.output_priority_list,
                                DeviceType::Input => &mut persistent_state.input_priority_list,
                            };
                            if let Some(pos) = list.iter().position(|x| x == &menu_info.device_id) {
                                list.remove(pos);
                                should_save = true;

                                if let Some(settings) =
                                    persistent_state.devices.get(&menu_info.device_id)
                                    && !settings.is_volume_locked
                                        && !settings.is_unmute_locked
                                        && !settings.notify_on_volume_lock
                                        && !settings.notify_on_unmute_lock
                                    {
                                        persistent_state.devices.remove(&menu_info.device_id);
                                    }
                            }
                        }
                        DeviceSettingType::MovePriorityUp => {
                            let list = match menu_info.device_type {
                                DeviceType::Output => &mut persistent_state.output_priority_list,
                                DeviceType::Input => &mut persistent_state.input_priority_list,
                            };
                            if let Some(pos) = list.iter().position(|x| x == &menu_info.device_id)
                                && pos > 0 {
                                    list.swap(pos, pos - 1);
                                    should_save = true;
                                }
                        }
                        DeviceSettingType::MovePriorityDown => {
                            let list = match menu_info.device_type {
                                DeviceType::Output => &mut persistent_state.output_priority_list,
                                DeviceType::Input => &mut persistent_state.input_priority_list,
                            };
                            if let Some(pos) = list.iter().position(|x| x == &menu_info.device_id)
                                && pos < list.len() - 1 {
                                    list.swap(pos, pos + 1);
                                    should_save = true;
                                }
                        }
                        DeviceSettingType::PriorityRestoreNotify => {
                            if let Some(item) = find_menu_item(&tray_menu, &event.id)
                                && let Some(check_item) = item.as_check_menuitem()
                            {
                                let is_checked = check_item.is_checked();
                                match menu_info.device_type {
                                    DeviceType::Output => {
                                        persistent_state.notify_on_priority_restore_output =
                                            is_checked
                                    }
                                    DeviceType::Input => {
                                        persistent_state.notify_on_priority_restore_input =
                                            is_checked
                                    }
                                }
                                should_save = true;
                            }
                        }
                        DeviceSettingType::SwitchCommunicationDevice => {
                            if let Some(item) = find_menu_item(&tray_menu, &event.id)
                                && let Some(check_item) = item.as_check_menuitem()
                            {
                                let is_checked = check_item.is_checked();
                                match menu_info.device_type {
                                    DeviceType::Output => {
                                        persistent_state.switch_communication_device_output =
                                            is_checked
                                    }
                                    DeviceType::Input => {
                                        persistent_state.switch_communication_device_input =
                                            is_checked
                                    }
                                }
                                should_save = true;
                            }
                        }
                        DeviceSettingType::SetTemporaryPriority => {
                            if let Some(item) = find_menu_item(&tray_menu, &event.id) {
                                let is_checked = if let Some(check_item) = item.as_check_menuitem()
                                {
                                    check_item.is_checked()
                                } else {
                                    // If it's not a check item (e.g. "Unset Temporary Default"), we treat it as unchecking
                                    false
                                };

                                match menu_info.device_type {
                                    DeviceType::Output => {
                                        temporary_priority_output = if is_checked {
                                            Some(menu_info.device_id.clone())
                                        } else {
                                            None
                                        };
                                    }
                                    DeviceType::Input => {
                                        temporary_priority_input = if is_checked {
                                            Some(menu_info.device_id.clone())
                                        } else {
                                            None
                                        };
                                    }
                                }
                                let _ = main_proxy.send_event(UserEvent::DevicesChanged);
                            }
                        }
                    }

                    if should_save {
                        let _ = main_proxy.send_event(UserEvent::ConfigurationChanged);
                    }
                }
            }

            // On right or left click of tray icon: reload the menu
            Event::UserEvent(UserEvent::TrayIcon(TrayIconEvent::Click { button, .. }))
                if button == MouseButton::Right || button == MouseButton::Left =>
            {
                menu_id_to_device = rebuild_tray_menu(
                    &tray_menu,
                    &device_enumerator,
                    &mut persistent_state,
                    &temporary_priority_output,
                    &temporary_priority_input,
                    auto_launch.is_enabled().unwrap(),
                    &auto_launch_check_item,
                    &quit_item,
                    &output_devices_heading_item,
                    &input_devices_heading_item,
                );
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
                                    &format!("volume_restore_{}", device_id),
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
                        let (notification_title, notification_suffix) =
                            get_unmute_notification_details(device_settings.device_type);

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

                enforce_priorities(
                    &device_enumerator,
                    &persistent_state,
                    &mut last_notification_times,
                    &temporary_priority_output,
                    &temporary_priority_input,
                );

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

                    let device_state = match get_device_state(&device) {
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
                        register_control_change_notify(&endpoint, &volume_callback)
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
                        let (notification_title, notification_suffix) =
                            get_unmute_notification_details(device_settings.device_type);

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
