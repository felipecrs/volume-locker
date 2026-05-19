use super::{lookup_device_name, register_menu_item};
use crate::audio::AudioBackend;
use crate::config::PersistentState;
use crate::types::{DeviceId, DeviceType, TemporaryPriorities};
use crate::ui::{DeviceAction, MenuAction, MenuIdMap, MenuItemInfo, PreferenceAction};
use tray_icon::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};

fn build_priority_item_submenu(
    index: usize,
    list_len: usize,
    device_id: &DeviceId,
    device_name: &str,
    device_type: DeviceType,
    map: &mut MenuIdMap,
) -> anyhow::Result<Submenu> {
    let label = format!("{}. {}", index + 1, device_name);
    let submenu = Submenu::new(&label, true);

    let move_items: [(&str, bool, DeviceAction); 4] = [
        ("Move up", index > 0, DeviceAction::MovePriorityUp),
        (
            "Move down",
            index < list_len - 1,
            DeviceAction::MovePriorityDown,
        ),
        ("Move to top", index > 0, DeviceAction::MovePriorityToTop),
        (
            "Move to bottom",
            index < list_len - 1,
            DeviceAction::MovePriorityToBottom,
        ),
    ];
    for (i, (label, enabled, action)) in move_items.into_iter().enumerate() {
        if i == 2 {
            submenu.append(&PredefinedMenuItem::separator())?;
        }
        let item = MenuItem::new(label, enabled, None);
        register_menu_item(
            map,
            item.id().clone(),
            action,
            device_id,
            device_name,
            device_type,
        );
        submenu.append(&item)?;
    }
    submenu.append(&PredefinedMenuItem::separator())?;

    let remove_item = MenuItem::new("Remove device", true, None);
    register_menu_item(
        map,
        remove_item.id().clone(),
        DeviceAction::RemoveFromPriority,
        device_id,
        device_name,
        device_type,
    );
    submenu.append(&remove_item)?;

    Ok(submenu)
}

pub fn append_priority_list_to_menu(
    tray_menu: &Menu,
    device_type: DeviceType,
    backend: &impl AudioBackend,
    persistent_state: &PersistentState,
    temporary_priority: Option<&DeviceId>,
    map: &mut MenuIdMap,
) -> anyhow::Result<()> {
    let priority_list = persistent_state.priority_list(device_type);
    let priority_label = match device_type {
        DeviceType::Output => "Default output device priority",
        DeviceType::Input => "Default input device priority",
    };

    let priority_header = MenuItem::new(priority_label, false, None);
    tray_menu.append(&priority_header)?;

    let devices = backend.devices(device_type).unwrap_or_else(|e| {
        log::warn!("Failed to get {device_type:?} devices: {e:#}");
        Vec::new()
    });
    let mut available_devices = Vec::new();
    for device in devices {
        available_devices.push((device.id().clone(), device.name()));
    }

    for (index, device_id) in priority_list.iter().enumerate() {
        let device_name = lookup_device_name(device_id, persistent_state, backend);
        let submenu = build_priority_item_submenu(
            index,
            priority_list.len(),
            device_id,
            &device_name,
            device_type,
            map,
        )?;
        tray_menu.append(&submenu)?;
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

    let notify_on_restore = persistent_state.notify_on_priority_restore(device_type);

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

    let switch_communication = persistent_state.switch_communication_device(device_type);

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

pub fn append_temporary_priority_section(
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
        let devices = backend.devices(device_type).unwrap_or_else(|e| {
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
            format!("{label_prefix}: {device_name}")
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