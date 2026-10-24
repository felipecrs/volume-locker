use crate::config::PersistentState;
use crate::notification::NotificationThrottler;
use crate::types::{CommunicationRoute, DeviceId, DeviceRole, DeviceType, TemporaryPriorities};

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
            temporary_priorities.get(device_type),
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
    temporary_priority: Option<&DeviceId>,
    throttler: &mut NotificationThrottler,
) {
    let mut priority_list = state.priority_list(device_type).to_vec();
    if let Some(temp_id) = temporary_priority {
        priority_list.insert(0, temp_id.clone());
    }

    let main_target = find_highest_priority_active_device(backend, &priority_list);
    // Follow manual Windows output changes only when no priority order is set.
    let routing_target = main_target.clone().or_else(|| {
        if device_type != DeviceType::Output || !priority_list.is_empty() {
            return None;
        }
        let current = backend
            .default_device(device_type, DeviceRole::Console)
            .ok()?;
        let settings = state.device_settings(current.id())?;
        (!settings.communication_route.is_inherited() && current.is_active().unwrap_or(false))
            .then(|| current.id().clone())
    });
    let comm_target =
        resolve_communication_target(backend, device_type, state, routing_target.as_ref());
    let mut switched = Vec::new();
    if let Some(target_id) = &main_target {
        for role in [DeviceRole::Console, DeviceRole::Multimedia] {
            if !is_default_device(backend, device_type, role, target_id) {
                match backend.set_default_device(target_id, role) {
                    Ok(()) if !switched.contains(target_id) => switched.push(target_id.clone()),
                    Ok(()) => {}
                    Err(e) => log::error!("Failed to set default {role} {device_type}: {e:#}"),
                }
            }
        }
    }
    if let Some(target_id) = &comm_target
        && !is_default_device(backend, device_type, DeviceRole::Communications, target_id)
    {
        match backend.set_default_device(target_id, DeviceRole::Communications) {
            Ok(()) if !switched.contains(target_id) => switched.push(target_id.clone()),
            Ok(()) => {}
            Err(e) => log::error!("Failed to set default communications {device_type}: {e:#}"),
        }
    }
    if state.notify_on_priority_restore(device_type) {
        for target_id in switched {
            notify_priority_restored(backend, device_type, &target_id, throttler);
        }
    }
}

fn notify_priority_restored(
    backend: &impl AudioBackend,
    device_type: DeviceType,
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
    let title = match device_type {
        DeviceType::Output => "Default Output Device Restored",
        DeviceType::Input => "Default Input Device Restored",
    };
    throttler.send_if_not_throttled(
        &format!("priority_restore_{target_id}"),
        title,
        &format!("Switched to {device_name} based on priority list."),
    );
}

/// Explicit output routes override the global communication checkbox.
fn resolve_communication_target(
    backend: &impl AudioBackend,
    device_type: DeviceType,
    state: &PersistentState,
    main_target: Option<&DeviceId>,
) -> Option<DeviceId> {
    if device_type == DeviceType::Output
        && let Some(main_id) = main_target
        && let Some(settings) = state.device_settings(main_id)
    {
        match &settings.communication_route {
            CommunicationRoute::SameDevice => return Some(main_id.clone()),
            CommunicationRoute::Paired(paired_id) => {
                // A missing voice endpoint falls back to the chosen main output,
                // rather than leaving chat on an unrelated previous device.
                let paired_is_available = backend
                    .devices(DeviceType::Output)
                    .is_ok_and(|devices| devices.iter().any(|d| d.id() == paired_id))
                    && backend
                        .device_by_id(paired_id)
                        .is_ok_and(|d| d.is_active().unwrap_or(false));
                return Some(if paired_is_available {
                    paired_id.clone()
                } else {
                    main_id.clone()
                });
            }
            CommunicationRoute::Inherited => {}
        }
    }
    if state.switch_communication_device(device_type) {
        main_target.cloned()
    } else {
        None
    }
}

fn find_highest_priority_active_device(
    backend: &impl AudioBackend,
    priority_list: &[DeviceId],
) -> Option<DeviceId> {
    priority_list
        .iter()
        .find_map(|device_id| match backend.device_by_id(device_id) {
            Ok(device) => match device.is_active() {
                Ok(true) => Some(device_id.clone()),
                Ok(false) => None,
                Err(e) => {
                    log::warn!("Failed to check if device {device_id} is active: {e:#}");
                    None
                }
            },
            Err(e) => {
                log::warn!("Failed to get device {device_id} for priority check: {e:#}");
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
    fn enforce_priorities_switches_to_highest_active() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("dev_a", "Device A", true),
            MockDevice::new("dev_b", "Device B", true),
        ]);
        backend.set_default("dev_b", DeviceType::Output);

        let mut state = PersistentState::default();
        state.output.priority_list = vec!["dev_a".into(), "dev_b".into()];

        let mut times = NotificationThrottler::new();
        let temp = TemporaryPriorities {
            output: None,
            input: None,
        };

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
        let temp = TemporaryPriorities {
            output: None,
            input: None,
        };

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
        let temp = TemporaryPriorities {
            output: None,
            input: None,
        };

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
        let temp = TemporaryPriorities {
            output: Some("dev_temp".into()),
            input: None,
        };

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
        let temp = TemporaryPriorities {
            output: None,
            input: None,
        };

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
        state.output.switch_communication_device = true;

        let mut times = NotificationThrottler::new();
        let temp = TemporaryPriorities {
            output: None,
            input: None,
        };

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
    fn find_highest_returns_first_active() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("dev_a", "A", false),
            MockDevice::new("dev_b", "B", true),
            MockDevice::new("dev_c", "C", true),
        ]);
        let list = vec!["dev_a".into(), "dev_b".into(), "dev_c".into()];
        assert_eq!(
            find_highest_priority_active_device(&backend, &list),
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
        assert_eq!(find_highest_priority_active_device(&backend, &list), None);
    }

    #[test]
    fn find_highest_returns_none_for_empty_list() {
        let backend = MockAudioBackend::new(vec![]);
        let list: Vec<DeviceId> = vec![];
        assert_eq!(find_highest_priority_active_device(&backend, &list), None);
    }
}
