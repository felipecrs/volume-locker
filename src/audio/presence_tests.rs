use super::tests::{MockAudioBackend, MockDevice};
use super::{enforce_priorities, presence::HeadsetPresence};
use crate::config::PersistentState;
use crate::notification::NotificationThrottler;
use crate::types::{DeviceFingerprint, DeviceType, PriorityKind, TemporaryPriorities};

fn setup(device_type: DeviceType) -> (MockAudioBackend, PersistentState) {
    let mut headset = MockDevice::new("headset", "Wireless headset", true);
    headset.device_type = device_type;
    headset.fingerprint = Some(DeviceFingerprint {
        usb_vid_pid: Some((0x046d, 0x0b0b)),
        ..DeviceFingerprint::default()
    });
    let mut fallback = MockDevice::new("fallback", "Wired device", true);
    fallback.device_type = device_type;
    let backend = MockAudioBackend::new(vec![headset, fallback]);
    let mut state = PersistentState::default();
    *state.priority_list_mut(device_type) = vec!["headset".into(), "fallback".into()];
    state.wireless_headset_detection_enabled = true;
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
        let (mut backend, state) = setup(device_type);
        backend.devices[0].fingerprint = Some(DeviceFingerprint {
            usb_vid_pid: Some((0x046d, 0x0a66)),
            ..DeviceFingerprint::default()
        });
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
fn global_detection_setting_survives_state_serialization() {
    let (_, state) = setup(DeviceType::Output);

    let json = serde_json::to_string(&state).unwrap();
    let loaded: PersistentState = serde_json::from_str(&json).unwrap();
    assert!(loaded.wireless_headset_detection_enabled);
}

#[test]
fn presence_is_applied_only_to_an_endpoint_with_a_supported_vid_pid() {
    let (mut backend, state) = setup(DeviceType::Output);
    *backend.presence.borrow_mut() = HeadsetPresence::Unavailable;
    backend.devices[0].fingerprint = Some(DeviceFingerprint {
        usb_vid_pid: Some((0x1234, 0x5678)),
        ..DeviceFingerprint::default()
    });
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

    backend.devices[0].fingerprint = Some(DeviceFingerprint {
        usb_vid_pid: Some((0x046d, 0x0a66)),
        ..DeviceFingerprint::default()
    });
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
    state.wireless_headset_detection_enabled = false;
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
    state.wireless_headset_detection_enabled = true;
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
fn global_detection_setting_does_not_create_per_device_settings() {
    let (_, state) = setup(DeviceType::Output);
    let json = serde_json::to_string(&state).unwrap();
    let loaded: PersistentState = serde_json::from_str(&json).unwrap();
    assert!(loaded.wireless_headset_detection_enabled);
    assert_eq!(loaded.device_count(), 0);
}
