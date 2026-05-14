use crate::audio::AudioBackend;
use crate::config::PersistentState;
use crate::types::{
    AppAction, DeviceAction, DeviceId, DeviceRole, DeviceType, MenuAction, MenuItemInfo,
    PreferenceAction, TemporaryPriorities, VolumePercent,
};
use crate::update::UpdateInfo;
use std::collections::HashMap;
use tray_icon::menu::{
    CheckMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu,
};

use super::MenuIdMap;

pub struct DeviceDisplayInfo<'a> {
    pub name: &'a str,
    pub volume_percent: VolumePercent,
    pub is_default: bool,
    pub is_locked: bool,
    pub is_muted: bool,
}

pub fn format_device_menu_label(info: &DeviceDisplayInfo) -> String {
    let default_indicator = if info.is_default { " · ☆" } else { "" };
    let locked_indicator = if info.is_locked { " · 🔒" } else { "" };
    let muted_indicator = if info.is_muted { " 🚫" } else { "" };
    format!(
        "{}{default_indicator} · {}%{muted_indicator}{locked_indicator}",
        info.name, info.volume_percent
    )
}

/// Creates a `MenuItem`, registers it in the device map, and appends it to the menu.
fn append_action_item(
    menu: &Menu,
    map: &mut MenuIdMap,
    label: &str,
    action: MenuAction,
) -> anyhow::Result<()> {
    let item = MenuItem::new(label, true, None);
    map.insert(
        item.id().clone(),
        MenuItemInfo {
            name: label.to_string(),
            action,
        },
    );
    menu.append(&item)?;
    Ok(())
}

/// Registers a menu item in the device map, associating it with a device and setting type.
fn register_menu_item(
    map: &mut MenuIdMap,
    menu_id: MenuId,
    action: DeviceAction,
    device_id: &DeviceId,
    name: &str,
    device_type: DeviceType,
) {
    map.insert(
        menu_id,
        MenuItemInfo {
            name: name.to_string(),
            action: MenuAction::Device {
                device_id: device_id.clone(),
                device_type,
                action,
            },
        },
    );
}

fn lookup_device_name(
    device_id: &DeviceId,
    persistent_state: &PersistentState,
    backend: &impl AudioBackend,
) -> String {
    if let Some(settings) = persistent_state.devices.get::<str>(device_id) {
        settings.name.clone()
    } else {
        match backend.get_device_by_id(device_id) {
            Ok(d) => d.name(),
            Err(e) => {
                log::warn!("Failed to look up device name for {device_id}: {e:#}");
                "Unknown Device".to_string()
            }
        }
    }
}

pub struct TrayMenuItems<'a> {
    pub auto_launch_check_item: &'a CheckMenuItem,
    pub check_updates_on_launch_item: &'a CheckMenuItem,
    pub quit_item: &'a MenuItem,
    pub output_devices_heading_item: &'a MenuItem,
    pub input_devices_heading_item: &'a MenuItem,
}

pub struct MenuContext<'a, B: AudioBackend> {
    pub backend: &'a B,
    pub persistent_state: &'a PersistentState,
    pub temporary_priorities: &'a TemporaryPriorities,
    pub auto_launch_enabled: bool,
    pub update_info: &'a Option<UpdateInfo>,
}

pub fn rebuild_tray_menu(
    tray_menu: &Menu,
    ctx: &MenuContext<impl AudioBackend>,
    items: &TrayMenuItems,
) -> anyhow::Result<MenuIdMap> {
    for _ in 0..tray_menu.items().len() {
        tray_menu.remove_at(0);
    }
    let mut map: MenuIdMap = HashMap::new();

    for (heading_item, device_type) in [
        (items.output_devices_heading_item, DeviceType::Output),
        (items.input_devices_heading_item, DeviceType::Input),
    ] {
        append_device_list_to_menu(
            tray_menu,
            heading_item,
            device_type,
            ctx.backend,
            ctx.persistent_state,
            &mut map,
        )?;
    }

    append_action_item(
        tray_menu,
        &mut map,
        "Sound settings...",
        MenuAction::App(AppAction::OpenSoundSettings),
    )?;
    append_action_item(
        tray_menu,
        &mut map,
        "Volume mixer...",
        MenuAction::App(AppAction::OpenVolumeMixer),
    )?;
    tray_menu.append(&PredefinedMenuItem::separator())?;

    for device_type in [DeviceType::Output, DeviceType::Input] {
        let temporary_priority = ctx.temporary_priorities.get(device_type);
        append_priority_list_to_menu(
            tray_menu,
            device_type,
            ctx.backend,
            ctx.persistent_state,
            temporary_priority,
            &mut map,
        )?;
    }

    append_temporary_priority_section(
        tray_menu,
        ctx.backend,
        ctx.persistent_state,
        ctx.temporary_priorities,
        &mut map,
    )?;

    append_preferences_section(
        tray_menu,
        ctx.auto_launch_enabled,
        ctx.persistent_state,
        items,
        &mut map,
    )?;

    append_footer_section(tray_menu, &mut map, ctx.update_info, items)?;

    Ok(map)
}

fn build_device_submenu(
    device: &dyn crate::audio::AudioDevice,
    device_type: DeviceType,
    default_device_id: &Option<DeviceId>,
    persistent_state: &PersistentState,
    map: &mut MenuIdMap,
) -> anyhow::Result<Submenu> {
    let name = device.name();
    let device_id = device.id();
    let volume = device
        .volume()
        .unwrap_or_else(|e| {
            log::warn!("Failed to get volume for device {name}: {e:#}");
            0.0.into()
        });
    let volume_percent = volume.to_percent();
    let is_muted = device
        .is_muted()
        .unwrap_or_else(|e| {
            log::warn!("Failed to get mute state for device {name}: {e:#}");
            false
        });
    let is_default = default_device_id
        .as_ref()
        .is_some_and(|id| **device_id == **id);

    let (is_volume_locked, notify_on_volume_lock, is_unmute_locked, notify_on_unmute_lock) =
        if let Some(settings) = persistent_state.devices.get(device_id) {
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

    let volume_lock_item =
        CheckMenuItem::new("Keep volume locked", true, is_volume_locked, None);
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

    let mut register = |menu_id: MenuId, action: DeviceAction| {
        register_menu_item(
            map,
            menu_id,
            action,
            &device_id,
            &name,
            device_type,
        );
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

fn append_device_list_to_menu(
    tray_menu: &Menu,
    heading_item: &MenuItem,
    device_type: DeviceType,
    backend: &impl AudioBackend,
    persistent_state: &PersistentState,
    map: &mut MenuIdMap,
) -> anyhow::Result<()> {
    tray_menu.append(heading_item)?;

    let devices = backend
        .get_devices(device_type)
        .unwrap_or_else(|e| {
            log::warn!("Failed to get {device_type:?} devices: {e:#}");
            Vec::new()
        });

    let default_device_id = backend
        .get_default_device(device_type, DeviceRole::Console)
        .map(|d| d.id().clone())
        .ok();

    for device in devices {
        let submenu = build_device_submenu(
            device.as_ref(),
            device_type,
            &default_device_id,
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

fn append_temporary_priority_section(
    tray_menu: &Menu,
    backend: &impl AudioBackend,
    persistent_state: &PersistentState,
    temporary_priorities: &TemporaryPriorities,
    map: &mut MenuIdMap,
) -> anyhow::Result<()> {
    tray_menu.append(&MenuItem::new(
        "Temporary default device priority",
        false,
        None,
    ))?;

    for device_type in [DeviceType::Output, DeviceType::Input] {
        let devices = backend
            .get_devices(device_type)
            .unwrap_or_else(|e| {
                log::warn!("Failed to get {device_type:?} devices: {e:#}");
                Vec::new()
            });
        let available_devices: Vec<_> = devices.iter().map(|d| (d.id(), d.name())).collect();

        let temp_id_opt = temporary_priorities.get(device_type);

        let label_prefix = match device_type {
            DeviceType::Output => "Output device",
            DeviceType::Input => "Input device",
        };

        let submenu_label = if let Some(temp_id) = temp_id_opt {
            let device_name = lookup_device_name(temp_id, persistent_state, backend);
            format!("{}: {}", label_prefix, device_name)
        } else {
            label_prefix.to_string()
        };

        let submenu = Submenu::new(&submenu_label, true);

        for (id, name) in &available_devices {
            let is_checked = temp_id_opt.is_some_and(|t| *t == **id);
            let item = CheckMenuItem::new(name, true, is_checked, None);
            register_menu_item(
                map,
                item.id().clone(),
                DeviceAction::SetTemporaryPriority,
                id,
                name,
                device_type,
            );
            submenu.append(&item)?;
        }
        tray_menu.append(&submenu)?;
    }
    tray_menu.append(&PredefinedMenuItem::separator())?;

    Ok(())
}

fn append_preferences_section(
    tray_menu: &Menu,
    auto_launch_enabled: bool,
    persistent_state: &PersistentState,
    items: &TrayMenuItems,
    map: &mut MenuIdMap,
) -> anyhow::Result<()> {
    tray_menu.append(&MenuItem::new("Preferences", false, None))?;

    items
        .auto_launch_check_item
        .set_checked(auto_launch_enabled);
    map.insert(
        items.auto_launch_check_item.id().clone(),
        MenuItemInfo {
            name: "Auto-launch".to_string(),
            action: MenuAction::App(AppAction::ToggleAutoLaunch),
        },
    );
    tray_menu.append(items.auto_launch_check_item)?;

    items
        .check_updates_on_launch_item
        .set_checked(persistent_state.check_updates_on_launch);
    map.insert(
        items.check_updates_on_launch_item.id().clone(),
        MenuItemInfo {
            name: "Check updates on launch".to_string(),
            action: MenuAction::App(AppAction::ToggleCheckUpdatesOnLaunch),
        },
    );
    tray_menu.append(items.check_updates_on_launch_item)?;
    tray_menu.append(&PredefinedMenuItem::separator())?;

    Ok(())
}

fn append_footer_section(
    tray_menu: &Menu,
    map: &mut MenuIdMap,
    update_info: &Option<UpdateInfo>,
    items: &TrayMenuItems,
) -> anyhow::Result<()> {
    tray_menu.append(&MenuItem::new("Troubleshooting", false, None))?;

    append_action_item(
        tray_menu,
        map,
        "Open app folder...",
        MenuAction::App(AppAction::OpenAppDirectory),
    )?;

    tray_menu.append(&PredefinedMenuItem::separator())?;

    append_action_item(
        tray_menu,
        map,
        "GitHub...",
        MenuAction::App(AppAction::OpenGitHubRepo),
    )?;

    let (label, action) = match update_info {
        Some(info) => (
            format!("Update to {}...", info.latest_version),
            AppAction::PerformUpdate,
        ),
        None => ("Check for updates".to_string(), AppAction::CheckForUpdates),
    };

    append_action_item(
        tray_menu,
        map,
        &label,
        MenuAction::App(action),
    )?;

    tray_menu.append(&PredefinedMenuItem::separator())?;
    tray_menu.append(items.quit_item)?;

    Ok(())
}

fn append_priority_list_to_menu(
    tray_menu: &Menu,
    device_type: DeviceType,
    backend: &impl AudioBackend,
    persistent_state: &PersistentState,
    temporary_priority: Option<&DeviceId>,
    map: &mut MenuIdMap,
) -> anyhow::Result<()> {
    let priority_list = persistent_state.get_priority_list(device_type);
    let priority_label = match device_type {
        DeviceType::Output => "Default output device priority",
        DeviceType::Input => "Default input device priority",
    };

    let priority_header = MenuItem::new(priority_label, false, None);
    tray_menu.append(&priority_header)?;

    let devices = backend
        .get_devices(device_type)
        .unwrap_or_else(|e| {
            log::warn!("Failed to get {device_type:?} devices: {e:#}");
            Vec::new()
        });
    let mut available_devices = Vec::new();
    for device in devices {
        available_devices.push((device.id().clone(), device.name()));
    }

    for (index, device_id) in priority_list.iter().enumerate() {
        let device_name = lookup_device_name(device_id, persistent_state, backend);

        let label = format!("{}. {}", index + 1, device_name);
        let priority_submenu = Submenu::new(&label, true);

        let move_items: [(&str, bool, DeviceAction); 4] = [
            ("Move up", index > 0, DeviceAction::MovePriorityUp),
            (
                "Move down",
                index < priority_list.len() - 1,
                DeviceAction::MovePriorityDown,
            ),
            ("Move to top", index > 0, DeviceAction::MovePriorityToTop),
            (
                "Move to bottom",
                index < priority_list.len() - 1,
                DeviceAction::MovePriorityToBottom,
            ),
        ];
        for (i, (label, enabled, action)) in move_items.into_iter().enumerate() {
            if i == 2 {
                priority_submenu.append(&PredefinedMenuItem::separator())?;
            }
            let item = MenuItem::new(label, enabled, None);
            register_menu_item(
                map,
                item.id().clone(),
                action,
                device_id,
                &device_name,
                device_type,
            );
            priority_submenu.append(&item)?;
        }
        priority_submenu.append(&PredefinedMenuItem::separator())?;

        let remove_priority_item = MenuItem::new("Remove device", true, None);
        register_menu_item(
            map,
            remove_priority_item.id().clone(),
            DeviceAction::RemoveFromPriority,
            device_id,
            &device_name,
            device_type,
        );
        priority_submenu.append(&remove_priority_item)?;

        tray_menu.append(&priority_submenu)?;
    }

    let mut devices_to_add = Vec::new();
    for (id, name) in &available_devices {
        if !priority_list.iter().any(|p| p == id) {
            devices_to_add.push((id, name));
        }
    }

    let add_device_submenu = Submenu::new("Add device", !devices_to_add.is_empty());
    for (id, name) in devices_to_add {
        let item = MenuItem::new(name, true, None);
        register_menu_item(
            map,
            item.id().clone(),
            DeviceAction::AddToPriority,
            id,
            name,
            device_type,
        );
        add_device_submenu.append(&item)?;
    }
    tray_menu.append(&add_device_submenu)?;

    let notify_on_restore = persistent_state.get_notify_on_priority_restore(device_type);

    let notify_item = CheckMenuItem::new(
        "Notify on priority restore",
        !priority_list.is_empty() || temporary_priority.is_some(),
        notify_on_restore,
        None,
    );

    map.insert(
        notify_item.id().clone(),
        MenuItemInfo {
            name: "Priority Restore Notify".to_string(),
            action: MenuAction::Preference {
                device_type,
                action: PreferenceAction::PriorityRestoreNotify,
            },
        },
    );
    tray_menu.append(&notify_item)?;

    let switch_communication = persistent_state.get_switch_communication_device(device_type);

    let switch_comm_item = CheckMenuItem::new(
        "Also switch default communication device",
        !priority_list.is_empty() || temporary_priority.is_some(),
        switch_communication,
        None,
    );

    map.insert(
        switch_comm_item.id().clone(),
        MenuItemInfo {
            name: "Switch Communication Device".to_string(),
            action: MenuAction::Preference {
                device_type,
                action: PreferenceAction::SwitchCommunicationDevice,
            },
        },
    );
    tray_menu.append(&switch_comm_item)?;

    tray_menu.append(&PredefinedMenuItem::separator())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{DeviceDisplayInfo, VolumePercent, format_device_menu_label};

    #[test]
    fn to_label_basic() {
        let label = format_device_menu_label(&DeviceDisplayInfo {
            name: "Speakers",
            volume_percent: VolumePercent::from(50.0),
            is_default: false,
            is_locked: false,
            is_muted: false,
        });
        assert_eq!(label, "Speakers · 50%");
    }

    #[test]
    fn to_label_default_device() {
        let label = format_device_menu_label(&DeviceDisplayInfo {
            name: "Speakers",
            volume_percent: VolumePercent::from(75.0),
            is_default: true,
            is_locked: false,
            is_muted: false,
        });
        assert_eq!(label, "Speakers · ☆ · 75%");
    }

    #[test]
    fn to_label_locked() {
        let label = format_device_menu_label(&DeviceDisplayInfo {
            name: "Speakers",
            volume_percent: VolumePercent::from(100.0),
            is_default: false,
            is_locked: true,
            is_muted: false,
        });
        assert_eq!(label, "Speakers · 100% · 🔒");
    }

    #[test]
    fn to_label_muted() {
        let label = format_device_menu_label(&DeviceDisplayInfo {
            name: "Mic",
            volume_percent: VolumePercent::from(0.0),
            is_default: false,
            is_locked: false,
            is_muted: true,
        });
        assert_eq!(label, "Mic · 0% 🚫");
    }

    #[test]
    fn to_label_all_indicators() {
        let label = format_device_menu_label(&DeviceDisplayInfo {
            name: "Headset",
            volume_percent: VolumePercent::from(42.0),
            is_default: true,
            is_locked: true,
            is_muted: true,
        });
        assert_eq!(label, "Headset · ☆ · 42% 🚫 · 🔒");
    }
}