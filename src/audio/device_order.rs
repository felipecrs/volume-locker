use crate::config::PersistentState;
use crate::i18n::t;
use crate::notification::NotificationThrottler;
use crate::types::{DeviceId, DeviceRole, DeviceType, OrderKind, TemporaryDefaults};

use super::AudioBackend;

pub fn enforce_device_order(
    backend: &impl AudioBackend,
    state: &PersistentState,
    throttler: &mut NotificationThrottler,
    temporary_defaults: &TemporaryDefaults,
) {
    for device_type in [DeviceType::Output, DeviceType::Input] {
        enforce_device_order_for_type(
            backend,
            device_type,
            state,
            temporary_defaults.get(device_type, OrderKind::Main),
            temporary_defaults.get(device_type, OrderKind::Communication),
            throttler,
        );
    }
}

fn is_default_device(
    backend: &impl AudioBackend,
    device_type: DeviceType,
    role: DeviceRole,
    target_id: &DeviceId,
) -> bool {
    match backend.default_device(device_type, role) {
        Ok(d) => *target_id == *d.id(),
        Err(e) => {
            log::warn!("Failed to get default {device_type} {role} device: {e:#}");
            false
        }
    }
}

fn enforce_device_order_for_type(
    backend: &impl AudioBackend,
    device_type: DeviceType,
    state: &PersistentState,
    temporary_main: Option<&DeviceId>,
    temporary_comm: Option<&DeviceId>,
    throttler: &mut NotificationThrottler,
) {
    let mut device_order = state.device_order(device_type).to_vec();
    if let Some(temp_id) = temporary_main {
        device_order.insert(0, temp_id.clone());
    }

    let main_target = first_available_device(backend, &device_order, state);
    let comm_target = resolve_communication_target(
        backend,
        device_type,
        state,
        temporary_comm,
        main_target.as_ref(),
    );

    // Devices that were actually switched to, paired with the list that chose them.
    let mut switched: Vec<(DeviceId, OrderKind)> = Vec::new();

    // Enforce Console and Multimedia roles together
    if let Some(target_id) = &main_target {
        let mut changed = false;
        for role in [DeviceRole::Console, DeviceRole::Multimedia] {
            if !is_default_device(backend, device_type, role, target_id) {
                match backend.set_default_device(target_id, role) {
                    Ok(()) => changed = true,
                    Err(e) => log::error!(
                        "Failed to set default {role} {device_type} device to {target_id}: {e:#}"
                    ),
                }
            }
        }
        if changed {
            let name = backend
                .device_by_id(target_id)
                .map(|d| d.name())
                .unwrap_or_else(|_| target_id.to_string());
            log::info!("Enforced {device_type} device order: switched to '{name}' ({target_id})");
            switched.push((target_id.clone(), OrderKind::Main));
        }
    }

    if let Some(target_id) = &comm_target
        && !is_default_device(backend, device_type, DeviceRole::Communications, target_id)
    {
        let name = backend
            .device_by_id(target_id)
            .map(|d| d.name())
            .unwrap_or_else(|_| target_id.to_string());
        match backend.set_default_device(target_id, DeviceRole::Communications) {
            Err(e) => log::error!(
                "Failed to set default {device_type} communications device to '{name}' ({target_id}): {e:#}"
            ),
            // Report a successful communication switch separately only when
            // the main roles did not already switch to the same device.
            Ok(()) if !switched.iter().any(|(id, _)| id == target_id) => {
                log::info!(
                    "Enforced {device_type} communication device order: switched to '{name}' ({target_id})"
                );
                switched.push((target_id.clone(), OrderKind::Communication));
            }
            Ok(()) => {}
        }
    }

    for (target_id, kind) in switched {
        if state.notify_on_switch_of(device_type, kind) {
            notify_switched(backend, device_type, kind, &target_id, throttler);
        }
    }
}

/// Determines which device the Communications role should resolve to.
///
/// A temporary communication default device takes precedence over everything.
/// Otherwise, entries in the communication device order are evaluated in order.
/// If an entry is [`DeviceId::FOLLOW_MAIN`], it dynamically resolves to the
/// device chosen by the main device order (`main_target`).
fn resolve_communication_target(
    backend: &impl AudioBackend,
    device_type: DeviceType,
    state: &PersistentState,
    temporary_comm: Option<&DeviceId>,
    main_target: Option<&DeviceId>,
) -> Option<DeviceId> {
    if let Some(temp_id) = temporary_comm
        && let Some(device_id) =
            first_available_device(backend, std::slice::from_ref(temp_id), state)
    {
        return Some(device_id);
    }
    let comm_list = state.communication_device_order(device_type);
    for entry_id in comm_list {
        if entry_id.is_follow_main() {
            if let Some(main_id) = main_target {
                return Some(main_id.clone());
            }
        } else if let Some(device_id) =
            first_available_device(backend, std::slice::from_ref(entry_id), state)
        {
            return Some(device_id);
        }
    }

    None
}

fn notify_switched(
    backend: &impl AudioBackend,
    device_type: DeviceType,
    kind: OrderKind,
    target_id: &DeviceId,
    throttler: &mut NotificationThrottler,
) {
    let device_name = backend.device_by_id(target_id).map_or_else(
        |e| {
            log::warn!("Could not get name for device {target_id}: {e:#}");
            t!("unknown-device")
        },
        |d| d.name(),
    );
    let title = match (device_type, kind) {
        (DeviceType::Output, OrderKind::Main) => t!("output-switched-title"),
        (DeviceType::Input, OrderKind::Main) => t!("input-switched-title"),
        (DeviceType::Output, OrderKind::Communication) => {
            t!("output-communication-switched-title")
        }
        (DeviceType::Input, OrderKind::Communication) => {
            t!("input-communication-switched-title")
        }
    };
    let throttle_key = match kind {
        OrderKind::Main => format!("default_switch_{target_id}"),
        OrderKind::Communication => format!("default_switch_communication_{target_id}"),
    };
    throttler.send_if_not_throttled(
        &throttle_key,
        &title,
        &t!("switched-by-order", device = device_name.as_str()),
    );
}

fn first_available_device(
    backend: &impl AudioBackend,
    device_order: &[DeviceId],
    state: &PersistentState,
) -> Option<DeviceId> {
    device_order
        .iter()
        .find_map(|device_id| match backend.device_by_id(device_id) {
            Ok(device) => match device.is_active() {
                Ok(true)
                    if state.skip_disconnected_wireless_headsets
                        && device
                            .fingerprint()
                            .and_then(|fingerprint| fingerprint.usb_vid_pid)
                            .is_some_and(|usb_vid_pid| {
                                backend.headset_presence(usb_vid_pid)
                                    == super::presence::HeadsetPresence::Unavailable
                            }) =>
                {
                    log::debug!(
                        "Device '{}' ({device_id}) has no wireless headset link, skipping in device order",
                        device.name()
                    );
                    None
                }
                Ok(true) => Some(device_id.clone()),
                Ok(false) => {
                    log::debug!(
                        "Device '{}' ({device_id}) is inactive, skipping in device order",
                        device.name()
                    );
                    None
                }
                Err(e) => {
                    log::warn!("Failed to check if device {device_id} is active: {e:#}");
                    None
                }
            },
            Err(e) => {
                log::debug!("Device {device_id} is not connected, skipping in device order: {e}");
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::tests::{MockAudioBackend, MockDevice};
    use crate::notification::NotificationThrottler;

    #[test]
    fn input_and_output_communication_orders_are_independent() {
        let mut mic = MockDevice::new("mic", "Microphone", true);
        mic.device_type = DeviceType::Input;
        let mut call_mic = MockDevice::new("call_mic", "Call microphone", true);
        call_mic.device_type = DeviceType::Input;
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("speakers", "Speakers", true),
            MockDevice::new("headset", "Headset", true),
            mic,
            call_mic,
        ]);
        let mut state = PersistentState::default();
        state.output.device_order = vec!["speakers".into()];
        state.output.communication_device_order = vec!["headset".into()];
        state.input.device_order = vec!["mic".into()];
        state.input.communication_device_order = vec!["call_mic".into()];
        enforce_device_order(
            &backend,
            &state,
            &mut NotificationThrottler::new(),
            &TemporaryDefaults::default(),
        );
        assert_eq!(
            backend.default_console.borrow()[&DeviceType::Output],
            "speakers"
        );
        assert_eq!(
            backend.default_communications.borrow()[&DeviceType::Output],
            "headset"
        );
        assert_eq!(backend.default_console.borrow()[&DeviceType::Input], "mic");
        assert_eq!(
            backend.default_communications.borrow()[&DeviceType::Input],
            "call_mic"
        );
    }

    #[test]
    fn multimedia_drift_is_restored_when_console_is_already_correct() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("speakers", "Speakers", true),
            MockDevice::new("headset", "Headset", true),
        ]);
        backend.set_default("speakers", DeviceType::Output);
        backend
            .default_multimedia
            .borrow_mut()
            .insert(DeviceType::Output, "headset".into());
        let mut state = PersistentState::default();
        state.output.device_order = vec!["speakers".into()];
        enforce_device_order(
            &backend,
            &state,
            &mut NotificationThrottler::new(),
            &TemporaryDefaults::default(),
        );
        assert_eq!(
            backend.default_multimedia.borrow()[&DeviceType::Output],
            "speakers"
        );
    }

    #[test]
    fn unavailable_communication_list_leaves_existing_default_alone() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("speakers", "Speakers", true),
            MockDevice::new("headset", "Headset", false),
        ]);
        backend
            .set_default_device(&"speakers".into(), DeviceRole::Communications)
            .unwrap();
        let mut state = PersistentState::default();
        state.output.communication_device_order = vec!["missing".into(), "headset".into()];
        enforce_device_order(
            &backend,
            &state,
            &mut NotificationThrottler::new(),
            &TemporaryDefaults::default(),
        );
        assert_eq!(
            backend.default_communications.borrow()[&DeviceType::Output],
            "speakers"
        );
    }

    #[test]
    fn enforce_device_order_switches_to_highest_active() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("dev_a", "Device A", true),
            MockDevice::new("dev_b", "Device B", true),
        ]);
        backend.set_default("dev_b", DeviceType::Output);

        let mut state = PersistentState::default();
        state.output.device_order = vec!["dev_a".into(), "dev_b".into()];

        let mut times = NotificationThrottler::new();
        let temp = TemporaryDefaults::default();

        enforce_device_order(&backend, &state, &mut times, &temp);

        assert_eq!(
            backend.default_console.borrow().get(&DeviceType::Output),
            Some(&"dev_a".to_string())
        );
    }

    #[test]
    fn enforce_device_order_no_switch_when_correct() {
        let backend = MockAudioBackend::new(vec![MockDevice::new("dev_a", "Device A", true)]);
        backend.set_default("dev_a", DeviceType::Output);

        let mut state = PersistentState::default();
        state.output.device_order = vec!["dev_a".into()];

        let mut times = NotificationThrottler::new();
        let temp = TemporaryDefaults::default();

        enforce_device_order(&backend, &state, &mut times, &temp);

        assert_eq!(
            backend.default_console.borrow().get(&DeviceType::Output),
            Some(&"dev_a".to_string())
        );
    }

    #[test]
    fn enforce_device_order_skips_inactive_devices() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("dev_a", "Device A", false),
            MockDevice::new("dev_b", "Device B", true),
        ]);
        backend.set_default("dev_b", DeviceType::Output);

        let mut state = PersistentState::default();
        state.output.device_order = vec!["dev_a".into(), "dev_b".into()];

        let mut times = NotificationThrottler::new();
        let temp = TemporaryDefaults::default();

        enforce_device_order(&backend, &state, &mut times, &temp);

        assert_eq!(
            backend.default_console.borrow().get(&DeviceType::Output),
            Some(&"dev_b".to_string())
        );
    }

    #[test]
    fn enforce_device_order_temporary_default_overrides() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("dev_a", "Device A", true),
            MockDevice::new("dev_temp", "Temp Device", true),
        ]);
        backend.set_default("dev_a", DeviceType::Output);

        let mut state = PersistentState::default();
        state.output.device_order = vec!["dev_a".into()];

        let mut times = NotificationThrottler::new();
        let mut temp = TemporaryDefaults::default();
        temp.set(DeviceType::Output, OrderKind::Main, Some("dev_temp".into()));

        enforce_device_order(&backend, &state, &mut times, &temp);

        assert_eq!(
            backend.default_console.borrow().get(&DeviceType::Output),
            Some(&"dev_temp".to_string())
        );
    }

    #[test]
    fn enforce_device_order_empty_list_does_nothing() {
        let backend = MockAudioBackend::new(vec![MockDevice::new("dev_a", "Device A", true)]);
        backend.set_default("dev_a", DeviceType::Output);

        let state = PersistentState::default();

        let mut times = NotificationThrottler::new();
        let temp = TemporaryDefaults::default();

        enforce_device_order(&backend, &state, &mut times, &temp);

        assert_eq!(
            backend.default_console.borrow().get(&DeviceType::Output),
            Some(&"dev_a".to_string())
        );
    }

    #[test]
    fn enforce_device_order_communication_device_switching() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("dev_a", "Device A", true),
            MockDevice::new("dev_b", "Device B", true),
        ]);
        backend.set_default("dev_b", DeviceType::Output);

        let mut state = PersistentState::default();
        state.output.device_order = vec!["dev_a".into(), "dev_b".into()];

        let mut times = NotificationThrottler::new();
        let temp = TemporaryDefaults::default();

        enforce_device_order(&backend, &state, &mut times, &temp);

        assert_eq!(
            backend
                .default_communications
                .borrow()
                .get(&DeviceType::Output),
            Some(&"dev_a".to_string())
        );
    }

    #[test]
    fn communication_list_routes_roles_independently() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("speakers", "Speakers", true),
            MockDevice::new("headset", "Headset", true),
        ]);
        backend.set_default("headset", DeviceType::Output);

        let mut state = PersistentState::default();
        state.output.device_order = vec!["speakers".into()];
        state.output.communication_device_order = vec!["headset".into()];

        let mut times = NotificationThrottler::new();
        let temp = TemporaryDefaults::default();

        enforce_device_order(&backend, &state, &mut times, &temp);

        assert_eq!(
            backend.default_console.borrow().get(&DeviceType::Output),
            Some(&"speakers".to_string())
        );
        assert_eq!(
            backend
                .default_communications
                .borrow()
                .get(&DeviceType::Output),
            Some(&"headset".to_string())
        );
    }

    #[test]
    fn communication_list_fallback_to_follow_main() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("speakers", "Speakers", true),
            MockDevice::new("headset", "Headset", false), // inactive!
        ]);
        backend.set_default("speakers", DeviceType::Output);

        let mut state = PersistentState::default();
        state.output.device_order = vec!["speakers".into()];
        // Headset comes first, but falls back to the main output device order
        state.output.communication_device_order = vec!["headset".into(), DeviceId::follow_main()];

        let mut times = NotificationThrottler::new();
        let temp = TemporaryDefaults::default();

        enforce_device_order(&backend, &state, &mut times, &temp);

        assert_eq!(
            backend.default_console.borrow().get(&DeviceType::Output),
            Some(&"speakers".to_string())
        );
        assert_eq!(
            backend
                .default_communications
                .borrow()
                .get(&DeviceType::Output),
            Some(&"speakers".to_string())
        );
    }

    #[test]
    fn communication_mirrors_main_when_follow_main_entry_present() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("speakers", "Speakers", true),
            MockDevice::new("headset", "Headset", true),
        ]);
        backend.set_default("headset", DeviceType::Output);

        let mut state = PersistentState::default();
        state.output.device_order = vec!["speakers".into()];
        state.output.communication_device_order = vec![DeviceId::follow_main()];

        let mut times = NotificationThrottler::new();
        let temp = TemporaryDefaults::default();

        enforce_device_order(&backend, &state, &mut times, &temp);

        assert_eq!(
            backend.default_console.borrow().get(&DeviceType::Output),
            Some(&"speakers".to_string())
        );
        assert_eq!(
            backend
                .default_communications
                .borrow()
                .get(&DeviceType::Output),
            Some(&"speakers".to_string())
        );
    }

    #[test]
    fn communication_list_skips_inactive_devices() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("speakers", "Speakers", true),
            MockDevice::new("headset", "Headset", false),
            MockDevice::new("earbuds", "Earbuds", true),
        ]);
        backend.set_default("speakers", DeviceType::Output);

        let mut state = PersistentState::default();
        state.output.device_order = vec!["speakers".into()];
        state.output.communication_device_order = vec!["headset".into(), "earbuds".into()];

        let mut times = NotificationThrottler::new();
        let temp = TemporaryDefaults::default();

        enforce_device_order(&backend, &state, &mut times, &temp);

        assert_eq!(
            backend
                .default_communications
                .borrow()
                .get(&DeviceType::Output),
            Some(&"earbuds".to_string())
        );
    }

    #[test]
    fn communication_list_applies_without_a_main_list() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("speakers", "Speakers", true),
            MockDevice::new("headset", "Headset", true),
        ]);
        backend.set_default("speakers", DeviceType::Output);

        let mut state = PersistentState::default();
        state.output.communication_device_order = vec!["headset".into()];

        let mut times = NotificationThrottler::new();
        let temp = TemporaryDefaults::default();

        enforce_device_order(&backend, &state, &mut times, &temp);

        // Console/Multimedia are left alone, only Communications moves.
        assert_eq!(
            backend.default_console.borrow().get(&DeviceType::Output),
            Some(&"speakers".to_string())
        );
        assert_eq!(
            backend
                .default_communications
                .borrow()
                .get(&DeviceType::Output),
            Some(&"headset".to_string())
        );
    }

    #[test]
    fn empty_communication_list_leaves_device_alone() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("dev_a", "Device A", true),
            MockDevice::new("dev_b", "Device B", true),
        ]);
        backend.set_default("dev_b", DeviceType::Output);
        backend
            .set_default_device(&"dev_b".into(), DeviceRole::Communications)
            .unwrap();

        let mut state = PersistentState::default();
        state.output.device_order = vec!["dev_a".into()];
        state.output.communication_device_order = vec![];

        let mut times = NotificationThrottler::new();
        let temp = TemporaryDefaults::default();

        enforce_device_order(&backend, &state, &mut times, &temp);

        assert_eq!(
            backend
                .default_communications
                .borrow()
                .get(&DeviceType::Output),
            Some(&"dev_b".to_string())
        );
    }

    #[test]
    fn temporary_default_does_not_hijack_communication_list() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("speakers", "Speakers", true),
            MockDevice::new("headset", "Headset", true),
            MockDevice::new("dev_temp", "Temp Device", true),
        ]);
        backend.set_default("speakers", DeviceType::Output);

        let mut state = PersistentState::default();
        state.output.device_order = vec!["speakers".into()];
        state.output.communication_device_order = vec!["headset".into()];

        let mut times = NotificationThrottler::new();
        let mut temp = TemporaryDefaults::default();
        temp.set(DeviceType::Output, OrderKind::Main, Some("dev_temp".into()));

        enforce_device_order(&backend, &state, &mut times, &temp);

        assert_eq!(
            backend.default_console.borrow().get(&DeviceType::Output),
            Some(&"dev_temp".to_string())
        );
        assert_eq!(
            backend
                .default_communications
                .borrow()
                .get(&DeviceType::Output),
            Some(&"headset".to_string())
        );
    }

    #[test]
    fn temporary_communication_default_overrides_follow_main_entry() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("speakers", "Speakers", true),
            MockDevice::new("headset", "Headset", true),
            MockDevice::new("call_mic", "Call Mic", true),
        ]);
        backend.set_default("speakers", DeviceType::Output);

        let mut state = PersistentState::default();
        state.output.device_order = vec!["speakers".into()];
        state.output.communication_device_order = vec![DeviceId::follow_main()];

        let mut times = NotificationThrottler::new();
        let mut temp = TemporaryDefaults::default();
        temp.set(
            DeviceType::Output,
            OrderKind::Communication,
            Some("call_mic".into()),
        );

        enforce_device_order(&backend, &state, &mut times, &temp);

        // Main output remains on speakers
        assert_eq!(
            backend.default_console.borrow().get(&DeviceType::Output),
            Some(&"speakers".to_string())
        );
        // Communication is overridden by temporary communication default device
        assert_eq!(
            backend
                .default_communications
                .borrow()
                .get(&DeviceType::Output),
            Some(&"call_mic".to_string())
        );
    }

    #[test]
    fn sync_enabled_does_not_send_duplicate_notifications() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("speakers", "Speakers", true),
            MockDevice::new("other", "Other", true),
        ]);
        backend.set_default("other", DeviceType::Output);

        let mut state = PersistentState::default();
        state.output.device_order = vec!["speakers".into()];
        state.output.communication_device_order = vec![DeviceId::follow_main()];
        state.output.notify_on_switch = true;
        state.output.communication_notify_on_switch = true;

        let mut times = NotificationThrottler::new();
        enforce_device_order(&backend, &state, &mut times, &TemporaryDefaults::default());

        // Both roles switched to speakers
        assert_eq!(
            backend.default_console.borrow().get(&DeviceType::Output),
            Some(&"speakers".to_string())
        );
        assert_eq!(
            backend
                .default_communications
                .borrow()
                .get(&DeviceType::Output),
            Some(&"speakers".to_string())
        );
        // Main key was throttled (notification sent)
        assert!(!times.should_notify("default_switch_speakers"));
        // Communication specific key was NOT triggered because it switched to the same device as main
        assert!(times.should_notify("default_switch_communication_speakers"));
    }

    #[test]
    fn find_highest_returns_first_active() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("dev_a", "A", false),
            MockDevice::new("dev_b", "B", true),
            MockDevice::new("dev_c", "C", true),
        ]);
        let list = vec!["dev_a".into(), "dev_b".into(), "dev_c".into()];
        assert_eq!(
            first_available_device(&backend, &list, &PersistentState::default()),
            Some(DeviceId::from("dev_b"))
        );
    }

    #[test]
    fn find_highest_returns_none_when_all_inactive() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("dev_a", "A", false),
            MockDevice::new("dev_b", "B", false),
        ]);
        let list = vec!["dev_a".into(), "dev_b".into()];
        assert_eq!(
            first_available_device(&backend, &list, &PersistentState::default()),
            None
        );
    }

    #[test]
    fn find_highest_returns_none_for_empty_list() {
        let backend = MockAudioBackend::new(vec![]);
        let list: Vec<DeviceId> = vec![];
        assert_eq!(
            first_available_device(&backend, &list, &PersistentState::default()),
            None
        );
    }
}
