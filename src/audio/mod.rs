use crate::types::{DeviceRole, DeviceType};

#[cfg(target_os = "windows")]
mod windows_com_policy_config;

pub trait AudioBackend {
    fn get_devices(&self, device_type: DeviceType) -> anyhow::Result<Vec<Box<dyn AudioDevice>>>;
    fn get_device_by_id(&self, id: &str) -> anyhow::Result<Box<dyn AudioDevice>>;
    fn get_default_device(
        &self,
        device_type: DeviceType,
        role: DeviceRole,
    ) -> anyhow::Result<Box<dyn AudioDevice>>;
    fn set_default_device(&self, device_id: &str, role: DeviceRole) -> anyhow::Result<()>;

    fn register_device_change_callback(
        &mut self,
        callback: Box<dyn Fn() + Send + Sync>,
    ) -> anyhow::Result<()>;
}

pub trait AudioDevice {
    fn id(&self) -> String;
    fn name(&self) -> String;
    fn volume(&self) -> anyhow::Result<f32>;
    fn set_volume(&self, volume: f32) -> anyhow::Result<()>;
    fn is_muted(&self) -> anyhow::Result<bool>;
    fn set_mute(&self, muted: bool) -> anyhow::Result<()>;
    fn is_active(&self) -> anyhow::Result<bool>;

    fn watch_volume(&self, callback: Box<dyn Fn(Option<f32>) + Send + Sync>) -> anyhow::Result<()>;
}

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use self::windows::WindowsAudioBackend as AudioBackendImpl;

mod migration;
mod priority;

pub use migration::migrate_device_ids;
pub use priority::enforce_priorities;

use crate::utils::send_notification_debounced;
use std::collections::HashMap;
use std::time::Instant;

pub fn check_and_unmute_device(
    device: &dyn AudioDevice,
    notify: bool,
    notification_title: &str,
    notification_message_suffix: &str,
    last_notification_times: &mut HashMap<String, Instant>,
) -> anyhow::Result<()> {
    if device.is_muted()? {
        device.set_mute(false)?;
        let device_name = device.name();
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
    Ok(())
}

pub fn get_unmute_notification_details(device_type: DeviceType) -> (&'static str, &'static str) {
    let title = match device_type {
        DeviceType::Input => "Input Device Unmuted",
        DeviceType::Output => "Output Device Unmuted",
    };
    (title, "was unmuted due to Keep unmuted setting.")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::config::PersistentState;
    use crate::types::{DeviceSettings, TemporaryPriorities};
    use std::cell::RefCell;
    use std::collections::HashMap;

    pub(crate) struct MockDevice {
        pub(crate) id: String,
        pub(crate) name: String,
        pub(crate) active: bool,
        pub(crate) device_type: DeviceType,
        pub(crate) volume: RefCell<f32>,
        pub(crate) muted: RefCell<bool>,
    }

    impl MockDevice {
        pub(crate) fn new(id: &str, name: &str, active: bool) -> Self {
            Self {
                id: id.to_string(),
                name: name.to_string(),
                active,
                device_type: DeviceType::Output,
                volume: RefCell::new(1.0),
                muted: RefCell::new(false),
            }
        }
    }

    impl AudioDevice for MockDevice {
        fn id(&self) -> String {
            self.id.clone()
        }
        fn name(&self) -> String {
            self.name.clone()
        }
        fn volume(&self) -> anyhow::Result<f32> {
            Ok(*self.volume.borrow())
        }
        fn set_volume(&self, volume: f32) -> anyhow::Result<()> {
            *self.volume.borrow_mut() = volume;
            Ok(())
        }
        fn is_muted(&self) -> anyhow::Result<bool> {
            Ok(*self.muted.borrow())
        }
        fn set_mute(&self, muted: bool) -> anyhow::Result<()> {
            *self.muted.borrow_mut() = muted;
            Ok(())
        }
        fn is_active(&self) -> anyhow::Result<bool> {
            Ok(self.active)
        }
        fn watch_volume(
            &self,
            _callback: Box<dyn Fn(Option<f32>) + Send + Sync>,
        ) -> anyhow::Result<()> {
            Ok(())
        }
    }

    pub(crate) struct MockAudioBackend {
        pub(crate) devices: Vec<MockDevice>,
        pub(crate) default_console: RefCell<HashMap<DeviceType, String>>,
        pub(crate) default_multimedia: RefCell<HashMap<DeviceType, String>>,
        pub(crate) default_communications: RefCell<HashMap<DeviceType, String>>,
    }

    impl MockAudioBackend {
        pub(crate) fn new(devices: Vec<MockDevice>) -> Self {
            Self {
                devices,
                default_console: RefCell::new(HashMap::new()),
                default_multimedia: RefCell::new(HashMap::new()),
                default_communications: RefCell::new(HashMap::new()),
            }
        }

        pub(crate) fn set_default(&self, device_id: &str, device_type: DeviceType) {
            self.default_console
                .borrow_mut()
                .insert(device_type, device_id.to_string());
            self.default_multimedia
                .borrow_mut()
                .insert(device_type, device_id.to_string());
        }
    }

    impl AudioBackend for MockAudioBackend {
        fn get_devices(
            &self,
            device_type: DeviceType,
        ) -> anyhow::Result<Vec<Box<dyn AudioDevice>>> {
            let _ = device_type;
            Ok(self
                .devices
                .iter()
                .map(|d| {
                    Box::new(MockDevice::new(&d.id, &d.name, d.active)) as Box<dyn AudioDevice>
                })
                .collect())
        }

        fn get_device_by_id(&self, id: &str) -> anyhow::Result<Box<dyn AudioDevice>> {
            self.devices
                .iter()
                .find(|d| d.id == id)
                .map(|d| {
                    Box::new(MockDevice::new(&d.id, &d.name, d.active)) as Box<dyn AudioDevice>
                })
                .ok_or_else(|| anyhow::anyhow!("Device not found: {id}"))
        }

        fn get_default_device(
            &self,
            device_type: DeviceType,
            role: DeviceRole,
        ) -> anyhow::Result<Box<dyn AudioDevice>> {
            let map = match role {
                DeviceRole::Console => self.default_console.borrow(),
                DeviceRole::Multimedia => self.default_multimedia.borrow(),
                DeviceRole::Communications => self.default_communications.borrow(),
            };
            let id = map
                .get(&device_type)
                .ok_or_else(|| anyhow::anyhow!("No default device"))?
                .clone();
            drop(map);
            self.get_device_by_id(&id)
        }

        fn set_default_device(&self, device_id: &str, role: DeviceRole) -> anyhow::Result<()> {
            let device_type = self
                .devices
                .iter()
                .find(|d| d.id == device_id)
                .map(|d| d.device_type)
                .unwrap_or(DeviceType::Output);
            match role {
                DeviceRole::Console => {
                    self.default_console
                        .borrow_mut()
                        .insert(device_type, device_id.to_string());
                }
                DeviceRole::Multimedia => {
                    self.default_multimedia
                        .borrow_mut()
                        .insert(device_type, device_id.to_string());
                }
                DeviceRole::Communications => {
                    self.default_communications
                        .borrow_mut()
                        .insert(device_type, device_id.to_string());
                }
            }
            Ok(())
        }

        fn register_device_change_callback(
            &mut self,
            _callback: Box<dyn Fn() + Send + Sync>,
        ) -> anyhow::Result<()> {
            Ok(())
        }
    }

    pub(crate) fn make_device_settings(name: &str, device_type: DeviceType) -> DeviceSettings {
        DeviceSettings {
            volume_percent: 50.0,
            ..DeviceSettings::new(name.to_string(), device_type)
        }
    }

    // --- check_and_unmute_device tests ---

    #[test]
    fn check_and_unmute_unmutes_muted_device() {
        let device = MockDevice::new("dev1", "Speaker", true);
        *device.muted.borrow_mut() = true;
        let mut times = HashMap::new();

        let result = check_and_unmute_device(&device, false, "Unmuted", "was unmuted", &mut times);
        assert!(result.is_ok());
        assert!(!*device.muted.borrow());
    }

    #[test]
    fn check_and_unmute_leaves_unmuted_device() {
        let device = MockDevice::new("dev1", "Speaker", true);
        let mut times = HashMap::new();

        let result = check_and_unmute_device(&device, false, "Unmuted", "was unmuted", &mut times);
        assert!(result.is_ok());
        assert!(!*device.muted.borrow());
    }

    // --- get_unmute_notification_details tests ---

    #[test]
    fn unmute_notification_details_output() {
        let (title, suffix) = get_unmute_notification_details(DeviceType::Output);
        assert_eq!(title, "Output Device Unmuted");
        assert!(suffix.contains("unmuted"));
    }

    #[test]
    fn unmute_notification_details_input() {
        let (title, suffix) = get_unmute_notification_details(DeviceType::Input);
        assert_eq!(title, "Input Device Unmuted");
        assert!(suffix.contains("unmuted"));
    }

    // --- Integration test ---

    #[test]
    fn migrate_then_enforce_integration() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("new_id", "Speakers", true),
            MockDevice::new("other", "Headphones", true),
        ]);
        backend.set_default("other", DeviceType::Output);

        let mut state = PersistentState::default();
        state.devices.insert(
            "old_id".to_string(),
            make_device_settings("Speakers", DeviceType::Output),
        );
        state.output_priority_list = vec!["old_id".to_string()];

        migrate_device_ids(&backend, &mut state);
        assert!(state.devices.contains_key("new_id"));
        assert!(!state.devices.contains_key("old_id"));
        assert_eq!(state.output_priority_list, vec!["new_id"]);

        let mut last_notification_times = HashMap::new();
        let temp_priorities = TemporaryPriorities {
            output: None,
            input: None,
        };
        enforce_priorities(
            &backend,
            &state,
            &mut last_notification_times,
            &temp_priorities,
        );

        let default = backend
            .get_default_device(DeviceType::Output, DeviceRole::Console)
            .unwrap();
        assert_eq!(default.id(), "new_id");
    }
}
