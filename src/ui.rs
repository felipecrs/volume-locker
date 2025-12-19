use crate::audio::AudioBackend;
use crate::config::PersistentState;
use crate::types::{DeviceRole, DeviceSettingType, DeviceSettings, DeviceType, MenuItemDeviceInfo};
use crate::utils::convert_float_to_percent;
use std::collections::HashMap;
use tray_icon::menu::{
    CheckMenuItem, Menu, MenuId, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu,
};

pub fn to_label(
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

#[allow(clippy::too_many_arguments)]
pub fn rebuild_tray_menu(
    tray_menu: &Menu,
    backend: &impl AudioBackend,
    persistent_state: &mut PersistentState,
    temporary_priority_output: &Option<String>,
    temporary_priority_input: &Option<String>,
    auto_launch_enabled: bool,
    auto_launch_check_item: &CheckMenuItem,
    quit_item: &MenuItem,
    output_devices_heading_item: &MenuItem,
    input_devices_heading_item: &MenuItem,
) -> HashMap<MenuId, MenuItemDeviceInfo> {
    // Clear the menu
    for _ in 0..tray_menu.items().len() {
        tray_menu.remove_at(0);
    }
    let mut menu_id_to_device: HashMap<MenuId, MenuItemDeviceInfo> = HashMap::new();

    for (heading_item, device_type) in [
        (output_devices_heading_item, DeviceType::Output),
        (input_devices_heading_item, DeviceType::Input),
    ] {
        append_device_list_to_menu(
            tray_menu,
            heading_item,
            device_type,
            backend,
            persistent_state,
            &mut menu_id_to_device,
        );
    }

    for device_type in [DeviceType::Output, DeviceType::Input] {
        let temporary_priority = match device_type {
            DeviceType::Output => temporary_priority_output,
            DeviceType::Input => temporary_priority_input,
        };
        append_priority_list_to_menu(
            tray_menu,
            device_type,
            backend,
            persistent_state,
            temporary_priority,
            &mut menu_id_to_device,
        );
    }

    tray_menu
        .append(&MenuItem::new(
            "Temporary default device priority",
            false,
            None,
        ))
        .unwrap();

    for device_type in [DeviceType::Output, DeviceType::Input] {
        let devices = backend.get_devices(device_type).unwrap_or_default();
        let mut available_devices = Vec::new();
        for device in devices {
            available_devices.push((device.id(), device.name()));
        }

        let temp_id_opt = match device_type {
            DeviceType::Output => temporary_priority_output.as_ref(),
            DeviceType::Input => temporary_priority_input.as_ref(),
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
                    device_id: id.clone(),
                    setting_type: DeviceSettingType::SetTemporaryPriority,
                    name: name.clone(),
                    device_type,
                },
            );
            submenu.append(&item).unwrap();
        }
        tray_menu.append(&submenu).unwrap();
    }
    tray_menu.append(&PredefinedMenuItem::separator()).unwrap();

    // Refresh check items
    auto_launch_check_item.set_checked(auto_launch_enabled);
    tray_menu.append(auto_launch_check_item).unwrap();
    tray_menu.append(&PredefinedMenuItem::separator()).unwrap();
    tray_menu.append(quit_item).unwrap();

    menu_id_to_device
}

fn append_device_list_to_menu(
    tray_menu: &Menu,
    heading_item: &MenuItem,
    device_type: DeviceType,
    backend: &impl AudioBackend,
    persistent_state: &mut PersistentState,
    menu_id_to_device: &mut HashMap<MenuId, MenuItemDeviceInfo>,
) {
    tray_menu.append(heading_item).unwrap();

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
            .map(|id| id == &device_id)
            .unwrap_or(false);

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
        let label = to_label(&name, volume_percent, is_default, is_locked, is_muted);

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

        menu_id_to_device.insert(
            volume_lock_item.id().clone(),
            MenuItemDeviceInfo {
                device_id: device_id.clone(),
                setting_type: DeviceSettingType::VolumeLock,
                name: name.clone(),
                device_type,
            },
        );
        menu_id_to_device.insert(
            volume_notify_item.id().clone(),
            MenuItemDeviceInfo {
                device_id: device_id.clone(),
                setting_type: DeviceSettingType::VolumeLockNotify,
                name: name.clone(),
                device_type,
            },
        );
        menu_id_to_device.insert(
            unmute_lock_item.id().clone(),
            MenuItemDeviceInfo {
                device_id: device_id.clone(),
                setting_type: DeviceSettingType::UnmuteLock,
                name: name.clone(),
                device_type,
            },
        );
        menu_id_to_device.insert(
            unmute_notify_item.id().clone(),
            MenuItemDeviceInfo {
                device_id: device_id.clone(),
                setting_type: DeviceSettingType::UnmuteLockNotify,
                name: name.clone(),
                device_type,
            },
        );

        // Ensure device exists in persistent state to facilitate updates
        if let Some(settings) = persistent_state.devices.get_mut(&device_id) {
            settings.name = name.clone();
            settings.device_type = device_type;
        }

        submenu.append(&volume_lock_item).unwrap();
        submenu.append(&unmute_lock_item).unwrap();
        submenu.append(&PredefinedMenuItem::separator()).unwrap();
        submenu.append(&volume_notify_item).unwrap();
        submenu.append(&unmute_notify_item).unwrap();

        tray_menu.append(&submenu).unwrap();
    }
    tray_menu.append(&PredefinedMenuItem::separator()).unwrap();
}

fn append_priority_list_to_menu(
    tray_menu: &Menu,
    device_type: DeviceType,
    backend: &impl AudioBackend,
    persistent_state: &mut PersistentState,
    temporary_priority: &Option<String>,
    menu_id_to_device: &mut HashMap<MenuId, MenuItemDeviceInfo>,
) {
    let priority_list = persistent_state.get_priority_list(device_type);
    let priority_label = match device_type {
        DeviceType::Output => "Default output device priority",
        DeviceType::Input => "Default input device priority",
    };

    let priority_header = MenuItem::new(priority_label, false, None);
    tray_menu.append(&priority_header).unwrap();

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
                    device_id: device_id.clone(),
                    setting_type: DeviceSettingType::MovePriorityUp,
                    name: device_name.clone(),
                    device_type,
                },
            );
        }
        priority_submenu.append(&move_up_item).unwrap();

        let move_down_item = MenuItem::new("Move down", index < priority_list.len() - 1, None);
        if index < priority_list.len() - 1 {
            menu_id_to_device.insert(
                move_down_item.id().clone(),
                MenuItemDeviceInfo {
                    device_id: device_id.clone(),
                    setting_type: DeviceSettingType::MovePriorityDown,
                    name: device_name.clone(),
                    device_type,
                },
            );
        }
        priority_submenu.append(&move_down_item).unwrap();
        priority_submenu
            .append(&PredefinedMenuItem::separator())
            .unwrap();

        let remove_priority_item = MenuItem::new("Remove device", true, None);
        menu_id_to_device.insert(
            remove_priority_item.id().clone(),
            MenuItemDeviceInfo {
                device_id: device_id.clone(),
                setting_type: DeviceSettingType::RemoveFromPriority,
                name: device_name.clone(),
                device_type,
            },
        );
        priority_submenu.append(&remove_priority_item).unwrap();

        tray_menu.append(&priority_submenu).unwrap();
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
                device_id: id.clone(),
                setting_type: DeviceSettingType::AddToPriority,
                name: name.clone(),
                device_type,
            },
        );
        add_device_submenu.append(&item).unwrap();
    }
    tray_menu.append(&add_device_submenu).unwrap();

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
            device_id: String::new(),
            setting_type: DeviceSettingType::PriorityRestoreNotify,
            name: "Priority Restore Notify".to_string(),
            device_type,
        },
    );
    tray_menu.append(&notify_item).unwrap();

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
            device_id: String::new(),
            setting_type: DeviceSettingType::SwitchCommunicationDevice,
            name: "Switch Communication Device".to_string(),
            device_type,
        },
    );
    tray_menu.append(&switch_comm_item).unwrap();

    let switch_foreground = persistent_state.get_switch_foreground_app(device_type);

    let switch_foreground_item = CheckMenuItem::new(
        "Also switch foreground program",
        !priority_list.is_empty() || temporary_priority.is_some(),
        switch_foreground,
        None,
    );

    menu_id_to_device.insert(
        switch_foreground_item.id().clone(),
        MenuItemDeviceInfo {
            device_id: String::new(),
            setting_type: DeviceSettingType::SwitchForegroundApp,
            name: "Switch Foreground App".to_string(),
            device_type,
        },
    );
    tray_menu.append(&switch_foreground_item).unwrap();

    tray_menu.append(&PredefinedMenuItem::separator()).unwrap();
}

pub struct MenuEventResult {
    pub should_save: bool,
    pub devices_changed: bool,
}

pub fn handle_menu_event(
    event: &tray_icon::menu::MenuEvent,
    menu_info: &MenuItemDeviceInfo,
    tray_menu: &Menu,
    persistent_state: &mut PersistentState,
    backend: &impl AudioBackend,
    temporary_priority_output: &mut Option<String>,
    temporary_priority_input: &mut Option<String>,
) -> MenuEventResult {
    let mut should_save = false;
    let mut devices_changed = false;

    match menu_info.setting_type {
        DeviceSettingType::VolumeLock
        | DeviceSettingType::VolumeLockNotify
        | DeviceSettingType::UnmuteLock
        | DeviceSettingType::UnmuteLockNotify => {
            if let Some(item) = find_menu_item(tray_menu, &event.id)
                && let Some(check_item) = item.as_check_menuitem()
            {
                let is_checked = check_item.is_checked();
                let mut should_remove = false;

                {
                    let device_settings = persistent_state
                        .devices
                        .entry(menu_info.device_id.clone())
                        .or_insert_with(|| DeviceSettings {
                            is_volume_locked: false,
                            volume_percent: 0.0,
                            notify_on_volume_lock: false,
                            is_unmute_locked: false,
                            notify_on_unmute_lock: false,
                            device_type: menu_info.device_type,
                            name: menu_info.name.clone(),
                        });

                    match menu_info.setting_type {
                        DeviceSettingType::VolumeLock => {
                            if is_checked {
                                if let Ok(device) = backend.get_device_by_id(&menu_info.device_id)
                                    && let Ok(vol) = device.volume()
                                {
                                    device_settings.volume_percent = convert_float_to_percent(vol);
                                    device_settings.is_volume_locked = true;
                                } else {
                                    log::error!(
                                        "Failed to get volume for device {}, cannot lock.",
                                        menu_info.name
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

                    if !device_settings.is_volume_locked
                        && !device_settings.is_unmute_locked
                        && !device_settings.notify_on_volume_lock
                        && !device_settings.notify_on_unmute_lock
                    {
                        should_remove = true;
                    }
                }

                if should_remove {
                    let is_in_priority = persistent_state
                        .output_priority_list
                        .contains(&menu_info.device_id)
                        || persistent_state
                            .input_priority_list
                            .contains(&menu_info.device_id);

                    if !is_in_priority {
                        persistent_state.devices.remove(&menu_info.device_id);
                    }
                }
                should_save = true;
            }
        }
        DeviceSettingType::AddToPriority => {
            let list = persistent_state.get_priority_list_mut(menu_info.device_type);
            if !list.contains(&menu_info.device_id) {
                list.push(menu_info.device_id.clone());

                persistent_state
                    .devices
                    .entry(menu_info.device_id.clone())
                    .or_insert_with(|| DeviceSettings {
                        is_volume_locked: false,
                        volume_percent: 0.0,
                        notify_on_volume_lock: false,
                        is_unmute_locked: false,
                        notify_on_unmute_lock: false,
                        device_type: menu_info.device_type,
                        name: menu_info.name.clone(),
                    });

                should_save = true;
            }
        }
        DeviceSettingType::RemoveFromPriority => {
            let list = persistent_state.get_priority_list_mut(menu_info.device_type);
            if let Some(pos) = list.iter().position(|x| x == &menu_info.device_id) {
                list.remove(pos);
                should_save = true;

                if let Some(settings) = persistent_state.devices.get(&menu_info.device_id)
                    && !settings.is_volume_locked
                    && !settings.is_unmute_locked
                    && !settings.notify_on_volume_lock
                    && !settings.notify_on_unmute_lock
                {
                    persistent_state.devices.remove(&menu_info.device_id);
                }
            }
        }
        DeviceSettingType::MovePriorityUp => {
            let list = persistent_state.get_priority_list_mut(menu_info.device_type);
            if let Some(pos) = list.iter().position(|x| x == &menu_info.device_id)
                && pos > 0
            {
                list.swap(pos, pos - 1);
                should_save = true;
            }
        }
        DeviceSettingType::MovePriorityDown => {
            let list = persistent_state.get_priority_list_mut(menu_info.device_type);
            if let Some(pos) = list.iter().position(|x| x == &menu_info.device_id)
                && pos < list.len() - 1
            {
                list.swap(pos, pos + 1);
                should_save = true;
            }
        }
        DeviceSettingType::PriorityRestoreNotify => {
            if let Some(item) = find_menu_item(tray_menu, &event.id)
                && let Some(check_item) = item.as_check_menuitem()
            {
                let is_checked = check_item.is_checked();
                persistent_state.set_notify_on_priority_restore(menu_info.device_type, is_checked);
                should_save = true;
            }
        }
        DeviceSettingType::SwitchCommunicationDevice => {
            if let Some(item) = find_menu_item(tray_menu, &event.id)
                && let Some(check_item) = item.as_check_menuitem()
            {
                let is_checked = check_item.is_checked();
                persistent_state.set_switch_communication_device(menu_info.device_type, is_checked);
                should_save = true;
            }
        }
        DeviceSettingType::SwitchForegroundApp => {
            if let Some(item) = find_menu_item(tray_menu, &event.id)
                && let Some(check_item) = item.as_check_menuitem()
            {
                let is_checked = check_item.is_checked();
                persistent_state.set_switch_foreground_app(menu_info.device_type, is_checked);
                should_save = true;
            }
        }
        DeviceSettingType::SetTemporaryPriority => {
            if let Some(item) = find_menu_item(tray_menu, &event.id) {
                let is_checked = if let Some(check_item) = item.as_check_menuitem() {
                    check_item.is_checked()
                } else {
                    false
                };

                match menu_info.device_type {
                    DeviceType::Output => {
                        *temporary_priority_output = if is_checked {
                            Some(menu_info.device_id.clone())
                        } else {
                            None
                        };
                    }
                    DeviceType::Input => {
                        *temporary_priority_input = if is_checked {
                            Some(menu_info.device_id.clone())
                        } else {
                            None
                        };
                    }
                }
                devices_changed = true;
            }
        }
    }

    MenuEventResult {
        should_save,
        devices_changed,
    }
}
