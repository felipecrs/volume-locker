use super::tests::{MockAudioBackend, MockDevice};
use super::{enforce_priorities, presence::HeadsetPresence};
use crate::config::PersistentState;
use crate::notification::NotificationThrottler;
use crate::types::{DeviceFingerprint, DeviceType, PriorityKind, TemporaryPriorities};

fn setup(device_type: DeviceType) -> (MockAudioBackend, PersistentState) {
    let mut headset =
        MockDevice::new("headset", "Wireless headset", true).with_fingerprint(DeviceFingerprint {
            usb_vid_pid: Some((0x046d, 0x0b0b)),
            ..DeviceFingerprint::default()
        });
    headset.device_type = device_type;
    let mut fallback = MockDevice::new("fallback", "Wired device", true);
    fallback.device_type = device_type;
    let backend = MockAudioBackend::new(vec![headset, fallback]);
    let mut state = PersistentState::default();
    *state.priority_list_mut(device_type) = vec!["headset".into(), "fallback".into()];
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
fn temporary_communication_priority_skips_an_unavailable_headset() {
    let (backend, mut state) = setup(DeviceType::Output);
    state.output.communication_priority_list = vec!["fallback".into()];
    *backend.presence.borrow_mut() = HeadsetPresence::Unavailable;
    let mut temporary = TemporaryPriorities::default();
    temporary.set(
        DeviceType::Output,
        PriorityKind::Communication,
        Some("headset".into()),
    );
    enforce_priorities(
        &backend,
        &state,
        &mut NotificationThrottler::new(),
        &temporary,
    );
    assert_eq!(
        backend.default_communications.borrow()[&DeviceType::Output],
        "fallback"
    );
}

#[test]
fn presence_is_ignored_for_an_endpoint_without_a_usb_identity() {
    let (mut backend, state) = setup(DeviceType::Output);
    *backend.presence.borrow_mut() = HeadsetPresence::Unavailable;
    backend.devices[0].fingerprint = None;
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
fn radio_state_requires_the_setting_and_cannot_revive_disabled_windows_devices() {
    let (mut backend, mut state) = setup(DeviceType::Output);
    state.skip_disconnected_wireless_headsets = false;
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
    state.skip_disconnected_wireless_headsets = true;
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
fn turning_detection_off_survives_serialization_without_per_device_settings() {
    let (_, mut state) = setup(DeviceType::Output);
    state.skip_disconnected_wireless_headsets = false;
    let json = serde_json::to_string(&state).unwrap();
    let loaded: PersistentState = serde_json::from_str(&json).unwrap();
    assert!(!loaded.skip_disconnected_wireless_headsets);
    assert_eq!(loaded.device_count(), 0);
}
