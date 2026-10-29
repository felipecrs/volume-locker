use super::tests::{MockAudioBackend, MockDevice};
use super::{enforce_priorities, migrate_device_ids, presence::HeadsetPresence};
use crate::config::PersistentState;
use crate::notification::NotificationThrottler;
use crate::types::{DeviceType, PresenceSource, PriorityKind, TemporaryPriorities};

fn setup(device_type: DeviceType) -> (MockAudioBackend, PersistentState) {
    let mut headset = MockDevice::new("headset", "Wireless headset", true);
    headset.device_type = device_type;
    let mut fallback = MockDevice::new("fallback", "Wired device", true);
    fallback.device_type = device_type;
    let backend = MockAudioBackend::new(vec![headset, fallback]);
    let mut state = PersistentState::default();
    *state.priority_list_mut(device_type) = vec!["headset".into(), "fallback".into()];
    state
        .ensure_device_settings("headset".into(), "Wireless headset".into(), device_type)
        .presence_source = PresenceSource::AstroA50X;
    (backend, state)
}

#[test]
fn input_and_output_fall_back_on_power_off_and_restore_on_power_on() {
    for device_type in [DeviceType::Output, DeviceType::Input] {
        let (backend, state) = setup(device_type);
        let mut temp = TemporaryPriorities::default();
        temp.set(device_type, PriorityKind::Main, Some("headset".into()));
        for (presence, expected) in [
            (HeadsetPresence::Unknown, "headset"),
            (HeadsetPresence::Unavailable, "fallback"),
            (HeadsetPresence::Available, "headset"),
        ] {
            *backend.presence.borrow_mut() = presence;
            enforce_priorities(&backend, &state, &mut NotificationThrottler::new(), &temp);
            assert_eq!(backend.default_console.borrow()[&device_type], expected);
            assert_eq!(backend.default_multimedia.borrow()[&device_type], expected);
            assert_eq!(
                backend.default_communications.borrow()[&device_type],
                expected
            );
        }
    }
}

#[test]
fn g533_link_events_fall_back_when_wireless_link_is_lost() {
    for device_type in [DeviceType::Output, DeviceType::Input] {
        let (backend, mut state) = setup(device_type);
        state.devices.get_mut("headset").unwrap().presence_source = PresenceSource::LogitechG533;
        let temporary = TemporaryPriorities::default();

        for (presence, expected) in [
            (HeadsetPresence::Unavailable, "fallback"),
            (HeadsetPresence::Available, "headset"),
        ] {
            *backend.presence.borrow_mut() = presence;
            enforce_priorities(
                &backend,
                &state,
                &mut NotificationThrottler::new(),
                &temporary,
            );
            assert_eq!(backend.default_console.borrow()[&device_type], expected);
            assert_eq!(backend.default_multimedia.borrow()[&device_type], expected);
            assert_eq!(
                backend.default_communications.borrow()[&device_type],
                expected
            );
        }
    }
}

#[test]
fn g533_presence_source_survives_state_serialization() {
    let (_, mut state) = setup(DeviceType::Output);
    state.devices.get_mut("headset").unwrap().presence_source = PresenceSource::LogitechG533;

    let json = serde_json::to_string(&state).unwrap();
    let loaded: PersistentState = serde_json::from_str(&json).unwrap();
    assert_eq!(
        loaded.devices["headset"].presence_source,
        PresenceSource::LogitechG533
    );
}

#[test]
fn independent_communication_priorities_follow_presence_without_a_main_list() {
    for device_type in [DeviceType::Output, DeviceType::Input] {
        let (backend, mut state) = setup(device_type);
        *state.communication_priority_list_mut(device_type) =
            vec!["headset".into(), "fallback".into()];
        state.priority_list_mut(device_type).clear();
        backend.set_default("fallback", device_type);
        for (presence, expected) in [
            (HeadsetPresence::Unknown, "headset"),
            (HeadsetPresence::Unavailable, "fallback"),
            (HeadsetPresence::Available, "headset"),
        ] {
            *backend.presence.borrow_mut() = presence;
            enforce_priorities(
                &backend,
                &state,
                &mut NotificationThrottler::new(),
                &TemporaryPriorities::default(),
            );
            assert_eq!(backend.default_console.borrow()[&device_type], "fallback");
            assert_eq!(
                backend.default_multimedia.borrow()[&device_type],
                "fallback"
            );
            assert_eq!(
                backend.default_communications.borrow()[&device_type],
                expected
            );
        }
    }
}

#[test]
fn radio_state_requires_opt_in_and_cannot_revive_disabled_windows_devices() {
    let (mut backend, mut state) = setup(DeviceType::Output);
    state.devices.get_mut("headset").unwrap().presence_source = PresenceSource::Windows;
    *backend.presence.borrow_mut() = HeadsetPresence::Unavailable;
    enforce_priorities(
        &backend,
        &state,
        &mut NotificationThrottler::new(),
        &TemporaryPriorities::default(),
    );
    assert_eq!(
        backend.default_console.borrow()[&DeviceType::Output],
        "headset"
    );
    state.devices.get_mut("headset").unwrap().presence_source = PresenceSource::AstroA50X;
    backend.devices[0].active = false;
    *backend.presence.borrow_mut() = HeadsetPresence::Available;
    enforce_priorities(
        &backend,
        &state,
        &mut NotificationThrottler::new(),
        &TemporaryPriorities::default(),
    );
    assert_eq!(
        backend.default_console.borrow()[&DeviceType::Output],
        "fallback"
    );
}

#[test]
fn presence_settings_roundtrip_and_survive_endpoint_migration_and_cleanup() {
    let (_, state) = setup(DeviceType::Output);
    let json = serde_json::to_string(&state).unwrap();
    let mut loaded: PersistentState = serde_json::from_str(&json).unwrap();
    loaded.output.priority_list.clear();
    loaded.remove_device_if_unused(&"headset".into());
    assert_eq!(
        loaded.devices["headset"].presence_source,
        PresenceSource::AstroA50X
    );
    let backend = MockAudioBackend::new(vec![MockDevice::new(
        "new_headset",
        "Wireless headset",
        true,
    )]);
    let result = migrate_device_ids(
        &backend,
        &mut loaded,
        &mut TemporaryPriorities::default(),
        &std::collections::HashMap::new(),
    );
    assert!(result.persistent_changed);
    assert_eq!(
        loaded.devices["new_headset"].presence_source,
        PresenceSource::AstroA50X
    );
    loaded
        .devices
        .get_mut("new_headset")
        .unwrap()
        .presence_source = PresenceSource::Windows;
    loaded.remove_device_if_unused(&"new_headset".into());
    assert_eq!(loaded.device_count(), 0);
    let legacy: crate::types::DeviceSettings =
        serde_json::from_str(r#"{"device_type":"Output","name":"Speakers"}"#).unwrap();
    assert!(legacy.presence_source.is_windows());
    assert!(
        !serde_json::to_string(&legacy)
            .unwrap()
            .contains("presence_source")
    );
}
