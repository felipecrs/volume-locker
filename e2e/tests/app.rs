//! Runs the real app against fake USB devices. See `docs/e2e-tests.md`.
#![cfg(target_os = "windows")]

use serde_json::{Value, json};
use volume_locker_e2e::audio::{Endpoint, default_output_name};
use volume_locker_e2e::fixture::{App, Machine, plug_in, unplug, wait_until};
use volume_locker_e2e::tray::Menu;
use volume_locker_e2e::usbip::{self, HEADSET, SPEAKER, report_headset_presence};

const LOCKED_VOLUME: f32 = 0.4;

fn headset() -> Endpoint {
    Endpoint::find(HEADSET.product).expect("the headset is plugged in")
}

fn speaker() -> Endpoint {
    Endpoint::find(SPEAKER.product).expect("the speaker is plugged in")
}

fn volume_is(endpoint: &Endpoint, level: f32) -> bool {
    (endpoint.volume() - level).abs() < 0.01
}

fn default_is(device: usbip::Device) -> bool {
    default_output_name().is_some_and(|name| name.contains(device.product))
}

/// Settings with the headset's volume and unmute locked.
fn settings(priority: &[String], skip_disconnected_headsets: bool) -> Value {
    let mut devices = serde_json::Map::new();
    devices.insert(
        headset().id(),
        json!({
            "is_volume_locked": true,
            "volume_percent": LOCKED_VOLUME * 100.0,
            "is_unmute_locked": true,
            "device_type": "Output",
            "name": format!("Headset Earphone ({})", HEADSET.product),
        }),
    );
    json!({
        "devices": devices,
        "output_priority_list": priority,
        "check_updates_on_launch": false,
        "skip_disconnected_wireless_headsets": skip_disconnected_headsets,
    })
}

fn headset_first() -> Vec<String> {
    vec![headset().id(), speaker().id()]
}

#[test]
fn applies_saved_settings_on_start() {
    let _machine = Machine::acquire();
    // Prioritize whichever device is not the default, to prove the app changed it.
    let (target, other) = if default_is(HEADSET) {
        (SPEAKER, HEADSET)
    } else {
        (HEADSET, SPEAKER)
    };
    let priority = [target, other].map(|device| {
        Endpoint::find(device.product)
            .expect("the device is plugged in")
            .id()
    });
    headset().set_volume(0.25);

    let _app = App::start(
        "applies_saved_settings_on_start",
        &settings(&priority, false),
    );

    wait_until("the priority device to become the default", || {
        default_is(target)
    });
    wait_until("the locked volume to apply", || {
        volume_is(&headset(), LOCKED_VOLUME)
    });
}

#[test]
fn restores_a_locked_volume() {
    let _machine = Machine::acquire();
    let _app = App::start(
        "restores_a_locked_volume",
        &settings(&headset_first(), false),
    );
    wait_until("the locked volume to apply", || {
        volume_is(&headset(), LOCKED_VOLUME)
    });

    headset().set_volume(0.9);

    wait_until("the locked volume to return", || {
        volume_is(&headset(), LOCKED_VOLUME)
    });
}

#[test]
fn unmutes_a_locked_device() {
    let _machine = Machine::acquire();
    let _app = App::start(
        "unmutes_a_locked_device",
        &settings(&headset_first(), false),
    );
    wait_until("the headset to become the default", || default_is(HEADSET));

    headset().set_muted(true);

    wait_until("the headset to unmute", || !headset().is_muted());
}

#[test]
fn follows_priority_when_a_device_is_unplugged_and_returns() {
    let _machine = Machine::acquire();
    let _app = App::start(
        "follows_priority_when_a_device_is_unplugged_and_returns",
        &settings(&headset_first(), false),
    );
    wait_until("the headset to become the default", || default_is(HEADSET));

    unplug(HEADSET);
    wait_until("the speaker to take over", || default_is(SPEAKER));

    plug_in(HEADSET);
    wait_until("the headset to return", || default_is(HEADSET));
    wait_until("the locked volume to still apply", || {
        volume_is(&headset(), LOCKED_VOLUME)
    });
}

#[test]
fn skips_a_headset_that_reports_it_is_off() {
    let _machine = Machine::acquire();
    let _app = App::start(
        "skips_a_headset_that_reports_it_is_off",
        &settings(&headset_first(), true),
    );

    // Reports sent before the app starts listening are lost, so keep repeating them.
    wait_until("the speaker to take over", || {
        report_headset_presence(false);
        default_is(SPEAKER)
    });
    wait_until("the headset to return", || {
        report_headset_presence(true);
        default_is(HEADSET)
    });
}

#[test]
fn tray_menu_changes_a_preference() {
    let _machine = Machine::acquire();
    let app = App::start(
        "tray_menu_changes_a_preference",
        &settings(&headset_first(), false),
    );
    wait_until("the headset to become the default", || default_is(HEADSET));

    let menu = Menu::open();
    let names = menu.item_names();
    assert!(
        names.iter().any(|name| name.contains(HEADSET.product)),
        "the menu lists the headset: {names:?}"
    );
    menu.click("Skip disconnected");

    wait_until("the preference to be saved", || {
        app.saved_state()["skip_disconnected_wireless_headsets"] == json!(true)
    });
    wait_until("the new preference to apply", || {
        report_headset_presence(false);
        default_is(SPEAKER)
    });
}
