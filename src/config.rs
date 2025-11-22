use crate::consts::STATE_FILE_NAME;
use crate::types::DeviceSettings;
use crate::types::DeviceType;
use crate::utils::get_executable_directory;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PersistentState {
    pub devices: HashMap<String, DeviceSettings>,
    pub output_priority_list: Vec<String>,
    pub input_priority_list: Vec<String>,
    pub notify_on_priority_restore_output: bool,
    pub notify_on_priority_restore_input: bool,
    pub switch_communication_device_output: bool,
    pub switch_communication_device_input: bool,
}

impl PersistentState {
    pub fn get_priority_list_mut(&mut self, device_type: DeviceType) -> &mut Vec<String> {
        match device_type {
            DeviceType::Output => &mut self.output_priority_list,
            DeviceType::Input => &mut self.input_priority_list,
        }
    }

    pub fn get_priority_list(&self, device_type: DeviceType) -> &Vec<String> {
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
        }
    }
}

fn get_state_file_path() -> PathBuf {
    get_executable_directory().join(STATE_FILE_NAME)
}

pub fn save_state(state: &PersistentState) {
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = fs::write(get_state_file_path(), json);
    }
}

pub fn load_state() -> PersistentState {
    let state_path = get_state_file_path();
    fs::read_to_string(state_path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}
