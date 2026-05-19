mod persistence;

pub use persistence::{load_state, save_state};

use crate::types::DeviceSettings;
use crate::types::{DeviceId, DeviceType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Per-device-type preferences (one instance for output, one for input).
#[derive(Debug, Clone, Default)]
pub(crate) struct PerTypeSettings {
    pub priority_list: Vec<DeviceId>,
    pub notify_on_priority_restore: bool,
    pub switch_communication_device: bool,
}

/// Flat serde representation for backward-compatible JSON serialization.
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct PersistentStateFlat {
    devices: HashMap<DeviceId, DeviceSettings>,
    output_priority_list: Vec<DeviceId>,
    input_priority_list: Vec<DeviceId>,
    notify_on_priority_restore_output: bool,
    notify_on_priority_restore_input: bool,
    switch_communication_device_output: bool,
    switch_communication_device_input: bool,
    check_updates_on_launch: bool,
}

impl Default for PersistentStateFlat {
    fn default() -> Self {
        let state = PersistentState::default();
        state.into()
    }
}

impl From<PersistentStateFlat> for PersistentState {
    fn from(flat: PersistentStateFlat) -> Self {
        Self {
            devices: flat.devices,
            output: PerTypeSettings {
                priority_list: flat.output_priority_list,
                notify_on_priority_restore: flat.notify_on_priority_restore_output,
                switch_communication_device: flat.switch_communication_device_output,
            },
            input: PerTypeSettings {
                priority_list: flat.input_priority_list,
                notify_on_priority_restore: flat.notify_on_priority_restore_input,
                switch_communication_device: flat.switch_communication_device_input,
            },
            check_updates_on_launch: flat.check_updates_on_launch,
        }
    }
}

impl From<PersistentState> for PersistentStateFlat {
    fn from(state: PersistentState) -> Self {
        Self {
            devices: state.devices,
            output_priority_list: state.output.priority_list,
            input_priority_list: state.input.priority_list,
            notify_on_priority_restore_output: state.output.notify_on_priority_restore,
            notify_on_priority_restore_input: state.input.notify_on_priority_restore,
            switch_communication_device_output: state.output.switch_communication_device,
            switch_communication_device_input: state.input.switch_communication_device,
            check_updates_on_launch: state.check_updates_on_launch,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "PersistentStateFlat", into = "PersistentStateFlat")]
pub struct PersistentState {
    pub(crate) devices: HashMap<DeviceId, DeviceSettings>,
    pub(crate) output: PerTypeSettings,
    pub(crate) input: PerTypeSettings,
    pub check_updates_on_launch: bool,
}

impl PersistentState {
    fn per_type(&self, dt: DeviceType) -> &PerTypeSettings {
        match dt {
            DeviceType::Output => &self.output,
            DeviceType::Input => &self.input,
        }
    }

    fn per_type_mut(&mut self, dt: DeviceType) -> &mut PerTypeSettings {
        match dt {
            DeviceType::Output => &mut self.output,
            DeviceType::Input => &mut self.input,
        }
    }

    pub fn priority_list(&self, device_type: DeviceType) -> &[DeviceId] {
        &self.per_type(device_type).priority_list
    }

    pub fn priority_list_mut(&mut self, device_type: DeviceType) -> &mut Vec<DeviceId> {
        &mut self.per_type_mut(device_type).priority_list
    }

    pub fn notify_on_priority_restore(&self, device_type: DeviceType) -> bool {
        self.per_type(device_type).notify_on_priority_restore
    }

    pub fn set_notify_on_priority_restore(&mut self, device_type: DeviceType, value: bool) {
        self.per_type_mut(device_type).notify_on_priority_restore = value;
    }

    pub fn switch_communication_device(&self, device_type: DeviceType) -> bool {
        self.per_type(device_type).switch_communication_device
    }

    pub fn set_switch_communication_device(&mut self, device_type: DeviceType, value: bool) {
        self.per_type_mut(device_type).switch_communication_device = value;
    }

    pub fn device_settings(&self, device_id: &DeviceId) -> Option<&DeviceSettings> {
        self.devices.get(device_id)
    }

    pub fn device_settings_mut(&mut self, device_id: &DeviceId) -> Option<&mut DeviceSettings> {
        self.devices.get_mut(device_id)
    }

    pub fn ensure_device_settings(
        &mut self,
        device_id: DeviceId,
        name: String,
        device_type: DeviceType,
    ) -> &mut DeviceSettings {
        self.devices
            .entry(device_id)
            .or_insert_with(|| DeviceSettings::new(name, device_type))
    }

    pub fn insert_device(&mut self, device_id: DeviceId, settings: DeviceSettings) {
        self.devices.insert(device_id, settings);
    }

    pub fn remove_device(&mut self, device_id: &DeviceId) {
        self.devices.remove(device_id);
    }

    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    pub fn locked_device_ids(&self) -> Vec<DeviceId> {
        self.devices
            .iter()
            .filter(|(_, s)| s.volume_lock.is_locked || s.unmute_lock.is_locked)
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub fn devices_iter(&self) -> impl Iterator<Item = (&DeviceId, &DeviceSettings)> {
        self.devices.iter()
    }

    /// Removes a device's settings entry if it has no active locks/notifications
    /// and is not referenced by any priority list.
    pub fn remove_device_if_unused(&mut self, device_id: &DeviceId) {
        let is_prunable = self
            .devices
            .get(device_id)
            .is_some_and(|s| !s.has_active_locks_or_notifications());
        if !is_prunable {
            return;
        }
        let in_priority = self.output.priority_list.contains(device_id)
            || self.input.priority_list.contains(device_id);
        if !in_priority {
            self.devices.remove(device_id);
        }
    }
}

impl Default for PersistentState {
    fn default() -> Self {
        Self {
            devices: HashMap::default(),
            output: PerTypeSettings {
                switch_communication_device: true,
                ..PerTypeSettings::default()
            },
            input: PerTypeSettings {
                switch_communication_device: true,
                ..PerTypeSettings::default()
            },
            check_updates_on_launch: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consts::STATE_FILE_NAME;
    use crate::types::VolumePercent;
    use crate::types::{UnmuteLockPolicy, VolumeLockPolicy};
    use std::fs;

    #[test]
    fn persistent_state_default_values() {
        let state = PersistentState::default();
        assert!(state.devices.is_empty());
        assert!(state.output.priority_list.is_empty());
        assert!(state.input.priority_list.is_empty());
        assert!(!state.output.notify_on_priority_restore);
        assert!(!state.input.notify_on_priority_restore);
        assert!(state.output.switch_communication_device);
        assert!(state.input.switch_communication_device);
        assert!(state.check_updates_on_launch);
    }

    #[test]
    fn persistent_state_serialization_roundtrip() {
        let state = PersistentState {
            output: PerTypeSettings {
                priority_list: vec!["device_a".into(), "device_b".into()],
                ..PerTypeSettings::default()
            },
            check_updates_on_launch: false,
            devices: HashMap::from([(
                "test_id".into(),
                DeviceSettings {
                    volume_lock: VolumeLockPolicy {
                        is_locked: true,
                        target_percent: VolumePercent::from(75.0),
                        notify: true,
                    },
                    unmute_lock: UnmuteLockPolicy::default(),
                    device_type: DeviceType::Output,
                    name: "Test Device".into(),
                },
            )]),
            ..Default::default()
        };

        let json = serde_json::to_string_pretty(&state).unwrap();
        let loaded: PersistentState = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.output.priority_list, vec!["device_a", "device_b"]);
        assert!(!loaded.check_updates_on_launch);
        let dev = loaded.devices.get("test_id").unwrap();
        assert!(dev.volume_lock.is_locked);
        assert_eq!(dev.volume_lock.target_percent, 75.0);
        assert_eq!(dev.name, "Test Device");
    }

    #[test]
    fn persistent_state_deserialize_missing_fields_uses_defaults() {
        let json = r#"{"devices": {}}"#;
        let state: PersistentState = serde_json::from_str(json).unwrap();
        assert!(state.output.priority_list.is_empty());
        assert!(state.check_updates_on_launch);
    }

    #[test]
    fn get_priority_list_returns_correct_type() {
        let state = PersistentState {
            output: PerTypeSettings {
                priority_list: vec!["out1".into()],
                ..PerTypeSettings::default()
            },
            input: PerTypeSettings {
                priority_list: vec!["in1".into(), "in2".into()],
                ..PerTypeSettings::default()
            },
            ..Default::default()
        };

        assert_eq!(state.priority_list(DeviceType::Output), &["out1"]);
        assert_eq!(state.priority_list(DeviceType::Input), &["in1", "in2"]);
    }

    #[test]
    fn get_priority_list_mut_modifies_correct_type() {
        let mut state = PersistentState::default();
        state
            .priority_list_mut(DeviceType::Output)
            .push("new_out".into());
        state
            .priority_list_mut(DeviceType::Input)
            .push("new_in".into());

        assert_eq!(state.output.priority_list, vec!["new_out"]);
        assert_eq!(state.input.priority_list, vec!["new_in"]);
    }

    #[test]
    fn notify_on_priority_restore_accessors() {
        let mut state = PersistentState::default();
        assert!(!state.notify_on_priority_restore(DeviceType::Output));
        state.set_notify_on_priority_restore(DeviceType::Output, true);
        assert!(state.notify_on_priority_restore(DeviceType::Output));
        assert!(!state.notify_on_priority_restore(DeviceType::Input));
    }

    #[test]
    fn switch_communication_device_accessors() {
        let mut state = PersistentState::default();
        assert!(state.switch_communication_device(DeviceType::Input));
        state.set_switch_communication_device(DeviceType::Input, false);
        assert!(!state.switch_communication_device(DeviceType::Input));
        assert!(state.switch_communication_device(DeviceType::Output));
    }

    #[test]
    fn file_roundtrip_preserves_state() {
        let dir = std::env::temp_dir().join("volume_locker_test_roundtrip");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test_state.json");

        // Build a non-trivial state
        let state = PersistentState {
            output: PerTypeSettings {
                priority_list: vec!["dev_a".into(), "dev_b".into()],
                notify_on_priority_restore: true,
                ..PerTypeSettings::default()
            },
            input: PerTypeSettings {
                priority_list: vec!["mic_1".into()],
                switch_communication_device: false,
                ..PerTypeSettings::default()
            },
            check_updates_on_launch: false,
            devices: HashMap::from([(
                "dev_a".into(),
                DeviceSettings {
                    volume_lock: VolumeLockPolicy {
                        is_locked: true,
                        target_percent: VolumePercent::from(80.0),
                        notify: true,
                    },
                    unmute_lock: UnmuteLockPolicy {
                        is_locked: true,
                        notify: false,
                    },
                    device_type: DeviceType::Output,
                    name: "Speakers".into(),
                },
            )]),
        };

        // Write to file
        let json = serde_json::to_string_pretty(&state).unwrap();
        fs::write(&path, &json).unwrap();

        // Read back and verify
        let data = fs::read_to_string(&path).unwrap();
        let loaded: PersistentState = serde_json::from_str(&data).unwrap();

        assert_eq!(loaded.output.priority_list, vec!["dev_a", "dev_b"]);
        assert_eq!(loaded.input.priority_list, vec!["mic_1"]);
        assert!(loaded.output.notify_on_priority_restore);
        assert!(!loaded.input.switch_communication_device);
        assert!(!loaded.check_updates_on_launch);

        let dev = loaded.devices.get("dev_a").unwrap();
        assert!(dev.volume_lock.is_locked);
        assert_eq!(dev.volume_lock.target_percent, 80.0);
        assert!(dev.volume_lock.notify);
        assert!(dev.unmute_lock.is_locked);
        assert_eq!(dev.name, "Speakers");

        // Cleanup
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn file_roundtrip_modify_and_reload() {
        let dir = std::env::temp_dir().join("volume_locker_test_modify");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test_state_modify.json");

        // Write initial state
        let mut state = PersistentState::default();
        state.devices.insert(
            "dev1".into(),
            DeviceSettings {
                volume_lock: VolumeLockPolicy {
                    target_percent: VolumePercent::from(50.0),
                    ..VolumeLockPolicy::default()
                },
                ..DeviceSettings::new("Initial Device".into(), DeviceType::Output)
            },
        );
        let json = serde_json::to_string_pretty(&state).unwrap();
        fs::write(&path, &json).unwrap();

        // Load, modify, save again
        let data = fs::read_to_string(&path).unwrap();
        let mut loaded: PersistentState = serde_json::from_str(&data).unwrap();
        loaded
            .devices
            .get_mut("dev1")
            .unwrap()
            .volume_lock
            .is_locked = true;
        loaded
            .devices
            .get_mut("dev1")
            .unwrap()
            .volume_lock
            .target_percent = VolumePercent::from(75.0);
        loaded.output.priority_list.push("dev1".into());

        let json2 = serde_json::to_string_pretty(&loaded).unwrap();
        fs::write(&path, &json2).unwrap();

        // Reload and verify modifications persisted
        let data2 = fs::read_to_string(&path).unwrap();
        let final_state: PersistentState = serde_json::from_str(&data2).unwrap();
        let dev = final_state.devices.get("dev1").unwrap();
        assert!(dev.volume_lock.is_locked);
        assert_eq!(dev.volume_lock.target_percent, 75.0);
        assert_eq!(final_state.output.priority_list, vec!["dev1"]);

        // Cleanup
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn load_state_malformed_json_returns_error() {
        let dir = std::env::temp_dir().join("volume_locker_test_malformed");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join(STATE_FILE_NAME);

        fs::write(&path, "{ this is not valid json }").unwrap();

        let result: Result<PersistentState, _> =
            serde_json::from_str(&fs::read_to_string(&path).unwrap());
        assert!(result.is_err());

        // Cleanup
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }
}
