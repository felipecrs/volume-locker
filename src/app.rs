use crate::audio::{
    AudioBackend, AudioBackendImpl, AudioDevice, check_and_unmute_device, collect_device_info,
    enforce_priorities, enforce_volume_lock, migrate_device_ids,
};
use crate::config::{PersistentState, save_state};
use crate::consts::{APP_NAME, APP_UID, CURRENT_VERSION};
use crate::notification::{NotificationThrottler, log_and_notify_error};
use crate::types::{
    DeviceChangeSource, DeviceFingerprint, DeviceId, DeviceType, TemporaryPriorities, UserEvent,
    VolumeChangedEvent, VolumeScalar,
};
use crate::ui::{
    MenuContext, MenuEventContext, MenuEventResult, MenuIdMap, TrayMenuItems, handle_menu_event,
    rebuild_tray_menu,
};
use crate::update;
use crate::update::UpdateInfo;
use auto_launch::AutoLaunch;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use tao::event_loop::{ControlFlow, EventLoopProxy};
use tray_icon::TrayIconBuilder;
use tray_icon::menu::{CheckMenuItem, Menu, MenuItem};

pub struct AppState {
    pub persistent_state: PersistentState,
    pub menu_id_map: MenuIdMap,
    pub watched_devices: Vec<Box<dyn AudioDevice>>,
    pub notification_throttler: NotificationThrottler,
    pub temporary_priorities: TemporaryPriorities,
    pub update_info: Option<UpdateInfo>,
    pub update_check_in_progress: bool,
    pub tray_icon: Option<tray_icon::TrayIcon>,
    pub backend: AudioBackendImpl,
    pub device_cache: HashMap<DeviceId, (String, DeviceType, Option<DeviceFingerprint>)>,
    pub menu_open: Arc<AtomicBool>,
    pub reopen_menu: Arc<AtomicBool>,
    pub tray_hwnd: Arc<AtomicIsize>,
}

pub struct EventLoopRefs<'a> {
    pub auto_launch: &'a AutoLaunch,
    pub auto_launch_check_item: &'a CheckMenuItem,
    pub check_updates_on_launch_item: &'a CheckMenuItem,
    pub notify_on_device_migration_item: &'a CheckMenuItem,
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
        let result = migrate_device_ids(
            &self.backend,
            &mut self.persistent_state,
            &mut self.temporary_priorities,
            &self.device_cache,
        );

        if result.persistent_changed {
            if let Err(e) = save_state(&self.persistent_state) {
                log_and_notify_error(
                    "Failed to Save State",
                    &format!("Failed to save state after device migration: {e:#}"),
                );
            } else {
                log::info!("Saved state after device migration");
            }
        }

        if self.persistent_state.notify_on_device_migration {
            for name in result.migrated_device_names {
                self.notification_throttler.send_if_not_throttled(
                    &format!("migration_{name}"),
                    "Device Settings Transferred",
                    &format!("Settings for '{name}' were transferred to its new connection."),
                );
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
                log::info!("Not watching {device_name}: device is not connected ({e})");
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

        if device_settings.volume_lock.is_locked {
            match device.volume() {
                Ok(vol) => {
                    enforce_volume_lock(
                        device_id,
                        device.as_ref(),
                        device_name,
                        device_settings.volume_lock,
                        vol,
                        &mut self.notification_throttler,
                    );
                }
                Err(e) => {
                    log::warn!("Failed to read volume of {device_name} during watch setup: {e:#}");
                }
            }
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
            "Watching '{device_name}' (Locked: {}, Unmute: {})",
            device_settings.volume_lock.is_locked,
            device_settings.unmute_lock.is_locked
        );

        Some(device)
    }

    fn update_tray_icon(
        &self,
        any_device_locked: bool,
        locked_icon: &tray_icon::Icon,
        unlocked_icon: &tray_icon::Icon,
    ) {
        if let Some(tray_icon) = &self.tray_icon {
            let icon = if any_device_locked {
                locked_icon
            } else {
                unlocked_icon
            };
            if let Err(e) = tray_icon.set_icon(Some(icon.clone())) {
                log::error!("Failed to update tray icon: {e:#}");
            }
        }
    }

    pub fn get_device_display_name(&self, id: &DeviceId) -> String {
        if id.is_empty() {
            return String::new();
        }
        if let Some((name, _, _)) = self.device_cache.get(id) {
            return name.clone();
        }
        if let Some(s) = self.persistent_state.device_settings(id) {
            return s.name.clone();
        }
        if let Ok(d) = self.backend.device_by_id(id) {
            return d.name();
        }
        id.to_string()
    }

    pub fn handle_devices_changed(
        &mut self,
        source: DeviceChangeSource,
        proxy: &EventLoopProxy<UserEvent>,
        locked_icon: &tray_icon::Icon,
        unlocked_icon: &tray_icon::Icon,
        refs: &EventLoopRefs,
    ) {
        let description = source.description(|id| self.get_device_display_name(id));
        log::info!("Audio event ({description}): updating devices and enforcing settings...");

        self.migrate_device_ids_if_needed();
        let device_info = collect_device_info(&self.backend);
        let presence_devices: Vec<_> = if self.persistent_state.wireless_headset_detection_enabled {
            device_info
                .iter()
                .filter_map(|(_, _, _, fingerprint)| fingerprint.as_ref()?.usb_vid_pid)
                .collect()
        } else {
            Vec::new()
        };
        self.backend
            .configure_presence_monitoring(&presence_devices);

        for (device_id, new_name, device_type, fingerprint) in device_info {
            self.device_cache.insert(
                device_id.clone(),
                (new_name.clone(), device_type, fingerprint.clone()),
            );
            if let Some(settings) = self.persistent_state.device_settings_mut(&device_id) {
                settings.name = new_name;
                settings.device_type = device_type;
                if fingerprint.is_some() {
                    settings.fingerprint = fingerprint;
                }
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

        if self.reopen_menu.swap(false, Ordering::SeqCst) {
            log::info!("Re-rendering and reopening tray menu due to audio device change...");
            self.handle_tray_click(refs);
        }
    }

    fn start_update_check(&mut self, manual_request: bool, proxy: &EventLoopProxy<UserEvent>) {
        if self.update_check_in_progress {
            return;
        }

        self.update_check_in_progress = true;
        let proxy = proxy.clone();
        std::thread::spawn(move || {
            let result = update::fetch_update_info().map_err(|e| format!("{e:#}"));
            if let Err(e) = proxy.send_event(UserEvent::UpdateCheckCompleted {
                manual_request,
                result,
            }) {
                log::warn!("Failed to send update check result: {e:#}");
            }
        });
    }

    pub fn handle_update_check_completed(
        &mut self,
        manual_request: bool,
        result: Result<Option<UpdateInfo>, String>,
    ) {
        self.update_check_in_progress = false;
        update::report_check_result(manual_request, &result);
        self.update_info = result.ok().flatten();
    }

    pub fn handle_configuration_changed(&mut self, proxy: &EventLoopProxy<UserEvent>) {
        if let Err(e) = save_state(&self.persistent_state) {
            log_and_notify_error(
                "Failed to Save State",
                &format!("Failed to save state: {e:#}"),
            );
            return;
        }
        log::info!(
            "Configuration saved ({} devices tracked)",
            self.persistent_state.device_count()
        );
        if let Err(e) = proxy.send_event(UserEvent::DevicesChanged(DeviceChangeSource::UserAction))
        {
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
        self.reopen_menu.store(false, Ordering::SeqCst);
        if event.id == refs.quit_item.id() {
            self.tray_hwnd.store(0, Ordering::SeqCst);
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
                    if let Err(e) =
                        proxy.send_event(UserEvent::DevicesChanged(DeviceChangeSource::UserAction))
                    {
                        log::warn!("Failed to send DevicesChanged event: {e:#}");
                    }
                }
                MenuEventResult::SaveConfig => {
                    if let Err(e) = proxy.send_event(UserEvent::ConfigurationChanged) {
                        log::warn!("Failed to send ConfigurationChanged event: {e:#}");
                    }
                }
                MenuEventResult::UpdatePerform(info) => match update::install_update(&info) {
                    Ok(()) => {
                        self.tray_hwnd.store(0, Ordering::SeqCst);
                        self.tray_icon.take();
                        *control_flow = ControlFlow::Exit;
                    }
                    Err(e) => {
                        log_and_notify_error("Update Failed", &format!("Update failed: {e:#}"));
                    }
                },
                MenuEventResult::UpdateCheck => {
                    self.start_update_check(true, proxy);
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
        let tray_guid = u128::from_str_radix(&APP_UID.replace('-', ""), 16)
            .expect("APP_UID must be a valid UUID");
        match TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu.clone()))
            .with_tooltip(&tooltip)
            .with_icon(unlocked_icon.clone())
            .with_id(APP_UID)
            .with_guid(tray_guid)
            .with_menu_on_left_click(false)
            .with_menu_on_right_click(false)
            .build()
        {
            Ok(icon) => {
                #[cfg(target_os = "windows")]
                self.tray_hwnd
                    .store(icon.window_handle() as isize, Ordering::SeqCst);
                self.tray_icon = Some(icon);
            }
            Err(e) => log::error!("Failed to build tray icon: {e:#}"),
        }

        if self.persistent_state.check_updates_on_launch {
            self.start_update_check(false, proxy);
        }

        if let Err(e) = proxy.send_event(UserEvent::DevicesChanged(DeviceChangeSource::Init)) {
            log::warn!("Failed to send initial DevicesChanged event: {e:#}");
        }
    }

    pub fn rebuild_menu(&mut self, refs: &EventLoopRefs) {
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
                notify_on_device_migration: refs.notify_on_device_migration_item,
                quit: refs.quit_item,
                output_devices_heading: refs.output_devices_heading_item,
                input_devices_heading: refs.input_devices_heading_item,
            },
        ) {
            Ok(map) => {
                self.menu_id_map = map;
            }
            Err(e) => {
                log_and_notify_error(
                    "Failed to Rebuild Tray Menu",
                    &format!("Failed to rebuild tray menu: {e:#}"),
                );
            }
        }
    }

    pub fn handle_tray_click(&mut self, refs: &EventLoopRefs) {
        self.rebuild_menu(refs);
        self.menu_open.store(true, Ordering::SeqCst);
        if let Some(tray_icon) = &self.tray_icon {
            tray_icon.show_menu();
        }
        self.menu_open.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use crate::types::{DeviceType, PriorityKind, TemporaryPriorities};

    #[test]
    fn temporary_priorities_get_returns_none_by_default() {
        let tp = TemporaryPriorities::default();
        assert!(tp.get(DeviceType::Output, PriorityKind::Main).is_none());
        assert!(tp.get(DeviceType::Input, PriorityKind::Main).is_none());
        assert!(
            tp.get(DeviceType::Output, PriorityKind::Communication)
                .is_none()
        );
        assert!(
            tp.get(DeviceType::Input, PriorityKind::Communication)
                .is_none()
        );
    }

    #[test]
    fn temporary_priorities_set_and_get_output() {
        let mut tp = TemporaryPriorities::default();
        tp.set(DeviceType::Output, PriorityKind::Main, Some("dev_a".into()));
        assert_eq!(
            tp.get(DeviceType::Output, PriorityKind::Main).unwrap(),
            "dev_a"
        );
        assert!(tp.get(DeviceType::Input, PriorityKind::Main).is_none());
    }

    #[test]
    fn temporary_priorities_set_and_get_communication() {
        let mut tp = TemporaryPriorities::default();
        tp.set(
            DeviceType::Output,
            PriorityKind::Communication,
            Some("headset".into()),
        );
        assert!(tp.get(DeviceType::Output, PriorityKind::Main).is_none());
        assert_eq!(
            tp.get(DeviceType::Output, PriorityKind::Communication)
                .unwrap(),
            "headset"
        );
    }

    #[test]
    fn temporary_priorities_set_and_get_input() {
        let mut tp = TemporaryPriorities::default();
        tp.set(DeviceType::Input, PriorityKind::Main, Some("mic_1".into()));
        assert!(tp.get(DeviceType::Output, PriorityKind::Main).is_none());
        assert_eq!(
            tp.get(DeviceType::Input, PriorityKind::Main).unwrap(),
            "mic_1"
        );
    }

    #[test]
    fn temporary_priorities_clear() {
        let mut tp = TemporaryPriorities::default();
        tp.set(DeviceType::Output, PriorityKind::Main, Some("dev_a".into()));
        tp.set(DeviceType::Output, PriorityKind::Main, None);
        assert!(tp.get(DeviceType::Output, PriorityKind::Main).is_none());
    }
}
