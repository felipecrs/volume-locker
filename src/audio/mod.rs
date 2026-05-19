use crate::types::{DeviceId, DeviceRole, DeviceType, VolumeScalar};

#[cfg(target_os = "windows")]
mod windows_com_policy_config;

pub trait AudioBackend {
    fn devices(&self, device_type: DeviceType) -> anyhow::Result<Vec<Box<dyn AudioDevice>>>;
    fn device_by_id(&self, id: &DeviceId) -> anyhow::Result<Box<dyn AudioDevice>>;
    fn default_device(
        &self,
        device_type: DeviceType,
        role: DeviceRole,
    ) -> anyhow::Result<Box<dyn AudioDevice>>;
    fn set_default_device(&self, device_id: &DeviceId, role: DeviceRole) -> anyhow::Result<()>;

    fn register_device_change_callback(
        &self,
        callback: Box<dyn Fn() + Send + Sync>,
    ) -> anyhow::Result<()>;
}

pub trait AudioDevice {
    fn id(&self) -> &DeviceId;
    fn name(&self) -> String;
    fn volume(&self) -> anyhow::Result<VolumeScalar>;
    fn set_volume(&self, volume: VolumeScalar) -> anyhow::Result<()>;
    fn is_muted(&self) -> anyhow::Result<bool>;
    fn set_mute(&self, muted: bool) -> anyhow::Result<()>;
    fn is_active(&self) -> anyhow::Result<bool>;

    fn watch_volume(
        &self,
        callback: Box<dyn Fn(Option<VolumeScalar>) + Send + Sync>,
    ) -> anyhow::Result<()>;
}

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use self::windows::WindowsAudioBackend as AudioBackendImpl;

mod migration;
mod priority;

pub use migration::migrate_device_ids;
pub use priority::enforce_priorities;

use crate::notification::NotificationThrottler;

/// Best-effort unmute enforcement. Logs errors internally — callers do not
/// need to handle failures since this is a background enforcement operation.
pub fn check_and_unmute_device(
    device: &dyn AudioDevice,
    device_type: DeviceType,
    notify: bool,
    throttler: &mut NotificationThrottler,
) {
    let is_muted = match device.is_muted() {
        Ok(m) => m,
        Err(e) => {
            log::warn!("Failed to check mute state of {}: {e:#}", device.name());
            return;
        }
    };
    if !is_muted {
        return;
    }
    if let Err(e) = device.set_mute(false) {
        log::error!("Failed to unmute {}: {e:#}", device.name());
        return;
    }
    let device_name = device.name();
    log::info!("Unmuted {device_name} due to lock settings");
    if notify {
        let (notification_title, notification_suffix) =
            get_unmute_notification_details(device_type);
        let message = format!("{device_name} {notification_suffix}");
        throttler.send_if_not_throttled(
            &format!("unmute_{id}", id = device.id()),
            notification_title,
            &message,
        );
    }
}

pub fn enforce_volume_lock(
    device_id: &DeviceId,
    device: &dyn AudioDevice,
    device_name: &str,
    lock: crate::types::VolumeLockPolicy,
    new_volume: VolumeScalar,
    throttler: &mut NotificationThrottler,
) {
    let new_volume_percent = new_volume.to_percent();
    let target_volume_percent = lock.target_percent;
    if new_volume_percent == target_volume_percent {
        return;
    }

    let target_volume = target_volume_percent.to_scalar();

    if let Err(e) = device.set_volume(target_volume) {
        log::error!("Failed to set volume of {device_name} to {target_volume_percent}%: {e:#}");
        return;
    }
    log::info!(
        "Restored volume of {device_name} from {new_volume_percent}% to {target_volume_percent}%"
    );
    if lock.notify {
        throttler.send_if_not_throttled(
            &format!("volume_restore_{device_id}"),
            "Volume Restored",
            &format!(
                "The volume of {device_name} has been restored from {new_volume_percent}% to {target_volume_percent}%."
            ),
        );
    }
}

fn get_unmute_notification_details(device_type: DeviceType) -> (&'static str, &'static str) {
    let title = match device_type {
        DeviceType::Input => "Input Device Unmuted",
        DeviceType::Output => "Output Device Unmuted",
    };
    (title, "was unmuted due to Keep unmuted setting.")
}

/// Returns a list of `(device_id, new_name, device_type)` tuples for all
/// known devices, so the caller can apply updates to persistent state.
pub fn collect_device_names(backend: &impl AudioBackend) -> Vec<(DeviceId, String, DeviceType)> {
    let mut updates = Vec::new();
    for device_type in [DeviceType::Output, DeviceType::Input] {
        let devices = backend.devices(device_type).unwrap_or_else(|e| {
            log::warn!("Failed to get {device_type:?} devices: {e:#}");
            Vec::new()
        });
        for device in devices {
            updates.push((device.id().clone(), device.name(), device_type));
        }
    }
    updates
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::config::PersistentState;
    use crate::notification::NotificationThrottler;
    use crate::types::{
        DeviceId, DeviceSettings, TemporaryPriorities, VolumePercent, VolumeScalar,
    };
    use std::cell::RefCell;
    use std::collections::HashMap;

    pub(crate) struct MockDevice {
        pub(crate) id: DeviceId,
        pub(crate) name: String,
        pub(crate) active: bool,
        pub(crate) device_type: DeviceType,
        pub(crate) volume: RefCell<f32>,
        pub(crate) muted: RefCell<bool>,
    }

    impl MockDevice {
        pub(crate) fn new(id: &str, name: &str, active: bool) -> Self {
            Self {
                id: DeviceId::from(id),
                name: name.to_string(),
                active,
                device_type: DeviceType::Output,
                volume: RefCell::new(1.0),
                muted: RefCell::new(false),
            }
        }
    }

    impl AudioDevice for MockDevice {
        fn id(&self) -> &DeviceId {
            &self.id
        }
        fn name(&self) -> String {
            self.name.clone()
        }
        fn volume(&self) -> anyhow::Result<VolumeScalar> {
            Ok(VolumeScalar::from(*self.volume.borrow()))
        }
        fn set_volume(&self, volume: VolumeScalar) -> anyhow::Result<()> {
            *self.volume.borrow_mut() = volume.as_f32();
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
            _callback: Box<dyn Fn(Option<VolumeScalar>) + Send + Sync>,
        ) -> anyhow::Result<()> {
            Ok(())
        }
    }

    pub(crate) struct MockAudioBackend {
        pub(crate) devices: Vec<MockDevice>,
        pub(crate) default_console: RefCell<HashMap<DeviceType, String>>,
        pub(crate) default_multimedia: RefCell<HashMap<DeviceType, String>>,
        pub(crate) default_communications: RefCell<HashMap<DeviceType, String>>,
        /// Device IDs for which `get_device_by_id` will return `Err`.
        pub(crate) failing_device_ids: RefCell<Vec<String>>,
        /// If true, `set_default_device` will return `Err`.
        pub(crate) set_default_fails: RefCell<bool>,
    }

    impl MockAudioBackend {
        pub(crate) fn new(devices: Vec<MockDevice>) -> Self {
            Self {
                devices,
                default_console: RefCell::new(HashMap::new()),
                default_multimedia: RefCell::new(HashMap::new()),
                default_communications: RefCell::new(HashMap::new()),
                failing_device_ids: RefCell::new(Vec::new()),
                set_default_fails: RefCell::new(false),
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
        fn devices(&self, device_type: DeviceType) -> anyhow::Result<Vec<Box<dyn AudioDevice>>> {
            Ok(self
                .devices
                .iter()
                .filter(|d| d.device_type == device_type)
                .map(|d| {
                    Box::new(MockDevice::new(&d.id, &d.name, d.active)) as Box<dyn AudioDevice>
                })
                .collect())
        }

        fn device_by_id(&self, id: &DeviceId) -> anyhow::Result<Box<dyn AudioDevice>> {
            if self.failing_device_ids.borrow().iter().any(|f| **f == **id) {
                return Err(anyhow::anyhow!("Injected error for device: {id}"));
            }
            self.devices
                .iter()
                .find(|d| d.id == **id)
                .map(|d| {
                    Box::new(MockDevice::new(&d.id, &d.name, d.active)) as Box<dyn AudioDevice>
                })
                .ok_or_else(|| anyhow::anyhow!("Device not found: {id}"))
        }

        fn default_device(
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
            self.device_by_id(&DeviceId::from(id))
        }

        fn set_default_device(&self, device_id: &DeviceId, role: DeviceRole) -> anyhow::Result<()> {
            if *self.set_default_fails.borrow() {
                return Err(anyhow::anyhow!("Injected set_default_device failure"));
            }
            let device_type = self
                .devices
                .iter()
                .find(|d| d.id == **device_id)
                .map_or(DeviceType::Output, |d| d.device_type);
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
            &self,
            _callback: Box<dyn Fn() + Send + Sync>,
        ) -> anyhow::Result<()> {
            Ok(())
        }
    }

    pub(crate) fn make_device_settings(name: &str, device_type: DeviceType) -> DeviceSettings {
        DeviceSettings {
            volume_lock: crate::types::VolumeLockPolicy {
                target_percent: VolumePercent::from(50.0),
                ..Default::default()
            },
            ..DeviceSettings::new(name.to_string(), device_type)
        }
    }

    // --- check_and_unmute_device tests ---

    #[test]
    fn check_and_unmute_unmutes_muted_device() {
        let device = MockDevice::new("dev1", "Speaker", true);
        *device.muted.borrow_mut() = true;
        let mut throttler = NotificationThrottler::new();

        check_and_unmute_device(&device, DeviceType::Output, false, &mut throttler);
        assert!(!*device.muted.borrow());
    }

    #[test]
    fn check_and_unmute_leaves_unmuted_device() {
        let device = MockDevice::new("dev1", "Speaker", true);
        let mut throttler = NotificationThrottler::new();

        check_and_unmute_device(&device, DeviceType::Output, false, &mut throttler);
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

    // --- Error path tests ---

    #[test]
    fn enforce_priorities_continues_when_device_lookup_fails() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("dev1", "Speaker", true),
            MockDevice::new("dev2", "Headphones", true),
        ]);
        backend.set_default("dev2", DeviceType::Output);
        // Make dev1 fail on lookup — enforce should skip it and not switch
        backend.failing_device_ids.borrow_mut().push("dev1".into());

        let mut state = PersistentState::default();
        state.output.priority_list = vec!["dev1".into(), "dev2".into()];
        let mut throttler = NotificationThrottler::new();
        let temp = TemporaryPriorities {
            output: None,
            input: None,
        };

        // Should not panic; dev1 lookup fails, but dev2 is already default
        enforce_priorities(&backend, &state, &mut throttler, &temp);

        let default = backend
            .default_device(DeviceType::Output, DeviceRole::Console)
            .unwrap();
        assert_eq!(default.id(), "dev2");
    }

    #[test]
    fn enforce_priorities_skips_failed_lookups_and_uses_next() {
        // dev1 is higher priority but its lookup fails — should fall back to dev2
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("dev1", "Speaker", true),
            MockDevice::new("dev2", "Headphones", true),
            MockDevice::new("dev3", "Monitor", true),
        ]);
        backend.set_default("dev3", DeviceType::Output);
        backend.failing_device_ids.borrow_mut().push("dev1".into());

        let mut state = PersistentState::default();
        state.output.priority_list = vec!["dev1".into(), "dev2".into()];
        let mut throttler = NotificationThrottler::new();
        let temp = TemporaryPriorities {
            output: None,
            input: None,
        };

        enforce_priorities(&backend, &state, &mut throttler, &temp);

        // dev1 failed lookup, so dev2 should become default
        let default = backend
            .default_device(DeviceType::Output, DeviceRole::Console)
            .unwrap();
        assert_eq!(*default.id(), *DeviceId::from("dev2"));
    }

    #[test]
    fn enforce_priorities_handles_set_default_failure() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("dev1", "Speaker", true),
            MockDevice::new("dev2", "Headphones", true),
        ]);
        backend.set_default("dev2", DeviceType::Output);
        *backend.set_default_fails.borrow_mut() = true;

        let mut state = PersistentState::default();
        state.output.priority_list = vec!["dev1".into(), "dev2".into()];
        let mut throttler = NotificationThrottler::new();
        let temp = TemporaryPriorities {
            output: None,
            input: None,
        };

        // Should not panic even though set_default_device fails
        enforce_priorities(&backend, &state, &mut throttler, &temp);
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
        state.output.priority_list = vec![DeviceId::from("old_id")];
        state.devices.insert(
            DeviceId::from("old_id"),
            make_device_settings("Speakers", DeviceType::Output),
        );

        migrate_device_ids(&backend, &mut state);
        assert!(state.devices.contains_key("new_id"));
        assert!(!state.devices.contains_key("old_id"));
        assert_eq!(state.output.priority_list, vec!["new_id"]);

        let mut throttler = NotificationThrottler::new();
        let temp_priorities = TemporaryPriorities {
            output: None,
            input: None,
        };
        enforce_priorities(&backend, &state, &mut throttler, &temp_priorities);

        let default = backend
            .default_device(DeviceType::Output, DeviceRole::Console)
            .unwrap();
        assert_eq!(default.id(), "new_id");
    }
}
