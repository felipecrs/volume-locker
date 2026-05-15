#![cfg_attr(
    all(target_os = "windows", not(debug_assertions),),
    windows_subsystem = "windows"
)]

mod audio;
mod config;
mod consts;
mod notification;
mod platform;
mod types;
mod ui;
mod update;
mod utils;

use crate::audio::{
    AudioBackend, AudioBackendImpl, AudioDevice, check_and_unmute_device, enforce_priorities,
    enforce_volume_lock, collect_device_names, migrate_device_ids,
};
use crate::config::{PersistentState, load_state, save_state};
use crate::consts::{APP_NAME, APP_UID, CURRENT_VERSION, LOG_FILE_NAME};
use crate::platform::{NotificationDuration, init_platform, send_notification};
use crate::types::{
    DeviceId, TemporaryPriorities, UserEvent, VolumeChangedEvent, VolumeScalar,
};
use crate::ui::{
    MenuContext, MenuEventContext, MenuEventResult, MenuIdMap, TrayMenuItems, handle_menu_event,
    rebuild_tray_menu,
};
use crate::notification::{NotificationThrottler, log_and_notify_error};
use crate::update::UpdateInfo;
use crate::utils::{
    get_executable_directory, get_executable_path_str,
};
use anyhow::Context;
use auto_launch::AutoLaunch;
use auto_launch::AutoLaunchBuilder;
use crate::platform::{SingleInstanceGuard, is_directory_writable};
use simplelog::{
    ColorChoice, CombinedLogger, Config, LevelFilter, SharedLogger, TermLogger, TerminalMode,
    WriteLogger,
};


use std::fs::File;
use tao::{
    event::Event,
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
};
use tray_icon::{
    MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem},
};

struct AppState {
    persistent_state: PersistentState,
    menu_id_map: MenuIdMap,
    watched_devices: Vec<Box<dyn AudioDevice>>,
    notification_throttler: NotificationThrottler,
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

        let Some(device_settings) = self.persistent_state.device_settings(&device_id) else {
            return;
        };

        let device_name = device_settings.name.clone();
        let device_type = device_settings.device_type;
        let volume_lock = device_settings.volume_lock;
        let unmute_lock = device_settings.unmute_lock;

        let device = match self.backend.device_by_id(&device_id) {
            Ok(d) => d,
            Err(e) => {
                log::error!("Failed to get device by id for {device_name}: {e}");
                return;
            }
        };

        let new_volume: VolumeScalar = match new_volume {
            Some(v) => v,
            None => match device.volume() {
                Ok(v) => v,
                Err(e) => {
                    log::error!("Failed to get volume for {device_id}: {e:#}");
                    return;
                }
            },
        };

        if volume_lock.is_locked {
            enforce_volume_lock(
                &device_id,
                device.as_ref(),
                &device_name,
                volume_lock,
                new_volume,
                &mut self.notification_throttler,
            );
        }

        if unmute_lock.is_locked {
            check_and_unmute_device(
                device.as_ref(),
                device_type,
                unmute_lock.notify,
                &mut self.notification_throttler,
            );
        }
    }

    fn migrate_device_ids_if_needed(&mut self) {
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
    }

    fn rebuild_watched_devices(&mut self, proxy: &EventLoopProxy<UserEvent>) -> bool {
        self.watched_devices.clear();

        // Collect device IDs first to avoid borrow conflict with `self`.
        let locked_device_ids = self.persistent_state.locked_device_ids();

        for device_id in locked_device_ids {
            if let Some(device) = self.try_watch_device(&device_id, proxy) {
                self.watched_devices.push(device);
            }
        }

        !self.watched_devices.is_empty()
    }

    /// Attempts to set up volume monitoring for a single locked device.
    /// Returns the device handle on success, or `None` if the device can't be watched.
    fn try_watch_device(
        &mut self,
        device_id: &DeviceId,
        proxy: &EventLoopProxy<UserEvent>,
    ) -> Option<Box<dyn AudioDevice>> {
        let device_settings = self.persistent_state.device_settings(device_id)?;
        let device_name = &device_settings.name;

        let device = match self.backend.device_by_id(device_id) {
            Ok(d) => d,
            Err(e) => {
                log::warn!("Not watching {device_name}: failed to get device by id: {e}");
                return None;
            }
        };

        if let Ok(false) = device.is_active() {
            log::info!("Not watching {device_name}: device is not active");
            return None;
        }

        let cb_proxy = proxy.clone();
        let cb_device_id = device_id.clone();
        if let Err(e) = device.watch_volume(Box::new(move |vol| {
            let _ = cb_proxy.send_event(UserEvent::VolumeChanged(VolumeChangedEvent {
                device_id: cb_device_id.clone(),
                new_volume: vol,
            }));
        })) {
            log::warn!("Not watching {device_name}: failed to register volume callback: {e}");
            return None;
        }

        if device_settings.unmute_lock.is_locked {
            check_and_unmute_device(
                device.as_ref(),
                device_settings.device_type,
                device_settings.unmute_lock.notify,
                &mut self.notification_throttler,
            );
        }

        log::info!(
            "Watching {device_name} (Locked: {}, Unmute: {})",
            device_settings.volume_lock.is_locked,
            device_settings.unmute_lock.is_locked
        );

        if let Err(e) = proxy.send_event(UserEvent::VolumeChanged(VolumeChangedEvent {
            device_id: device_id.clone(),
            new_volume: None,
        })) {
            log::warn!("Failed to send initial VolumeChanged event: {e:#}");
        }

        Some(device)
    }

    fn update_tray_icon(
        &self,
        any_device_locked: bool,
        locked_icon: &tray_icon::Icon,
        unlocked_icon: &tray_icon::Icon,
    ) {
        if let Some(tray_icon) = &self.tray_icon {
            let icon = if any_device_locked { locked_icon } else { unlocked_icon };
            if let Err(e) = tray_icon.set_icon(Some(icon.clone())) {
                log::error!("Failed to update tray icon: {e:#}");
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

        self.migrate_device_ids_if_needed();
        for (device_id, new_name, device_type) in collect_device_names(&self.backend) {
            if let Some(settings) = self.persistent_state.device_settings_mut(&device_id) {
                settings.name = new_name;
                settings.device_type = device_type;
            }
        }

        enforce_priorities(
            &self.backend,
            &self.persistent_state,
            &mut self.notification_throttler,
            &self.temporary_priorities,
        );

        let any_device_locked = self.rebuild_watched_devices(proxy);

        self.update_tray_icon(any_device_locked, locked_icon, unlocked_icon);
    }

    fn handle_configuration_changed(&mut self, proxy: &EventLoopProxy<UserEvent>) {
        if let Err(e) = save_state(&self.persistent_state) {
            log_and_notify_error(
                "Failed to Save State",
                &format!("Failed to save state: {e:#}"),
            );
            return;
        }
        log::info!("Configuration saved ({} devices tracked)", self.persistent_state.device_count());
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
        if event.id == refs.quit_item.id() {
            self.tray_icon.take();
            *control_flow = ControlFlow::Exit;
        } else if let Some(menu_info) = self.menu_id_map.get(&event.id) {
            let mut ctx = MenuEventContext {
                tray_menu: refs.tray_menu,
                persistent_state: &mut self.persistent_state,
                backend: &self.backend,
                temporary_priorities: &mut self.temporary_priorities,
                update_info: &self.update_info,
            };
            let result = handle_menu_event(event, menu_info, &mut ctx);

            match result {
                MenuEventResult::DevicesChanged => {
                    if let Err(e) = proxy.send_event(UserEvent::DevicesChanged) {
                        log::warn!("Failed to send DevicesChanged event: {e:#}");
                    }
                }
                MenuEventResult::SaveConfig => {
                    if let Err(e) = proxy.send_event(UserEvent::ConfigurationChanged) {
                        log::warn!("Failed to send ConfigurationChanged event: {e:#}");
                    }
                }
                MenuEventResult::UpdatePerform(info) => {
                    match update::install_update(&info) {
                        Ok(()) => {
                            self.tray_icon.take();
                            *control_flow = ControlFlow::Exit;
                        }
                        Err(e) => {
                            log_and_notify_error(
                                "Update Failed",
                                &format!("Update failed: {e:#}"),
                            );
                        }
                    }
                }
                MenuEventResult::UpdateCheck => {
                    self.update_info = update::check_for_update(true).unwrap_or(None);
                }
                MenuEventResult::ToggleAutoLaunch(checked) => {
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
                }
                MenuEventResult::NoChange => {}
            }
        }
    }

    fn handle_tray_click(&mut self, refs: &EventLoopRefs) {
        let ctx = MenuContext {
            backend: &self.backend,
            persistent_state: &self.persistent_state,
            temporary_priorities: &self.temporary_priorities,
            auto_launch_enabled: refs.auto_launch.is_enabled().unwrap_or_else(|e| {
                log::warn!("Failed to check auto-launch state: {e:#}");
                false
            }),
            update_info: &self.update_info,
        };
        match rebuild_tray_menu(
            refs.tray_menu,
            &ctx,
            &TrayMenuItems {
                auto_launch_check: refs.auto_launch_check_item,
                check_updates_on_launch: refs.check_updates_on_launch_item,
                quit: refs.quit_item,
                output_devices_heading: refs.output_devices_heading_item,
                input_devices_heading: refs.input_devices_heading_item,
            },
        ) {
            Ok(map) => {
                self.menu_id_map = map;
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

fn main() -> std::process::ExitCode {
    if let Err(e) = run() {
        eprintln!("Fatal error: {e:#}");
        log::error!("Fatal error: {e:#}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

fn setup_logging(executable_directory: &std::path::Path) -> anyhow::Result<()> {
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

    Ok(())
}

fn ensure_writable_directory(executable_directory: &std::path::Path) -> anyhow::Result<()> {
    if !is_directory_writable(executable_directory) {
        let error_title = "Volume Locker Directory Not Writable";
        let error_message = format!(
            "Please move Volume Locker to a directory that is writable or fix the permissions of '{}'.",
            executable_directory.display(),
        );
        let _ = send_notification(error_title, &error_message, NotificationDuration::Long);
        anyhow::bail!("{error_title}: {error_message}");
    }
    Ok(())
}

fn run() -> anyhow::Result<()> {
    let executable_directory = get_executable_directory()?;
    setup_logging(&executable_directory)?;

    let com_token = init_platform(&executable_directory)?;
    ensure_writable_directory(&executable_directory)?;
    let _instance = SingleInstanceGuard::acquire(APP_UID)
        .context("failed to acquire single instance lock")?;

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
    let backend =
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
    log::info!("Loaded state ({} devices tracked)", persistent_state.device_count());

    let mut app = AppState {
        persistent_state,
        menu_id_map: MenuIdMap::new(),
        watched_devices: Vec::new(),
        notification_throttler: NotificationThrottler::new(),
        temporary_priorities: TemporaryPriorities::default(),
        update_info: None,
        tray_icon: None,
        backend,
    };

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        let make_refs = || EventLoopRefs {
            auto_launch: &auto_launch,
            auto_launch_check_item: &auto_launch_check_item,
            check_updates_on_launch_item: &check_updates_on_launch_item,
            quit_item: &quit_item,
            tray_menu: &tray_menu,
            output_devices_heading_item: &output_devices_heading_item,
            input_devices_heading_item: &input_devices_heading_item,
        };

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
                    app.update_info = update::check_for_update(false).unwrap_or(None);
                }

                if let Err(e) = main_proxy.send_event(UserEvent::DevicesChanged) {
                    log::warn!("Failed to send initial DevicesChanged event: {e:#}");
                }
            }

            Event::UserEvent(UserEvent::Menu(event)) => {
                let refs = make_refs();
                app.handle_menu_click(&event, &refs, &main_proxy, control_flow);
            }

            Event::UserEvent(UserEvent::TrayIcon(TrayIconEvent::Click {
                button,
                button_state: MouseButtonState::Down,
                ..
            })) if button == MouseButton::Right || button == MouseButton::Left => {
                let refs = make_refs();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::tests::MockDevice;
    use crate::types::{DeviceId, VolumeLockPolicy, VolumePercent, VolumeScalar};

    fn make_lock(target_percent: f32, notify: bool) -> VolumeLockPolicy {
        VolumeLockPolicy {
            is_locked: true,
            target_percent: VolumePercent::from(target_percent),
            notify,
        }
    }

    #[test]
    fn enforce_volume_lock_restores_when_volume_differs() {
        let device = MockDevice::new("dev1", "Speaker", true);
        let lock = make_lock(100.0, false);
        let device_id: DeviceId = "dev1".into();
        let mut throttler = NotificationThrottler::new();

        enforce_volume_lock(
            &device_id,
            &device,
            "Speaker",
            lock,
            VolumeScalar::from(0.5_f32),
            &mut throttler,
        );

        assert_eq!(*device.volume.borrow(), 1.0_f32);
    }

    #[test]
    fn enforce_volume_lock_noop_when_volume_matches() {
        let device = MockDevice::new("dev1", "Speaker", true);
        let lock = make_lock(100.0, false);
        let device_id: DeviceId = "dev1".into();
        let mut throttler = NotificationThrottler::new();

        enforce_volume_lock(
            &device_id,
            &device,
            "Speaker",
            lock,
            VolumeScalar::from(1.0_f32),
            &mut throttler,
        );

        // Volume should remain unchanged since it already matches target
        assert_eq!(*device.volume.borrow(), 1.0_f32);
    }
}
