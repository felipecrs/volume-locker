use crate::config::PersistentState;
use crate::types::{DeviceId, DeviceRole, DeviceType, TemporaryPriorities};
use crate::utils::send_notification_debounced;
use std::collections::HashMap;
use std::time::Instant;

use super::AudioBackend;

pub fn enforce_priorities(
    backend: &impl AudioBackend,
    state: &PersistentState,
    last_notification_times: &mut HashMap<String, Instant>,
    temporary_priorities: &TemporaryPriorities,
) {
    for device_type in [DeviceType::Output, DeviceType::Input] {
        enforce_priority_for_type(
            backend,
            state,
            device_type,
            temporary_priorities.get(device_type),
            last_notification_times,
        );
    }
}

fn enforce_priority_for_type(
    backend: &impl AudioBackend,
    state: &PersistentState,
    device_type: DeviceType,
    temporary_priority: Option<&DeviceId>,
    last_notification_times: &mut HashMap<String, Instant>,
) {
    let mut priority_list = state.get_priority_list(device_type).to_vec();
    if let Some(temp_id) = temporary_priority {
        priority_list.insert(0, temp_id.clone());
    }

    let Some(target_id) = find_highest_priority_active_device(backend, &priority_list) else {
        return;
    };

    let mut switched = false;

    let type_str = match device_type {
        DeviceType::Output => "output",
        DeviceType::Input => "input",
    };

    // Enforce Console and Multimedia roles together
    let is_console_correct = match backend.get_default_device(device_type, DeviceRole::Console) {
        Ok(d) => target_id == d.id(),
        Err(e) => {
            log::warn!("Failed to get default {type_str} console device: {e:#}");
            false
        }
    };

    if !is_console_correct {
        log::info!("Enforcing {type_str} priority: Switching to {target_id}");
        for role in [DeviceRole::Console, DeviceRole::Multimedia] {
            if let Err(e) = backend.set_default_device(&target_id, role) {
                log::error!(
                    "Failed to set default {role:?} {type_str} device to {target_id}: {e:#}"
                );
            }
        }
        switched = true;
    }

    // Enforce Communications role if enabled
    if state.get_switch_communication_device(device_type) {
        let is_comm_correct =
            match backend.get_default_device(device_type, DeviceRole::Communications) {
                Ok(d) => target_id == d.id(),
                Err(e) => {
                    log::warn!("Failed to get default {type_str} communications device: {e:#}");
                    false
                }
            };

        if !is_comm_correct {
            log::info!("Enforcing {type_str} priority (Communication): Switching to {target_id}");
            if let Err(e) = backend.set_default_device(&target_id, DeviceRole::Communications) {
                log::error!(
                    "Failed to set default {type_str} communications device to {target_id}: {e:#}"
                );
            }
            switched = true;
        }
    }

    if switched && state.get_notify_on_priority_restore(device_type) {
        let device_name = backend
            .get_device_by_id(&target_id)
            .map(|d| d.name())
            .unwrap_or_else(|_| "Unknown Device".to_string());
        let title = match device_type {
            DeviceType::Output => "Default Output Device Restored",
            DeviceType::Input => "Default Input Device Restored",
        };
        send_notification_debounced(
            &format!("priority_restore_{}", target_id),
            title,
            &format!("Switched to {} based on priority list.", device_name),
            last_notification_times,
        );
    }
}

fn find_highest_priority_active_device(
    backend: &impl AudioBackend,
    priority_list: &[DeviceId],
) -> Option<DeviceId> {
    priority_list
        .iter()
        .find_map(|device_id| match backend.get_device_by_id(device_id) {
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

    #[test]
    fn enforce_priorities_switches_to_highest_active() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("dev_a", "Device A", true),
            MockDevice::new("dev_b", "Device B", true),
        ]);
        backend.set_default("dev_b", DeviceType::Output);

        let state = PersistentState {
            output_priority_list: vec!["dev_a".into(), "dev_b".into()],
            ..Default::default()
        };

        let mut times = HashMap::new();
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

        let state = PersistentState {
            output_priority_list: vec!["dev_a".into()],
            ..Default::default()
        };

        let mut times = HashMap::new();
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

        let state = PersistentState {
            output_priority_list: vec!["dev_a".into(), "dev_b".into()],
            ..Default::default()
        };

        let mut times = HashMap::new();
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

        let state = PersistentState {
            output_priority_list: vec!["dev_a".into()],
            ..Default::default()
        };

        let mut times = HashMap::new();
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

        let mut times = HashMap::new();
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

        let state = PersistentState {
            output_priority_list: vec!["dev_a".into(), "dev_b".into()],
            switch_communication_device_output: true,
            ..Default::default()
        };

        let mut times = HashMap::new();
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
