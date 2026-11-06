use super::{
    CheckMenuItem, DeviceAction, DeviceDisplayInfo, DeviceId, DeviceType, HashMap, Menu, MenuId,
    MenuIdMap, MenuItem, PersistentState, TrayMenuItems, UpdateInfo, VolumePercent,
    append_footer_section, append_preferences_section, format_device_menu_label,
    register_menu_item,
};
use crate::types::PriorityKind;

#[test]
fn registered_names_stay_english_for_follow_entry_only() {
    let mut map = MenuIdMap::new();
    let mut name_for = |id: &str, device_id: DeviceId, device_type| {
        let menu_id = MenuId::new(id);
        register_menu_item(
            &mut map,
            menu_id.clone(),
            DeviceAction::RemoveFromPriority(PriorityKind::Communication),
            &device_id,
            "Seguir",
            device_type,
        );
        map[&menu_id].name.clone()
    };

    assert_eq!(
        name_for("output", DeviceId::main_priority(), DeviceType::Output),
        "Follow output device priority"
    );
    assert_eq!(
        name_for("input", DeviceId::main_priority(), DeviceType::Input),
        "Follow input device priority"
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

#[test]
fn preferences_group_transfer_options_in_first_submenu() {
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

    append_preferences_section(
        &menu,
        false,
        &PersistentState::default(),
        &items,
        &mut HashMap::new(),
    )
    .unwrap();

    let top_level = menu.items();
    let submenu = top_level[1].as_submenu().unwrap();
    assert_eq!(submenu.text(), "Settings transfer");
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
            ("Automatically transfer device settings".to_string(), true),
            ("Also transfer device name and icon".to_string(), true),
            ("Notify on settings transfer".to_string(), false),
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
