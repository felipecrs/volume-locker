mod persistence;

pub use persistence::{load_state, save_state};

use crate::types::DeviceSettings;
use crate::types::{DeviceId, DeviceType, OrderKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Per-device-type preferences (one instance for output, one for input).
#[derive(Debug, Clone, Default)]
pub(crate) struct PerTypeSettings {
    pub device_order: Vec<DeviceId>,
    /// Independent device order for the Communications role. May contain
    /// normal device IDs as well as [`DeviceId::follow_main()`], which
    /// dynamically follows whatever device the main device order resolves to.
    pub communication_device_order: Vec<DeviceId>,
    pub notify_on_switch: bool,
    pub communication_notify_on_switch: bool,
}

/// Flat serde representation for backward-compatible JSON serialization.
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct PersistentStateFlat {
    devices: HashMap<DeviceId, DeviceSettings>,
    #[serde(alias = "output_priority_list")]
    output_device_order: Vec<DeviceId>,
    #[serde(alias = "input_priority_list")]
    input_device_order: Vec<DeviceId>,
    #[serde(alias = "output_communication_priority_list")]
    output_communication_device_order: Vec<DeviceId>,
    #[serde(alias = "input_communication_priority_list")]
    input_communication_device_order: Vec<DeviceId>,
    #[serde(alias = "notify_on_priority_restore_output")]
    notify_on_switch_output: bool,
    #[serde(alias = "notify_on_priority_restore_input")]
    notify_on_switch_input: bool,
    #[serde(alias = "notify_on_priority_restore_output_communication")]
    notify_on_switch_output_communication: bool,
    #[serde(alias = "notify_on_priority_restore_input_communication")]
    notify_on_switch_input_communication: bool,
    switch_communication_device_output: bool,
    switch_communication_device_input: bool,
    check_updates_on_launch: bool,
    transfer_device_settings: bool,
    #[serde(alias = "notify_on_device_migration")]
    notify_on_settings_transfer: bool,
    transfer_device_name_and_icon: bool,
    skip_disconnected_wireless_headsets: bool,
}

impl Default for PersistentStateFlat {
    fn default() -> Self {
        let state = PersistentState::default();
        state.into()
    }
}

/// Versions before the "device order" rename stored the follow-main entry as this ID.
const LEGACY_FOLLOW_MAIN: &str = "__main_priority__";

fn upgrade_follow_main(mut list: Vec<DeviceId>) -> Vec<DeviceId> {
    for id in &mut list {
        if *id == *LEGACY_FOLLOW_MAIN {
            *id = DeviceId::follow_main();
        }
    }
    list
}

impl From<PersistentStateFlat> for PersistentState {
    fn from(flat: PersistentStateFlat) -> Self {
        let output_comm = if flat.output_communication_device_order.is_empty()
            && flat.switch_communication_device_output
        {
            vec![DeviceId::follow_main()]
        } else {
            upgrade_follow_main(flat.output_communication_device_order)
        };

        let input_comm = if flat.input_communication_device_order.is_empty()
            && flat.switch_communication_device_input
        {
            vec![DeviceId::follow_main()]
        } else {
            upgrade_follow_main(flat.input_communication_device_order)
        };

        Self {
            devices: flat.devices,
            output: PerTypeSettings {
                device_order: flat.output_device_order,
                communication_device_order: output_comm,
                notify_on_switch: flat.notify_on_switch_output,
                communication_notify_on_switch: flat.notify_on_switch_output_communication,
            },
            input: PerTypeSettings {
                device_order: flat.input_device_order,
                communication_device_order: input_comm,
                notify_on_switch: flat.notify_on_switch_input,
                communication_notify_on_switch: flat.notify_on_switch_input_communication,
            },
            check_updates_on_launch: flat.check_updates_on_launch,
            transfer_device_settings: flat.transfer_device_settings,
            notify_on_settings_transfer: flat.notify_on_settings_transfer,
            transfer_device_name_and_icon: flat.transfer_device_name_and_icon,
            skip_disconnected_wireless_headsets: flat.skip_disconnected_wireless_headsets,
        }
    }
}

impl From<PersistentState> for PersistentStateFlat {
    fn from(state: PersistentState) -> Self {
        let switch_comm_out = state
            .output
            .communication_device_order
            .iter()
            .any(|id| id.is_follow_main());
        let switch_comm_in = state
            .input
            .communication_device_order
            .iter()
            .any(|id| id.is_follow_main());

        Self {
            devices: state.devices,
            output_device_order: state.output.device_order,
            input_device_order: state.input.device_order,
            output_communication_device_order: state.output.communication_device_order,
            input_communication_device_order: state.input.communication_device_order,
            notify_on_switch_output: state.output.notify_on_switch,
            notify_on_switch_input: state.input.notify_on_switch,
            notify_on_switch_output_communication: state.output.communication_notify_on_switch,
            notify_on_switch_input_communication: state.input.communication_notify_on_switch,
            switch_communication_device_output: switch_comm_out,
            switch_communication_device_input: switch_comm_in,
            check_updates_on_launch: state.check_updates_on_launch,
            transfer_device_settings: state.transfer_device_settings,
            notify_on_settings_transfer: state.notify_on_settings_transfer,
            transfer_device_name_and_icon: state.transfer_device_name_and_icon,
            skip_disconnected_wireless_headsets: state.skip_disconnected_wireless_headsets,
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
    pub transfer_device_settings: bool,
    pub notify_on_settings_transfer: bool,
    pub transfer_device_name_and_icon: bool,
    pub skip_disconnected_wireless_headsets: bool,
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

    pub fn device_order(&self, device_type: DeviceType) -> &[DeviceId] {
        &self.per_type(device_type).device_order
    }

    pub fn device_order_mut(&mut self, device_type: DeviceType) -> &mut Vec<DeviceId> {
        &mut self.per_type_mut(device_type).device_order
    }

    pub fn communication_device_order(&self, device_type: DeviceType) -> &[DeviceId] {
        &self.per_type(device_type).communication_device_order
    }

    pub fn communication_device_order_mut(
        &mut self,
        device_type: DeviceType,
    ) -> &mut Vec<DeviceId> {
        &mut self.per_type_mut(device_type).communication_device_order
    }

    pub fn device_order_of(&self, device_type: DeviceType, kind: OrderKind) -> &[DeviceId] {
        match kind {
            OrderKind::Main => self.device_order(device_type),
            OrderKind::Communication => self.communication_device_order(device_type),
        }
    }

    pub fn device_order_of_mut(
        &mut self,
        device_type: DeviceType,
        kind: OrderKind,
    ) -> &mut Vec<DeviceId> {
        match kind {
            OrderKind::Main => self.device_order_mut(device_type),
            OrderKind::Communication => self.communication_device_order_mut(device_type),
        }
    }

    pub fn notify_on_switch_of(&self, device_type: DeviceType, kind: OrderKind) -> bool {
        match kind {
            OrderKind::Main => self.per_type(device_type).notify_on_switch,
            OrderKind::Communication => self.per_type(device_type).communication_notify_on_switch,
        }
    }

    pub fn set_notify_on_switch_of(
        &mut self,
        device_type: DeviceType,
        kind: OrderKind,
        value: bool,
    ) {
        match kind {
            OrderKind::Main => self.per_type_mut(device_type).notify_on_switch = value,
            OrderKind::Communication => {
                self.per_type_mut(device_type)
                    .communication_notify_on_switch = value;
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
    /// and is not referenced by any device order.
    pub fn remove_device_if_unused(&mut self, device_id: &DeviceId) {
        if device_id.is_follow_main() {
            return;
        }
        let is_prunable = self
            .devices
            .get(device_id)
            .is_some_and(|s| !s.has_active_locks_or_notifications());
        if !is_prunable {
            return;
        }
        let in_device_order = [&self.output, &self.input].iter().any(|s| {
            s.device_order.contains(device_id) || s.communication_device_order.contains(device_id)
        });
        if !in_device_order {
            self.devices.remove(device_id);
        }
    }
}

impl Default for PersistentState {
    fn default() -> Self {
        Self {
            devices: HashMap::default(),
            output: PerTypeSettings {
                communication_device_order: vec![DeviceId::follow_main()],
                ..PerTypeSettings::default()
            },
            input: PerTypeSettings {
                communication_device_order: vec![DeviceId::follow_main()],
                ..PerTypeSettings::default()
            },
            check_updates_on_launch: true,
            transfer_device_settings: true,
            notify_on_settings_transfer: false,
            transfer_device_name_and_icon: true,
            skip_disconnected_wireless_headsets: true,
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
        assert!(state.output.device_order.is_empty());
        assert!(state.input.device_order.is_empty());
        assert!(!state.output.notify_on_switch);
        assert!(!state.input.notify_on_switch);
        assert_eq!(
            state.output.communication_device_order,
            vec![DeviceId::follow_main()]
        );
        assert_eq!(
            state.input.communication_device_order,
            vec![DeviceId::follow_main()]
        );
        assert!(state.check_updates_on_launch);
        assert!(state.skip_disconnected_wireless_headsets);
    }

    #[test]
    fn persistent_state_serialization_roundtrip() {
        let state = PersistentState {
            output: PerTypeSettings {
                device_order: vec!["device_a".into(), "device_b".into()],
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
                    fingerprint: None,
                },
            )]),
            ..Default::default()
        };

        let json = serde_json::to_string_pretty(&state).unwrap();
        let loaded: PersistentState = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.output.device_order, vec!["device_a", "device_b"]);
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
        assert!(state.output.device_order.is_empty());
        assert!(state.check_updates_on_launch);
        assert!(state.transfer_device_settings);
        assert!(!state.notify_on_settings_transfer);
        assert!(state.transfer_device_name_and_icon);
        assert!(state.skip_disconnected_wireless_headsets);
    }

    #[test]
    fn communication_device_order_roundtrips() {
        let state = PersistentState {
            output: PerTypeSettings {
                device_order: vec!["speakers".into()],
                communication_device_order: vec!["headset".into()],
                ..PerTypeSettings::default()
            },
            input: PerTypeSettings {
                communication_device_order: vec!["mic_usb".into()],
                ..PerTypeSettings::default()
            },
            ..Default::default()
        };

        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("output_communication_device_order"));

        let loaded: PersistentState = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.output.device_order, vec!["speakers"]);
        assert_eq!(loaded.output.communication_device_order, vec!["headset"]);
        assert_eq!(loaded.input.communication_device_order, vec!["mic_usb"]);
    }

    #[test]
    fn pre_device_order_config_still_loads() {
        // A state file written before "priority list" became "device order".
        let json = r#"{
            "output_priority_list": ["speakers"],
            "input_priority_list": ["mic"],
            "output_communication_priority_list": ["headset", "__main_priority__"],
            "input_communication_priority_list": ["__main_priority__"],
            "notify_on_priority_restore_output": true,
            "notify_on_priority_restore_input_communication": true
        }"#;
        let state: PersistentState = serde_json::from_str(json).unwrap();
        assert_eq!(state.output.device_order, vec!["speakers"]);
        assert_eq!(state.input.device_order, vec!["mic"]);
        assert_eq!(
            state.output.communication_device_order,
            vec![DeviceId::from("headset"), DeviceId::follow_main()]
        );
        assert_eq!(
            state.input.communication_device_order,
            vec![DeviceId::follow_main()]
        );
        assert!(state.output.notify_on_switch);
        assert!(state.input.communication_notify_on_switch);
        assert!(!state.input.notify_on_switch);

        let json = serde_json::to_string(&state).unwrap();
        assert!(!json.contains("priorit"));
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
        assert_eq!(state.output.device_order, vec!["speakers"]);
        assert_eq!(
            state.output.communication_device_order,
            vec![DeviceId::follow_main()]
        );
    }

    #[test]
    fn device_order_of_selects_the_requested_list() {
        let mut state = PersistentState::default();
        state
            .device_order_of_mut(DeviceType::Output, OrderKind::Main)
            .push("main".into());
        *state.device_order_of_mut(DeviceType::Output, OrderKind::Communication) =
            vec!["comm".into()];
        state
            .device_order_of_mut(DeviceType::Input, OrderKind::Communication)
            .clear();

        assert_eq!(
            state.device_order_of(DeviceType::Output, OrderKind::Main),
            &["main"]
        );
        assert_eq!(
            state.device_order_of(DeviceType::Output, OrderKind::Communication),
            &["comm"]
        );
        assert!(
            state
                .device_order_of(DeviceType::Input, OrderKind::Communication)
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
            .communication_device_order_mut(DeviceType::Output)
            .push("dev1".into());

        state.remove_device_if_unused(&"dev1".into());
        assert!(state.devices.contains_key("dev1"));

        state
            .communication_device_order_mut(DeviceType::Output)
            .clear();
        state.remove_device_if_unused(&"dev1".into());
        assert!(!state.devices.contains_key("dev1"));
    }

    #[test]
    fn get_device_order_returns_correct_type() {
        let state = PersistentState {
            output: PerTypeSettings {
                device_order: vec!["out1".into()],
                ..PerTypeSettings::default()
            },
            input: PerTypeSettings {
                device_order: vec!["in1".into(), "in2".into()],
                ..PerTypeSettings::default()
            },
            ..Default::default()
        };

        assert_eq!(state.device_order(DeviceType::Output), &["out1"]);
        assert_eq!(state.device_order(DeviceType::Input), &["in1", "in2"]);
    }

    #[test]
    fn get_device_order_mut_modifies_correct_type() {
        let mut state = PersistentState::default();
        state
            .device_order_mut(DeviceType::Output)
            .push("new_out".into());
        state
            .device_order_mut(DeviceType::Input)
            .push("new_in".into());

        assert_eq!(state.output.device_order, vec!["new_out"]);
        assert_eq!(state.input.device_order, vec!["new_in"]);
    }

    #[test]
    fn notify_on_switch_accessors() {
        let mut state = PersistentState::default();
        assert!(!state.notify_on_switch_of(DeviceType::Output, OrderKind::Main));
        assert!(!state.notify_on_switch_of(DeviceType::Output, OrderKind::Communication));
        state.set_notify_on_switch_of(DeviceType::Output, OrderKind::Main, true);
        assert!(state.notify_on_switch_of(DeviceType::Output, OrderKind::Main));
        assert!(!state.notify_on_switch_of(DeviceType::Output, OrderKind::Communication));
        assert!(!state.notify_on_switch_of(DeviceType::Input, OrderKind::Main));
    }

    #[test]
    fn file_roundtrip_preserves_state() {
        let dir = std::env::temp_dir().join("volume_locker_test_roundtrip");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test_state.json");

        // Build a non-trivial state
        let state = PersistentState {
            output: PerTypeSettings {
                device_order: vec!["dev_a".into(), "dev_b".into()],
                notify_on_switch: true,
                ..PerTypeSettings::default()
            },
            input: PerTypeSettings {
                device_order: vec!["mic_1".into()],
                communication_device_order: vec![],
                ..PerTypeSettings::default()
            },
            check_updates_on_launch: false,
            transfer_device_settings: false,
            notify_on_settings_transfer: true,
            transfer_device_name_and_icon: false,
            skip_disconnected_wireless_headsets: false,
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

        assert_eq!(loaded.output.device_order, vec!["dev_a", "dev_b"]);
        assert_eq!(loaded.input.device_order, vec!["mic_1"]);
        assert!(loaded.output.notify_on_switch);
        assert!(loaded.input.communication_device_order.is_empty());
        assert!(!loaded.check_updates_on_launch);
        assert!(!loaded.transfer_device_settings);
        assert!(loaded.notify_on_settings_transfer);
        assert!(!loaded.transfer_device_name_and_icon);

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
        loaded.output.device_order.push("dev1".into());

        let json2 = serde_json::to_string_pretty(&loaded).unwrap();
        fs::write(&path, &json2).unwrap();

        // Reload and verify modifications persisted
        let data2 = fs::read_to_string(&path).unwrap();
        let final_state: PersistentState = serde_json::from_str(&data2).unwrap();
        let dev = final_state.devices.get("dev1").unwrap();
        assert!(dev.volume_lock.is_locked);
        assert_eq!(dev.volume_lock.target_percent, 75.0);
        assert_eq!(final_state.output.device_order, vec!["dev1"]);

        // Cleanup
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn notify_preference_loads_from_its_released_key() {
        let state: PersistentState =
            serde_json::from_str(r#"{"notify_on_device_migration": true}"#).unwrap();
        assert!(state.notify_on_settings_transfer);
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
