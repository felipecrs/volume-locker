mod persistence;

pub use persistence::{load_state, save_state};

use crate::types::DeviceSettings;
use crate::types::{DeviceId, DeviceType, PriorityKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Per-device-type preferences (one instance for output, one for input).
#[derive(Debug, Clone, Default)]
pub(crate) struct PerTypeSettings {
    pub priority_list: Vec<DeviceId>,
    /// Independent priority list for the Communications role. May contain
    /// normal device IDs as well as [`DeviceId::main_priority()`], which
    /// dynamically follows whatever device the main priority resolves to.
    pub communication_priority_list: Vec<DeviceId>,
    pub notify_on_priority_restore: bool,
    pub communication_notify_on_priority_restore: bool,
}

/// Flat serde representation for backward-compatible JSON serialization.
#[allow(clippy::struct_excessive_bools)]
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct PersistentStateFlat {
    devices: HashMap<DeviceId, DeviceSettings>,
    output_priority_list: Vec<DeviceId>,
    input_priority_list: Vec<DeviceId>,
    output_communication_priority_list: Vec<DeviceId>,
    input_communication_priority_list: Vec<DeviceId>,
    notify_on_priority_restore_output: bool,
    notify_on_priority_restore_input: bool,
    notify_on_priority_restore_output_communication: bool,
    notify_on_priority_restore_input_communication: bool,
    switch_communication_device_output: bool,
    switch_communication_device_input: bool,
    check_updates_on_launch: bool,
    notify_on_device_migration: bool,
}

impl Default for PersistentStateFlat {
    fn default() -> Self {
        let state = PersistentState::default();
        state.into()
    }
}

impl From<PersistentStateFlat> for PersistentState {
    fn from(flat: PersistentStateFlat) -> Self {
        let output_comm = if flat.output_communication_priority_list.is_empty()
            && flat.switch_communication_device_output
        {
            vec![DeviceId::main_priority()]
        } else {
            flat.output_communication_priority_list
        };

        let input_comm = if flat.input_communication_priority_list.is_empty()
            && flat.switch_communication_device_input
        {
            vec![DeviceId::main_priority()]
        } else {
            flat.input_communication_priority_list
        };

        Self {
            devices: flat.devices,
            output: PerTypeSettings {
                priority_list: flat.output_priority_list,
                communication_priority_list: output_comm,
                notify_on_priority_restore: flat.notify_on_priority_restore_output,
                communication_notify_on_priority_restore: flat
                    .notify_on_priority_restore_output_communication,
            },
            input: PerTypeSettings {
                priority_list: flat.input_priority_list,
                communication_priority_list: input_comm,
                notify_on_priority_restore: flat.notify_on_priority_restore_input,
                communication_notify_on_priority_restore: flat
                    .notify_on_priority_restore_input_communication,
            },
            check_updates_on_launch: flat.check_updates_on_launch,
            notify_on_device_migration: flat.notify_on_device_migration,
        }
    }
}

impl From<PersistentState> for PersistentStateFlat {
    fn from(state: PersistentState) -> Self {
        let switch_comm_out = state
            .output
            .communication_priority_list
            .iter()
            .any(|id| id.is_main_priority());
        let switch_comm_in = state
            .input
            .communication_priority_list
            .iter()
            .any(|id| id.is_main_priority());

        Self {
            devices: state.devices,
            output_priority_list: state.output.priority_list,
            input_priority_list: state.input.priority_list,
            output_communication_priority_list: state.output.communication_priority_list,
            input_communication_priority_list: state.input.communication_priority_list,
            notify_on_priority_restore_output: state.output.notify_on_priority_restore,
            notify_on_priority_restore_input: state.input.notify_on_priority_restore,
            notify_on_priority_restore_output_communication: state
                .output
                .communication_notify_on_priority_restore,
            notify_on_priority_restore_input_communication: state
                .input
                .communication_notify_on_priority_restore,
            switch_communication_device_output: switch_comm_out,
            switch_communication_device_input: switch_comm_in,
            check_updates_on_launch: state.check_updates_on_launch,
            notify_on_device_migration: state.notify_on_device_migration,
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
    pub notify_on_device_migration: bool,
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

    pub fn communication_priority_list(&self, device_type: DeviceType) -> &[DeviceId] {
        &self.per_type(device_type).communication_priority_list
    }

    pub fn communication_priority_list_mut(
        &mut self,
        device_type: DeviceType,
    ) -> &mut Vec<DeviceId> {
        &mut self.per_type_mut(device_type).communication_priority_list
    }

    pub fn priority_list_of(&self, device_type: DeviceType, kind: PriorityKind) -> &[DeviceId] {
        match kind {
            PriorityKind::Main => self.priority_list(device_type),
            PriorityKind::Communication => self.communication_priority_list(device_type),
        }
    }

    pub fn priority_list_of_mut(
        &mut self,
        device_type: DeviceType,
        kind: PriorityKind,
    ) -> &mut Vec<DeviceId> {
        match kind {
            PriorityKind::Main => self.priority_list_mut(device_type),
            PriorityKind::Communication => self.communication_priority_list_mut(device_type),
        }
    }

    pub fn notify_on_priority_restore_of(
        &self,
        device_type: DeviceType,
        kind: PriorityKind,
    ) -> bool {
        match kind {
            PriorityKind::Main => self.per_type(device_type).notify_on_priority_restore,
            PriorityKind::Communication => {
                self.per_type(device_type)
                    .communication_notify_on_priority_restore
            }
        }
    }

    pub fn set_notify_on_priority_restore_of(
        &mut self,
        device_type: DeviceType,
        kind: PriorityKind,
        value: bool,
    ) {
        match kind {
            PriorityKind::Main => self.per_type_mut(device_type).notify_on_priority_restore = value,
            PriorityKind::Communication => {
                self.per_type_mut(device_type)
                    .communication_notify_on_priority_restore = value;
            }
        }
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
        if device_id.is_main_priority() {
            return;
        }
        let is_prunable = self.devices.get(device_id).is_some_and(|s| {
            !s.has_active_locks_or_notifications() && s.presence_source.is_windows()
        });
        if !is_prunable {
            return;
        }
        let in_priority = [&self.output, &self.input].iter().any(|s| {
            s.priority_list.contains(device_id) || s.communication_priority_list.contains(device_id)
        });
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
                communication_priority_list: vec![DeviceId::main_priority()],
                ..PerTypeSettings::default()
            },
            input: PerTypeSettings {
                communication_priority_list: vec![DeviceId::main_priority()],
                ..PerTypeSettings::default()
            },
            check_updates_on_launch: true,
            notify_on_device_migration: false,
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
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
        assert_eq!(
            state.output.communication_priority_list,
            vec![DeviceId::main_priority()]
        );
        assert_eq!(
            state.input.communication_priority_list,
            vec![DeviceId::main_priority()]
        );
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
                    presence_source: Default::default(),
                    fingerprint: None,
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
        assert!(!state.notify_on_device_migration);
    }

    #[test]
    fn communication_priority_list_roundtrips() {
        let state = PersistentState {
            output: PerTypeSettings {
                priority_list: vec!["speakers".into()],
                communication_priority_list: vec!["headset".into()],
                ..PerTypeSettings::default()
            },
            input: PerTypeSettings {
                communication_priority_list: vec!["mic_usb".into()],
                ..PerTypeSettings::default()
            },
            ..Default::default()
        };

        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("output_communication_priority_list"));

        let loaded: PersistentState = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.output.priority_list, vec!["speakers"]);
        assert_eq!(loaded.output.communication_priority_list, vec!["headset"]);
        assert_eq!(loaded.input.communication_priority_list, vec!["mic_usb"]);
    }

    #[test]
    fn legacy_config_without_communication_lists_still_loads() {
        // A state file written by a version predating the feature.
        let json = r#"{
            "devices": {},
            "output_priority_list": ["speakers"],
            "switch_communication_device_output": true
        }"#;
        let state: PersistentState = serde_json::from_str(json).unwrap();
        assert_eq!(state.output.priority_list, vec!["speakers"]);
        assert_eq!(
            state.output.communication_priority_list,
            vec![DeviceId::main_priority()]
        );
    }

    #[test]
    fn priority_list_of_selects_the_requested_list() {
        let mut state = PersistentState::default();
        state
            .priority_list_of_mut(DeviceType::Output, PriorityKind::Main)
            .push("main".into());
        *state.priority_list_of_mut(DeviceType::Output, PriorityKind::Communication) =
            vec!["comm".into()];
        state
            .priority_list_of_mut(DeviceType::Input, PriorityKind::Communication)
            .clear();

        assert_eq!(
            state.priority_list_of(DeviceType::Output, PriorityKind::Main),
            &["main"]
        );
        assert_eq!(
            state.priority_list_of(DeviceType::Output, PriorityKind::Communication),
            &["comm"]
        );
        assert!(
            state
                .priority_list_of(DeviceType::Input, PriorityKind::Communication)
                .is_empty()
        );
    }

    #[test]
    fn device_kept_while_referenced_by_communication_list() {
        let mut state = PersistentState::default();
        state.devices.insert(
            "dev1".into(),
            DeviceSettings::new("Headset".into(), DeviceType::Output),
        );
        state
            .communication_priority_list_mut(DeviceType::Output)
            .push("dev1".into());

        state.remove_device_if_unused(&"dev1".into());
        assert!(state.devices.contains_key("dev1"));

        state
            .communication_priority_list_mut(DeviceType::Output)
            .clear();
        state.remove_device_if_unused(&"dev1".into());
        assert!(!state.devices.contains_key("dev1"));
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
        assert!(!state.notify_on_priority_restore_of(DeviceType::Output, PriorityKind::Main));
        assert!(
            !state.notify_on_priority_restore_of(DeviceType::Output, PriorityKind::Communication)
        );
        state.set_notify_on_priority_restore_of(DeviceType::Output, PriorityKind::Main, true);
        assert!(state.notify_on_priority_restore_of(DeviceType::Output, PriorityKind::Main));
        assert!(
            !state.notify_on_priority_restore_of(DeviceType::Output, PriorityKind::Communication)
        );
        assert!(!state.notify_on_priority_restore_of(DeviceType::Input, PriorityKind::Main));
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
                communication_priority_list: vec![],
                ..PerTypeSettings::default()
            },
            check_updates_on_launch: false,
            notify_on_device_migration: true,
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
                    presence_source: Default::default(),
                    fingerprint: None,
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
        assert!(loaded.input.communication_priority_list.is_empty());
        assert!(!loaded.check_updates_on_launch);
        assert!(loaded.notify_on_device_migration);

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
