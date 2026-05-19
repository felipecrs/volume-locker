use super::{DeviceDisplayInfo, MenuIdMap, VolumePercent, format_device_menu_label};
use super::device_section::build_device_submenu;
use super::priority_section::append_priority_list_to_menu;
use crate::audio::AudioDevice;
use crate::audio::tests::{MockAudioBackend, MockDevice};
use crate::config::PersistentState;
use crate::types::DeviceType;
use crate::ui::DeviceAction;
use tray_icon::menu::Menu;

#[test]
fn to_label_basic() {
    let label = format_device_menu_label(&DeviceDisplayInfo {
        name: "Speakers",
        volume_percent: VolumePercent::from(50.0),
        is_default: false,
        is_locked: false,
        is_muted: false,
    });
    assert_eq!(label, "Speakers · 50%");
}

#[test]
fn to_label_default_device() {
    let label = format_device_menu_label(&DeviceDisplayInfo {
        name: "Speakers",
        volume_percent: VolumePercent::from(75.0),
        is_default: true,
        is_locked: false,
        is_muted: false,
    });
    assert_eq!(label, "Speakers · ☆ · 75%");
}

#[test]
fn to_label_locked() {
    let label = format_device_menu_label(&DeviceDisplayInfo {
        name: "Speakers",
        volume_percent: VolumePercent::from(100.0),
        is_default: false,
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
        is_default: false,
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
        is_default: true,
        is_locked: true,
        is_muted: true,
    });
    assert_eq!(label, "Headset · ☆ · 42% 🚫 · 🔒");
}

// ── device_section tests ──

#[test]
fn build_device_submenu_registers_lock_actions() {
    let device = MockDevice::new("dev1", "Speakers", true);
    let state = PersistentState::default();
    let mut map = MenuIdMap::new();

    let submenu = build_device_submenu(
        &device,
        DeviceType::Output,
        Some(device.id()),
        &state,
        &mut map,
    )
    .expect("build_device_submenu should succeed");

    // Should register 6 actions: VolumeLock, VolumeLockNotify, UnmuteLock, UnmuteLockNotify, OpenProperties, OpenSettings
    assert_eq!(map.len(), 6);
    assert!(submenu.text().contains("Speakers"));
}

#[test]
fn build_device_submenu_shows_default_indicator() {
    let device = MockDevice::new("dev1", "Speakers", true);
    let state = PersistentState::default();
    let mut map = MenuIdMap::new();

    let submenu = build_device_submenu(
        &device,
        DeviceType::Output,
        Some(device.id()),
        &state,
        &mut map,
    )
    .expect("should succeed");

    // Default device should have the star indicator
    assert!(submenu.text().contains("☆"));
}

#[test]
fn build_device_submenu_no_default_indicator_when_not_default() {
    let device = MockDevice::new("dev1", "Speakers", true);
    let state = PersistentState::default();
    let mut map = MenuIdMap::new();

    let submenu = build_device_submenu(
        &device,
        DeviceType::Output,
        None, // no default device
        &state,
        &mut map,
    )
    .expect("should succeed");

    assert!(!submenu.text().contains("☆"));
}

// ── priority_section tests ──

#[test]
fn append_priority_list_registers_add_device_actions() {
    let backend = MockAudioBackend::new(vec![
        MockDevice::new("dev1", "Speakers", true),
        MockDevice::new("dev2", "Headphones", true),
    ]);
    let mut state = PersistentState::default();
    // Only dev1 is in priority list, so dev2 should appear in "Add device"
    state.output.priority_list = vec!["dev1".into()];

    let tray_menu = Menu::new();
    let mut map = MenuIdMap::new();

    append_priority_list_to_menu(
        &tray_menu,
        DeviceType::Output,
        &backend,
        &state,
        None,
        &mut map,
    )
    .expect("should succeed");

    // Should have: priority item submenu entries (move up/down/top/bottom + remove = 5 for dev1)
    // + AddToPriority for dev2 (1)
    // + PriorityRestoreNotify (1) + SwitchCommunicationDevice (1) = at minimum 8
    assert!(map.len() >= 7);

    // Verify that "Add to priority" action exists for the non-priority device
    let has_add_action = map.values().any(|info| {
        matches!(
            &info.action,
            crate::ui::MenuAction::Device { action: DeviceAction::AddToPriority, .. }
        )
    });
    assert!(has_add_action, "should have AddToPriority action for dev2");
}

#[test]
fn append_priority_list_empty_priority() {
    let backend = MockAudioBackend::new(vec![MockDevice::new("dev1", "Speakers", true)]);
    let state = PersistentState::default();

    let tray_menu = Menu::new();
    let mut map = MenuIdMap::new();

    append_priority_list_to_menu(
        &tray_menu,
        DeviceType::Output,
        &backend,
        &state,
        None,
        &mut map,
    )
    .expect("should succeed");

    // With empty priority list: AddToPriority for dev1 (1) + notify item (1) + switch comm (1) = 3
    assert_eq!(map.len(), 3);
}
