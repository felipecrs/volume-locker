use crate::consts::STATE_FILE_NAME;
use crate::types::DeviceSettings;
use crate::types::DeviceType;
use crate::utils::get_executable_directory;
use anyhow::Context;
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
    pub check_updates_on_launch: bool,
}

impl PersistentState {
    pub fn get_priority_list_mut(&mut self, device_type: DeviceType) -> &mut Vec<String> {
        match device_type {
            DeviceType::Output => &mut self.output_priority_list,
            DeviceType::Input => &mut self.input_priority_list,
        }
    }

    pub fn get_priority_list(&self, device_type: DeviceType) -> &[String] {
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

fn get_state_file_path() -> PathBuf {
    get_executable_directory().join(STATE_FILE_NAME)
}

pub fn save_state(state: &PersistentState) {
    let path = get_state_file_path();
    let tmp_path = path.with_extension("json.tmp");

    let json = match serde_json::to_string_pretty(state) {
        Ok(json) => json,
        Err(e) => {
            log::error!("Failed to serialize state: {e}");
            return;
        }
    };

    // Write to a temporary file first, then atomically rename to the target.
    // This prevents corruption if the process is interrupted mid-write.
    if let Err(e) = fs::write(&tmp_path, &json) {
        log::error!(
            "Failed to write temporary state file '{}': {e}",
            tmp_path.display()
        );
        return;
    }

    if let Err(e) = fs::rename(&tmp_path, &path) {
        log::error!(
            "Failed to rename temporary state file '{}' to '{}': {e}",
            tmp_path.display(),
            path.display()
        );
        // Try to clean up the temporary file
        let _ = fs::remove_file(&tmp_path);
    }
}

pub fn load_state() -> anyhow::Result<PersistentState> {
    let path = get_state_file_path();

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
