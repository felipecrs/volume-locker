use super::{
    CheckMenuItem, DeviceAction, DeviceDisplayInfo, DeviceId, DeviceType, HashMap, Menu, MenuId,
    MenuIdMap, MenuItem, PersistentState, TrayMenuItems, UpdateInfo, VolumePercent,
    append_footer_section, append_preferences_section, format_device_menu_label,
    register_menu_item,
};
use crate::types::OrderKind;

#[test]
fn registered_names_stay_english_for_follow_entry_only() {
    let mut map = MenuIdMap::new();
    let mut name_for = |id: &str, device_id: DeviceId, device_type| {
        let menu_id = MenuId::new(id);
        register_menu_item(
            &mut map,
            menu_id.clone(),
            DeviceAction::RemoveFromOrder(OrderKind::Communication),
            &device_id,
            "Seguir",
            device_type,
        );
        map[&menu_id].name.clone()
    };

    assert_eq!(
        name_for("output", DeviceId::follow_main(), DeviceType::Output),
        "Follow default output device"
    );
    assert_eq!(
        name_for("input", DeviceId::follow_main(), DeviceType::Input),
        "Follow default input device"
    );
    assert_eq!(
        name_for("device", "dev1".into(), DeviceType::Output),
        "Seguir"
    );
}

fn footer_update_item(update_download_in_progress: bool) -> (String, bool, usize) {
    let menu = Menu::new();
    let auto_launch = CheckMenuItem::new("Auto-launch on startup", true, false, None);
    let check_updates = CheckMenuItem::new("Check for updates on launch", true, false, None);
    let quit = MenuItem::new("Quit", true, None);
    let output_heading = MenuItem::new("Output devices", false, None);
    let input_heading = MenuItem::new("Input devices", false, None);
    let items = TrayMenuItems {
        auto_launch_check: &auto_launch,
        check_updates_on_launch: &check_updates,
        quit: &quit,
        output_devices_heading: &output_heading,
        input_devices_heading: &input_heading,
    };
    let update_info = UpdateInfo {
        latest_version: "1.2.3".to_string(),
        download_url: String::new(),
        release_url: String::new(),
    };
    let mut map = HashMap::new();

    append_footer_section(
        &menu,
        &mut map,
        Some(&update_info),
        update_download_in_progress,
        &items,
    )
    .unwrap();

    let top_level = menu.items();
    let item = top_level[top_level.len() - 3].as_menuitem().unwrap();
    (item.text(), item.is_enabled(), map.len())
}

#[test]
fn footer_offers_available_update() {
    let (label, enabled, actions) = footer_update_item(false);
    assert_eq!(label, "Update to 1.2.3...");
    assert!(enabled);
    assert_eq!(actions, 3);
}

#[test]
fn footer_greys_out_update_while_downloading() {
    let (label, enabled, actions) = footer_update_item(true);
    assert_eq!(label, "Downloading update...");
    assert!(!enabled);
    assert_eq!(actions, 2);
}

#[test]
fn to_label_basic() {
    let label = format_device_menu_label(&DeviceDisplayInfo {
        name: "Speakers",
        volume_percent: VolumePercent::from(50.0),
        is_locked: false,
        is_muted: false,
    });
    assert_eq!(label, "Speakers · 50%");
}

#[test]
fn to_label_locked() {
    let label = format_device_menu_label(&DeviceDisplayInfo {
        name: "Speakers",
        volume_percent: VolumePercent::from(100.0),
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
        is_locked: true,
        is_muted: true,
    });
    assert_eq!(label, "Headset · 42% 🚫 · 🔒");
}

fn preferences_menu(state: &PersistentState) -> Menu {
    let menu = Menu::new();
    let auto_launch = CheckMenuItem::new("Auto-launch on startup", true, false, None);
    let check_updates = CheckMenuItem::new("Check for updates on launch", true, false, None);
    let quit = MenuItem::new("Quit", true, None);
    let output_heading = MenuItem::new("Output devices", false, None);
    let input_heading = MenuItem::new("Input devices", false, None);
    let items = TrayMenuItems {
        auto_launch_check: &auto_launch,
        check_updates_on_launch: &check_updates,
        quit: &quit,
        output_devices_heading: &output_heading,
        input_devices_heading: &input_heading,
    };

    append_preferences_section(&menu, false, state, &items, &mut HashMap::new()).unwrap();
    menu
}

#[test]
fn preferences_group_seen_as_new_options_in_first_submenu() {
    let menu = preferences_menu(&PersistentState::default());
    let top_level = menu.items();
    let submenu = top_level[1].as_submenu().unwrap();
    assert_eq!(submenu.text(), "Devices seen as new");
    let options: Vec<_> = submenu
        .items()
        .iter()
        .map(|item| {
            let check = item.as_check_menuitem().unwrap().clone();
            (check.text(), check.is_checked())
        })
        .collect();
    assert_eq!(
        options,
        [
            ("Keep their settings".to_string(), true),
            ("Keep their name and icon".to_string(), true),
            ("Notify when it happens".to_string(), false),
        ]
    );
    let rest: Vec<_> = top_level[2..5]
        .iter()
        .map(|item| item.as_check_menuitem().unwrap().text())
        .collect();
    assert_eq!(
        rest,
        [
            "Skip disconnected wireless headsets",
            "Auto-launch on startup",
            "Check for updates on launch",
        ]
    );
}

#[test]
fn seen_as_new_options_are_disabled_while_settings_are_not_kept() {
    let enabled = |keep_settings| {
        let state = PersistentState {
            keep_settings_when_seen_as_new: keep_settings,
            ..PersistentState::default()
        };
        let menu = preferences_menu(&state);
        let submenu = menu.items()[1].as_submenu().unwrap().clone();
        submenu
            .items()
            .iter()
            .map(|item| item.as_check_menuitem().unwrap().is_enabled())
            .collect::<Vec<_>>()
    };
    assert_eq!(enabled(true), [true, true, true]);
    assert_eq!(enabled(false), [true, false, false]);
}
