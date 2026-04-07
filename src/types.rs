use serde::{Deserialize, Serialize};

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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeviceSettings {
    #[serde(default)]
    pub is_volume_locked: bool,
    #[serde(default)]
    pub volume_percent: f32,
    #[serde(default)]
    pub notify_on_volume_lock: bool,
    #[serde(default)]
    pub is_unmute_locked: bool,
    #[serde(default)]
    pub notify_on_unmute_lock: bool,
    pub device_type: DeviceType,
    pub name: String,
}

#[derive(Debug)]
pub enum DeviceSettingType {
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
    PriorityRestoreNotify,
    SwitchCommunicationDevice,
    SetTemporaryPriority,
    OpenDevicesList,
    OpenDeviceProperties,
    OpenSoundSettings,
    OpenDeviceSettings,
    OpenVolumeMixer,
    CheckForUpdates,
    PerformUpdate,
    OpenGitHubRepo,
    OpenAppDirectory,
}

#[derive(Debug)]
pub struct MenuItemDeviceInfo {
    pub device_id: Option<String>,
    pub setting_type: DeviceSettingType,
    pub name: String,
    pub device_type: Option<DeviceType>,
}

#[derive(Debug)]
pub struct VolumeChangedEvent {
    pub device_id: String,
    pub new_volume: Option<f32>,
}

pub struct TemporaryPriorities {
    pub output: Option<String>,
    pub input: Option<String>,
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
    use super::*;

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
        assert!(!settings.is_volume_locked);
        assert_eq!(settings.volume_percent, 0.0);
        assert!(!settings.notify_on_volume_lock);
        assert!(!settings.is_unmute_locked);
        assert!(!settings.notify_on_unmute_lock);
        assert_eq!(settings.device_type, DeviceType::Output);
        assert_eq!(settings.name, "Test");
    }

    #[test]
    fn device_settings_full_roundtrip() {
        let settings = DeviceSettings {
            is_volume_locked: true,
            volume_percent: 75.0,
            notify_on_volume_lock: true,
            is_unmute_locked: true,
            notify_on_unmute_lock: false,
            device_type: DeviceType::Input,
            name: "Microphone".into(),
        };
        let json = serde_json::to_string(&settings).unwrap();
        let loaded: DeviceSettings = serde_json::from_str(&json).unwrap();
        assert!(loaded.is_volume_locked);
        assert_eq!(loaded.volume_percent, 75.0);
        assert!(loaded.notify_on_volume_lock);
        assert!(loaded.is_unmute_locked);
        assert!(!loaded.notify_on_unmute_lock);
        assert_eq!(loaded.device_type, DeviceType::Input);
        assert_eq!(loaded.name, "Microphone");
    }

    #[test]
    fn device_type_equality_and_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(DeviceType::Output);
        set.insert(DeviceType::Input);
        set.insert(DeviceType::Output); // duplicate
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn device_type_clone_and_copy() {
        let dt = DeviceType::Output;
        let cloned = dt;
        let copied = dt;
        assert_eq!(dt, cloned);
        assert_eq!(dt, copied);
    }

    #[test]
    fn device_role_variants() {
        let roles = [
            DeviceRole::Console,
            DeviceRole::Multimedia,
            DeviceRole::Communications,
        ];
        assert_eq!(roles.len(), 3);
        assert_ne!(DeviceRole::Console, DeviceRole::Multimedia);
        assert_ne!(DeviceRole::Console, DeviceRole::Communications);
        assert_ne!(DeviceRole::Multimedia, DeviceRole::Communications);
    }
}
