use super::{DeviceDisplayInfo, VolumePercent, format_device_menu_label};

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
