use crate::audio::AudioBackend;
use crate::config::PersistentState;
use crate::consts::GITHUB_REPO_URL;
use crate::platform::{
    open_device_properties, open_device_settings, open_devices_list, open_sound_settings,
    open_volume_mixer,
};
use crate::types::{
    DeviceRole, DeviceSettingType, DeviceSettings, DeviceType, MenuItemDeviceInfo,
    TemporaryPriorities,
};
use crate::update::UpdateInfo;
use crate::utils::{
    convert_float_to_percent, get_executable_directory, log_and_notify_error, open_path, open_url,
};
use std::collections::HashMap;
use tray_icon::menu::{
    CheckMenuItem, Menu, MenuId, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu,
};

pub enum UpdateAction {
    None,
    Check,
    Perform(UpdateInfo),
}

pub fn format_device_menu_label(
    name: &str,
    volume_percent: f32,
    is_default: bool,
    is_locked: bool,
    is_muted: bool,
) -> String {
    let default_indicator = if is_default { " · ☆" } else { "" };
    let locked_indicator = if is_locked { " · 🔒" } else { "" };
    let muted_indicator = if is_muted { " 🚫" } else { "" };
    format!("{name}{default_indicator} · {volume_percent}%{muted_indicator}{locked_indicator}")
}

pub fn find_menu_item(menu: &Menu, id: &MenuId) -> Option<MenuItemKind> {
    find_in_items(&menu.items(), id)
}

fn find_in_items(items: &[MenuItemKind], id: &MenuId) -> Option<MenuItemKind> {
    for item in items {
        if item.id() == id {
            return Some(item.clone());
        }
        if let Some(submenu) = item.as_submenu()
            && let Some(sub_item) = find_in_items(&submenu.items(), id)
        {
            return Some(sub_item);
        }
    }
    None
}

/// Creates a `MenuItem`, registers it in the device map, and appends it to the menu.
fn append_action_item(
    menu: &Menu,
    map: &mut HashMap<MenuId, MenuItemDeviceInfo>,
    label: &str,
    setting_type: DeviceSettingType,
    device_id: Option<String>,
    device_type: Option<DeviceType>,
) -> anyhow::Result<()> {
    let item = MenuItem::new(label, true, None);
    map.insert(
        item.id().clone(),
        MenuItemDeviceInfo {
            device_id,
            setting_type,
            name: label.to_string(),
            device_type,
        },
    );
    menu.append(&item)?;
    Ok(())
}

/// Registers a menu item in the device map, associating it with a device and setting type.
fn register_menu_item(
    map: &mut HashMap<MenuId, MenuItemDeviceInfo>,
    menu_id: MenuId,
    setting_type: DeviceSettingType,
    device_id: &str,
    name: &str,
    device_type: DeviceType,
) {
    map.insert(
        menu_id,
        MenuItemDeviceInfo {
            device_id: Some(device_id.to_string()),
            setting_type,
            name: name.to_string(),
            device_type: Some(device_type),
        },
    );
}

pub struct TrayMenuItems<'a> {
    pub auto_launch_check_item: &'a CheckMenuItem,
    pub check_updates_on_launch_item: &'a CheckMenuItem,
    pub quit_item: &'a MenuItem,
    pub output_devices_heading_item: &'a MenuItem,
    pub input_devices_heading_item: &'a MenuItem,
}

pub fn rebuild_tray_menu(
    tray_menu: &Menu,
    backend: &impl AudioBackend,
    persistent_state: &mut PersistentState,
    temporary_priorities: &TemporaryPriorities,
    auto_launch_enabled: bool,
    items: &TrayMenuItems,
    update_info: &Option<UpdateInfo>,
) -> anyhow::Result<HashMap<MenuId, MenuItemDeviceInfo>> {
    // Clear the menu
    for _ in 0..tray_menu.items().len() {
        tray_menu.remove_at(0);
    }
    let mut menu_id_to_device: HashMap<MenuId, MenuItemDeviceInfo> = HashMap::new();

    // Devices section
    for (heading_item, device_type) in [
        (items.output_devices_heading_item, DeviceType::Output),
        (items.input_devices_heading_item, DeviceType::Input),
    ] {
        append_device_list_to_menu(
            tray_menu,
            heading_item,
            device_type,
            backend,
            persistent_state,
            &mut menu_id_to_device,
        )?;
    }

    // Sound shortcuts section
    append_action_item(
        tray_menu,
        &mut menu_id_to_device,
        "Sound settings...",
        DeviceSettingType::OpenSoundSettings,
        None,
        None,
    )?;
    append_action_item(
        tray_menu,
        &mut menu_id_to_device,
        "Volume mixer...",
        DeviceSettingType::OpenVolumeMixer,
        None,
        None,
    )?;
    tray_menu.append(&PredefinedMenuItem::separator())?;

    // Default device priority section
    for device_type in [DeviceType::Output, DeviceType::Input] {
        let temporary_priority = match device_type {
            DeviceType::Output => &temporary_priorities.output,
            DeviceType::Input => &temporary_priorities.input,
        };
        append_priority_list_to_menu(
            tray_menu,
            device_type,
            backend,
            persistent_state,
            temporary_priority,
            &mut menu_id_to_device,
        )?;
    }

    // Temporary priority section
    tray_menu.append(&MenuItem::new(
        "Temporary default device priority",
        false,
        None,
    ))?;

    for device_type in [DeviceType::Output, DeviceType::Input] {
        let devices = backend.get_devices(device_type).unwrap_or_default();
        let mut available_devices = Vec::new();
        for device in devices {
            available_devices.push((device.id(), device.name()));
        }

        let temp_id_opt = match device_type {
            DeviceType::Output => temporary_priorities.output.as_ref(),
            DeviceType::Input => temporary_priorities.input.as_ref(),
        };

        let label_prefix = match device_type {
            DeviceType::Output => "Output device",
            DeviceType::Input => "Input device",
        };

        let submenu_label = if let Some(temp_id) = temp_id_opt {
            let device_name = if let Some(settings) = persistent_state.devices.get(temp_id) {
                settings.name.clone()
            } else {
                match backend.get_device_by_id(temp_id) {
                    Ok(d) => d.name(),
                    Err(_) => "Unknown Device".to_string(),
                }
            };
            format!("{}: {}", label_prefix, device_name)
        } else {
            label_prefix.to_string()
        };

        let submenu = Submenu::new(&submenu_label, true);

        for (id, name) in &available_devices {
            let is_checked = Some(id) == temp_id_opt;
            let item = CheckMenuItem::new(name, true, is_checked, None);
            menu_id_to_device.insert(
                item.id().clone(),
                MenuItemDeviceInfo {
                    device_id: Some(id.clone()),
                    setting_type: DeviceSettingType::SetTemporaryPriority,
                    name: name.clone(),
                    device_type: Some(device_type),
                },
            );
            submenu.append(&item)?;
        }
        tray_menu.append(&submenu)?;
    }
    tray_menu.append(&PredefinedMenuItem::separator())?;

    // Preferences section
    tray_menu.append(&MenuItem::new("Preferences", false, None))?;

    items
        .auto_launch_check_item
        .set_checked(auto_launch_enabled);
    tray_menu.append(items.auto_launch_check_item)?;

    items
        .check_updates_on_launch_item
        .set_checked(persistent_state.check_updates_on_launch);
    tray_menu.append(items.check_updates_on_launch_item)?;
    tray_menu.append(&PredefinedMenuItem::separator())?;

    // Troubleshooting section
    tray_menu.append(&MenuItem::new("Troubleshooting", false, None))?;

    append_action_item(
        tray_menu,
        &mut menu_id_to_device,
        "Open app folder...",
        DeviceSettingType::OpenAppDirectory,
        None,
        None,
    )?;

    // Update section
    tray_menu.append(&PredefinedMenuItem::separator())?;

    append_action_item(
        tray_menu,
        &mut menu_id_to_device,
        "GitHub...",
        DeviceSettingType::OpenGitHubRepo,
        None,
        None,
    )?;

    let (label, setting_type) = match update_info {
        Some(info) => (
            format!("Update to {}...", info.latest_version),
            DeviceSettingType::PerformUpdate,
        ),
        None => (
            "Check for updates".to_string(),
            DeviceSettingType::CheckForUpdates,
        ),
    };

    append_action_item(
        tray_menu,
        &mut menu_id_to_device,
        &label,
        setting_type,
        None,
        None,
    )?;

    // Quit section
    tray_menu.append(&PredefinedMenuItem::separator())?;

    tray_menu.append(items.quit_item)?;

    Ok(menu_id_to_device)
}

fn append_device_list_to_menu(
    tray_menu: &Menu,
    heading_item: &MenuItem,
    device_type: DeviceType,
    backend: &impl AudioBackend,
    persistent_state: &mut PersistentState,
    menu_id_to_device: &mut HashMap<MenuId, MenuItemDeviceInfo>,
) -> anyhow::Result<()> {
    tray_menu.append(heading_item)?;

    let devices = backend.get_devices(device_type).unwrap_or_default();

    // Get default device ID for Console role to mark it
    let default_device_id = backend
        .get_default_device(device_type, DeviceRole::Console)
        .map(|d| d.id())
        .ok();

    for device in devices {
        let name = device.name();
        let device_id = device.id();
        let volume = device.volume().unwrap_or(0.0);
        let volume_percent = convert_float_to_percent(volume);
        let is_muted = device.is_muted().unwrap_or(false);
        let is_default = default_device_id
            .as_ref()
            .is_some_and(|id| id == &device_id);

        let (is_volume_locked, notify_on_volume_lock, is_unmute_locked, notify_on_unmute_lock) =
            if let Some(settings) = persistent_state.devices.get(&device_id) {
                (
                    settings.is_volume_locked,
                    settings.notify_on_volume_lock,
                    settings.is_unmute_locked,
                    settings.notify_on_unmute_lock,
                )
            } else {
                (false, false, false, false)
            };

        let is_locked = is_volume_locked || is_unmute_locked;
        let label =
            format_device_menu_label(&name, volume_percent, is_default, is_locked, is_muted);

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

        register_menu_item(
            &mut *menu_id_to_device,
            volume_lock_item.id().clone(),
            DeviceSettingType::VolumeLock,
            &device_id,
            &name,
            device_type,
        );
        register_menu_item(
            &mut *menu_id_to_device,
            volume_notify_item.id().clone(),
            DeviceSettingType::VolumeLockNotify,
            &device_id,
            &name,
            device_type,
        );
        register_menu_item(
            &mut *menu_id_to_device,
            unmute_lock_item.id().clone(),
            DeviceSettingType::UnmuteLock,
            &device_id,
            &name,
            device_type,
        );
        register_menu_item(
            &mut *menu_id_to_device,
            unmute_notify_item.id().clone(),
            DeviceSettingType::UnmuteLockNotify,
            &device_id,
            &name,
            device_type,
        );

        // Ensure device exists in persistent state to facilitate updates
        if let Some(settings) = persistent_state.devices.get_mut(&device_id) {
            settings.name = name.clone();
            settings.device_type = device_type;
        }

        submenu.append(&volume_lock_item)?;
        submenu.append(&unmute_lock_item)?;
        submenu.append(&PredefinedMenuItem::separator())?;
        submenu.append(&volume_notify_item)?;
        submenu.append(&unmute_notify_item)?;
        submenu.append(&PredefinedMenuItem::separator())?;

        let properties_item = MenuItem::new("Properties...", true, None);
        register_menu_item(
            &mut *menu_id_to_device,
            properties_item.id().clone(),
            DeviceSettingType::OpenDeviceProperties,
            &device_id,
            &name,
            device_type,
        );
        submenu.append(&properties_item)?;

        let settings_item = MenuItem::new("Settings...", true, None);
        register_menu_item(
            &mut *menu_id_to_device,
            settings_item.id().clone(),
            DeviceSettingType::OpenDeviceSettings,
            &device_id,
            &name,
            device_type,
        );
        submenu.append(&settings_item)?;

        tray_menu.append(&submenu)?;
    }

    let properties_label = match device_type {
        DeviceType::Output => "Playback devices...",
        DeviceType::Input => "Recording devices...",
    };
    append_action_item(
        tray_menu,
        menu_id_to_device,
        properties_label,
        DeviceSettingType::OpenDevicesList,
        None,
        Some(device_type),
    )?;

    tray_menu.append(&PredefinedMenuItem::separator())?;

    Ok(())
}

fn append_priority_list_to_menu(
    tray_menu: &Menu,
    device_type: DeviceType,
    backend: &impl AudioBackend,
    persistent_state: &mut PersistentState,
    temporary_priority: &Option<String>,
    menu_id_to_device: &mut HashMap<MenuId, MenuItemDeviceInfo>,
) -> anyhow::Result<()> {
    let priority_list = persistent_state.get_priority_list(device_type);
    let priority_label = match device_type {
        DeviceType::Output => "Default output device priority",
        DeviceType::Input => "Default input device priority",
    };

    let priority_header = MenuItem::new(priority_label, false, None);
    tray_menu.append(&priority_header)?;

    // Need available devices for "Add device"
    let devices = backend.get_devices(device_type).unwrap_or_default();
    let mut available_devices = Vec::new();
    for device in devices {
        available_devices.push((device.id(), device.name()));
    }

    for (index, device_id) in priority_list.iter().enumerate() {
        let device_name = if let Some(settings) = persistent_state.devices.get(device_id) {
            settings.name.clone()
        } else {
            match backend.get_device_by_id(device_id) {
                Ok(d) => d.name(),
                Err(_) => "Unknown Device".to_string(),
            }
        };

        let label = format!("{}. {}", index + 1, device_name);
        let priority_submenu = Submenu::new(&label, true);

        let move_up_item = MenuItem::new("Move up", index > 0, None);
        if index > 0 {
            menu_id_to_device.insert(
                move_up_item.id().clone(),
                MenuItemDeviceInfo {
                    device_id: Some(device_id.clone()),
                    setting_type: DeviceSettingType::MovePriorityUp,
                    name: device_name.clone(),
                    device_type: Some(device_type),
                },
            );
        }
        priority_submenu.append(&move_up_item)?;

        let move_down_item = MenuItem::new("Move down", index < priority_list.len() - 1, None);
        if index < priority_list.len() - 1 {
            menu_id_to_device.insert(
                move_down_item.id().clone(),
                MenuItemDeviceInfo {
                    device_id: Some(device_id.clone()),
                    setting_type: DeviceSettingType::MovePriorityDown,
                    name: device_name.clone(),
                    device_type: Some(device_type),
                },
            );
        }
        priority_submenu.append(&move_down_item)?;
        priority_submenu.append(&PredefinedMenuItem::separator())?;

        let move_to_top_item = MenuItem::new("Move to top", index > 0, None);
        if index > 0 {
            menu_id_to_device.insert(
                move_to_top_item.id().clone(),
                MenuItemDeviceInfo {
                    device_id: Some(device_id.clone()),
                    setting_type: DeviceSettingType::MovePriorityToTop,
                    name: device_name.clone(),
                    device_type: Some(device_type),
                },
            );
        }
        priority_submenu.append(&move_to_top_item)?;

        let move_to_bottom_item =
            MenuItem::new("Move to bottom", index < priority_list.len() - 1, None);
        if index < priority_list.len() - 1 {
            menu_id_to_device.insert(
                move_to_bottom_item.id().clone(),
                MenuItemDeviceInfo {
                    device_id: Some(device_id.clone()),
                    setting_type: DeviceSettingType::MovePriorityToBottom,
                    name: device_name.clone(),
                    device_type: Some(device_type),
                },
            );
        }
        priority_submenu.append(&move_to_bottom_item)?;
        priority_submenu.append(&PredefinedMenuItem::separator())?;

        let remove_priority_item = MenuItem::new("Remove device", true, None);
        menu_id_to_device.insert(
            remove_priority_item.id().clone(),
            MenuItemDeviceInfo {
                device_id: Some(device_id.clone()),
                setting_type: DeviceSettingType::RemoveFromPriority,
                name: device_name.clone(),
                device_type: Some(device_type),
            },
        );
        priority_submenu.append(&remove_priority_item)?;

        tray_menu.append(&priority_submenu)?;
    }

    let mut devices_to_add = Vec::new();
    for (id, name) in &available_devices {
        if !priority_list.contains(id) {
            devices_to_add.push((id, name));
        }
    }

    let add_device_submenu = Submenu::new("Add device", !devices_to_add.is_empty());
    for (id, name) in devices_to_add {
        let item = MenuItem::new(name, true, None);
        menu_id_to_device.insert(
            item.id().clone(),
            MenuItemDeviceInfo {
                device_id: Some(id.clone()),
                setting_type: DeviceSettingType::AddToPriority,
                name: name.clone(),
                device_type: Some(device_type),
            },
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

    menu_id_to_device.insert(
        notify_item.id().clone(),
        MenuItemDeviceInfo {
            device_id: None,
            setting_type: DeviceSettingType::PriorityRestoreNotify,
            name: "Priority Restore Notify".to_string(),
            device_type: Some(device_type),
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

    menu_id_to_device.insert(
        switch_comm_item.id().clone(),
        MenuItemDeviceInfo {
            device_id: None,
            setting_type: DeviceSettingType::SwitchCommunicationDevice,
            name: "Switch Communication Device".to_string(),
            device_type: Some(device_type),
        },
    );
    tray_menu.append(&switch_comm_item)?;

    tray_menu.append(&PredefinedMenuItem::separator())?;

    Ok(())
}

pub struct MenuEventResult {
    pub should_save: bool,
    pub devices_changed: bool,
    pub update_action: UpdateAction,
}

/// Returns `true` if the device has no active locks or notifications,
/// meaning its settings entry can be removed when not in a priority list.
fn device_settings_are_empty(settings: &DeviceSettings) -> bool {
    !settings.is_volume_locked
        && !settings.is_unmute_locked
        && !settings.notify_on_volume_lock
        && !settings.notify_on_unmute_lock
}

/// Applies a device lock/notify toggle and returns whether the device entry should be removed.
fn apply_device_lock_toggle(
    setting_type: &DeviceSettingType,
    is_checked: bool,
    device_id: &str,
    device_name: &str,
    device_type: DeviceType,
    persistent_state: &mut PersistentState,
    backend: &impl AudioBackend,
) -> bool {
    let device_settings = persistent_state
        .devices
        .entry(device_id.to_string())
        .or_insert_with(|| DeviceSettings {
            is_volume_locked: false,
            volume_percent: 0.0,
            notify_on_volume_lock: false,
            is_unmute_locked: false,
            notify_on_unmute_lock: false,
            device_type,
            name: device_name.to_string(),
        });

    match setting_type {
        DeviceSettingType::VolumeLock => {
            if is_checked {
                if let Ok(device) = backend.get_device_by_id(device_id)
                    && let Ok(vol) = device.volume()
                {
                    device_settings.volume_percent = convert_float_to_percent(vol);
                    device_settings.is_volume_locked = true;
                } else {
                    log_and_notify_error(
                        "Failed to Lock Volume",
                        &format!("Failed to get volume for device {device_name}, cannot lock."),
                    );
                    device_settings.is_volume_locked = false;
                }
            } else {
                device_settings.is_volume_locked = false;
            }
        }
        DeviceSettingType::VolumeLockNotify => {
            device_settings.notify_on_volume_lock = is_checked;
        }
        DeviceSettingType::UnmuteLock => {
            device_settings.is_unmute_locked = is_checked;
        }
        DeviceSettingType::UnmuteLockNotify => {
            device_settings.notify_on_unmute_lock = is_checked;
        }
        _ => {}
    }

    device_settings_are_empty(device_settings)
}

fn handle_priority_event(
    setting_type: &DeviceSettingType,
    device_id: &str,
    device_type: DeviceType,
    device_name: &str,
    persistent_state: &mut PersistentState,
) -> bool {
    match setting_type {
        DeviceSettingType::AddToPriority => {
            let list = persistent_state.get_priority_list_mut(device_type);
            if !list.contains(&device_id.to_string()) {
                list.push(device_id.to_string());
                persistent_state
                    .devices
                    .entry(device_id.to_string())
                    .or_insert_with(|| DeviceSettings {
                        is_volume_locked: false,
                        volume_percent: 0.0,
                        notify_on_volume_lock: false,
                        is_unmute_locked: false,
                        notify_on_unmute_lock: false,
                        device_type,
                        name: device_name.to_string(),
                    });
                true
            } else {
                false
            }
        }
        DeviceSettingType::RemoveFromPriority => {
            let list = persistent_state.get_priority_list_mut(device_type);
            if let Some(pos) = list.iter().position(|x| x == device_id) {
                list.remove(pos);
                if let Some(settings) = persistent_state.devices.get(device_id)
                    && device_settings_are_empty(settings)
                {
                    persistent_state.devices.remove(device_id);
                }
                true
            } else {
                false
            }
        }
        DeviceSettingType::MovePriorityUp => {
            let list = persistent_state.get_priority_list_mut(device_type);
            if let Some(pos) = list.iter().position(|x| x == device_id)
                && pos > 0
            {
                list.swap(pos, pos - 1);
                true
            } else {
                false
            }
        }
        DeviceSettingType::MovePriorityDown => {
            let list = persistent_state.get_priority_list_mut(device_type);
            if let Some(pos) = list.iter().position(|x| x == device_id)
                && pos < list.len() - 1
            {
                list.swap(pos, pos + 1);
                true
            } else {
                false
            }
        }
        DeviceSettingType::MovePriorityToTop => {
            let list = persistent_state.get_priority_list_mut(device_type);
            if let Some(pos) = list.iter().position(|x| x == device_id)
                && pos > 0
            {
                let device_id = list.remove(pos);
                list.insert(0, device_id);
                true
            } else {
                false
            }
        }
        DeviceSettingType::MovePriorityToBottom => {
            let list = persistent_state.get_priority_list_mut(device_type);
            if let Some(pos) = list.iter().position(|x| x == device_id)
                && pos < list.len() - 1
            {
                let device_id = list.remove(pos);
                list.push(device_id);
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

pub fn handle_menu_event(
    event: &tray_icon::menu::MenuEvent,
    menu_info: &MenuItemDeviceInfo,
    tray_menu: &Menu,
    persistent_state: &mut PersistentState,
    backend: &impl AudioBackend,
    temporary_priorities: &mut TemporaryPriorities,
    update_info: &Option<UpdateInfo>,
) -> MenuEventResult {
    let mut should_save = false;
    let mut devices_changed = false;
    let mut update_action = UpdateAction::None;

    match menu_info.setting_type {
        DeviceSettingType::VolumeLock
        | DeviceSettingType::VolumeLockNotify
        | DeviceSettingType::UnmuteLock
        | DeviceSettingType::UnmuteLockNotify => {
            let (Some(device_id), Some(device_type)) =
                (&menu_info.device_id, menu_info.device_type)
            else {
                return MenuEventResult {
                    should_save,
                    devices_changed,
                    update_action,
                };
            };
            if let Some(item) = find_menu_item(tray_menu, &event.id)
                && let Some(check_item) = item.as_check_menuitem()
            {
                let should_remove = apply_device_lock_toggle(
                    &menu_info.setting_type,
                    check_item.is_checked(),
                    device_id,
                    &menu_info.name,
                    device_type,
                    persistent_state,
                    backend,
                );

                if should_remove {
                    let is_in_priority = persistent_state.output_priority_list.contains(device_id)
                        || persistent_state.input_priority_list.contains(device_id);

                    if !is_in_priority {
                        persistent_state.devices.remove(device_id);
                    }
                }
                should_save = true;
            }
        }
        DeviceSettingType::AddToPriority
        | DeviceSettingType::RemoveFromPriority
        | DeviceSettingType::MovePriorityUp
        | DeviceSettingType::MovePriorityDown
        | DeviceSettingType::MovePriorityToTop
        | DeviceSettingType::MovePriorityToBottom => {
            let (Some(device_id), Some(device_type)) =
                (&menu_info.device_id, menu_info.device_type)
            else {
                return MenuEventResult {
                    should_save,
                    devices_changed,
                    update_action,
                };
            };
            should_save = handle_priority_event(
                &menu_info.setting_type,
                device_id,
                device_type,
                &menu_info.name,
                persistent_state,
            );
        }
        DeviceSettingType::PriorityRestoreNotify => {
            if let Some(device_type) = menu_info.device_type
                && let Some(item) = find_menu_item(tray_menu, &event.id)
                && let Some(check_item) = item.as_check_menuitem()
            {
                let is_checked = check_item.is_checked();
                persistent_state.set_notify_on_priority_restore(device_type, is_checked);
                should_save = true;
            }
        }
        DeviceSettingType::SwitchCommunicationDevice => {
            if let Some(device_type) = menu_info.device_type
                && let Some(item) = find_menu_item(tray_menu, &event.id)
                && let Some(check_item) = item.as_check_menuitem()
            {
                let is_checked = check_item.is_checked();
                persistent_state.set_switch_communication_device(device_type, is_checked);
                should_save = true;
            }
        }
        DeviceSettingType::SetTemporaryPriority => {
            let (Some(device_id), Some(device_type)) =
                (&menu_info.device_id, menu_info.device_type)
            else {
                return MenuEventResult {
                    should_save,
                    devices_changed,
                    update_action,
                };
            };
            if let Some(item) = find_menu_item(tray_menu, &event.id) {
                let is_checked = if let Some(check_item) = item.as_check_menuitem() {
                    check_item.is_checked()
                } else {
                    false
                };

                match device_type {
                    DeviceType::Output => {
                        temporary_priorities.output = if is_checked {
                            Some(device_id.clone())
                        } else {
                            None
                        };
                    }
                    DeviceType::Input => {
                        temporary_priorities.input = if is_checked {
                            Some(device_id.clone())
                        } else {
                            None
                        };
                    }
                }
                devices_changed = true;
            }
        }
        DeviceSettingType::OpenDevicesList => {
            if let Some(device_type) = menu_info.device_type
                && let Err(e) = open_devices_list(device_type)
            {
                log::error!("Failed to open devices list: {e:#}");
            }
        }
        DeviceSettingType::OpenDeviceProperties => {
            if let Some(device_id) = &menu_info.device_id {
                // mmsys.cpl accepts device_id as a tab parameter to open the correct properties page
                if let Err(e) = open_device_properties(device_id) {
                    log::error!("Failed to open device properties: {e:#}");
                }
            }
        }
        DeviceSettingType::OpenSoundSettings => {
            if let Err(e) = open_sound_settings() {
                log::error!("Failed to open sound settings: {e:#}");
            }
        }
        DeviceSettingType::OpenDeviceSettings => {
            if let Some(device_id) = &menu_info.device_id
                && let Err(e) = open_device_settings(device_id)
            {
                log::error!("Failed to open device settings: {e:#}");
            }
        }
        DeviceSettingType::OpenVolumeMixer => {
            if let Err(e) = open_volume_mixer() {
                log::error!("Failed to open volume mixer: {e:#}");
            }
        }
        DeviceSettingType::CheckForUpdates => {
            update_action = UpdateAction::Check;
        }
        DeviceSettingType::PerformUpdate => {
            if let Some(info) = update_info {
                update_action = UpdateAction::Perform(info.clone());
            }
        }
        DeviceSettingType::OpenGitHubRepo => {
            if let Err(e) = open_url(GITHUB_REPO_URL) {
                log::error!("Failed to open GitHub repo: {e:#}");
            }
        }
        DeviceSettingType::OpenAppDirectory => match get_executable_directory() {
            Ok(dir) => {
                if let Err(e) = open_path(&dir) {
                    log::error!("Failed to open app directory: {e:#}");
                }
            }
            Err(e) => log::error!("Failed to get executable directory: {e:#}"),
        },
    }

    MenuEventResult {
        should_save,
        devices_changed,
        update_action,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_label_basic() {
        let label = format_device_menu_label("Speakers", 50.0, false, false, false);
        assert_eq!(label, "Speakers · 50%");
    }

    #[test]
    fn to_label_default_device() {
        let label = format_device_menu_label("Speakers", 75.0, true, false, false);
        assert_eq!(label, "Speakers · ☆ · 75%");
    }

    #[test]
    fn to_label_locked() {
        let label = format_device_menu_label("Speakers", 100.0, false, true, false);
        assert_eq!(label, "Speakers · 100% · 🔒");
    }

    #[test]
    fn to_label_muted() {
        let label = format_device_menu_label("Mic", 0.0, false, false, true);
        assert_eq!(label, "Mic · 0% 🚫");
    }

    #[test]
    fn to_label_all_indicators() {
        let label = format_device_menu_label("Headset", 42.0, true, true, true);
        assert_eq!(label, "Headset · ☆ · 42% 🚫 · 🔒");
    }

    #[test]
    fn device_settings_empty_when_all_false() {
        let settings = DeviceSettings {
            is_volume_locked: false,
            volume_percent: 50.0,
            notify_on_volume_lock: false,
            is_unmute_locked: false,
            notify_on_unmute_lock: false,
            device_type: DeviceType::Output,
            name: "Test".to_string(),
        };
        assert!(device_settings_are_empty(&settings));
    }

    #[test]
    fn device_settings_not_empty_when_locked() {
        let settings = DeviceSettings {
            is_volume_locked: true,
            volume_percent: 50.0,
            notify_on_volume_lock: false,
            is_unmute_locked: false,
            notify_on_unmute_lock: false,
            device_type: DeviceType::Output,
            name: "Test".to_string(),
        };
        assert!(!device_settings_are_empty(&settings));
    }

    fn make_state_with_device(device_id: &str, device_type: DeviceType) -> PersistentState {
        let mut state = PersistentState::default();
        state.devices.insert(
            device_id.to_string(),
            DeviceSettings {
                is_volume_locked: false,
                volume_percent: 0.0,
                notify_on_volume_lock: false,
                is_unmute_locked: false,
                notify_on_unmute_lock: false,
                device_type,
                name: "Test Device".to_string(),
            },
        );
        state
    }

    #[test]
    fn priority_add_inserts_device() {
        let mut state = PersistentState::default();
        let changed = handle_priority_event(
            &DeviceSettingType::AddToPriority,
            "dev1",
            DeviceType::Output,
            "Speaker",
            &mut state,
        );
        assert!(changed);
        assert_eq!(state.output_priority_list, vec!["dev1"]);
        assert!(state.devices.contains_key("dev1"));
    }

    #[test]
    fn priority_add_duplicate_no_op() {
        let mut state = PersistentState::default();
        state.output_priority_list.push("dev1".to_string());
        let changed = handle_priority_event(
            &DeviceSettingType::AddToPriority,
            "dev1",
            DeviceType::Output,
            "Speaker",
            &mut state,
        );
        assert!(!changed);
    }

    #[test]
    fn priority_remove_cleans_empty_device() {
        let mut state = make_state_with_device("dev1", DeviceType::Output);
        state.output_priority_list.push("dev1".to_string());
        let changed = handle_priority_event(
            &DeviceSettingType::RemoveFromPriority,
            "dev1",
            DeviceType::Output,
            "Speaker",
            &mut state,
        );
        assert!(changed);
        assert!(state.output_priority_list.is_empty());
        assert!(!state.devices.contains_key("dev1"));
    }

    #[test]
    fn priority_move_up() {
        let mut state = PersistentState {
            output_priority_list: vec!["a".into(), "b".into(), "c".into()],
            ..Default::default()
        };
        let changed = handle_priority_event(
            &DeviceSettingType::MovePriorityUp,
            "b",
            DeviceType::Output,
            "B",
            &mut state,
        );
        assert!(changed);
        assert_eq!(state.output_priority_list, vec!["b", "a", "c"]);
    }

    #[test]
    fn priority_move_up_already_top() {
        let mut state = PersistentState {
            output_priority_list: vec!["a".into(), "b".into()],
            ..Default::default()
        };
        let changed = handle_priority_event(
            &DeviceSettingType::MovePriorityUp,
            "a",
            DeviceType::Output,
            "A",
            &mut state,
        );
        assert!(!changed);
    }

    #[test]
    fn priority_move_down() {
        let mut state = PersistentState {
            output_priority_list: vec!["a".into(), "b".into(), "c".into()],
            ..Default::default()
        };
        let changed = handle_priority_event(
            &DeviceSettingType::MovePriorityDown,
            "b",
            DeviceType::Output,
            "B",
            &mut state,
        );
        assert!(changed);
        assert_eq!(state.output_priority_list, vec!["a", "c", "b"]);
    }

    #[test]
    fn priority_move_to_top() {
        let mut state = PersistentState {
            output_priority_list: vec!["a".into(), "b".into(), "c".into()],
            ..Default::default()
        };
        let changed = handle_priority_event(
            &DeviceSettingType::MovePriorityToTop,
            "c",
            DeviceType::Output,
            "C",
            &mut state,
        );
        assert!(changed);
        assert_eq!(state.output_priority_list, vec!["c", "a", "b"]);
    }

    #[test]
    fn priority_move_to_bottom() {
        let mut state = PersistentState {
            output_priority_list: vec!["a".into(), "b".into(), "c".into()],
            ..Default::default()
        };
        let changed = handle_priority_event(
            &DeviceSettingType::MovePriorityToBottom,
            "a",
            DeviceType::Output,
            "A",
            &mut state,
        );
        assert!(changed);
        assert_eq!(state.output_priority_list, vec!["b", "c", "a"]);
    }

    #[test]
    fn priority_input_type_uses_input_list() {
        let mut state = PersistentState::default();
        handle_priority_event(
            &DeviceSettingType::AddToPriority,
            "mic1",
            DeviceType::Input,
            "Mic",
            &mut state,
        );
        assert!(state.output_priority_list.is_empty());
        assert_eq!(state.input_priority_list, vec!["mic1"]);
    }
}
