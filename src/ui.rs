use crate::audio::{
    convert_float_to_percent, get_audio_endpoint, get_device_by_id, get_device_id, get_device_name,
    get_mute, get_volume, is_default_device,
};
use crate::config::PersistentState;
use crate::types::{DeviceSettingType, DeviceType, MenuItemDeviceInfo};
use std::collections::HashMap;
use tray_icon::menu::{
    CheckMenuItem, Menu, MenuId, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu,
};
use windows::Win32::Media::Audio::{
    DEVICE_STATE_ACTIVE, IMMDeviceCollection, IMMDeviceEnumerator, eCapture, eRender,
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
    device_enumerator: &IMMDeviceEnumerator,
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
        tray_menu.append(heading_item).unwrap();
        let endpoint_type = match device_type {
            DeviceType::Output => eRender,
            DeviceType::Input => eCapture,
        };
        let devices: IMMDeviceCollection = unsafe {
            device_enumerator
                .EnumAudioEndpoints(endpoint_type, DEVICE_STATE_ACTIVE)
                .unwrap()
        };
        let count = unsafe { devices.GetCount().unwrap() };

        for i in 0..count {
            let device = unsafe { devices.Item(i).unwrap() };
            let name = get_device_name(&device).unwrap();
            let device_id = get_device_id(&device).unwrap();
            let endpoint = get_audio_endpoint(&device).unwrap();
            let volume = get_volume(&endpoint).unwrap();
            let volume_percent = convert_float_to_percent(volume);
            let is_muted = get_mute(&endpoint).unwrap_or(false);
            let is_default = is_default_device(device_enumerator, &device, device_type);

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
            let unmute_lock_item =
                CheckMenuItem::new("Keep unmuted", true, is_unmute_locked, None);
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

    for device_type in [DeviceType::Output, DeviceType::Input] {
        let (priority_list, priority_label) = match device_type {
            DeviceType::Output => (
                &persistent_state.output_priority_list,
                "Default output device priority",
            ),
            DeviceType::Input => (
                &persistent_state.input_priority_list,
                "Default input device priority",
            ),
        };

        let priority_header = MenuItem::new(priority_label, false, None);
        tray_menu.append(&priority_header).unwrap();

        // Need available devices for "Add device"
        let endpoint_type = match device_type {
            DeviceType::Output => eRender,
            DeviceType::Input => eCapture,
        };
        let devices: IMMDeviceCollection = unsafe {
            device_enumerator
                .EnumAudioEndpoints(endpoint_type, DEVICE_STATE_ACTIVE)
                .unwrap()
        };
        let count = unsafe { devices.GetCount().unwrap() };
        let mut available_devices = Vec::new();
        for i in 0..count {
            let device = unsafe { devices.Item(i).unwrap() };
            let name = get_device_name(&device).unwrap();
            let device_id = get_device_id(&device).unwrap();
            available_devices.push((device_id, name));
        }

        let temp_id_opt = match device_type {
            DeviceType::Output => temporary_priority_output.as_ref(),
            DeviceType::Input => temporary_priority_input.as_ref(),
        };

        for (index, device_id) in priority_list.iter().enumerate() {
            let device_name = if let Some(settings) = persistent_state.devices.get(device_id) {
                settings.name.clone()
            } else {
                match get_device_by_id(device_enumerator, device_id) {
                    Ok(d) => get_device_name(&d).unwrap_or_else(|_| "Unknown Device".to_string()),
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
            priority_submenu.append(&PredefinedMenuItem::separator()).unwrap();

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

        let notify_on_restore = match device_type {
            DeviceType::Output => persistent_state.notify_on_priority_restore_output,
            DeviceType::Input => persistent_state.notify_on_priority_restore_input,
        };

        let notify_item = CheckMenuItem::new(
            "Notify on priority restore",
            !priority_list.is_empty() || temp_id_opt.is_some(),
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

        let switch_communication = match device_type {
            DeviceType::Output => persistent_state.switch_communication_device_output,
            DeviceType::Input => persistent_state.switch_communication_device_input,
        };

        let switch_comm_item = CheckMenuItem::new(
            "Also switch default communication device",
            !priority_list.is_empty() || temp_id_opt.is_some(),
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

        tray_menu.append(&PredefinedMenuItem::separator()).unwrap();
    }

    tray_menu
        .append(&MenuItem::new(
            "Temporary default device priority",
            false,
            None,
        ))
        .unwrap();

    for device_type in [DeviceType::Output, DeviceType::Input] {
        let endpoint_type = match device_type {
            DeviceType::Output => eRender,
            DeviceType::Input => eCapture,
        };
        let devices: IMMDeviceCollection = unsafe {
            device_enumerator
                .EnumAudioEndpoints(endpoint_type, DEVICE_STATE_ACTIVE)
                .unwrap()
        };
        let count = unsafe { devices.GetCount().unwrap() };
        let mut available_devices = Vec::new();
        for i in 0..count {
            let device = unsafe { devices.Item(i).unwrap() };
            let name = get_device_name(&device).unwrap();
            let device_id = get_device_id(&device).unwrap();
            available_devices.push((device_id, name));
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
                match get_device_by_id(device_enumerator, temp_id) {
                    Ok(d) => get_device_name(&d).unwrap_or_else(|_| "Unknown Device".to_string()),
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
