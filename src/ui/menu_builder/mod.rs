mod device_section;
mod priority_section;

use super::{AppAction, DeviceAction, MenuAction, MenuItemInfo};
use crate::audio::AudioBackend;
use crate::config::PersistentState;
use crate::types::{DeviceId, DeviceType, TemporaryPriorities, VolumePercent};
use crate::update::UpdateInfo;
use std::collections::HashMap;
use tray_icon::menu::{CheckMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem};

use super::MenuIdMap;

use device_section::append_device_list_to_menu;
use priority_section::{append_priority_list_to_menu, append_temporary_priority_section};

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
    if let Some(settings) = persistent_state.device_settings(device_id) {
        settings.name.clone()
    } else {
        match backend.device_by_id(device_id) {
            Ok(d) => d.name(),
            Err(e) => {
                log::warn!("Failed to look up device name for {device_id}: {e:#}");
                "Unknown Device".to_string()
            }
        }
    }
}

pub struct TrayMenuItems<'a> {
    pub auto_launch_check: &'a CheckMenuItem,
    pub check_updates_on_launch: &'a CheckMenuItem,
    pub quit: &'a MenuItem,
    pub output_devices_heading: &'a MenuItem,
    pub input_devices_heading: &'a MenuItem,
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
        (items.output_devices_heading, DeviceType::Output),
        (items.input_devices_heading, DeviceType::Input),
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

    append_footer_section(tray_menu, &mut map, ctx.update_info.as_ref(), items)?;

    Ok(map)
}

fn append_preferences_section(
    tray_menu: &Menu,
    auto_launch_enabled: bool,
    persistent_state: &PersistentState,
    items: &TrayMenuItems,
    map: &mut MenuIdMap,
) -> anyhow::Result<()> {
    tray_menu.append(&MenuItem::new("Preferences", false, None))?;

    items.auto_launch_check.set_checked(auto_launch_enabled);
    map.insert(
        items.auto_launch_check.id().clone(),
        MenuItemInfo {
            name: "Auto-launch".to_string(),
            action: MenuAction::App(AppAction::ToggleAutoLaunch),
        },
    );
    tray_menu.append(items.auto_launch_check)?;

    items
        .check_updates_on_launch
        .set_checked(persistent_state.check_updates_on_launch);
    map.insert(
        items.check_updates_on_launch.id().clone(),
        MenuItemInfo {
            name: "Check updates on launch".to_string(),
            action: MenuAction::App(AppAction::ToggleCheckUpdatesOnLaunch),
        },
    );
    tray_menu.append(items.check_updates_on_launch)?;
    tray_menu.append(&PredefinedMenuItem::separator())?;

    Ok(())
}

fn append_footer_section(
    tray_menu: &Menu,
    map: &mut MenuIdMap,
    update_info: Option<&UpdateInfo>,
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

    append_action_item(tray_menu, map, &label, MenuAction::App(action))?;

    tray_menu.append(&PredefinedMenuItem::separator())?;
    tray_menu.append(items.quit)?;

    Ok(())
}

#[cfg(test)]
mod tests;
