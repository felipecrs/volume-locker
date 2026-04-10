use crate::consts::STATE_FILE_NAME;
use crate::types::DeviceSettings;
use crate::types::{DeviceId, DeviceType};
use crate::utils::get_executable_directory;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PersistentState {
    pub devices: HashMap<DeviceId, DeviceSettings>,
    pub output_priority_list: Vec<DeviceId>,
    pub input_priority_list: Vec<DeviceId>,
    pub notify_on_priority_restore_output: bool,
    pub notify_on_priority_restore_input: bool,
    pub switch_communication_device_output: bool,
    pub switch_communication_device_input: bool,
    pub check_updates_on_launch: bool,
}

impl PersistentState {
    pub fn get_priority_list_mut(&mut self, device_type: DeviceType) -> &mut Vec<DeviceId> {
        match device_type {
            DeviceType::Output => &mut self.output_priority_list,
            DeviceType::Input => &mut self.input_priority_list,
        }
    }

    pub fn get_priority_list(&self, device_type: DeviceType) -> &[DeviceId] {
        match device_type {
            DeviceType::Output => &self.output_priority_list,
            DeviceType::Input => &self.input_priority_list,
        }
    }

    pub fn set_notify_on_priority_restore(&mut self, device_type: DeviceType, notify: bool) {
        match device_type {
            DeviceType::Output => self.notify_on_priority_restore_output = notify,
            DeviceType::Input => self.notify_on_priority_restore_input = notify,
        }
    }

    pub fn get_notify_on_priority_restore(&self, device_type: DeviceType) -> bool {
        match device_type {
            DeviceType::Output => self.notify_on_priority_restore_output,
            DeviceType::Input => self.notify_on_priority_restore_input,
        }
    }

    pub fn set_switch_communication_device(&mut self, device_type: DeviceType, switch: bool) {
        match device_type {
            DeviceType::Output => self.switch_communication_device_output = switch,
            DeviceType::Input => self.switch_communication_device_input = switch,
        }
    }

    pub fn get_switch_communication_device(&self, device_type: DeviceType) -> bool {
        match device_type {
            DeviceType::Output => self.switch_communication_device_output,
            DeviceType::Input => self.switch_communication_device_input,
        }
    }
}

impl Default for PersistentState {
    fn default() -> Self {
        Self {
            devices: HashMap::default(),
            output_priority_list: Vec::default(),
            input_priority_list: Vec::default(),
            notify_on_priority_restore_output: false,
            notify_on_priority_restore_input: false,
            switch_communication_device_output: true,
            switch_communication_device_input: true,
            check_updates_on_launch: true,
        }
    }
}

fn get_state_file_path() -> anyhow::Result<PathBuf> {
    Ok(get_executable_directory()?.join(STATE_FILE_NAME))
}

pub fn save_state(state: &PersistentState) -> anyhow::Result<()> {
    let path = get_state_file_path()?;
    let tmp_path = path.with_extension("json.tmp");

    let json = serde_json::to_string_pretty(state).context("failed to serialize state")?;

    // Write to a temporary file first, then atomically rename to the target.
    // This prevents corruption if the process is interrupted mid-write.
    fs::write(&tmp_path, &json).with_context(|| {
        format!(
            "failed to write temporary state file '{}'",
            tmp_path.display()
        )
    })?;

    if let Err(e) = fs::rename(&tmp_path, &path) {
        // Try to clean up the temporary file
        let _ = fs::remove_file(&tmp_path);
        return Err(anyhow::anyhow!(e).context(format!(
            "failed to rename temporary state file '{}' to '{}'",
            tmp_path.display(),
            path.display()
        )));
    }

    Ok(())
}

pub fn load_state() -> anyhow::Result<PersistentState> {
    let path = get_state_file_path()?;

    let data = match fs::read_to_string(&path) {
        Ok(data) => data,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // First run or file was deleted intentionally — use defaults
            return Ok(PersistentState::default());
        }
        Err(e) => {
            return Err(anyhow::anyhow!(e))
                .with_context(|| format!("Failed to read state file '{}'", path.display()));
        }
    };

    serde_json::from_str(&data)
        .with_context(|| format!("Failed to parse state file '{}'", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::VolumePercent;
    use crate::types::{UnmuteLockPolicy, VolumeLockPolicy};

    #[test]
    fn persistent_state_default_values() {
        let state = PersistentState::default();
        assert!(state.devices.is_empty());
        assert!(state.output_priority_list.is_empty());
        assert!(state.input_priority_list.is_empty());
        assert!(!state.notify_on_priority_restore_output);
        assert!(!state.notify_on_priority_restore_input);
        assert!(state.switch_communication_device_output);
        assert!(state.switch_communication_device_input);
        assert!(state.check_updates_on_launch);
    }

    #[test]
    fn persistent_state_serialization_roundtrip() {
        let state = PersistentState {
            output_priority_list: vec!["device_a".into(), "device_b".into()],
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

        assert_eq!(loaded.output_priority_list, vec!["device_a", "device_b"]);
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
        assert!(state.output_priority_list.is_empty());
        assert!(state.check_updates_on_launch);
    }

    #[test]
    fn get_priority_list_returns_correct_type() {
        let state = PersistentState {
            output_priority_list: vec!["out1".into()],
            input_priority_list: vec!["in1".into(), "in2".into()],
            ..Default::default()
        };

        assert_eq!(state.get_priority_list(DeviceType::Output), &["out1"]);
        assert_eq!(state.get_priority_list(DeviceType::Input), &["in1", "in2"]);
    }

    #[test]
    fn get_priority_list_mut_modifies_correct_type() {
        let mut state = PersistentState::default();
        state
            .get_priority_list_mut(DeviceType::Output)
            .push("new_out".into());
        state
            .get_priority_list_mut(DeviceType::Input)
            .push("new_in".into());

        assert_eq!(state.output_priority_list, vec!["new_out"]);
        assert_eq!(state.input_priority_list, vec!["new_in"]);
    }

    #[test]
    fn notify_on_priority_restore_accessors() {
        let mut state = PersistentState::default();
        assert!(!state.get_notify_on_priority_restore(DeviceType::Output));
        state.set_notify_on_priority_restore(DeviceType::Output, true);
        assert!(state.get_notify_on_priority_restore(DeviceType::Output));
        assert!(!state.get_notify_on_priority_restore(DeviceType::Input));
    }

    #[test]
    fn switch_communication_device_accessors() {
        let mut state = PersistentState::default();
        assert!(state.get_switch_communication_device(DeviceType::Input));
        state.set_switch_communication_device(DeviceType::Input, false);
        assert!(!state.get_switch_communication_device(DeviceType::Input));
        assert!(state.get_switch_communication_device(DeviceType::Output));
    }

    #[test]
    fn file_roundtrip_preserves_state() {
        let dir = std::env::temp_dir().join("volume_locker_test_roundtrip");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test_state.json");

        // Build a non-trivial state
        let state = PersistentState {
            output_priority_list: vec!["dev_a".into(), "dev_b".into()],
            input_priority_list: vec!["mic_1".into()],
            notify_on_priority_restore_output: true,
            switch_communication_device_input: false,
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
            ..Default::default()
        };

        // Write to file
        let json = serde_json::to_string_pretty(&state).unwrap();
        fs::write(&path, &json).unwrap();

        // Read back and verify
        let data = fs::read_to_string(&path).unwrap();
        let loaded: PersistentState = serde_json::from_str(&data).unwrap();

        assert_eq!(loaded.output_priority_list, vec!["dev_a", "dev_b"]);
        assert_eq!(loaded.input_priority_list, vec!["mic_1"]);
        assert!(loaded.notify_on_priority_restore_output);
        assert!(!loaded.switch_communication_device_input);
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
        loaded.output_priority_list.push("dev1".into());

        let json2 = serde_json::to_string_pretty(&loaded).unwrap();
        fs::write(&path, &json2).unwrap();

        // Reload and verify modifications persisted
        let data2 = fs::read_to_string(&path).unwrap();
        let final_state: PersistentState = serde_json::from_str(&data2).unwrap();
        let dev = final_state.devices.get("dev1").unwrap();
        assert!(dev.volume_lock.is_locked);
        assert_eq!(dev.volume_lock.target_percent, 75.0);
        assert_eq!(final_state.output_priority_list, vec!["dev1"]);

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
