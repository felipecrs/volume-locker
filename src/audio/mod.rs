use crate::types::{DeviceRole, DeviceType};

pub type AudioResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub trait AudioBackend {
    fn get_devices(&self, device_type: DeviceType) -> AudioResult<Vec<Box<dyn AudioDevice>>>;
    fn get_device_by_id(&self, id: &str) -> AudioResult<Box<dyn AudioDevice>>;
    fn get_default_device(
        &self,
        device_type: DeviceType,
        role: DeviceRole,
    ) -> AudioResult<Box<dyn AudioDevice>>;
    fn set_default_device(&self, device_id: &str, role: DeviceRole) -> AudioResult<()>;

    fn register_device_change_callback(
        &mut self,
        callback: Box<dyn Fn() + Send + Sync>,
    ) -> AudioResult<()>;
}

pub trait AudioDevice {
    fn id(&self) -> String;
    fn name(&self) -> String;
    fn volume(&self) -> AudioResult<f32>;
    fn set_volume(&self, volume: f32) -> AudioResult<()>;
    fn is_muted(&self) -> AudioResult<bool>;
    fn set_mute(&self, muted: bool) -> AudioResult<()>;
    fn is_active(&self) -> AudioResult<bool>;

    fn watch_volume(&self, callback: Box<dyn Fn(Option<f32>) + Send + Sync>) -> AudioResult<()>;
}

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use self::windows::WindowsAudioBackend as AudioBackendImpl;

use crate::config::PersistentState;
use crate::types::DeviceSettings;
use crate::ui::TemporaryPriorities;
use crate::utils::send_notification_debounced;
use std::collections::HashMap;
use std::time::Instant;

pub fn migrate_device_ids(
    backend: &impl AudioBackend,
    persistent_state: &mut PersistentState,
) -> bool {
    let mut devices_to_migrate: Vec<(String, DeviceSettings)> = Vec::new();
    let mut devices_to_update: Vec<(String, DeviceSettings)> = Vec::new();

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

fn find_device_by_name_and_type(
    backend: &impl AudioBackend,
    target_name: &str,
    device_type: DeviceType,
) -> AudioResult<String> {
    let devices = backend.get_devices(device_type)?;
    for device in devices {
        if device.name() == target_name {
            return Ok(device.id());
        }
    }
    Err(Box::new(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "Device not found",
    )))
}

pub fn check_and_unmute_device(
    device: &dyn AudioDevice,
    device_name: &str,
    notify: bool,
    notification_title: &str,
    notification_message_suffix: &str,
    last_notification_times: &mut HashMap<String, Instant>,
) {
    if let Ok(true) = device.is_muted() {
        if let Err(e) = device.set_mute(false) {
            log::error!("Failed to unmute {device_name}: {e}");
        } else {
            log::info!("Unmuted {device_name} due to lock settings");
            if notify {
                let message = format!("{device_name} {notification_message_suffix}");
                send_notification_debounced(
                    &format!("unmute_{}", device.id()),
                    notification_title,
                    &message,
                    last_notification_times,
                );
            }
        }
    }
}

pub fn get_unmute_notification_details(device_type: DeviceType) -> (&'static str, &'static str) {
    let title = match device_type {
        DeviceType::Input => "Input Device Unmuted",
        DeviceType::Output => "Output Device Unmuted",
    };
    (title, "was unmuted due to Keep unmuted setting.")
}

pub fn enforce_priorities(
    backend: &impl AudioBackend,
    state: &PersistentState,
    last_notification_times: &mut HashMap<String, Instant>,
    temporary_priorities: &TemporaryPriorities,
) {
    enforce_priority_for_type(
        backend,
        state,
        DeviceType::Output,
        &temporary_priorities.output,
        last_notification_times,
    );
    enforce_priority_for_type(
        backend,
        state,
        DeviceType::Input,
        &temporary_priorities.input,
        last_notification_times,
    );
}

fn enforce_priority_for_type(
    backend: &impl AudioBackend,
    state: &PersistentState,
    device_type: DeviceType,
    temporary_priority: &Option<String>,
    last_notification_times: &mut HashMap<String, Instant>,
) {
    let mut priority_list = state.get_priority_list(device_type).clone();
    if let Some(temp_id) = temporary_priority {
        priority_list.insert(0, temp_id.clone());
    }

    if let Some(target_id) = find_highest_priority_active_device(backend, &priority_list) {
        let mut switched = false;

        // Check Console/Multimedia
        let is_console_correct = if let Ok(default_device) =
            backend.get_default_device(device_type, DeviceRole::Console)
        {
            default_device.id() == target_id
        } else {
            false
        };

        if !is_console_correct {
            let type_str = match device_type {
                DeviceType::Output => "output",
                DeviceType::Input => "input",
            };
            log::info!(
                "Enforcing {} priority: Switching to {}",
                type_str,
                target_id
            );
            let _ = backend.set_default_device(&target_id, DeviceRole::Console);
            let _ = backend.set_default_device(&target_id, DeviceRole::Multimedia);
            switched = true;
        }

        // Check Communications
        if state.get_switch_communication_device(device_type) {
            let is_comm_correct = if let Ok(default_device) =
                backend.get_default_device(device_type, DeviceRole::Communications)
            {
                default_device.id() == target_id
            } else {
                false
            };

            if !is_comm_correct {
                let type_str = match device_type {
                    DeviceType::Output => "output",
                    DeviceType::Input => "input",
                };
                log::info!(
                    "Enforcing {} priority (Communication): Switching to {}",
                    type_str,
                    target_id
                );
                let _ = backend.set_default_device(&target_id, DeviceRole::Communications);
                switched = true;
            }
        }

        if switched && state.get_notify_on_priority_restore(device_type) {
            let device_name = match backend.get_device_by_id(&target_id) {
                Ok(d) => d.name(),
                Err(_) => "Unknown Device".to_string(),
            };
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
}

fn find_highest_priority_active_device(
    backend: &impl AudioBackend,
    priority_list: &[String],
) -> Option<String> {
    for device_id in priority_list {
        if let Ok(device) = backend.get_device_by_id(device_id)
            && let Ok(true) = device.is_active()
        {
            return Some(device_id.clone());
        }
    }
    None
}
