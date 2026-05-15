#![cfg_attr(
    all(target_os = "windows", not(debug_assertions),),
    windows_subsystem = "windows"
)]

mod app;
mod audio;
mod config;
mod consts;
mod notification;
mod platform;
mod types;
mod ui;
mod update;
mod utils;

use crate::app::{AppState, EventLoopRefs};
use crate::audio::AudioBackend;
use crate::audio::AudioBackendImpl;
use crate::config::load_state;
use crate::consts::{APP_NAME, APP_UID, LOG_FILE_NAME};
use crate::notification::NotificationThrottler;
use crate::platform::{
    NotificationDuration, SingleInstanceGuard, init_platform, is_directory_writable,
    send_notification,
};
use crate::types::{TemporaryPriorities, UserEvent};
use crate::ui::MenuIdMap;
use crate::utils::{get_executable_directory, get_executable_path_str};
use anyhow::Context;
use auto_launch::{AutoLaunch, AutoLaunchBuilder};
use simplelog::{
    ColorChoice, CombinedLogger, Config, LevelFilter, SharedLogger, TermLogger, TerminalMode,
    WriteLogger,
};
use std::fs::File;
use tao::{
    event::Event,
    event_loop::{ControlFlow, EventLoopBuilder},
};
use tray_icon::{
    MouseButton, MouseButtonState, TrayIconEvent,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem},
};

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

fn wire_event_proxies(event_loop: &tao::event_loop::EventLoop<UserEvent>) {
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
}

fn create_auto_launch() -> anyhow::Result<AutoLaunch> {
    let app_path = get_executable_path_str()?;
    AutoLaunchBuilder::new()
        .set_app_name(APP_NAME)
        .set_app_path(&app_path)
        .build()
        .context("failed to build auto-launch")
}

fn run() -> anyhow::Result<()> {
    let executable_directory = get_executable_directory()?;
    setup_logging(&executable_directory)?;

    let com_token = init_platform(&executable_directory)?;
    ensure_writable_directory(&executable_directory)?;
    let _instance =
        SingleInstanceGuard::acquire(APP_UID).context("failed to acquire single instance lock")?;

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    wire_event_proxies(&event_loop);

    let auto_launch = create_auto_launch()?;

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
    log::info!(
        "Loaded state ({} devices tracked)",
        persistent_state.device_count()
    );

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
                app.handle_init(&tray_menu, &unlocked_icon, &main_proxy);
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
    use crate::audio::{enforce_volume_lock, tests::MockDevice};
    use crate::notification::NotificationThrottler;
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
