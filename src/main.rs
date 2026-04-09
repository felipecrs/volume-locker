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
mod update;
mod utils;

use crate::audio::{
    AudioBackend, AudioBackendImpl, AudioDevice, check_and_unmute_device, enforce_priorities,
    get_unmute_notification_details, migrate_device_ids,
};
use crate::config::{PersistentState, load_state, save_state};
use crate::consts::{APP_NAME, APP_UID, CURRENT_VERSION, LOG_FILE_NAME};
use crate::platform::{NotificationDuration, init_platform, send_notification};
use crate::types::{MenuItemInfo, TemporaryPriorities, UserEvent, VolumeChangedEvent};
use crate::ui::{
    MenuContext, TrayMenuItems, handle_menu_event, rebuild_tray_menu, sync_device_names,
};
use crate::update::UpdateInfo;
use crate::utils::{
    convert_float_to_percent, convert_percent_to_float, get_executable_directory,
    get_executable_path_str, log_and_notify_error, send_notification_debounced,
};
use anyhow::Context;
use auto_launch::AutoLaunch;
use auto_launch::AutoLaunchBuilder;
use faccess::PathExt;
use simplelog::{
    ColorChoice, CombinedLogger, Config, LevelFilter, SharedLogger, TermLogger, TerminalMode,
    WriteLogger,
};
use single_instance::SingleInstance;
use std::collections::HashMap;
use std::fs::File;
use std::time::Instant;
use tao::{
    event::Event,
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
};
use tray_icon::{
    MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem},
};

struct AppState {
    persistent_state: PersistentState,
    menu_id_to_device: HashMap<MenuId, MenuItemInfo>,
    watched_devices: Vec<Box<dyn AudioDevice>>,
    last_notification_times: HashMap<String, Instant>,
    temporary_priorities: TemporaryPriorities,
    update_info: Option<UpdateInfo>,
    tray_icon: Option<tray_icon::TrayIcon>,
    backend: AudioBackendImpl,
}

struct EventLoopRefs<'a> {
    auto_launch: &'a AutoLaunch,
    auto_launch_check_item: &'a CheckMenuItem,
    check_updates_on_launch_item: &'a CheckMenuItem,
    quit_item: &'a MenuItem,
    tray_menu: &'a Menu,
    output_devices_heading_item: &'a MenuItem,
    input_devices_heading_item: &'a MenuItem,
}

impl AppState {
    fn handle_volume_changed(&mut self, event: VolumeChangedEvent) {
        let VolumeChangedEvent {
            device_id,
            new_volume,
        } = event;

        let device_settings = match self.persistent_state.devices.get(&device_id) {
            Some(s) => s,
            None => return,
        };

        let device = match self.backend.get_device_by_id(&device_id) {
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
                    log::error!("Failed to get volume for {device_id}: {e:#}");
                    return;
                }
            },
        };

        if device_settings.volume_lock.is_locked {
            enforce_volume_lock(
                &device_id,
                device.as_ref(),
                device_settings,
                new_volume,
                &mut self.last_notification_times,
            );
        }

        if device_settings.unmute_lock.is_locked {
            let (notification_title, notification_suffix) =
                get_unmute_notification_details(device_settings.device_type);

            if let Err(e) = check_and_unmute_device(
                device.as_ref(),
                device_settings.unmute_lock.notify,
                notification_title,
                notification_suffix,
                &mut self.last_notification_times,
            ) {
                log::error!("Failed to unmute {}: {e:#}", device_settings.name);
            }
        }
    }

    fn handle_devices_changed(
        &mut self,
        proxy: &EventLoopProxy<UserEvent>,
        locked_icon: &tray_icon::Icon,
        unlocked_icon: &tray_icon::Icon,
    ) {
        log::info!("Reloading list of watched devices...");

        let migrations_occurred = migrate_device_ids(&self.backend, &mut self.persistent_state);

        if migrations_occurred {
            if let Err(e) = save_state(&self.persistent_state) {
                log_and_notify_error(
                    "Failed to Save State",
                    &format!("Failed to save state after device migration: {e:#}"),
                );
            } else {
                log::info!("Saved state after device migration");
            }
        }

        enforce_priorities(
            &self.backend,
            &self.persistent_state,
            &mut self.last_notification_times,
            &self.temporary_priorities,
        );

        self.watched_devices.clear();
        let mut some_locked = false;

        for (device_id, device_settings) in self.persistent_state.devices.iter() {
            if !device_settings.volume_lock.is_locked && !device_settings.unmute_lock.is_locked {
                continue;
            }

            let device = match self.backend.get_device_by_id(device_id) {
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

            let volume_proxy = proxy.clone();
            let dev_id = device_id.clone();
            if let Err(e) = device.watch_volume(Box::new(move |vol| {
                let _ = volume_proxy.send_event(UserEvent::VolumeChanged(VolumeChangedEvent {
                    device_id: dev_id.clone(),
                    new_volume: vol,
                }));
            })) {
                log::warn!(
                    "Not watching volume of {} as failed to register for volume changes: {}",
                    device_settings.name,
                    e
                );
                continue;
            }

            if device_settings.unmute_lock.is_locked {
                let (notification_title, notification_suffix) =
                    get_unmute_notification_details(device_settings.device_type);

                if let Err(e) = check_and_unmute_device(
                    device.as_ref(),
                    device_settings.unmute_lock.notify,
                    notification_title,
                    notification_suffix,
                    &mut self.last_notification_times,
                ) {
                    log::error!("Failed to unmute {}: {e:#}", device_settings.name);
                }
            }

            self.watched_devices.push(device);

            log::info!(
                "Watching volume of {} (Locked: {}, Unmute: {})",
                device_settings.name,
                device_settings.volume_lock.is_locked,
                device_settings.unmute_lock.is_locked
            );

            if let Err(e) = proxy.send_event(UserEvent::VolumeChanged(VolumeChangedEvent {
                device_id: device_id.clone(),
                new_volume: None,
            })) {
                log::warn!("Failed to send VolumeChanged event: {e:#}");
            }

            some_locked = true;
        }

        if let Some(tray_icon) = &self.tray_icon {
            if some_locked {
                if let Err(e) = tray_icon.set_icon(Some(locked_icon.clone())) {
                    log::error!("Failed to update tray icon to locked: {e:#}");
                }
            } else if let Err(e) = tray_icon.set_icon(Some(unlocked_icon.clone())) {
                log::error!("Failed to update tray icon to unlocked: {e:#}");
            }
        }
    }

    fn handle_configuration_changed(&mut self, proxy: &EventLoopProxy<UserEvent>) {
        if let Err(e) = save_state(&self.persistent_state) {
            log_and_notify_error(
                "Failed to Save State",
                &format!("Failed to save state: {e:#}"),
            );
            return;
        }
        log::info!("Saved: {:?}", self.persistent_state);
        if let Err(e) = proxy.send_event(UserEvent::DevicesChanged) {
            log::warn!("Failed to send DevicesChanged event: {e:#}");
        }
    }

    fn handle_menu_click(
        &mut self,
        event: &tray_icon::menu::MenuEvent,
        refs: &EventLoopRefs,
        proxy: &EventLoopProxy<UserEvent>,
        control_flow: &mut ControlFlow,
    ) {
        if event.id == refs.auto_launch_check_item.id() {
            let checked = refs.auto_launch_check_item.is_checked();
            let result = if checked {
                refs.auto_launch.enable()
            } else {
                refs.auto_launch.disable()
            };
            if let Err(e) = result {
                log_and_notify_error(
                    "Failed to Toggle Auto-Launch",
                    &format!("Failed to toggle auto-launch: {e:#}"),
                );
            }
        } else if event.id == refs.check_updates_on_launch_item.id() {
            self.persistent_state.check_updates_on_launch =
                refs.check_updates_on_launch_item.is_checked();
            let _ = proxy.send_event(UserEvent::ConfigurationChanged);
        } else if event.id == refs.quit_item.id() {
            self.tray_icon.take();
            *control_flow = ControlFlow::Exit;
        } else if let Some(menu_info) = self.menu_id_to_device.get(&event.id) {
            let result = handle_menu_event(
                event,
                menu_info,
                refs.tray_menu,
                &mut self.persistent_state,
                &self.backend,
                &mut self.temporary_priorities,
                &self.update_info,
            );

            if result.devices_changed {
                let _ = proxy.send_event(UserEvent::DevicesChanged);
            }

            if result.should_save {
                let _ = proxy.send_event(UserEvent::ConfigurationChanged);
            }

            match result.update_action {
                ui::UpdateAction::Perform(info) => {
                    if update::perform(&info) {
                        self.tray_icon.take();
                        *control_flow = ControlFlow::Exit;
                    }
                }
                ui::UpdateAction::Check => self.update_info = update::check(true),
                ui::UpdateAction::None => {}
            }
        }
    }

    fn handle_tray_click(&mut self, refs: &EventLoopRefs) {
        sync_device_names(&self.backend, &mut self.persistent_state);
        let ctx = MenuContext {
            backend: &self.backend,
            persistent_state: &self.persistent_state,
            temporary_priorities: &self.temporary_priorities,
            auto_launch_enabled: refs.auto_launch.is_enabled().unwrap_or(false),
            update_info: &self.update_info,
        };
        match rebuild_tray_menu(
            refs.tray_menu,
            &ctx,
            &TrayMenuItems {
                auto_launch_check_item: refs.auto_launch_check_item,
                check_updates_on_launch_item: refs.check_updates_on_launch_item,
                quit_item: refs.quit_item,
                output_devices_heading_item: refs.output_devices_heading_item,
                input_devices_heading_item: refs.input_devices_heading_item,
            },
        ) {
            Ok(map) => {
                self.menu_id_to_device = map;
                if let Some(tray_icon) = &self.tray_icon {
                    tray_icon.show_menu();
                }
            }
            Err(e) => {
                log_and_notify_error(
                    "Failed to Rebuild Tray Menu",
                    &format!("Failed to rebuild tray menu: {e:#}"),
                );
            }
        }
    }
}

fn enforce_volume_lock(
    device_id: &str,
    device: &dyn AudioDevice,
    settings: &types::DeviceSettings,
    new_volume: f32,
    last_notification_times: &mut HashMap<String, Instant>,
) {
    let new_volume_percent = convert_float_to_percent(new_volume);
    let target_volume_percent = settings.volume_lock.target_percent;
    if new_volume_percent == target_volume_percent {
        return;
    }

    let target_volume = convert_percent_to_float(target_volume_percent);
    let device_name = device.name();

    if let Err(e) = device.set_volume(target_volume) {
        log::error!("Failed to set volume of {device_name} to {target_volume_percent}%: {e:#}");
        return;
    }
    log::info!(
        "Restored volume of {device_name} from {new_volume_percent}% to {target_volume_percent}%"
    );
    if settings.volume_lock.notify {
        send_notification_debounced(
            &format!("volume_restore_{}", device_id),
            "Volume Restored",
            &format!(
                "The volume of {device_name} has been restored from {new_volume_percent}% to {target_volume_percent}%."
            ),
            last_notification_times,
        );
    }
}

fn main() -> std::process::ExitCode {
    if let Err(e) = run() {
        eprintln!("Fatal error: {e:#}");
        log::error!("Fatal error: {e:#}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

fn run() -> anyhow::Result<()> {
    let executable_directory = get_executable_directory()?;

    let com_token = init_platform(&executable_directory)?;

    if !executable_directory.writable() {
        let error_title = "Volume Locker Directory Not Writable";
        let error_message = format!(
            "Please move Volume Locker to a directory that is writable or fix the permissions of '{}'.",
            executable_directory.display(),
        );

        let _ = send_notification(error_title, &error_message, NotificationDuration::Long);

        anyhow::bail!("{error_title}: {error_message}");
    }

    let log_path = executable_directory.join(LOG_FILE_NAME);
    let loggers: Vec<Box<dyn SharedLogger>> = vec![
        WriteLogger::new(
            LevelFilter::Info,
            Config::default(),
            File::create(&log_path).context("failed to create log file")?,
        ),
        #[cfg(debug_assertions)]
        TermLogger::new(
            LevelFilter::Info,
            Config::default(),
            TerminalMode::Stderr,
            ColorChoice::Auto,
        ),
    ];

    CombinedLogger::init(loggers).context("failed to init logger")?;

    // windows_subsystem = "windows" suppresses stderr, so log panics before exit
    std::panic::set_hook(Box::new(|panic_info| {
        log::error!("Panic occurred: {panic_info}");
    }));

    // Only allow one instance of the application to run at a time
    let instance = SingleInstance::new(APP_UID).context("failed to create single instance")?;
    if !instance.is_single() {
        anyhow::bail!("Another instance is already running.");
    }

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();

    let proxy = event_loop.create_proxy();
    TrayIconEvent::set_event_handler(Some(move |event| {
        if let Err(e) = proxy.send_event(UserEvent::TrayIcon(event)) {
            log::warn!("Failed to send TrayIcon event: {e:#}");
        }
    }));
    TrayIconEvent::receiver();

    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        if let Err(e) = proxy.send_event(UserEvent::Menu(event)) {
            log::warn!("Failed to send Menu event: {e:#}");
        }
    }));
    MenuEvent::receiver();

    let app_path = get_executable_path_str()?;
    let auto_launch = AutoLaunchBuilder::new()
        .set_app_name(APP_NAME)
        .set_app_path(&app_path)
        .build()
        .context("failed to build auto-launch")?;

    let output_devices_heading_item = MenuItem::new("Output devices", false, None);
    let input_devices_heading_item = MenuItem::new("Input devices", false, None);
    let auto_launch_check_item: CheckMenuItem =
        CheckMenuItem::new("Auto-launch on startup", true, false, None);
    let check_updates_on_launch_item: CheckMenuItem =
        CheckMenuItem::new("Check for updates on launch", true, false, None);
    let quit_item = MenuItem::new("Quit", true, None);

    let tray_menu = Menu::new();
    // At least one item must be added to the menu on initialization, otherwise
    // the menu will not be shown on first click
    tray_menu
        .append(&quit_item)
        .context("failed to append initial quit item")?;

    let unlocked_icon = tray_icon::Icon::from_resource_name("volume-unlocked-icon", None)
        .context("failed to load unlocked icon")?;
    let locked_icon = tray_icon::Icon::from_resource_name("volume-locked-icon", None)
        .context("failed to load locked icon")?;

    #[cfg(target_os = "windows")]
    let mut backend =
        AudioBackendImpl::new(&com_token).context("failed to initialize audio backend")?;

    let proxy = event_loop.create_proxy();
    backend
        .register_device_change_callback(Box::new(move || {
            if let Err(e) = proxy.send_event(UserEvent::DevicesChanged) {
                log::warn!("Failed to send DevicesChanged event: {e:#}");
            }
        }))
        .context("failed to register device change callback")?;

    let main_proxy = event_loop.create_proxy();

    let persistent_state = load_state()
        .context("failed to load preferences — exiting to prevent overwriting your preferences")?;
    log::info!("Loaded: {persistent_state:?}");

    let mut app = AppState {
        persistent_state,
        menu_id_to_device: HashMap::new(),
        watched_devices: Vec::new(),
        last_notification_times: HashMap::new(),
        temporary_priorities: TemporaryPriorities {
            output: None,
            input: None,
        },
        update_info: None,
        tray_icon: None,
        backend,
    };

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::NewEvents(tao::event::StartCause::Init) => {
                let tooltip = format!("{APP_NAME} v{CURRENT_VERSION}");
                match TrayIconBuilder::new()
                    .with_menu(Box::new(tray_menu.clone()))
                    .with_tooltip(&tooltip)
                    .with_icon(unlocked_icon.clone())
                    .with_id(APP_UID)
                    .with_menu_on_left_click(false)
                    .with_menu_on_right_click(false)
                    .build()
                {
                    Ok(icon) => app.tray_icon = Some(icon),
                    Err(e) => log::error!("Failed to build tray icon: {e:#}"),
                }

                if app.persistent_state.check_updates_on_launch {
                    app.update_info = update::check(false);
                }

                let _ = main_proxy.send_event(UserEvent::DevicesChanged);
            }

            Event::UserEvent(UserEvent::Menu(event)) => {
                let refs = EventLoopRefs {
                    auto_launch: &auto_launch,
                    auto_launch_check_item: &auto_launch_check_item,
                    check_updates_on_launch_item: &check_updates_on_launch_item,
                    quit_item: &quit_item,
                    tray_menu: &tray_menu,
                    output_devices_heading_item: &output_devices_heading_item,
                    input_devices_heading_item: &input_devices_heading_item,
                };
                app.handle_menu_click(&event, &refs, &main_proxy, control_flow);
            }

            Event::UserEvent(UserEvent::TrayIcon(TrayIconEvent::Click {
                button,
                button_state: MouseButtonState::Down,
                ..
            })) if button == MouseButton::Right || button == MouseButton::Left => {
                let refs = EventLoopRefs {
                    auto_launch: &auto_launch,
                    auto_launch_check_item: &auto_launch_check_item,
                    check_updates_on_launch_item: &check_updates_on_launch_item,
                    quit_item: &quit_item,
                    tray_menu: &tray_menu,
                    output_devices_heading_item: &output_devices_heading_item,
                    input_devices_heading_item: &input_devices_heading_item,
                };
                app.handle_tray_click(&refs);
            }

            Event::UserEvent(UserEvent::VolumeChanged(event)) => {
                app.handle_volume_changed(event);
            }

            Event::UserEvent(UserEvent::DevicesChanged) => {
                app.handle_devices_changed(&main_proxy, &locked_icon, &unlocked_icon);
            }

            Event::UserEvent(UserEvent::ConfigurationChanged) => {
                app.handle_configuration_changed(&main_proxy);
            }

            _ => {}
        }
    })
}
