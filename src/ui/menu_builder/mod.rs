mod device_order_section;
mod device_section;

use super::{AppAction, DeviceAction, MenuAction, MenuItemInfo};
use crate::audio::AudioBackend;
use crate::config::PersistentState;
use crate::i18n::{t, t_en};
use crate::types::{DeviceId, DeviceType, TemporaryDefaults, VolumePercent};
use crate::update::UpdateInfo;
use std::collections::HashMap;
use tray_icon::menu::{CheckMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu};

use super::MenuIdMap;

use device_order_section::{append_device_order_to_menu, append_temporary_default_section};
use device_section::append_device_list_to_menu;

pub struct DeviceDisplayInfo<'a> {
    pub name: &'a str,
    pub volume_percent: VolumePercent,
    pub is_locked: bool,
    pub is_muted: bool,
}

pub fn format_device_menu_label(info: &DeviceDisplayInfo) -> String {
    let locked_indicator = if info.is_locked { " · 🔒" } else { "" };
    let muted_indicator = if info.is_muted { " 🚫" } else { "" };
    format!(
        "{} · {}%{muted_indicator}{locked_indicator}",
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
    // This name goes to the logs. The Follow entry's label is translated app text, so it needs an
    // English name, unlike real devices, whose names come from Windows.
    let name = if device_id.is_follow_main() {
        match device_type {
            DeviceType::Output => t_en!("follow-default-output-device"),
            DeviceType::Input => t_en!("follow-default-input-device"),
        }
    } else {
        name.to_string()
    };
    map.insert(
        menu_id,
        MenuItemInfo {
            name,
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
                log::info!("Device {device_id} is not connected for name lookup: {e:#}");
                device_id.to_string()
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
    pub temporary_defaults: &'a TemporaryDefaults,
    pub auto_launch_enabled: bool,
    pub update_info: &'a Option<UpdateInfo>,
    pub update_download_in_progress: bool,
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
        &t!("sound-settings"),
        MenuAction::App(AppAction::OpenSoundSettings),
    )?;
    append_action_item(
        tray_menu,
        &mut map,
        &t!("volume-mixer"),
        MenuAction::App(AppAction::OpenVolumeMixer),
    )?;
    tray_menu.append(&PredefinedMenuItem::separator())?;

    for device_type in [DeviceType::Output, DeviceType::Input] {
        append_device_order_to_menu(
            tray_menu,
            device_type,
            ctx.backend,
            ctx.persistent_state,
            &mut map,
        )?;
    }

    append_temporary_default_section(
        tray_menu,
        ctx.backend,
        ctx.persistent_state,
        ctx.temporary_defaults,
        &mut map,
    )?;

    append_preferences_section(
        tray_menu,
        ctx.auto_launch_enabled,
        ctx.persistent_state,
        items,
        &mut map,
    )?;

    append_footer_section(
        tray_menu,
        &mut map,
        ctx.update_info.as_ref(),
        ctx.update_download_in_progress,
        items,
    )?;

    Ok(map)
}

fn append_preferences_section(
    tray_menu: &Menu,
    auto_launch_enabled: bool,
    persistent_state: &PersistentState,
    items: &TrayMenuItems,
    map: &mut MenuIdMap,
) -> anyhow::Result<()> {
    tray_menu.append(&MenuItem::new(t!("preferences"), false, None))?;

    let devices_seen_as_new = Submenu::new(t!("devices-seen-as-new"), true);
    let keep_settings = persistent_state.keep_settings_when_seen_as_new;
    // The name, icon and notification only apply while the settings are kept.
    for (label, enabled, checked, action) in [
        (
            t!("keep-their-settings"),
            true,
            keep_settings,
            AppAction::ToggleKeepSettingsWhenSeenAsNew,
        ),
        (
            t!("keep-their-name-and-icon"),
            keep_settings,
            persistent_state.keep_name_and_icon_when_seen_as_new,
            AppAction::ToggleKeepNameAndIconWhenSeenAsNew,
        ),
        (
            t!("notify-when-seen-as-new"),
            keep_settings,
            persistent_state.notify_when_seen_as_new,
            AppAction::ToggleNotifyWhenSeenAsNew,
        ),
    ] {
        let item = CheckMenuItem::new(&label, enabled, checked, None);
        map.insert(
            item.id().clone(),
            MenuItemInfo {
                name: label,
                action: MenuAction::App(action),
            },
        );
        devices_seen_as_new.append(&item)?;
    }
    tray_menu.append(&devices_seen_as_new)?;

    let skip_disconnected_wireless_headsets = CheckMenuItem::new(
        t!("skip-disconnected-wireless-headsets"),
        true,
        persistent_state.skip_disconnected_wireless_headsets,
        None,
    );
    map.insert(
        skip_disconnected_wireless_headsets.id().clone(),
        MenuItemInfo {
            name: "Skip disconnected wireless headsets".to_string(),
            action: MenuAction::App(AppAction::ToggleSkipDisconnectedWirelessHeadsets),
        },
    );
    tray_menu.append(&skip_disconnected_wireless_headsets)?;

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
    update_download_in_progress: bool,
    items: &TrayMenuItems,
) -> anyhow::Result<()> {
    tray_menu.append(&MenuItem::new(t!("troubleshooting"), false, None))?;

    append_action_item(
        tray_menu,
        map,
        &t!("open-app-folder"),
        MenuAction::App(AppAction::OpenAppDirectory),
    )?;

    tray_menu.append(&PredefinedMenuItem::separator())?;

    append_action_item(
        tray_menu,
        map,
        "GitHub...",
        MenuAction::App(AppAction::OpenGitHubRepo),
    )?;

    if update_download_in_progress {
        tray_menu.append(&MenuItem::new(t!("downloading-update"), false, None))?;
    } else {
        let (label, action) = match update_info {
            Some(info) => (
                t!("update-to-version", version = info.latest_version.as_str()),
                AppAction::PerformUpdate,
            ),
            None => (t!("check-for-updates"), AppAction::CheckForUpdates),
        };

        append_action_item(tray_menu, map, &label, MenuAction::App(action))?;
    }

    tray_menu.append(&PredefinedMenuItem::separator())?;
    tray_menu.append(items.quit)?;

    Ok(())
}

#[cfg(test)]
mod tests;
