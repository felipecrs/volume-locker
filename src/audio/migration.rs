use crate::config::PersistentState;
use crate::types::{DeviceId, DeviceSettings, DeviceType};

use super::AudioBackend;

pub fn migrate_device_ids(
    backend: &impl AudioBackend,
    persistent_state: &mut PersistentState,
) -> bool {
    let mut devices_to_migrate: Vec<(DeviceId, DeviceSettings)> = Vec::new();
    let mut devices_to_update: Vec<(DeviceId, DeviceSettings)> = Vec::new();

    // Check which devices need migration
    for (device_id, device_settings) in persistent_state.devices.iter() {
        if let Ok(device) = backend.get_device_by_id(device_id) {
            // Device exists, check if name has changed
            let current_name = device.name();
            if current_name != device_settings.name {
                log::info!(
                    "Device {} with ID {} had the name changed to {}",
                    device_settings.name,
                    device_id,
                    current_name,
                );
                let mut updated_settings = device_settings.clone();
                updated_settings.name = current_name;
                devices_to_update.push((device_id.clone(), updated_settings));
            }
        } else {
            devices_to_migrate.push((device_id.clone(), device_settings.clone()));
        }
    }

    // Check if any migrations will occur
    let migrations_occurred = !devices_to_update.is_empty() || !devices_to_migrate.is_empty();

    // Apply the name updates
    for (device_id, updated_settings) in devices_to_update {
        persistent_state.devices.insert(device_id, updated_settings);
    }

    // Attempt to migrate each device
    for (old_device_id, device_settings) in devices_to_migrate {
        let device_name = device_settings.name.clone();
        if let Ok(new_device_id) =
            find_device_by_name_and_type(backend, &device_name, device_settings.device_type)
        {
            // Swap the old device with the new one
            persistent_state.devices.remove(&old_device_id);
            persistent_state
                .devices
                .insert(new_device_id.clone(), device_settings.clone());

            // Update priority lists
            let priority_list = match device_settings.device_type {
                DeviceType::Output => &mut persistent_state.output_priority_list,
                DeviceType::Input => &mut persistent_state.input_priority_list,
            };

            if let Some(pos) = priority_list.iter().position(|id| id == &old_device_id) {
                priority_list[pos] = new_device_id.clone();
            }

            log::info!("Migrated device {device_name} from ID {old_device_id} to {new_device_id}");
        } else {
            log::warn!(
                "Device {device_name} with ID {old_device_id} could not be found, keeping it in case it returns"
            );
        }
    }

    migrations_occurred
}

pub(crate) fn find_device_by_name_and_type(
    backend: &impl AudioBackend,
    target_name: &str,
    device_type: DeviceType,
) -> anyhow::Result<DeviceId> {
    let devices = backend.get_devices(device_type)?;
    for device in devices {
        if device.name() == target_name {
            return Ok(DeviceId::new(device.id()));
        }
    }
    anyhow::bail!("Device not found: {target_name}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::tests::{MockAudioBackend, MockDevice, make_device_settings};

    #[test]
    fn migrate_no_changes_when_devices_match() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("id1", "Speakers", true),
            MockDevice::new("id2", "Headphones", true),
        ]);
        let mut state = PersistentState::default();
        state.devices.insert(
            "id1".into(),
            make_device_settings("Speakers", DeviceType::Output),
        );
        state.devices.insert(
            "id2".into(),
            make_device_settings("Headphones", DeviceType::Output),
        );

        let changed = migrate_device_ids(&backend, &mut state);
        assert!(!changed);
        assert!(state.devices.contains_key("id1"));
        assert!(state.devices.contains_key("id2"));
    }

    #[test]
    fn migrate_updates_name_when_device_renamed() {
        let backend = MockAudioBackend::new(vec![MockDevice::new("id1", "New Speaker Name", true)]);
        let mut state = PersistentState::default();
        state.devices.insert(
            "id1".into(),
            make_device_settings("Old Speaker Name", DeviceType::Output),
        );

        let changed = migrate_device_ids(&backend, &mut state);
        assert!(changed);
        assert_eq!(state.devices["id1"].name, "New Speaker Name");
    }

    #[test]
    fn migrate_moves_device_when_id_changes() {
        let backend = MockAudioBackend::new(vec![MockDevice::new("id_new", "Speakers", true)]);
        let mut state = PersistentState::default();
        state.devices.insert(
            "id_old".into(),
            make_device_settings("Speakers", DeviceType::Output),
        );
        state.output_priority_list = vec!["id_old".into()];

        let changed = migrate_device_ids(&backend, &mut state);
        assert!(changed);
        assert!(!state.devices.contains_key("id_old"));
        assert!(state.devices.contains_key("id_new"));
        assert_eq!(state.output_priority_list, vec!["id_new".to_string()]);
    }

    #[test]
    fn migrate_keeps_missing_device_when_no_match() {
        let backend = MockAudioBackend::new(vec![MockDevice::new("id_other", "Microphone", true)]);
        let mut state = PersistentState::default();
        state.devices.insert(
            "id_gone".into(),
            make_device_settings("Speakers", DeviceType::Output),
        );

        let changed = migrate_device_ids(&backend, &mut state);
        assert!(changed);
        assert!(state.devices.contains_key("id_gone"));
    }

    #[test]
    fn migrate_updates_input_priority_list() {
        let backend = MockAudioBackend::new(vec![MockDevice::new("mic_new", "Microphone", true)]);
        let mut state = PersistentState::default();
        state.devices.insert(
            "mic_old".into(),
            make_device_settings("Microphone", DeviceType::Input),
        );
        state.input_priority_list = vec!["mic_old".into()];

        let changed = migrate_device_ids(&backend, &mut state);
        assert!(changed);
        assert!(state.devices.contains_key("mic_new"));
        assert_eq!(state.input_priority_list, vec!["mic_new"]);
    }

    #[test]
    fn find_device_by_name_found() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("id1", "Speakers", true),
            MockDevice::new("id2", "Headphones", true),
        ]);
        let result = find_device_by_name_and_type(&backend, "Headphones", DeviceType::Output);
        assert_eq!(result.unwrap(), "id2");
    }

    #[test]
    fn find_device_by_name_not_found() {
        let backend = MockAudioBackend::new(vec![MockDevice::new("id1", "Speakers", true)]);
        let result = find_device_by_name_and_type(&backend, "Microphone", DeviceType::Output);
        assert!(result.is_err());
    }
}
