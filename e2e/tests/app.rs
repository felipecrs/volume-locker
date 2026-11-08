//! Runs the real app against fake USB devices. See `docs/e2e-tests.md`.
#![cfg(target_os = "windows")]

use serde_json::{Value, json};
use volume_locker_e2e::audio::{Endpoint, Flow, default_name};
use volume_locker_e2e::fixture::{App, Machine, plug_in, unplug, wait_until};
use volume_locker_e2e::tray::Menu;
use volume_locker_e2e::usbip::{Device, HEADSET, MICROPHONE, SPEAKER, report_headset_presence};

const LOCKED_VOLUME: f32 = 0.4;
const FLOWS: [Flow; 2] = [Flow::Output, Flow::Input];

/// The device each direction falls back to when the headset is gone or skipped.
fn fallback(flow: Flow) -> Device {
    match flow {
        Flow::Output => SPEAKER,
        Flow::Input => MICROPHONE,
    }
}

fn endpoint(flow: Flow, device: Device) -> Endpoint {
    Endpoint::find(flow, device.product)
        .unwrap_or_else(|| panic!("{} has a {flow:?} endpoint", device.product))
}

fn volume_is(flow: Flow, device: Device, level: f32) -> bool {
    (endpoint(flow, device).volume() - level).abs() < 0.01
}

fn default_is(flow: Flow, device: Device) -> bool {
    default_name(flow).is_some_and(|name| name.contains(device.product))
}

/// Lists `first` before the other device for that direction.
fn priority(flow: Flow, first: Device) -> Vec<String> {
    let second = if first.product == HEADSET.product {
        fallback(flow)
    } else {
        HEADSET
    };
    vec![endpoint(flow, first).id(), endpoint(flow, second).id()]
}

/// Settings with the headset's volume and unmute locked in both directions.
fn settings(output_priority: &[String], input_priority: &[String], skip: bool) -> Value {
    let devices: serde_json::Map<String, Value> = FLOWS
        .into_iter()
        .map(|flow| {
            let headset = endpoint(flow, HEADSET);
            let settings = json!({
                "is_volume_locked": true,
                "volume_percent": LOCKED_VOLUME * 100.0,
                "is_unmute_locked": true,
                "device_type": format!("{flow:?}"),
                "name": headset.name(),
            });
            (headset.id(), settings)
        })
        .collect();
    json!({
        "devices": devices,
        "output_priority_list": output_priority,
        "input_priority_list": input_priority,
        "check_updates_on_launch": false,
        "skip_disconnected_wireless_headsets": skip,
    })
}

fn headset_first(skip: bool) -> Value {
    settings(
        &priority(Flow::Output, HEADSET),
        &priority(Flow::Input, HEADSET),
        skip,
    )
}

#[track_caller]
fn wait_for_default(flow: Flow, device: Device) {
    wait_until(
        &format!("{} to become the {flow:?} default", device.product),
        || default_is(flow, device),
    );
}

#[track_caller]
fn wait_for_locked_volume(flow: Flow) {
    wait_until(&format!("the locked {flow:?} volume to apply"), || {
        volume_is(flow, HEADSET, LOCKED_VOLUME)
    });
}

#[test]
fn applies_saved_settings_on_start() {
    let _machine = Machine::acquire();
    // Prioritize whichever device is not the default, to prove the app changed it.
    let targets = FLOWS.map(|flow| {
        if default_is(flow, HEADSET) {
            fallback(flow)
        } else {
            HEADSET
        }
    });
    for flow in FLOWS {
        endpoint(flow, HEADSET).set_volume(0.25);
    }

    let _app = App::start(
        "applies_saved_settings_on_start",
        &settings(
            &priority(Flow::Output, targets[0]),
            &priority(Flow::Input, targets[1]),
            false,
        ),
    );

    for (flow, target) in FLOWS.into_iter().zip(targets) {
        wait_for_default(flow, target);
        wait_for_locked_volume(flow);
    }
}

#[test]
fn restores_a_locked_volume() {
    let _machine = Machine::acquire();
    let _app = App::start("restores_a_locked_volume", &headset_first(false));

    for flow in FLOWS {
        wait_for_locked_volume(flow);
        endpoint(flow, HEADSET).set_volume(0.9);
        wait_for_locked_volume(flow);
    }
}

#[test]
fn unmutes_a_locked_device() {
    let _machine = Machine::acquire();
    let _app = App::start("unmutes_a_locked_device", &headset_first(false));

    for flow in FLOWS {
        wait_for_default(flow, HEADSET);
        endpoint(flow, HEADSET).set_muted(true);
        wait_until(&format!("the {flow:?} headset to unmute"), || {
            !endpoint(flow, HEADSET).is_muted()
        });
    }
}

#[test]
fn follows_priority_when_a_device_is_unplugged_and_returns() {
    let _machine = Machine::acquire();
    let _app = App::start(
        "follows_priority_when_a_device_is_unplugged_and_returns",
        &headset_first(false),
    );
    for flow in FLOWS {
        wait_for_default(flow, HEADSET);
    }

    unplug(HEADSET);
    for flow in FLOWS {
        wait_for_default(flow, fallback(flow));
    }

    plug_in(HEADSET);
    for flow in FLOWS {
        wait_for_default(flow, HEADSET);
        wait_for_locked_volume(flow);
    }
}

#[test]
fn skips_a_headset_that_reports_it_is_off() {
    let _machine = Machine::acquire();
    let _app = App::start(
        "skips_a_headset_that_reports_it_is_off",
        &headset_first(true),
    );

    // Reports sent before the app starts listening are lost, so keep repeating them.
    wait_until("both directions to fall back", || {
        report_headset_presence(false);
        FLOWS
            .into_iter()
            .all(|flow| default_is(flow, fallback(flow)))
    });
    wait_until("the headset to return in both directions", || {
        report_headset_presence(true);
        FLOWS.into_iter().all(|flow| default_is(flow, HEADSET))
    });
}

#[test]
fn tray_menu_changes_a_preference() {
    let _machine = Machine::acquire();
    let app = App::start("tray_menu_changes_a_preference", &headset_first(false));
    wait_for_default(Flow::Output, HEADSET);

    let menu = Menu::open();
    let names = menu.item_names();
    for flow in FLOWS {
        let name = endpoint(flow, HEADSET).name();
        assert!(
            names.iter().any(|item| item.contains(&name)),
            "the menu lists {name}: {names:?}"
        );
    }
    menu.click("Skip disconnected");

    wait_until("the preference to be saved", || {
        app.saved_state()["skip_disconnected_wireless_headsets"] == json!(true)
    });
    wait_until("the new preference to apply", || {
        report_headset_presence(false);
        default_is(Flow::Output, SPEAKER)
    });
}
