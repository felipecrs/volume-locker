#![cfg_attr(
    all(target_os = "windows", not(debug_assertions),),
    windows_subsystem = "windows"
)]

mod audio;
mod config;
mod consts;
mod platform;
mod types;
mod ui;
mod utils;

use crate::audio::{
    AudioBackend, AudioBackendImpl, AudioDevice, check_and_unmute_device, enforce_priorities,
    get_unmute_notification_details, migrate_device_ids,
};
use crate::config::{load_state, save_state};
use crate::consts::{APP_NAME, APP_UID, LOG_FILE_NAME};
use crate::platform::{NotificationDuration, init_platform, send_notification, setup_app_aumid};
use crate::types::{MenuItemDeviceInfo, UserEvent, VolumeChangedEvent};
use crate::ui::{handle_menu_event, rebuild_tray_menu};
use crate::utils::{
    convert_float_to_percent, convert_percent_to_float, get_executable_directory,
    get_executable_path, send_notification_debounced,
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
use tray_icon::{
    MouseButton, TrayIconBuilder, TrayIconEvent,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem},
};

fn main() {
    init_platform();

    let executable_directory = get_executable_directory();

    if !executable_directory.writable() {
        let error_title = "Volume Locker Directory Not Writable";
        let error_message = format!(
            "Please move Volume Locker to a directory that is writable or fix the permissions of '{}'.",
            executable_directory.display(),
        );

        eprintln!("{error_title}: {error_message}");

        if let Err(e) = send_notification(error_title, &error_message, NotificationDuration::Long) {
            eprintln!("Failed to show {error_title} notification: {e}");
        }

        std::process::exit(1);
    }

    let log_path = executable_directory.join(LOG_FILE_NAME);
    let loggers: Vec<Box<dyn SharedLogger>> = vec![
        WriteLogger::new(
            LevelFilter::Info,
            Config::default(),
            File::create(&log_path).expect("Failed to create log file"),
        ),
        #[cfg(debug_assertions)]
        TermLogger::new(
            LevelFilter::Info,
            Config::default(),
            TerminalMode::Stderr,
            ColorChoice::Auto,
        ),
    ];

    CombinedLogger::init(loggers).expect("Failed to init logger");

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
        .expect("Failed to build auto launch");

    let output_devices_heading_item = MenuItem::new("Output devices", false, None);
    let input_devices_heading_item = MenuItem::new("Input devices", false, None);
    let auto_launch_check_item: CheckMenuItem =
        CheckMenuItem::new("Auto launch on startup", true, false, None);
    let quit_item = MenuItem::new("Quit", true, None);

    let tray_menu = Menu::new();
    // At least one item must be added to the menu on initialization, otherwise
    // the menu will not be shown on first click
    tray_menu
        .append(&quit_item)
        .expect("Failed to append quit item");

    let mut tray_icon = None;

    let unlocked_icon = tray_icon::Icon::from_resource_name("volume-unlocked-icon", None).unwrap();
    let locked_icon = tray_icon::Icon::from_resource_name("volume-locked-icon", None).unwrap();

    let mut menu_id_to_device: HashMap<MenuId, MenuItemDeviceInfo> = HashMap::new();

    #[cfg(target_os = "windows")]
    let mut backend = AudioBackendImpl::new().expect("Failed to initialize audio backend");

    let proxy = event_loop.create_proxy();
    backend
        .register_device_change_callback(Box::new(move || {
            let _ = proxy.send_event(UserEvent::DevicesChanged);
        }))
        .expect("Failed to register device change callback");

    let mut watched_devices: Vec<Box<dyn AudioDevice>> = Vec::new();

    let mut last_notification_times: HashMap<String, Instant> = HashMap::new();

    let mut temporary_priority_output: Option<String> = None;
    let mut temporary_priority_input: Option<String> = None;

    let main_proxy = event_loop.create_proxy();

    let mut persistent_state = load_state();
    log::info!("Loaded: {persistent_state:?}");

    // Migrate device IDs if they have changed
    migrate_device_ids(&backend, &mut persistent_state);

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
                        .expect("Failed to build tray icon"),
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
                    let result = handle_menu_event(
                        &event,
                        menu_info,
                        &tray_menu,
                        &mut persistent_state,
                        &backend,
                        &mut temporary_priority_output,
                        &mut temporary_priority_input,
                    );

                    if result.devices_changed {
                        let _ = main_proxy.send_event(UserEvent::DevicesChanged);
                    }

                    if result.should_save {
                        let _ = main_proxy.send_event(UserEvent::ConfigurationChanged);
                    }
                }
            }

            Event::UserEvent(UserEvent::TrayIcon(TrayIconEvent::Click { button, .. }))
                if button == MouseButton::Right || button == MouseButton::Left =>
            {
                menu_id_to_device = rebuild_tray_menu(
                    &tray_menu,
                    &backend,
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

                // We need to check if the device is in our managed list
                if let Some(device_settings) = persistent_state.devices.get_mut(&device_id) {
                    let device = match backend.get_device_by_id(&device_id) {
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

                    let new_volume: f32 = match new_volume {
                        Some(v) => v,
                        None => match device.volume() {
                            Ok(v) => v,
                            Err(e) => {
                                log::error!("Failed to get volume for {device_id}: {e}");
                                return;
                            }
                        },
                    };
                    let new_volume_percent = convert_float_to_percent(new_volume);

                    // Check volume lock
                    if device_settings.is_volume_locked {
                        let target_volume_percent = device_settings.volume_percent;
                        if new_volume_percent != target_volume_percent {
                            let target_volume = convert_percent_to_float(target_volume_percent);
                            let device_name = device.name();

                            if let Err(e) = device.set_volume(target_volume) {
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
                            device.as_ref(),
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
                    &backend,
                    &persistent_state,
                    &mut last_notification_times,
                    &temporary_priority_output,
                    &temporary_priority_input,
                );

                watched_devices.clear();
                let mut some_locked = false;

                for (device_id, device_settings) in persistent_state.devices.iter() {
                    // Only watch if at least one setting is enabled
                    if !device_settings.is_volume_locked && !device_settings.is_unmute_locked {
                        continue;
                    }

                    let device = match backend.get_device_by_id(device_id) {
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

                    if let Ok(false) = device.is_active() {
                        log::info!(
                            "Not watching volume of {} as it is not active",
                            device_settings.name
                        );
                        continue;
                    }

                    let proxy = main_proxy.clone();
                    let dev_id = device_id.clone();
                    if let Err(e) = device.watch_volume(Box::new(move |vol| {
                        let _ = proxy.send_event(UserEvent::VolumeChanged(
                            VolumeChangedEvent {
                                device_id: dev_id.clone(),
                                new_volume: vol,
                            },
                        ));
                    })) {
                        log::warn!(
                            "Not watching volume of {} as failed to register for volume changes: {}",
                            device_settings.name,
                            e
                        );
                        continue;
                    }

                    // Enforce unmute on refresh if enabled
                    if device_settings.is_unmute_locked {
                        let (notification_title, notification_suffix) =
                            get_unmute_notification_details(device_settings.device_type);

                        check_and_unmute_device(
                            device.as_ref(),
                            &device_settings.name,
                            device_settings.notify_on_unmute_lock,
                            notification_title,
                            notification_suffix,
                            &mut last_notification_times,
                        );
                    }

                    watched_devices.push(device);

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
