use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
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
}

#[derive(Debug)]
pub struct MenuItemDeviceInfo {
    pub device_id: String,
    pub setting_type: DeviceSettingType,
    pub name: String,
    pub device_type: DeviceType,
}

#[derive(Debug)]
pub struct VolumeChangedEvent {
    pub device_id: String,
    pub new_volume: Option<f32>,
}

#[derive(Debug)]
pub enum UserEvent {
    TrayIcon(tray_icon::TrayIconEvent),
    Menu(tray_icon::menu::MenuEvent),
    VolumeChanged(VolumeChangedEvent),
    DevicesChanged,
    ConfigurationChanged,
}
