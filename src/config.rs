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
#[allow(clippy::struct_excessive_bools)]
pub struct PersistentState {
    pub devices: HashMap<DeviceId, DeviceSettings>,
    #[serde(rename = "output_priority_list")]
    pub(crate) output_priority_list: Vec<DeviceId>,
    #[serde(rename = "input_priority_list")]
    pub(crate) input_priority_list: Vec<DeviceId>,
    #[serde(rename = "notify_on_priority_restore_output")]
    pub(crate) notify_on_priority_restore_output: bool,
    #[serde(rename = "notify_on_priority_restore_input")]
    pub(crate) notify_on_priority_restore_input: bool,
    #[serde(rename = "switch_communication_device_output")]
    pub(crate) switch_communication_device_output: bool,
    #[serde(rename = "switch_communication_device_input")]
    pub(crate) switch_communication_device_input: bool,
    pub check_updates_on_launch: bool,
}

impl PersistentState {
    /// Single dispatch point for all per-device-type fields (shared ref).
    fn fields_ref(&self, dt: DeviceType) -> (&Vec<DeviceId>, bool, bool) {
        match dt {
            DeviceType::Output => (
                &self.output_priority_list,
                self.notify_on_priority_restore_output,
                self.switch_communication_device_output,
            ),
            DeviceType::Input => (
                &self.input_priority_list,
                self.notify_on_priority_restore_input,
                self.switch_communication_device_input,
            ),
        }
    }

    /// Single dispatch point for all per-device-type fields (mutable ref).
    fn fields_mut(&mut self, dt: DeviceType) -> (&mut Vec<DeviceId>, &mut bool, &mut bool) {
        match dt {
            DeviceType::Output => (
                &mut self.output_priority_list,
                &mut self.notify_on_priority_restore_output,
                &mut self.switch_communication_device_output,
            ),
            DeviceType::Input => (
                &mut self.input_priority_list,
                &mut self.notify_on_priority_restore_input,
                &mut self.switch_communication_device_input,
            ),
        }
    }

    pub fn priority_list(&self, device_type: DeviceType) -> &[DeviceId] {
        self.fields_ref(device_type).0
    }

    pub fn priority_list_mut(&mut self, device_type: DeviceType) -> &mut Vec<DeviceId> {
        self.fields_mut(device_type).0
    }

    pub fn notify_on_priority_restore(&self, device_type: DeviceType) -> bool {
        self.fields_ref(device_type).1
    }

    pub fn set_notify_on_priority_restore(&mut self, device_type: DeviceType, value: bool) {
        *self.fields_mut(device_type).1 = value;
    }

    pub fn switch_communication_device(&self, device_type: DeviceType) -> bool {
        self.fields_ref(device_type).2
    }

    pub fn set_switch_communication_device(&mut self, device_type: DeviceType, value: bool) {
        *self.fields_mut(device_type).2 = value;
    }

    pub fn device_settings(&self, device_id: &DeviceId) -> Option<&DeviceSettings> {
        self.devices.get(device_id)
    }

    pub fn device_settings_mut(&mut self, device_id: &DeviceId) -> Option<&mut DeviceSettings> {
        self.devices.get_mut(device_id)
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
        let in_priority = self.output_priority_list.contains(device_id)
            || self.input_priority_list.contains(device_id);
        if !in_priority {
            self.devices.remove(device_id);
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
    save_state_to(&get_state_file_path()?, state)
}

pub fn load_state() -> anyhow::Result<PersistentState> {
    load_state_from(&get_state_file_path()?)
}

/// Writes `state` to `path` via a temp file + rename for crash safety.
pub(crate) fn save_state_to(path: &std::path::Path, state: &PersistentState) -> anyhow::Result<()> {
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

    if let Err(e) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(anyhow::anyhow!(e).context(format!(
            "failed to rename temporary state file '{}' to '{}'",
            tmp_path.display(),
            path.display()
        )));
    }

    Ok(())
}

/// Loads state from `path`, returning defaults if the file doesn't exist.
pub(crate) fn load_state_from(path: &std::path::Path) -> anyhow::Result<PersistentState> {
    let data = match fs::read_to_string(path) {
        Ok(data) => data,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PersistentState::default());
        }
        Err(e) => {
            return Err(anyhow::anyhow!(e))
                .with_context(|| format!("failed to read state file '{}'", path.display()));
        }
    };

    serde_json::from_str(&data)
        .with_context(|| format!("failed to parse state file '{}'", path.display()))
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

        assert_eq!(state.output_priority_list, vec!["new_out"]);
        assert_eq!(state.input_priority_list, vec!["new_in"]);
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

    // --- save_state_to / load_state_from tests ---

    #[test]
    fn save_and_load_state_roundtrip() {
        let dir = std::env::temp_dir().join("vl_test_save_load_roundtrip");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("state.json");

        let mut state = PersistentState {
            output_priority_list: vec!["dev_a".into(), "dev_b".into()],
            check_updates_on_launch: false,
            ..Default::default()
        };
        state.devices.insert(
            "dev_a".into(),
            DeviceSettings {
                volume_lock: VolumeLockPolicy {
                    is_locked: true,
                    target_percent: VolumePercent::from(60.0),
                    notify: true,
                },
                unmute_lock: UnmuteLockPolicy::default(),
                device_type: DeviceType::Output,
                name: "Speakers".into(),
            },
        );

        super::save_state_to(&path, &state).unwrap();

        // Verify temp file was cleaned up
        assert!(!path.with_extension("json.tmp").exists());
        assert!(path.exists());

        let loaded = super::load_state_from(&path).unwrap();
        assert_eq!(loaded.output_priority_list, vec!["dev_a", "dev_b"]);
        assert!(!loaded.check_updates_on_launch);
        let dev = loaded.devices.get("dev_a").unwrap();
        assert!(dev.volume_lock.is_locked);
        assert_eq!(dev.volume_lock.target_percent, 60.0);

        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn load_state_missing_file_returns_default() {
        let path = std::env::temp_dir().join("vl_test_missing_state.json");
        let _ = fs::remove_file(&path); // ensure it doesn't exist

        let loaded = super::load_state_from(&path).unwrap();
        assert!(loaded.devices.is_empty());
        assert!(loaded.output_priority_list.is_empty());
        assert!(loaded.check_updates_on_launch); // default is true
    }

    #[test]
    fn load_state_malformed_file_returns_error_with_context() {
        let dir = std::env::temp_dir().join("vl_test_malformed_load");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("state.json");

        fs::write(&path, "not json at all").unwrap();

        let result = super::load_state_from(&path);
        assert!(result.is_err());
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(err_msg.contains("failed to parse state file"));

        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn save_state_overwrites_existing_file() {
        let dir = std::env::temp_dir().join("vl_test_overwrite");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("state.json");

        // Save initial
        let mut state = PersistentState {
            check_updates_on_launch: false,
            ..Default::default()
        };
        super::save_state_to(&path, &state).unwrap();

        // Save modified
        state.check_updates_on_launch = true;
        state.output_priority_list = vec!["new_dev".into()];
        super::save_state_to(&path, &state).unwrap();

        // Load and verify latest
        let loaded = super::load_state_from(&path).unwrap();
        assert!(loaded.check_updates_on_launch);
        assert_eq!(loaded.output_priority_list, vec!["new_dev"]);

        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn save_state_uses_atomic_write() {
        let dir = std::env::temp_dir().join("vl_test_atomic_write");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("state.json");
        let tmp_path = path.with_extension("json.tmp");

        let state = PersistentState::default();
        super::save_state_to(&path, &state).unwrap();

        // After a successful save, the temp file should not exist
        assert!(!tmp_path.exists(), "temp file should be cleaned up after successful write");
        // The target file should exist with valid content
        assert!(path.exists(), "target file should exist after save");
        let loaded = super::load_state_from(&path).unwrap();
        assert!(loaded.devices.is_empty());

        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }
}
