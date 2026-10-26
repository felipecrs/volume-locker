use super::{lookup_device_name, register_menu_item};
use crate::audio::AudioBackend;
use crate::config::PersistentState;
use crate::types::{DeviceId, DeviceRole, DeviceType, PriorityKind, TemporaryPriorities};
use crate::ui::{DeviceAction, MenuAction, MenuIdMap, MenuItemInfo, PreferenceAction};
use tray_icon::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};

fn current_default_device_name(
    backend: &impl AudioBackend,
    device_type: DeviceType,
    role: DeviceRole,
) -> String {
    backend
        .default_device(device_type, role)
        .map_or_else(|_| "None".to_string(), |d| d.name())
}

fn build_priority_item_submenu(
    index: usize,
    list_len: usize,
    device_id: &DeviceId,
    device_name: &str,
    device_type: DeviceType,
    kind: PriorityKind,
    map: &mut MenuIdMap,
) -> anyhow::Result<Submenu> {
    let label = format!("{}. {}", index + 1, device_name);
    let submenu = Submenu::new(&label, true);

    let move_items: [(&str, bool, DeviceAction); 4] = [
        ("Move up", index > 0, DeviceAction::MovePriorityUp(kind)),
        (
            "Move down",
            index < list_len - 1,
            DeviceAction::MovePriorityDown(kind),
        ),
        (
            "Move to top",
            index > 0,
            DeviceAction::MovePriorityToTop(kind),
        ),
        (
            "Move to bottom",
            index < list_len - 1,
            DeviceAction::MovePriorityToBottom(kind),
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
        DeviceAction::RemoveFromPriority(kind),
        device_id,
        device_name,
        device_type,
    );
    submenu.append(&remove_item)?;

    Ok(submenu)
}

fn append_priority_list_and_add_entries(
    submenu: &Submenu,
    device_type: DeviceType,
    kind: PriorityKind,
    backend: &impl AudioBackend,
    persistent_state: &PersistentState,
    map: &mut MenuIdMap,
) -> anyhow::Result<()> {
    submenu.append(&MenuItem::new("Priority list", false, None))?;

    let priority_list = persistent_state.priority_list_of(device_type, kind);
    if priority_list.is_empty() {
        submenu.append(&MenuItem::new("No devices configured", false, None))?;
    } else {
        for (index, device_id) in priority_list.iter().enumerate() {
            let device_name = if device_id.is_main_priority() {
                match device_type {
                    DeviceType::Output => "Output device priority".to_string(),
                    DeviceType::Input => "Input device priority".to_string(),
                }
            } else {
                lookup_device_name(device_id, persistent_state, backend)
            };
            let item_submenu = build_priority_item_submenu(
                index,
                priority_list.len(),
                device_id,
                &device_name,
                device_type,
                kind,
                map,
            )?;
            submenu.append(&item_submenu)?;
        }
    }

    submenu.append(&MenuItem::new("Add device", false, None))?;

    let mut devices_to_add: Vec<(DeviceId, String)> = Vec::new();

    if kind == PriorityKind::Communication && !priority_list.iter().any(|id| id.is_main_priority())
    {
        let label = match device_type {
            DeviceType::Output => "Output device priority".to_string(),
            DeviceType::Input => "Input device priority".to_string(),
        };
        devices_to_add.push((DeviceId::main_priority(), label));
    }

    let devices = backend.devices(device_type).unwrap_or_else(|e| {
        log::warn!("Failed to get {device_type:?} devices: {e:#}");
        Vec::new()
    });
    devices_to_add.extend(
        devices
            .iter()
            .filter(|d| !priority_list.contains(d.id()))
            .map(|d| (d.id().clone(), d.name())),
    );

    if devices_to_add.is_empty() {
        submenu.append(&MenuItem::new("No devices available", false, None))?;
    } else {
        for (id, name) in &devices_to_add {
            let device_submenu = Submenu::new(name, true);

            let add_top = MenuItem::new("Add to top", true, None);
            register_menu_item(
                map,
                add_top.id().clone(),
                DeviceAction::AddToPriorityTop(kind),
                id,
                name,
                device_type,
            );
            device_submenu.append(&add_top)?;

            let add_bottom = MenuItem::new("Add to bottom", true, None);
            register_menu_item(
                map,
                add_bottom.id().clone(),
                DeviceAction::AddToPriorityBottom(kind),
                id,
                name,
                device_type,
            );
            device_submenu.append(&add_bottom)?;

            submenu.append(&device_submenu)?;
        }
    }

    Ok(())
}

fn build_priority_submenu(
    device_type: DeviceType,
    kind: PriorityKind,
    backend: &impl AudioBackend,
    persistent_state: &PersistentState,
    map: &mut MenuIdMap,
) -> anyhow::Result<Submenu> {
    let title = match kind {
        PriorityKind::Main => "Device priority settings",
        PriorityKind::Communication => "Communication priority settings",
    };
    let submenu = Submenu::new(title, true);

    append_priority_list_and_add_entries(
        &submenu,
        device_type,
        kind,
        backend,
        persistent_state,
        map,
    )?;

    submenu.append(&PredefinedMenuItem::separator())?;

    let notify = persistent_state.notify_on_priority_restore_of(device_type, kind);
    let notify_item = CheckMenuItem::new("Notify on restore", true, notify, None);
    let notify_name = match kind {
        PriorityKind::Main => "Priority Restore Notify",
        PriorityKind::Communication => "Communication Priority Restore Notify",
    };
    map.insert(
        notify_item.id().clone(),
        MenuItemInfo {
            name: notify_name.to_string(),
            action: MenuAction::Preference {
                device_type,
                action: PreferenceAction::PriorityRestoreNotify(kind),
            },
        },
    );
    submenu.append(&notify_item)?;

    Ok(submenu)
}

pub fn append_priority_list_to_menu(
    tray_menu: &Menu,
    device_type: DeviceType,
    backend: &impl AudioBackend,
    persistent_state: &PersistentState,
    map: &mut MenuIdMap,
) -> anyhow::Result<()> {
    let header_label = match device_type {
        DeviceType::Output => "Output device priority",
        DeviceType::Input => "Input device priority",
    };
    let header = MenuItem::new(header_label, false, None);
    tray_menu.append(&header)?;

    let main_submenu = build_priority_submenu(
        device_type,
        PriorityKind::Main,
        backend,
        persistent_state,
        map,
    )?;
    tray_menu.append(&main_submenu)?;

    let current_name = current_default_device_name(backend, device_type, DeviceRole::Console);
    let current_info = MenuItem::new(format!("Current: {current_name}"), false, None);
    tray_menu.append(&current_info)?;

    let comm_submenu = build_priority_submenu(
        device_type,
        PriorityKind::Communication,
        backend,
        persistent_state,
        map,
    )?;
    tray_menu.append(&comm_submenu)?;

    let current_comm_name =
        current_default_device_name(backend, device_type, DeviceRole::Communications);
    let current_comm_info = MenuItem::new(format!("Current: {current_comm_name}"), false, None);
    tray_menu.append(&current_comm_info)?;

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
    tray_menu.append(&MenuItem::new("Temporary priority", false, None))?;

    let targets = [
        (DeviceType::Output, PriorityKind::Main, "Output"),
        (
            DeviceType::Output,
            PriorityKind::Communication,
            "Communication",
        ),
        (DeviceType::Input, PriorityKind::Main, "Input"),
        (
            DeviceType::Input,
            PriorityKind::Communication,
            "Communication",
        ),
    ];

    for (device_type, kind, label_prefix) in targets {
        let devices = backend.devices(device_type).unwrap_or_else(|e| {
            log::warn!("Failed to get {device_type:?} devices: {e:#}");
            Vec::new()
        });

        let temp_id_opt = temporary_priorities.get(device_type, kind);

        let submenu_label = if let Some(temp_id) = temp_id_opt {
            let device_name = lookup_device_name(temp_id, persistent_state, backend);
            format!("{label_prefix}: {device_name}")
        } else {
            label_prefix.to_string()
        };

        let submenu = Submenu::new(&submenu_label, true);

        if devices.is_empty() {
            submenu.append(&MenuItem::new("No devices available", false, None))?;
        } else {
            for device in &devices {
                let id = device.id();
                let name = device.name();
                let is_checked = temp_id_opt.is_some_and(|t| t == id);
                let item = CheckMenuItem::new(&name, true, is_checked, None);
                register_menu_item(
                    map,
                    item.id().clone(),
                    DeviceAction::SetTemporaryPriority(kind),
                    id,
                    &name,
                    device_type,
                );
                submenu.append(&item)?;
            }
        }
        tray_menu.append(&submenu)?;
    }
    tray_menu.append(&PredefinedMenuItem::separator())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use crate::audio::tests::{MockAudioBackend, MockDevice};

    #[test]
    fn registers_add_device_actions() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("dev1", "Speakers", true),
            MockDevice::new("dev2", "Headphones", true),
        ]);
        let mut state = PersistentState::default();
        state.output.priority_list = vec!["dev1".into()];

        let tray_menu = Menu::new();
        let mut map = MenuIdMap::new();

        append_priority_list_to_menu(&tray_menu, DeviceType::Output, &backend, &state, &mut map)
            .expect("should succeed");

        let has_add_top = map.values().any(|info| {
            matches!(
                &info.action,
                MenuAction::Device {
                    action: DeviceAction::AddToPriorityTop(PriorityKind::Main),
                    ..
                }
            )
        });
        assert!(has_add_top, "should have AddToPriorityTop action for dev2");

        let has_add_bottom = map.values().any(|info| {
            matches!(
                &info.action,
                MenuAction::Device {
                    action: DeviceAction::AddToPriorityBottom(PriorityKind::Main),
                    ..
                }
            )
        });
        assert!(
            has_add_bottom,
            "should have AddToPriorityBottom action for dev2"
        );
    }

    #[test]
    fn priority_menus_structure_and_communication_main_priority() {
        for device_type in [DeviceType::Output, DeviceType::Input] {
            let mut device = MockDevice::new("dev1", "Headset", true);
            device.device_type = device_type;
            let backend = MockAudioBackend::new(vec![device]);

            let mut state = PersistentState::default();
            state.priority_list_mut(device_type).push("dev1".into());

            let menu = Menu::new();
            append_priority_list_to_menu(
                &menu,
                device_type,
                &backend,
                &state,
                &mut MenuIdMap::new(),
            )
            .unwrap();

            let items = menu.items();

            let header_title = match device_type {
                DeviceType::Output => "Output device priority",
                DeviceType::Input => "Input device priority",
            };
            assert!(
                items
                    .iter()
                    .any(|i| i.as_menuitem().is_some_and(|m| m.text() == header_title))
            );

            // Main priority submenu
            let main_sub = items
                .iter()
                .find_map(|i| {
                    i.as_submenu()
                        .filter(|s| s.text() == "Device priority settings")
                        .cloned()
                })
                .expect("Device priority settings submenu must be present");

            let main_items = main_sub.items();
            assert_eq!(main_items[0].as_menuitem().unwrap().text(), "Priority list");
            assert_eq!(
                main_items
                    .last()
                    .unwrap()
                    .as_check_menuitem()
                    .unwrap()
                    .text(),
                "Notify on restore"
            );

            // Communication priority submenu
            let comm_sub = items
                .iter()
                .find_map(|i| {
                    i.as_submenu()
                        .filter(|s| s.text() == "Communication priority settings")
                        .cloned()
                })
                .expect("Communication priority settings submenu must be present");

            let comm_items = comm_sub.items();
            assert_eq!(comm_items[0].as_menuitem().unwrap().text(), "Priority list");
            // Default communication priority list contains main priority entry
            let main_priority_title = match device_type {
                DeviceType::Output => "1. Output device priority",
                DeviceType::Input => "1. Input device priority",
            };
            assert!(comm_items.iter().any(|i| {
                i.as_submenu()
                    .is_some_and(|s| s.text() == main_priority_title)
            }));
            assert_eq!(
                comm_items
                    .last()
                    .unwrap()
                    .as_check_menuitem()
                    .unwrap()
                    .text(),
                "Notify on restore"
            );

            // Both Current: entries are in the root menu
            let current_items: Vec<_> = items
                .iter()
                .filter_map(|i| {
                    i.as_menuitem()
                        .filter(|m| m.text().starts_with("Current:"))
                        .map(|m| m.text())
                })
                .collect();
            assert_eq!(current_items.len(), 2);
        }
    }

    #[test]
    fn temporary_priority_section_shows_chosen_devices() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("speakers", "Speakers", true),
            MockDevice::new("headset", "Headset", true),
        ]);
        let state = PersistentState::default();
        let mut temp = TemporaryPriorities::default();
        temp.set(
            DeviceType::Output,
            PriorityKind::Main,
            Some("speakers".into()),
        );
        temp.set(
            DeviceType::Output,
            PriorityKind::Communication,
            Some("headset".into()),
        );

        let menu = Menu::new();
        let mut map = MenuIdMap::new();
        append_temporary_priority_section(&menu, &backend, &state, &temp, &mut map).unwrap();

        let items = menu.items();
        assert_eq!(items[0].as_menuitem().unwrap().text(), "Temporary priority");

        assert_eq!(items[1].as_submenu().unwrap().text(), "Output: Speakers");
        assert_eq!(
            items[2].as_submenu().unwrap().text(),
            "Communication: Headset"
        );
        assert_eq!(items[3].as_submenu().unwrap().text(), "Input");
        assert_eq!(items[4].as_submenu().unwrap().text(), "Communication");

        // Submenu items contain the devices
        let sub_items = items[1].as_submenu().unwrap().items();
        assert_eq!(sub_items.len(), 2);
        assert!(sub_items[0].as_check_menuitem().unwrap().is_checked()); // speakers is checked
        assert!(!sub_items[1].as_check_menuitem().unwrap().is_checked()); // headset is not checked
    }
}
