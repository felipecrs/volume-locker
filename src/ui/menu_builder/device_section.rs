use super::{
    append_action_item, register_menu_item, DeviceDisplayInfo, format_device_menu_label,
};
use crate::audio::{AudioBackend, AudioDevice};
use crate::config::PersistentState;
use crate::types::{DeviceId, DeviceRole, DeviceType};
use crate::ui::{DeviceAction, MenuAction, MenuIdMap, PreferenceAction};
use tray_icon::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};

pub fn build_device_submenu(
    device: &dyn AudioDevice,
    device_type: DeviceType,
    default_device_id: Option<&DeviceId>,
    persistent_state: &PersistentState,
    map: &mut MenuIdMap,
) -> anyhow::Result<Submenu> {
    let name = device.name();
    let device_id = device.id();
    let volume = device.volume().unwrap_or_else(|e| {
        log::warn!("Failed to get volume for device {name}: {e:#}");
        0.0.into()
    });
    let volume_percent = volume.to_percent();
    let is_muted = device.is_muted().unwrap_or_else(|e| {
        log::warn!("Failed to get mute state for device {name}: {e:#}");
        false
    });
    let is_default = default_device_id.is_some_and(|id| **device_id == **id);

    let (is_volume_locked, notify_on_volume_lock, is_unmute_locked, notify_on_unmute_lock) =
        if let Some(settings) = persistent_state.device_settings(device_id) {
            (
                settings.volume_lock.is_locked,
                settings.volume_lock.notify,
                settings.unmute_lock.is_locked,
                settings.unmute_lock.notify,
            )
        } else {
            (false, false, false, false)
        };

    let is_locked = is_volume_locked || is_unmute_locked;
    let label = format_device_menu_label(&DeviceDisplayInfo {
        name: &name,
        volume_percent,
        is_default,
        is_locked,
        is_muted,
    });

    let submenu = Submenu::new(&label, true);

    let volume_lock_item = CheckMenuItem::new("Keep volume locked", true, is_volume_locked, None);
    let volume_notify_item = CheckMenuItem::new(
        "Notify on volume restore",
        is_volume_locked,
        notify_on_volume_lock,
        None,
    );
    let unmute_lock_item = CheckMenuItem::new("Keep unmuted", true, is_unmute_locked, None);
    let unmute_notify_item = CheckMenuItem::new(
        "Notify on unmute",
        is_unmute_locked,
        notify_on_unmute_lock,
        None,
    );

    let mut register = |menu_id: tray_icon::menu::MenuId, action: DeviceAction| {
        register_menu_item(map, menu_id, action, device_id, &name, device_type);
    };
    register(volume_lock_item.id().clone(), DeviceAction::VolumeLock);
    register(
        volume_notify_item.id().clone(),
        DeviceAction::VolumeLockNotify,
    );
    register(unmute_lock_item.id().clone(), DeviceAction::UnmuteLock);
    register(
        unmute_notify_item.id().clone(),
        DeviceAction::UnmuteLockNotify,
    );

    submenu.append(&volume_lock_item)?;
    submenu.append(&unmute_lock_item)?;
    submenu.append(&PredefinedMenuItem::separator())?;
    submenu.append(&volume_notify_item)?;
    submenu.append(&unmute_notify_item)?;
    submenu.append(&PredefinedMenuItem::separator())?;

    let properties_item = MenuItem::new("Properties...", true, None);
    register(properties_item.id().clone(), DeviceAction::OpenProperties);
    submenu.append(&properties_item)?;

    let settings_item = MenuItem::new("Settings...", true, None);
    register(settings_item.id().clone(), DeviceAction::OpenSettings);
    submenu.append(&settings_item)?;

    Ok(submenu)
}

pub fn append_device_list_to_menu(
    tray_menu: &Menu,
    heading_item: &MenuItem,
    device_type: DeviceType,
    backend: &impl AudioBackend,
    persistent_state: &PersistentState,
    map: &mut MenuIdMap,
) -> anyhow::Result<()> {
    tray_menu.append(heading_item)?;

    let devices = backend.devices(device_type).unwrap_or_else(|e| {
        log::warn!("Failed to get {device_type:?} devices: {e:#}");
        Vec::new()
    });

    let default_device_id = backend
        .default_device(device_type, DeviceRole::Console)
        .map(|d| d.id().clone())
        .ok();

    for device in devices {
        let submenu = build_device_submenu(
            device.as_ref(),
            device_type,
            default_device_id.as_ref(),
            persistent_state,
            map,
        )?;
        tray_menu.append(&submenu)?;
    }

    let properties_label = match device_type {
        DeviceType::Output => "Playback devices...",
        DeviceType::Input => "Recording devices...",
    };
    append_action_item(
        tray_menu,
        map,
        properties_label,
        MenuAction::Preference {
            device_type,
            action: PreferenceAction::OpenDevicesList,
        },
    )?;

    tray_menu.append(&PredefinedMenuItem::separator())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use crate::audio::tests::MockDevice;
    use crate::audio::AudioDevice;

    #[test]
    fn submenu_registers_all_actions() {
        let device = MockDevice::new("dev1", "Speakers", true);
        let state = PersistentState::default();
        let mut map = MenuIdMap::new();

        let submenu = build_device_submenu(
            &device,
            DeviceType::Output,
            Some(device.id()),
            &state,
            &mut map,
        )
        .expect("build_device_submenu should succeed");

        // Should register 6 actions: VolumeLock, VolumeLockNotify, UnmuteLock,
        // UnmuteLockNotify, OpenProperties, OpenSettings
        assert_eq!(map.len(), 6);
        assert!(submenu.text().contains("Speakers"));
    }

    #[test]
    fn submenu_shows_default_indicator() {
        let device = MockDevice::new("dev1", "Speakers", true);
        let state = PersistentState::default();
        let mut map = MenuIdMap::new();

        let submenu = build_device_submenu(
            &device,
            DeviceType::Output,
            Some(device.id()),
            &state,
            &mut map,
        )
        .expect("should succeed");

        assert!(submenu.text().contains("☆"));
    }

    #[test]
    fn submenu_omits_default_indicator_when_not_default() {
        let device = MockDevice::new("dev1", "Speakers", true);
        let state = PersistentState::default();
        let mut map = MenuIdMap::new();

        let submenu = build_device_submenu(
            &device,
            DeviceType::Output,
            None,
            &state,
            &mut map,
        )
        .expect("should succeed");

        assert!(!submenu.text().contains("☆"));
    }
}