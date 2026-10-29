use crate::config::PersistentState;
use crate::notification::NotificationThrottler;
use crate::types::{DeviceId, DeviceRole, DeviceType, PriorityKind, TemporaryPriorities};

use super::AudioBackend;

pub fn enforce_priorities(
    backend: &impl AudioBackend,
    state: &PersistentState,
    throttler: &mut NotificationThrottler,
    temporary_priorities: &TemporaryPriorities,
) {
    for device_type in [DeviceType::Output, DeviceType::Input] {
        enforce_priority_for_type(
            backend,
            device_type,
            state,
            temporary_priorities.get(device_type, PriorityKind::Main),
            temporary_priorities.get(device_type, PriorityKind::Communication),
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

fn enforce_priority_for_type(
    backend: &impl AudioBackend,
    device_type: DeviceType,
    state: &PersistentState,
    temporary_main: Option<&DeviceId>,
    temporary_comm: Option<&DeviceId>,
    throttler: &mut NotificationThrottler,
) {
    let mut priority_list = state.priority_list(device_type).to_vec();
    if let Some(temp_id) = temporary_main {
        priority_list.insert(0, temp_id.clone());
    }

    let main_target = find_highest_priority_active_device(backend, &priority_list, state);
    let comm_target = resolve_communication_target(
        backend,
        device_type,
        state,
        temporary_comm,
        main_target.as_ref(),
    );

    // Devices that were actually switched to, paired with the list that chose them.
    let mut switched: Vec<(DeviceId, PriorityKind)> = Vec::new();

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
            log::info!("Enforced {device_type} priority: switched to '{name}' ({target_id})");
            switched.push((target_id.clone(), PriorityKind::Main));
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
                    "Enforced {device_type} communication priority: switched to '{name}' ({target_id})"
                );
                switched.push((target_id.clone(), PriorityKind::Communication));
            }
            Ok(()) => {}
        }
    }

    for (target_id, kind) in switched {
        if state.notify_on_priority_restore_of(device_type, kind) {
            notify_priority_restored(backend, device_type, kind, &target_id, throttler);
        }
    }
}

/// Determines which device the Communications role should resolve to.
///
/// A temporary communication priority takes precedence over everything.
/// Otherwise, entries in the communication priority list are evaluated in order.
/// If an entry is [`DeviceId::MAIN_PRIORITY`], it dynamically resolves to the
/// device chosen by the main priority list (`main_target`).
fn resolve_communication_target(
    backend: &impl AudioBackend,
    device_type: DeviceType,
    state: &PersistentState,
    temporary_comm: Option<&DeviceId>,
    main_target: Option<&DeviceId>,
) -> Option<DeviceId> {
    if let Some(temp_id) = temporary_comm
        && let Ok(device) = backend.device_by_id(temp_id)
        && let Ok(true) = device.is_active()
    {
        return Some(temp_id.clone());
    }
    let comm_list = state.communication_priority_list(device_type);
    for entry_id in comm_list {
        if entry_id.is_main_priority() {
            if let Some(main_id) = main_target {
                return Some(main_id.clone());
            }
        } else if let Some(device_id) =
            find_highest_priority_active_device(backend, std::slice::from_ref(entry_id), state)
        {
            return Some(device_id);
        }
    }

    None
}

fn notify_priority_restored(
    backend: &impl AudioBackend,
    device_type: DeviceType,
    kind: PriorityKind,
    target_id: &DeviceId,
    throttler: &mut NotificationThrottler,
) {
    let device_name = backend.device_by_id(target_id).map_or_else(
        |e| {
            log::warn!("Could not get name for device {target_id}: {e:#}");
            "Unknown Device".to_string()
        },
        |d| d.name(),
    );
    let title = match (device_type, kind) {
        (DeviceType::Output, PriorityKind::Main) => "Output Device Restored",
        (DeviceType::Input, PriorityKind::Main) => "Input Device Restored",
        (DeviceType::Output, PriorityKind::Communication) => "Output Communication Device Restored",
        (DeviceType::Input, PriorityKind::Communication) => "Input Communication Device Restored",
    };
    let throttle_key = match kind {
        PriorityKind::Main => format!("priority_restore_{target_id}"),
        PriorityKind::Communication => format!("priority_restore_communication_{target_id}"),
    };
    throttler.send_if_not_throttled(
        &throttle_key,
        title,
        &format!("Switched to {device_name} based on priority list."),
    );
}

fn find_highest_priority_active_device(
    backend: &impl AudioBackend,
    priority_list: &[DeviceId],
    state: &PersistentState,
) -> Option<DeviceId> {
    priority_list
        .iter()
        .filter(|device_id| {
            state.device_settings(device_id).is_none_or(|s| {
                backend.headset_presence(s.presence_source)
                    != super::presence::HeadsetPresence::Unavailable
            })
        })
        .find_map(|device_id| match backend.device_by_id(device_id) {
            Ok(device) => match device.is_active() {
                Ok(true) => Some(device_id.clone()),
                Ok(false) => {
                    log::debug!(
                        "Device '{}' ({device_id}) is inactive, skipping in priority order",
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
                log::debug!("Device {device_id} is not connected, skipping in priority order: {e}");
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
    fn input_and_output_communication_priorities_are_independent() {
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
        state.output.priority_list = vec!["speakers".into()];
        state.output.communication_priority_list = vec!["headset".into()];
        state.input.priority_list = vec!["mic".into()];
        state.input.communication_priority_list = vec!["call_mic".into()];
        enforce_priorities(
            &backend,
            &state,
            &mut NotificationThrottler::new(),
            &TemporaryPriorities::default(),
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
        state.output.priority_list = vec!["speakers".into()];
        enforce_priorities(
            &backend,
            &state,
            &mut NotificationThrottler::new(),
            &TemporaryPriorities::default(),
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
        state.output.communication_priority_list = vec!["missing".into(), "headset".into()];
        enforce_priorities(
            &backend,
            &state,
            &mut NotificationThrottler::new(),
            &TemporaryPriorities::default(),
        );
        assert_eq!(
            backend.default_communications.borrow()[&DeviceType::Output],
            "speakers"
        );
    }

    #[test]
    fn enforce_priorities_switches_to_highest_active() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("dev_a", "Device A", true),
            MockDevice::new("dev_b", "Device B", true),
        ]);
        backend.set_default("dev_b", DeviceType::Output);

        let mut state = PersistentState::default();
        state.output.priority_list = vec!["dev_a".into(), "dev_b".into()];

        let mut times = NotificationThrottler::new();
        let temp = TemporaryPriorities::default();

        enforce_priorities(&backend, &state, &mut times, &temp);

        assert_eq!(
            backend.default_console.borrow().get(&DeviceType::Output),
            Some(&"dev_a".to_string())
        );
    }

    #[test]
    fn enforce_priorities_no_switch_when_correct() {
        let backend = MockAudioBackend::new(vec![MockDevice::new("dev_a", "Device A", true)]);
        backend.set_default("dev_a", DeviceType::Output);

        let mut state = PersistentState::default();
        state.output.priority_list = vec!["dev_a".into()];

        let mut times = NotificationThrottler::new();
        let temp = TemporaryPriorities::default();

        enforce_priorities(&backend, &state, &mut times, &temp);

        assert_eq!(
            backend.default_console.borrow().get(&DeviceType::Output),
            Some(&"dev_a".to_string())
        );
    }

    #[test]
    fn enforce_priorities_skips_inactive_devices() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("dev_a", "Device A", false),
            MockDevice::new("dev_b", "Device B", true),
        ]);
        backend.set_default("dev_b", DeviceType::Output);

        let mut state = PersistentState::default();
        state.output.priority_list = vec!["dev_a".into(), "dev_b".into()];

        let mut times = NotificationThrottler::new();
        let temp = TemporaryPriorities::default();

        enforce_priorities(&backend, &state, &mut times, &temp);

        assert_eq!(
            backend.default_console.borrow().get(&DeviceType::Output),
            Some(&"dev_b".to_string())
        );
    }

    #[test]
    fn enforce_priorities_temporary_priority_overrides() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("dev_a", "Device A", true),
            MockDevice::new("dev_temp", "Temp Device", true),
        ]);
        backend.set_default("dev_a", DeviceType::Output);

        let mut state = PersistentState::default();
        state.output.priority_list = vec!["dev_a".into()];

        let mut times = NotificationThrottler::new();
        let mut temp = TemporaryPriorities::default();
        temp.set(
            DeviceType::Output,
            PriorityKind::Main,
            Some("dev_temp".into()),
        );

        enforce_priorities(&backend, &state, &mut times, &temp);

        assert_eq!(
            backend.default_console.borrow().get(&DeviceType::Output),
            Some(&"dev_temp".to_string())
        );
    }

    #[test]
    fn enforce_priorities_empty_list_does_nothing() {
        let backend = MockAudioBackend::new(vec![MockDevice::new("dev_a", "Device A", true)]);
        backend.set_default("dev_a", DeviceType::Output);

        let state = PersistentState::default();

        let mut times = NotificationThrottler::new();
        let temp = TemporaryPriorities::default();

        enforce_priorities(&backend, &state, &mut times, &temp);

        assert_eq!(
            backend.default_console.borrow().get(&DeviceType::Output),
            Some(&"dev_a".to_string())
        );
    }

    #[test]
    fn enforce_priorities_communication_device_switching() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("dev_a", "Device A", true),
            MockDevice::new("dev_b", "Device B", true),
        ]);
        backend.set_default("dev_b", DeviceType::Output);

        let mut state = PersistentState::default();
        state.output.priority_list = vec!["dev_a".into(), "dev_b".into()];

        let mut times = NotificationThrottler::new();
        let temp = TemporaryPriorities::default();

        enforce_priorities(&backend, &state, &mut times, &temp);

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
        state.output.priority_list = vec!["speakers".into()];
        state.output.communication_priority_list = vec!["headset".into()];

        let mut times = NotificationThrottler::new();
        let temp = TemporaryPriorities::default();

        enforce_priorities(&backend, &state, &mut times, &temp);

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
    fn communication_list_fallback_to_main_priority() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("speakers", "Speakers", true),
            MockDevice::new("headset", "Headset", false), // inactive!
        ]);
        backend.set_default("speakers", DeviceType::Output);

        let mut state = PersistentState::default();
        state.output.priority_list = vec!["speakers".into()];
        // Headset is top priority, but falls back to main output device priority
        state.output.communication_priority_list =
            vec!["headset".into(), DeviceId::main_priority()];

        let mut times = NotificationThrottler::new();
        let temp = TemporaryPriorities::default();

        enforce_priorities(&backend, &state, &mut times, &temp);

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
    fn communication_mirrors_main_when_main_priority_entry_present() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("speakers", "Speakers", true),
            MockDevice::new("headset", "Headset", true),
        ]);
        backend.set_default("headset", DeviceType::Output);

        let mut state = PersistentState::default();
        state.output.priority_list = vec!["speakers".into()];
        state.output.communication_priority_list = vec![DeviceId::main_priority()];

        let mut times = NotificationThrottler::new();
        let temp = TemporaryPriorities::default();

        enforce_priorities(&backend, &state, &mut times, &temp);

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
        state.output.priority_list = vec!["speakers".into()];
        state.output.communication_priority_list = vec!["headset".into(), "earbuds".into()];

        let mut times = NotificationThrottler::new();
        let temp = TemporaryPriorities::default();

        enforce_priorities(&backend, &state, &mut times, &temp);

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
        state.output.communication_priority_list = vec!["headset".into()];

        let mut times = NotificationThrottler::new();
        let temp = TemporaryPriorities::default();

        enforce_priorities(&backend, &state, &mut times, &temp);

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
        state.output.priority_list = vec!["dev_a".into()];
        state.output.communication_priority_list = vec![];

        let mut times = NotificationThrottler::new();
        let temp = TemporaryPriorities::default();

        enforce_priorities(&backend, &state, &mut times, &temp);

        assert_eq!(
            backend
                .default_communications
                .borrow()
                .get(&DeviceType::Output),
            Some(&"dev_b".to_string())
        );
    }

    #[test]
    fn temporary_priority_does_not_hijack_communication_list() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("speakers", "Speakers", true),
            MockDevice::new("headset", "Headset", true),
            MockDevice::new("dev_temp", "Temp Device", true),
        ]);
        backend.set_default("speakers", DeviceType::Output);

        let mut state = PersistentState::default();
        state.output.priority_list = vec!["speakers".into()];
        state.output.communication_priority_list = vec!["headset".into()];

        let mut times = NotificationThrottler::new();
        let mut temp = TemporaryPriorities::default();
        temp.set(
            DeviceType::Output,
            PriorityKind::Main,
            Some("dev_temp".into()),
        );

        enforce_priorities(&backend, &state, &mut times, &temp);

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
    fn temporary_communication_priority_overrides_main_priority_entry() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("speakers", "Speakers", true),
            MockDevice::new("headset", "Headset", true),
            MockDevice::new("call_mic", "Call Mic", true),
        ]);
        backend.set_default("speakers", DeviceType::Output);

        let mut state = PersistentState::default();
        state.output.priority_list = vec!["speakers".into()];
        state.output.communication_priority_list = vec![DeviceId::main_priority()];

        let mut times = NotificationThrottler::new();
        let mut temp = TemporaryPriorities::default();
        temp.set(
            DeviceType::Output,
            PriorityKind::Communication,
            Some("call_mic".into()),
        );

        enforce_priorities(&backend, &state, &mut times, &temp);

        // Main output remains on speakers
        assert_eq!(
            backend.default_console.borrow().get(&DeviceType::Output),
            Some(&"speakers".to_string())
        );
        // Communication is overridden by temporary communication priority
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
        state.output.priority_list = vec!["speakers".into()];
        state.output.communication_priority_list = vec![DeviceId::main_priority()];
        state.output.notify_on_priority_restore = true;
        state.output.communication_notify_on_priority_restore = true;

        let mut times = NotificationThrottler::new();
        enforce_priorities(
            &backend,
            &state,
            &mut times,
            &TemporaryPriorities::default(),
        );

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
        assert!(!times.should_notify("priority_restore_speakers"));
        // Communication specific key was NOT triggered because it switched to the same device as main
        assert!(times.should_notify("priority_restore_communication_speakers"));
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
            find_highest_priority_active_device(&backend, &list, &PersistentState::default()),
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
            find_highest_priority_active_device(&backend, &list, &PersistentState::default()),
            None
        );
    }

    #[test]
    fn find_highest_returns_none_for_empty_list() {
        let backend = MockAudioBackend::new(vec![]);
        let list: Vec<DeviceId> = vec![];
        assert_eq!(
            find_highest_priority_active_device(&backend, &list, &PersistentState::default()),
            None
        );
    }
}
