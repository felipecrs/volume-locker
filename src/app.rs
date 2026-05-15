use crate::audio::{
    AudioBackend, AudioBackendImpl, AudioDevice, check_and_unmute_device, collect_device_names,
    enforce_priorities, enforce_volume_lock, migrate_device_ids,
};
use crate::config::{save_state, PersistentState};
use crate::consts::{APP_NAME, APP_UID, CURRENT_VERSION};
use crate::notification::{log_and_notify_error, NotificationThrottler};
use crate::types::{
    DeviceId, TemporaryPriorities, UserEvent, VolumeChangedEvent, VolumeScalar,
};
use crate::ui::{
    MenuContext, MenuEventContext, MenuEventResult, MenuIdMap, TrayMenuItems, handle_menu_event,
    rebuild_tray_menu,
};
use crate::update;
use crate::update::UpdateInfo;
use auto_launch::AutoLaunch;
use tao::event_loop::{ControlFlow, EventLoopProxy};
use tray_icon::menu::{CheckMenuItem, Menu, MenuItem};
use tray_icon::TrayIconBuilder;

pub struct AppState {
    pub persistent_state: PersistentState,
    pub menu_id_map: MenuIdMap,
    pub watched_devices: Vec<Box<dyn AudioDevice>>,
    pub notification_throttler: NotificationThrottler,
    pub temporary_priorities: TemporaryPriorities,
    pub update_info: Option<UpdateInfo>,
    pub tray_icon: Option<tray_icon::TrayIcon>,
    pub backend: AudioBackendImpl,
}

pub struct EventLoopRefs<'a> {
    pub auto_launch: &'a AutoLaunch,
    pub auto_launch_check_item: &'a CheckMenuItem,
    pub check_updates_on_launch_item: &'a CheckMenuItem,
    pub quit_item: &'a MenuItem,
    pub tray_menu: &'a Menu,
    pub output_devices_heading_item: &'a MenuItem,
    pub input_devices_heading_item: &'a MenuItem,
}

impl AppState {
    pub fn handle_volume_changed(&mut self, event: VolumeChangedEvent) {
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

    pub fn migrate_device_ids_if_needed(&mut self) {
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

    pub fn rebuild_watched_devices(&mut self, proxy: &EventLoopProxy<UserEvent>) -> bool {
        self.watched_devices.clear();

        let locked_device_ids = self.persistent_state.locked_device_ids();

        for device_id in locked_device_ids {
            if let Some(device) = self.try_watch_device(&device_id, proxy) {
                self.watched_devices.push(device);
            }
        }

        !self.watched_devices.is_empty()
    }

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

    pub fn handle_devices_changed(
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

    pub fn handle_configuration_changed(&mut self, proxy: &EventLoopProxy<UserEvent>) {
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

    pub fn handle_menu_click(
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

    pub fn handle_init(
        &mut self,
        tray_menu: &Menu,
        unlocked_icon: &tray_icon::Icon,
        proxy: &EventLoopProxy<UserEvent>,
    ) {
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
            Ok(icon) => self.tray_icon = Some(icon),
            Err(e) => log::error!("Failed to build tray icon: {e:#}"),
        }

        if self.persistent_state.check_updates_on_launch {
            self.update_info = update::check_for_update(false).unwrap_or(None);
        }

        if let Err(e) = proxy.send_event(UserEvent::DevicesChanged) {
            log::warn!("Failed to send initial DevicesChanged event: {e:#}");
        }
    }

    pub fn handle_tray_click(&mut self, refs: &EventLoopRefs) {
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