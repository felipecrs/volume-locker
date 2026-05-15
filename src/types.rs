use serde::{Deserialize, Serialize};
use std::borrow::Borrow;
use std::fmt;

/// Volume level in the 0.0–1.0 range used by the Windows audio API.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct VolumeScalar(f32);

impl VolumeScalar {
    pub fn as_f32(self) -> f32 {
        self.0
    }

    pub fn to_percent(self) -> VolumePercent {
        VolumePercent((self.0 * 100.0).round())
    }
}

impl From<f32> for VolumeScalar {
    fn from(v: f32) -> Self {
        Self(v.clamp(0.0, 1.0))
    }
}

/// Volume level expressed as a 0–100 percentage.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VolumePercent(f32);

impl VolumePercent {
    pub fn as_f32(self) -> f32 {
        self.0
    }

    pub fn to_scalar(self) -> VolumeScalar {
        VolumeScalar(self.0 / 100.0)
    }
}

impl fmt::Display for VolumePercent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<f32> for VolumePercent {
    fn from(v: f32) -> Self {
        Self(v.clamp(0.0, 100.0))
    }
}

impl PartialEq<f32> for VolumePercent {
    fn eq(&self, other: &f32) -> bool {
        self.0 == *other
    }
}

/// A strongly-typed wrapper around a device identifier string.
/// Prevents accidental confusion between device IDs and device names.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct DeviceId(String);

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::ops::Deref for DeviceId {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for DeviceId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl From<String> for DeviceId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for DeviceId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl PartialEq<str> for DeviceId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for DeviceId {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<String> for DeviceId {
    fn eq(&self, other: &String) -> bool {
        self.0 == *other
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceType {
    Input,
    Output,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceRole {
    Console,
    Multimedia,
    Communications,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default)]
pub struct VolumeLockPolicy {
    #[serde(default, rename = "is_volume_locked")]
    pub is_locked: bool,
    #[serde(
        default,
        rename = "volume_percent",
        deserialize_with = "deserialize_clamped_percent"
    )]
    pub target_percent: VolumePercent,
    #[serde(default, rename = "notify_on_volume_lock")]
    pub notify: bool,
}

fn deserialize_clamped_percent<'de, D>(deserializer: D) -> Result<VolumePercent, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value: f32 = serde::Deserialize::deserialize(deserializer)?;
    Ok(VolumePercent(value.clamp(0.0, 100.0)))
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default)]
pub struct UnmuteLockPolicy {
    #[serde(default, rename = "is_unmute_locked")]
    pub is_locked: bool,
    #[serde(default, rename = "notify_on_unmute_lock")]
    pub notify: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeviceSettings {
    #[serde(flatten)]
    pub volume_lock: VolumeLockPolicy,
    #[serde(flatten)]
    pub unmute_lock: UnmuteLockPolicy,
    pub device_type: DeviceType,
    pub name: String,
}

impl DeviceSettings {
    pub fn new(name: String, device_type: DeviceType) -> Self {
        Self {
            volume_lock: VolumeLockPolicy::default(),
            unmute_lock: UnmuteLockPolicy::default(),
            device_type,
            name,
        }
    }

    /// Returns true if the device has any active volume/unmute lock or notification setting.
    /// Used to decide whether a `DeviceSettings` entry can be pruned when no longer referenced
    /// by a priority list.
    pub fn has_active_locks_or_notifications(&self) -> bool {
        self.volume_lock.is_locked
            || self.unmute_lock.is_locked
            || self.volume_lock.notify
            || self.unmute_lock.notify
    }
}

#[derive(Debug)]
pub enum DeviceAction {
    VolumeLock,
    VolumeLockNotify,
    UnmuteLock,
    UnmuteLockNotify,
    AddToPriority,
    RemoveFromPriority,
    MovePriorityUp,
    MovePriorityDown,
    MovePriorityToTop,
    MovePriorityToBottom,
    SetTemporaryPriority,
    OpenProperties,
    OpenSettings,
}

#[derive(Debug)]
pub enum PreferenceAction {
    PriorityRestoreNotify,
    SwitchCommunicationDevice,
    OpenDevicesList,
}

#[derive(Debug)]
pub enum AppAction {
    OpenSoundSettings,
    OpenVolumeMixer,
    CheckForUpdates,
    PerformUpdate,
    OpenGitHubRepo,
    OpenAppDirectory,
    ToggleAutoLaunch,
    ToggleCheckUpdatesOnLaunch,
}

#[derive(Debug)]
pub enum MenuAction {
    Device {
        device_id: DeviceId,
        device_type: DeviceType,
        action: DeviceAction,
    },
    Preference {
        device_type: DeviceType,
        action: PreferenceAction,
    },
    App(AppAction),
}

#[derive(Debug)]
pub struct MenuItemInfo {
    pub name: String,
    pub action: MenuAction,
}

#[derive(Debug)]
pub struct VolumeChangedEvent {
    pub device_id: DeviceId,
    pub new_volume: Option<VolumeScalar>,
}

pub struct TemporaryPriorities {
    pub output: Option<DeviceId>,
    pub input: Option<DeviceId>,
}

impl TemporaryPriorities {
    pub fn get(&self, device_type: DeviceType) -> Option<&DeviceId> {
        match device_type {
            DeviceType::Output => self.output.as_ref(),
            DeviceType::Input => self.input.as_ref(),
        }
    }

    pub fn set(&mut self, device_type: DeviceType, value: Option<DeviceId>) {
        match device_type {
            DeviceType::Output => self.output = value,
            DeviceType::Input => self.input = value,
        }
    }
}

#[derive(Debug)]
pub enum UserEvent {
    TrayIcon(tray_icon::TrayIconEvent),
    Menu(tray_icon::menu::MenuEvent),
    VolumeChanged(VolumeChangedEvent),
    DevicesChanged,
    ConfigurationChanged,
}

#[cfg(test)]
mod tests {
    use super::{DeviceSettings, DeviceType, VolumePercent, VolumeScalar};

    #[test]
    fn device_type_serialization_roundtrip() {
        let output_json = serde_json::to_string(&DeviceType::Output).unwrap();
        let input_json = serde_json::to_string(&DeviceType::Input).unwrap();
        assert_eq!(
            serde_json::from_str::<DeviceType>(&output_json).unwrap(),
            DeviceType::Output
        );
        assert_eq!(
            serde_json::from_str::<DeviceType>(&input_json).unwrap(),
            DeviceType::Input
        );
    }

    #[test]
    fn device_settings_default_fields() {
        let json = r#"{"device_type": "Output", "name": "Test"}"#;
        let settings: DeviceSettings = serde_json::from_str(json).unwrap();
        assert!(!settings.volume_lock.is_locked);
        assert_eq!(settings.volume_lock.target_percent, 0.0);
        assert!(!settings.volume_lock.notify);
        assert!(!settings.unmute_lock.is_locked);
        assert!(!settings.unmute_lock.notify);
        assert_eq!(settings.device_type, DeviceType::Output);
        assert_eq!(settings.name, "Test");
    }

    #[test]
    fn device_settings_full_roundtrip() {
        use super::{UnmuteLockPolicy, VolumeLockPolicy, VolumePercent};
        let settings = DeviceSettings {
            volume_lock: VolumeLockPolicy {
                is_locked: true,
                target_percent: VolumePercent::from(75.0),
                notify: true,
            },
            unmute_lock: UnmuteLockPolicy {
                is_locked: true,
                notify: false,
            },
            device_type: DeviceType::Input,
            name: "Microphone".into(),
        };
        let json = serde_json::to_string(&settings).unwrap();
        let loaded: DeviceSettings = serde_json::from_str(&json).unwrap();
        assert!(loaded.volume_lock.is_locked);
        assert_eq!(loaded.volume_lock.target_percent, 75.0);
        assert!(loaded.volume_lock.notify);
        assert!(loaded.unmute_lock.is_locked);
        assert!(!loaded.unmute_lock.notify);
        assert_eq!(loaded.device_type, DeviceType::Input);
        assert_eq!(loaded.name, "Microphone");
    }

    #[test]
    fn convert_float_to_percent_zero() {
        assert_eq!(VolumeScalar::from(0.0).to_percent().as_f32(), 0.0);
    }

    #[test]
    fn convert_float_to_percent_full() {
        assert_eq!(VolumeScalar::from(1.0).to_percent().as_f32(), 100.0);
    }

    #[test]
    fn convert_float_to_percent_half() {
        assert_eq!(VolumeScalar::from(0.5).to_percent().as_f32(), 50.0);
    }

    #[test]
    fn convert_float_to_percent_rounds() {
        assert_eq!(VolumeScalar::from(0.333).to_percent().as_f32(), 33.0);
        assert_eq!(VolumeScalar::from(0.335).to_percent().as_f32(), 34.0);
    }

    #[test]
    fn convert_percent_to_float_zero() {
        assert_eq!(VolumePercent::from(0.0).to_scalar().as_f32(), 0.0);
    }

    #[test]
    fn convert_percent_to_float_full() {
        assert_eq!(VolumePercent::from(100.0).to_scalar().as_f32(), 1.0);
    }

    #[test]
    fn convert_percent_to_float_half() {
        assert_eq!(VolumePercent::from(50.0).to_scalar().as_f32(), 0.5);
    }

    #[test]
    fn roundtrip_float_percent() {
        let original = 0.75;
        let percent = VolumeScalar::from(original).to_percent();
        let back = percent.to_scalar().as_f32();
        assert_eq!(back, original);
    }

    #[test]
    fn volume_scalar_clamps_above_one() {
        assert_eq!(VolumeScalar::from(1.5).as_f32(), 1.0);
    }

    #[test]
    fn volume_percent_clamps_above_100() {
        assert_eq!(VolumePercent::from(200.0).as_f32(), 100.0);
    }

    #[test]
    fn volume_scalar_clamps_below_zero() {
        assert_eq!(VolumeScalar::from(-0.1).as_f32(), 0.0);
    }

    #[test]
    fn volume_percent_clamps_below_zero() {
        assert_eq!(VolumePercent::from(-10.0).as_f32(), 0.0);
    }
}
