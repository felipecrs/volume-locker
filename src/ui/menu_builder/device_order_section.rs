use super::{lookup_device_name, register_menu_item};
use crate::audio::AudioBackend;
use crate::config::PersistentState;
use crate::i18n::t;
use crate::types::{DeviceId, DeviceRole, DeviceType, OrderKind, TemporaryDefaults};
use crate::ui::{DeviceAction, MenuAction, MenuIdMap, MenuItemInfo, PreferenceAction};
use tray_icon::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};

fn current_default_device_name(
    backend: &impl AudioBackend,
    device_type: DeviceType,
    role: DeviceRole,
) -> String {
    backend
        .default_device(device_type, role)
        .map_or_else(|_| t!("no-current-device"), |d| d.name())
}

fn build_device_order_item_submenu(
    index: usize,
    list_len: usize,
    device_id: &DeviceId,
    device_name: &str,
    device_type: DeviceType,
    kind: OrderKind,
    map: &mut MenuIdMap,
) -> anyhow::Result<Submenu> {
    let label = format!("{}. {}", index + 1, device_name);
    let submenu = Submenu::new(&label, true);

    let move_items: [(String, bool, DeviceAction); 4] = [
        (t!("move-up"), index > 0, DeviceAction::MoveUp(kind)),
        (
            t!("move-down"),
            index < list_len - 1,
            DeviceAction::MoveDown(kind),
        ),
        (t!("move-to-top"), index > 0, DeviceAction::MoveToTop(kind)),
        (
            t!("move-to-bottom"),
            index < list_len - 1,
            DeviceAction::MoveToBottom(kind),
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

    let remove_item = MenuItem::new(t!("remove-device"), true, None);
    register_menu_item(
        map,
        remove_item.id().clone(),
        DeviceAction::RemoveFromOrder(kind),
        device_id,
        device_name,
        device_type,
    );
    submenu.append(&remove_item)?;

    Ok(submenu)
}

fn append_device_order_and_add_entries(
    submenu: &Submenu,
    device_type: DeviceType,
    kind: OrderKind,
    backend: &impl AudioBackend,
    persistent_state: &PersistentState,
    map: &mut MenuIdMap,
) -> anyhow::Result<()> {
    submenu.append(&MenuItem::new(t!("current-order"), false, None))?;

    let device_order = persistent_state.device_order_of(device_type, kind);
    if device_order.is_empty() {
        submenu.append(&MenuItem::new(t!("no-devices-added"), false, None))?;
    } else {
        for (index, device_id) in device_order.iter().enumerate() {
            let device_name = if device_id.is_follow_main() {
                match device_type {
                    DeviceType::Output => t!("follow-default-output-device"),
                    DeviceType::Input => t!("follow-default-input-device"),
                }
            } else {
                lookup_device_name(device_id, persistent_state, backend)
            };
            let item_submenu = build_device_order_item_submenu(
                index,
                device_order.len(),
                device_id,
                &device_name,
                device_type,
                kind,
                map,
            )?;
            submenu.append(&item_submenu)?;
        }
    }

    submenu.append(&MenuItem::new(t!("add-device"), false, None))?;

    let mut devices_to_add: Vec<(DeviceId, String)> = Vec::new();

    if kind == OrderKind::Communication && !device_order.iter().any(|id| id.is_follow_main()) {
        let label = match device_type {
            DeviceType::Output => t!("follow-default-output-device"),
            DeviceType::Input => t!("follow-default-input-device"),
        };
        devices_to_add.push((DeviceId::follow_main(), label));
    }

    let devices = backend.devices(device_type).unwrap_or_else(|e| {
        log::warn!("Failed to get {device_type:?} devices: {e:#}");
        Vec::new()
    });
    devices_to_add.extend(
        devices
            .iter()
            .filter(|d| !device_order.contains(d.id()))
            .map(|d| (d.id().clone(), d.name())),
    );

    if devices_to_add.is_empty() {
        submenu.append(&MenuItem::new(t!("no-devices-to-add"), false, None))?;
    } else {
        for (id, name) in &devices_to_add {
            let device_submenu = Submenu::new(name, true);

            let add_top = MenuItem::new(t!("add-to-top"), true, None);
            register_menu_item(
                map,
                add_top.id().clone(),
                DeviceAction::AddToTop(kind),
                id,
                name,
                device_type,
            );
            device_submenu.append(&add_top)?;

            let add_bottom = MenuItem::new(t!("add-to-bottom"), true, None);
            register_menu_item(
                map,
                add_bottom.id().clone(),
                DeviceAction::AddToBottom(kind),
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

fn build_device_order_submenu(
    device_type: DeviceType,
    kind: OrderKind,
    backend: &impl AudioBackend,
    persistent_state: &PersistentState,
    map: &mut MenuIdMap,
) -> anyhow::Result<Submenu> {
    let title = match kind {
        OrderKind::Main => t!("device-order"),
        OrderKind::Communication => t!("communication-device-order"),
    };
    let submenu = Submenu::new(&title, true);

    append_device_order_and_add_entries(
        &submenu,
        device_type,
        kind,
        backend,
        persistent_state,
        map,
    )?;

    submenu.append(&PredefinedMenuItem::separator())?;

    let notify = persistent_state.notify_on_switch_of(device_type, kind);
    let notify_item = CheckMenuItem::new(t!("notify-on-switch"), true, notify, None);
    let notify_name = match kind {
        OrderKind::Main => "Notify On Switch",
        OrderKind::Communication => "Communication Notify On Switch",
    };
    map.insert(
        notify_item.id().clone(),
        MenuItemInfo {
            name: notify_name.to_string(),
            action: MenuAction::Preference {
                device_type,
                action: PreferenceAction::NotifyOnSwitch(kind),
            },
        },
    );
    submenu.append(&notify_item)?;

    Ok(submenu)
}

pub fn append_device_order_to_menu(
    tray_menu: &Menu,
    device_type: DeviceType,
    backend: &impl AudioBackend,
    persistent_state: &PersistentState,
    map: &mut MenuIdMap,
) -> anyhow::Result<()> {
    let header_label = match device_type {
        DeviceType::Output => t!("default-output-device-order"),
        DeviceType::Input => t!("default-input-device-order"),
    };
    let header = MenuItem::new(header_label, false, None);
    tray_menu.append(&header)?;

    let main_submenu =
        build_device_order_submenu(device_type, OrderKind::Main, backend, persistent_state, map)?;
    tray_menu.append(&main_submenu)?;

    let current_name = current_default_device_name(backend, device_type, DeviceRole::Console);
    let current_info = MenuItem::new(
        t!("current-device", device = current_name.as_str()),
        false,
        None,
    );
    tray_menu.append(&current_info)?;

    let comm_submenu = build_device_order_submenu(
        device_type,
        OrderKind::Communication,
        backend,
        persistent_state,
        map,
    )?;
    tray_menu.append(&comm_submenu)?;

    let current_comm_name =
        current_default_device_name(backend, device_type, DeviceRole::Communications);
    let current_comm_info = MenuItem::new(
        t!("current-device", device = current_comm_name.as_str()),
        false,
        None,
    );
    tray_menu.append(&current_comm_info)?;

    tray_menu.append(&PredefinedMenuItem::separator())?;

    Ok(())
}

pub fn append_temporary_default_section(
    tray_menu: &Menu,
    backend: &impl AudioBackend,
    persistent_state: &PersistentState,
    temporary_defaults: &TemporaryDefaults,
    map: &mut MenuIdMap,
) -> anyhow::Result<()> {
    tray_menu.append(&MenuItem::new(t!("temporary-default-device"), false, None))?;

    let targets = [
        (DeviceType::Output, OrderKind::Main, t!("temporary-output")),
        (
            DeviceType::Output,
            OrderKind::Communication,
            t!("temporary-communication"),
        ),
        (DeviceType::Input, OrderKind::Main, t!("temporary-input")),
        (
            DeviceType::Input,
            OrderKind::Communication,
            t!("temporary-communication"),
        ),
    ];

    for (device_type, kind, label_prefix) in targets {
        let devices = backend.devices(device_type).unwrap_or_else(|e| {
            log::warn!("Failed to get {device_type:?} devices: {e:#}");
            Vec::new()
        });

        let temp_id_opt = temporary_defaults.get(device_type, kind);

        let submenu_label = if let Some(temp_id) = temp_id_opt {
            let device_name = lookup_device_name(temp_id, persistent_state, backend);
            t!(
                "temporary-with-device",
                target = label_prefix.as_str(),
                device = device_name.as_str()
            )
        } else {
            label_prefix
        };

        let submenu = Submenu::new(&submenu_label, true);

        if devices.is_empty() {
            submenu.append(&MenuItem::new(t!("no-devices-available"), false, None))?;
        } else {
            for device in &devices {
                let id = device.id();
                let name = device.name();
                let is_checked = temp_id_opt.is_some_and(|t| t == id);
                let item = CheckMenuItem::new(&name, true, is_checked, None);
                register_menu_item(
                    map,
                    item.id().clone(),
                    DeviceAction::SetTemporaryDefault(kind),
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
    use super::*;
    use crate::audio::tests::{MockAudioBackend, MockDevice};

    #[test]
    fn registers_add_device_actions() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("dev1", "Speakers", true),
            MockDevice::new("dev2", "Headphones", true),
        ]);
        let mut state = PersistentState::default();
        state.output.device_order = vec!["dev1".into()];

        let tray_menu = Menu::new();
        let mut map = MenuIdMap::new();

        append_device_order_to_menu(&tray_menu, DeviceType::Output, &backend, &state, &mut map)
            .expect("should succeed");

        let has_add_top = map.values().any(|info| {
            matches!(
                &info.action,
                MenuAction::Device {
                    action: DeviceAction::AddToTop(OrderKind::Main),
                    ..
                }
            )
        });
        assert!(has_add_top, "should have AddToTop action for dev2");

        let has_add_bottom = map.values().any(|info| {
            matches!(
                &info.action,
                MenuAction::Device {
                    action: DeviceAction::AddToBottom(OrderKind::Main),
                    ..
                }
            )
        });
        assert!(has_add_bottom, "should have AddToBottom action for dev2");
    }

    #[test]
    fn device_order_menus_structure_and_communication_follow_main() {
        for device_type in [DeviceType::Output, DeviceType::Input] {
            let mut device = MockDevice::new("dev1", "Headset", true);
            device.device_type = device_type;
            let backend = MockAudioBackend::new(vec![device]);

            let mut state = PersistentState::default();
            state.device_order_mut(device_type).push("dev1".into());

            let menu = Menu::new();
            append_device_order_to_menu(
                &menu,
                device_type,
                &backend,
                &state,
                &mut MenuIdMap::new(),
            )
            .unwrap();

            let items = menu.items();

            let header_title = match device_type {
                DeviceType::Output => "Default output device order",
                DeviceType::Input => "Default input device order",
            };
            assert!(
                items
                    .iter()
                    .any(|i| i.as_menuitem().is_some_and(|m| m.text() == header_title))
            );

            // Main device order submenu
            let main_sub = items
                .iter()
                .find_map(|i| {
                    i.as_submenu()
                        .filter(|s| s.text() == "Device order")
                        .cloned()
                })
                .expect("Device order submenu must be present");

            let main_items = main_sub.items();
            assert_eq!(main_items[0].as_menuitem().unwrap().text(), "Current order");
            assert_eq!(
                main_items
                    .last()
                    .unwrap()
                    .as_check_menuitem()
                    .unwrap()
                    .text(),
                "Notify on switch"
            );

            // Communication device order submenu
            let comm_sub = items
                .iter()
                .find_map(|i| {
                    i.as_submenu()
                        .filter(|s| s.text() == "Communication device order")
                        .cloned()
                })
                .expect("Communication device order submenu must be present");

            let comm_items = comm_sub.items();
            assert_eq!(comm_items[0].as_menuitem().unwrap().text(), "Current order");
            // Default communication device order contains the follow-main entry
            let main_order_title = match device_type {
                DeviceType::Output => "1. Follow default output device",
                DeviceType::Input => "1. Follow default input device",
            };
            assert!(
                comm_items
                    .iter()
                    .any(|i| { i.as_submenu().is_some_and(|s| s.text() == main_order_title) })
            );
            assert_eq!(
                comm_items
                    .last()
                    .unwrap()
                    .as_check_menuitem()
                    .unwrap()
                    .text(),
                "Notify on switch"
            );

            // Both "Current device:" entries are in the root menu
            let current_items: Vec<_> = items
                .iter()
                .filter_map(|i| {
                    i.as_menuitem()
                        .filter(|m| m.text().starts_with("Current device:"))
                        .map(|m| m.text())
                })
                .collect();
            assert_eq!(current_items.len(), 2);
        }
    }

    #[test]
    fn temporary_default_section_shows_chosen_devices() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("speakers", "Speakers", true),
            MockDevice::new("headset", "Headset", true),
        ]);
        let state = PersistentState::default();
        let mut temp = TemporaryDefaults::default();
        temp.set(DeviceType::Output, OrderKind::Main, Some("speakers".into()));
        temp.set(
            DeviceType::Output,
            OrderKind::Communication,
            Some("headset".into()),
        );

        let menu = Menu::new();
        let mut map = MenuIdMap::new();
        append_temporary_default_section(&menu, &backend, &state, &temp, &mut map).unwrap();

        let items = menu.items();
        assert_eq!(
            items[0].as_menuitem().unwrap().text(),
            "Temporary default device"
        );

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
