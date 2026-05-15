use crate::audio::AudioBackend;
use crate::config::PersistentState;
use crate::consts::GITHUB_REPO_URL;
use crate::platform::{
    open_device_settings, open_devices_list, open_sound_control_panel, open_sound_settings,
    open_volume_mixer,
};
use crate::types::{
    AppAction, DeviceAction, DeviceId, DeviceSettings, DeviceType, MenuAction, MenuItemInfo,
    PreferenceAction, TemporaryPriorities,
};
use crate::update::UpdateInfo;
use crate::notification::log_and_notify_error;
use crate::utils::{get_executable_directory, open_path, open_url};
use tray_icon::menu::Menu;

use super::find_menu_item;

fn get_check_item_state(menu: &Menu, id: &tray_icon::menu::MenuId) -> Option<bool> {
    find_menu_item(menu, id).and_then(|item| item.as_check_menuitem().map(|c| c.is_checked()))
}

/// Reads a check-menu-item's state, applies `f` with it, and returns `SaveConfig`.
/// Returns `NoChange` if the menu item can't be found.
fn with_check_state(
    menu: &Menu,
    id: &tray_icon::menu::MenuId,
    f: impl FnOnce(bool),
) -> MenuEventResult {
    match get_check_item_state(menu, id) {
        Some(checked) => {
            f(checked);
            MenuEventResult::SaveConfig
        }
        None => MenuEventResult::NoChange,
    }
}

pub enum MenuEventResult {
    NoChange,
    SaveConfig,
    DevicesChanged,
    UpdateCheck,
    UpdatePerform(UpdateInfo),
    ToggleAutoLaunch(bool),
}

/// Returns `true` if the device has no active locks or notifications,
/// meaning its settings entry can be removed when not in a priority list.
#[cfg(test)]
fn device_settings_are_empty(settings: &DeviceSettings) -> bool {
    !settings.has_active_locks_or_notifications()
}

/// Applies a device lock/notify toggle to the device's settings entry.
fn apply_device_lock_toggle(
    action: &DeviceAction,
    is_checked: bool,
    device_id: &DeviceId,
    device_name: &str,
    device_type: DeviceType,
    persistent_state: &mut PersistentState,
    backend: &impl AudioBackend,
) {
    let device_settings = persistent_state
        .devices
        .entry(device_id.clone())
        .or_insert_with(|| DeviceSettings::new(device_name.to_string(), device_type));

    match action {
        DeviceAction::VolumeLock => {
            if is_checked {
                if let Ok(device) = backend.device_by_id(device_id)
                    && let Ok(vol) = device.volume()
                {
                    device_settings.volume_lock.target_percent = vol.to_percent();
                    device_settings.volume_lock.is_locked = true;
                } else {
                    log_and_notify_error(
                        "Failed to Lock Volume",
                        &format!("Failed to get volume for device {device_name}, cannot lock."),
                    );
                    device_settings.volume_lock.is_locked = false;
                }
            } else {
                device_settings.volume_lock.is_locked = false;
            }
        }
        DeviceAction::VolumeLockNotify => {
            device_settings.volume_lock.notify = is_checked;
        }
        DeviceAction::UnmuteLock => {
            device_settings.unmute_lock.is_locked = is_checked;
        }
        DeviceAction::UnmuteLockNotify => {
            device_settings.unmute_lock.notify = is_checked;
        }
        _ => {}
    }
}

fn handle_priority_event(
    action: &DeviceAction,
    device_id: &DeviceId,
    device_type: DeviceType,
    device_name: &str,
    persistent_state: &mut PersistentState,
) -> bool {
    match action {
        DeviceAction::AddToPriority => {
            let list = persistent_state.priority_list_mut(device_type);
            if !list.iter().any(|x| *x == **device_id) {
                list.push(device_id.clone());
                persistent_state
                    .devices
                    .entry(device_id.clone())
                    .or_insert_with(|| DeviceSettings::new(device_name.to_string(), device_type));
                true
            } else {
                false
            }
        }
        DeviceAction::RemoveFromPriority => {
            let list = persistent_state.priority_list_mut(device_type);
            if let Some(pos) = list.iter().position(|x| x == device_id) {
                list.remove(pos);
                persistent_state.remove_device_if_unused(device_id);
                true
            } else {
                false
            }
        }
        DeviceAction::MovePriorityUp
        | DeviceAction::MovePriorityDown
        | DeviceAction::MovePriorityToTop
        | DeviceAction::MovePriorityToBottom => {
            move_priority_item(action, device_id, device_type, persistent_state)
        }
        _ => false,
    }
}

fn move_priority_item(
    action: &DeviceAction,
    device_id: &DeviceId,
    device_type: DeviceType,
    persistent_state: &mut PersistentState,
) -> bool {
    let list = persistent_state.priority_list_mut(device_type);
    let Some(pos) = list.iter().position(|x| x == device_id) else {
        return false;
    };

    match action {
        DeviceAction::MovePriorityUp if pos > 0 => {
            list.swap(pos, pos - 1);
            true
        }
        DeviceAction::MovePriorityDown if pos < list.len() - 1 => {
            list.swap(pos, pos + 1);
            true
        }
        DeviceAction::MovePriorityToTop if pos > 0 => {
            let id = list.remove(pos);
            list.insert(0, id);
            true
        }
        DeviceAction::MovePriorityToBottom if pos < list.len() - 1 => {
            let id = list.remove(pos);
            list.push(id);
            true
        }
        _ => false,
    }
}

pub struct MenuEventContext<'a, B: AudioBackend> {
    pub tray_menu: &'a Menu,
    pub persistent_state: &'a mut PersistentState,
    pub backend: &'a B,
    pub temporary_priorities: &'a mut TemporaryPriorities,
    pub update_info: &'a Option<UpdateInfo>,
}

pub fn handle_menu_event(
    event: &tray_icon::menu::MenuEvent,
    menu_info: &MenuItemInfo,
    ctx: &mut MenuEventContext<'_, impl AudioBackend>,
) -> MenuEventResult {
    match &menu_info.action {
        MenuAction::Device {
            device_id,
            device_type,
            action,
        } => match action {
            DeviceAction::VolumeLock
            | DeviceAction::VolumeLockNotify
            | DeviceAction::UnmuteLock
            | DeviceAction::UnmuteLockNotify => {
                if let Some(is_checked) = get_check_item_state(ctx.tray_menu, &event.id) {
                    apply_device_lock_toggle(
                        action,
                        is_checked,
                        device_id,
                        &menu_info.name,
                        *device_type,
                        ctx.persistent_state,
                        ctx.backend,
                    );

                    ctx.persistent_state.remove_device_if_unused(device_id);
                    MenuEventResult::SaveConfig
                } else {
                    MenuEventResult::NoChange
                }
            }
            DeviceAction::AddToPriority
            | DeviceAction::RemoveFromPriority
            | DeviceAction::MovePriorityUp
            | DeviceAction::MovePriorityDown
            | DeviceAction::MovePriorityToTop
            | DeviceAction::MovePriorityToBottom => {
                if handle_priority_event(
                    action,
                    device_id,
                    *device_type,
                    &menu_info.name,
                    ctx.persistent_state,
                ) {
                    MenuEventResult::SaveConfig
                } else {
                    MenuEventResult::NoChange
                }
            }
            DeviceAction::SetTemporaryPriority => {
                let is_checked = get_check_item_state(ctx.tray_menu, &event.id).unwrap_or(false);

                ctx.temporary_priorities.set(
                    *device_type,
                    if is_checked {
                        Some(device_id.clone())
                    } else {
                        None
                    },
                );
                MenuEventResult::DevicesChanged
            }
            DeviceAction::OpenProperties => {
                let tab = match device_type {
                    DeviceType::Output => "0",
                    DeviceType::Input => "1",
                };
                if let Err(e) = open_sound_control_panel(tab) {
                    log::error!("Failed to open sound control panel: {e:#}");
                }
                MenuEventResult::NoChange
            }
            DeviceAction::OpenSettings => {
                if let Err(e) = open_device_settings(device_id) {
                    log::error!("Failed to open device settings: {e:#}");
                }
                MenuEventResult::NoChange
            }
        },
        MenuAction::Preference {
            device_type,
            action,
        } => match action {
            PreferenceAction::PriorityRestoreNotify => {
                let dt = *device_type;
                with_check_state(ctx.tray_menu, &event.id, |checked| {
                    ctx.persistent_state.set_notify_on_priority_restore(dt, checked);
                })
            }
            PreferenceAction::SwitchCommunicationDevice => {
                let dt = *device_type;
                with_check_state(ctx.tray_menu, &event.id, |checked| {
                    ctx.persistent_state.set_switch_communication_device(dt, checked);
                })
            }
            PreferenceAction::OpenDevicesList => {
                if let Err(e) = open_devices_list(*device_type) {
                    log::error!("Failed to open devices list: {e:#}");
                }
                MenuEventResult::NoChange
            }
        },
        MenuAction::App(action) => match action {
            AppAction::OpenSoundSettings => {
                if let Err(e) = open_sound_settings() {
                    log::error!("Failed to open sound settings: {e:#}");
                }
                MenuEventResult::NoChange
            }
            AppAction::OpenVolumeMixer => {
                if let Err(e) = open_volume_mixer() {
                    log::error!("Failed to open volume mixer: {e:#}");
                }
                MenuEventResult::NoChange
            }
            AppAction::CheckForUpdates => MenuEventResult::UpdateCheck,
            AppAction::PerformUpdate => {
                if let Some(info) = ctx.update_info {
                    MenuEventResult::UpdatePerform(info.clone())
                } else {
                    MenuEventResult::NoChange
                }
            }
            AppAction::OpenGitHubRepo => {
                if let Err(e) = open_url(GITHUB_REPO_URL) {
                    log::error!("Failed to open GitHub repo: {e:#}");
                }
                MenuEventResult::NoChange
            }
            AppAction::ToggleAutoLaunch => {
                if let Some(checked) = get_check_item_state(ctx.tray_menu, &event.id) {
                    MenuEventResult::ToggleAutoLaunch(checked)
                } else {
                    MenuEventResult::NoChange
                }
            }
            AppAction::ToggleCheckUpdatesOnLaunch => {
                with_check_state(ctx.tray_menu, &event.id, |checked| {
                    ctx.persistent_state.check_updates_on_launch = checked;
                })
            }
            AppAction::OpenAppDirectory => {
                match get_executable_directory() {
                    Ok(dir) => {
                        if let Err(e) = open_path(&dir) {
                            log::error!("Failed to open app directory: {e:#}");
                        }
                    }
                    Err(e) => log::error!("Failed to get executable directory: {e:#}"),
                }
                MenuEventResult::NoChange
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DeviceAction, DeviceId, DeviceSettings, DeviceType, PersistentState,
        device_settings_are_empty, handle_priority_event,
    };

    #[test]
    fn device_settings_empty_when_all_false() {
        let settings = DeviceSettings::new("Test".to_string(), DeviceType::Output);
        assert!(device_settings_are_empty(&settings));
    }

    #[test]
    fn device_settings_not_empty_when_locked() {
        let mut settings = DeviceSettings::new("Test".to_string(), DeviceType::Output);
        settings.volume_lock.is_locked = true;
        assert!(!device_settings_are_empty(&settings));
    }

    fn make_state_with_device(device_id: &str, device_type: DeviceType) -> PersistentState {
        let mut state = PersistentState::default();
        state.devices.insert(
            DeviceId::from(device_id),
            DeviceSettings::new("Test Device".to_string(), device_type),
        );
        state
    }

    #[test]
    fn priority_add_inserts_device() {
        let mut state = PersistentState::default();
        let changed = handle_priority_event(
            &DeviceAction::AddToPriority,
            &DeviceId::from("dev1"),
            DeviceType::Output,
            "Speaker",
            &mut state,
        );
        assert!(changed);
        assert_eq!(state.priority_list(DeviceType::Output), &["dev1"]);
        assert!(state.devices.contains_key("dev1"));
    }

    #[test]
    fn priority_add_duplicate_no_op() {
        let mut state = PersistentState::default();
        state
            .priority_list_mut(DeviceType::Output)
            .push("dev1".into());
        let changed = handle_priority_event(
            &DeviceAction::AddToPriority,
            &DeviceId::from("dev1"),
            DeviceType::Output,
            "Speaker",
            &mut state,
        );
        assert!(!changed);
    }

    #[test]
    fn priority_remove_cleans_empty_device() {
        let mut state = make_state_with_device("dev1", DeviceType::Output);
        state
            .priority_list_mut(DeviceType::Output)
            .push("dev1".into());
        let changed = handle_priority_event(
            &DeviceAction::RemoveFromPriority,
            &DeviceId::from("dev1"),
            DeviceType::Output,
            "Speaker",
            &mut state,
        );
        assert!(changed);
        assert!(state.priority_list(DeviceType::Output).is_empty());
        assert!(!state.devices.contains_key("dev1"));
    }

    #[test]
    fn priority_move_up() {
        let mut state = PersistentState::default();
        *state.priority_list_mut(DeviceType::Output) =
            vec!["a".into(), "b".into(), "c".into()];
        let changed = handle_priority_event(
            &DeviceAction::MovePriorityUp,
            &DeviceId::from("b"),
            DeviceType::Output,
            "B",
            &mut state,
        );
        assert!(changed);
        assert_eq!(state.priority_list(DeviceType::Output), &["b", "a", "c"]);
    }

    #[test]
    fn priority_move_up_already_top() {
        let mut state = PersistentState::default();
        *state.priority_list_mut(DeviceType::Output) = vec!["a".into(), "b".into()];
        let changed = handle_priority_event(
            &DeviceAction::MovePriorityUp,
            &DeviceId::from("a"),
            DeviceType::Output,
            "A",
            &mut state,
        );
        assert!(!changed);
    }

    #[test]
    fn priority_move_down() {
        let mut state = PersistentState::default();
        *state.priority_list_mut(DeviceType::Output) =
            vec!["a".into(), "b".into(), "c".into()];
        let changed = handle_priority_event(
            &DeviceAction::MovePriorityDown,
            &DeviceId::from("b"),
            DeviceType::Output,
            "B",
            &mut state,
        );
        assert!(changed);
        assert_eq!(state.priority_list(DeviceType::Output), &["a", "c", "b"]);
    }

    #[test]
    fn priority_move_to_top() {
        let mut state = PersistentState::default();
        *state.priority_list_mut(DeviceType::Output) =
            vec!["a".into(), "b".into(), "c".into()];
        let changed = handle_priority_event(
            &DeviceAction::MovePriorityToTop,
            &DeviceId::from("c"),
            DeviceType::Output,
            "C",
            &mut state,
        );
        assert!(changed);
        assert_eq!(state.priority_list(DeviceType::Output), &["c", "a", "b"]);
    }

    #[test]
    fn priority_move_to_bottom() {
        let mut state = PersistentState::default();
        *state.priority_list_mut(DeviceType::Output) =
            vec!["a".into(), "b".into(), "c".into()];
        let changed = handle_priority_event(
            &DeviceAction::MovePriorityToBottom,
            &DeviceId::from("a"),
            DeviceType::Output,
            "A",
            &mut state,
        );
        assert!(changed);
        assert_eq!(state.priority_list(DeviceType::Output), &["b", "c", "a"]);
    }

    #[test]
    fn priority_input_type_uses_input_list() {
        let mut state = PersistentState::default();
        handle_priority_event(
            &DeviceAction::AddToPriority,
            &DeviceId::from("mic1"),
            DeviceType::Input,
            "Mic",
            &mut state,
        );
        assert!(state.priority_list(DeviceType::Output).is_empty());
        assert_eq!(state.priority_list(DeviceType::Input), &["mic1"]);
    }

    // --- apply_device_lock_toggle tests ---

    use crate::audio::tests::MockAudioBackend;
    use crate::audio::tests::MockDevice;
    use super::apply_device_lock_toggle;

    fn make_backend_with_device(id: &str, name: &str) -> MockAudioBackend {
        let mut dev = MockDevice::new(id, name, true);
        dev.device_type = DeviceType::Output;
        MockAudioBackend::new(vec![dev])
    }

    #[test]
    fn volume_lock_enable_captures_current_volume() {
        let backend = make_backend_with_device("dev1", "Speaker");
        let mut state = PersistentState::default();

        apply_device_lock_toggle(
            &DeviceAction::VolumeLock,
            true,
            &DeviceId::from("dev1"),
            "Speaker",
            DeviceType::Output,
            &mut state,
            &backend,
        );

        let settings = state.devices.get("dev1").unwrap();
        assert!(settings.volume_lock.is_locked);
        // MockDevice::new creates devices with volume 1.0 (100%)
        assert_eq!(settings.volume_lock.target_percent, 100.0);
    }

    #[test]
    fn volume_lock_disable_clears_locked_state() {
        let backend = make_backend_with_device("dev1", "Speaker");
        let mut state = PersistentState::default();

        // Enable first
        apply_device_lock_toggle(
            &DeviceAction::VolumeLock,
            true,
            &DeviceId::from("dev1"),
            "Speaker",
            DeviceType::Output,
            &mut state,
            &backend,
        );
        assert!(state.devices.get("dev1").unwrap().volume_lock.is_locked);

        // Disable
        apply_device_lock_toggle(
            &DeviceAction::VolumeLock,
            false,
            &DeviceId::from("dev1"),
            "Speaker",
            DeviceType::Output,
            &mut state,
            &backend,
        );
        assert!(!state.devices.get("dev1").unwrap().volume_lock.is_locked);
    }

    #[test]
    fn volume_lock_fails_when_device_not_found() {
        // Empty backend — device lookup will fail
        let backend = MockAudioBackend::new(vec![]);
        let mut state = PersistentState::default();

        apply_device_lock_toggle(
            &DeviceAction::VolumeLock,
            true,
            &DeviceId::from("missing"),
            "Ghost",
            DeviceType::Output,
            &mut state,
            &backend,
        );

        let settings = state.devices.get("missing").unwrap();
        assert!(!settings.volume_lock.is_locked);
    }

    #[test]
    fn unmute_lock_toggle() {
        let backend = make_backend_with_device("dev1", "Speaker");
        let mut state = PersistentState::default();

        apply_device_lock_toggle(
            &DeviceAction::UnmuteLock,
            true,
            &DeviceId::from("dev1"),
            "Speaker",
            DeviceType::Output,
            &mut state,
            &backend,
        );
        assert!(state.devices.get("dev1").unwrap().unmute_lock.is_locked);

        apply_device_lock_toggle(
            &DeviceAction::UnmuteLock,
            false,
            &DeviceId::from("dev1"),
            "Speaker",
            DeviceType::Output,
            &mut state,
            &backend,
        );
        assert!(!state.devices.get("dev1").unwrap().unmute_lock.is_locked);
    }

    #[test]
    fn notify_toggles_independent_of_lock() {
        let backend = make_backend_with_device("dev1", "Speaker");
        let mut state = PersistentState::default();

        apply_device_lock_toggle(
            &DeviceAction::VolumeLockNotify,
            true,
            &DeviceId::from("dev1"),
            "Speaker",
            DeviceType::Output,
            &mut state,
            &backend,
        );
        let settings = state.devices.get("dev1").unwrap();
        assert!(settings.volume_lock.notify);
        assert!(!settings.volume_lock.is_locked);
    }

    #[test]
    fn empty_settings_detected_after_all_unlocked() {
        let backend = make_backend_with_device("dev1", "Speaker");
        let mut state = PersistentState::default();

        // Lock then unlock — settings should be empty
        apply_device_lock_toggle(
            &DeviceAction::VolumeLock,
            true,
            &DeviceId::from("dev1"),
            "Speaker",
            DeviceType::Output,
            &mut state,
            &backend,
        );
        apply_device_lock_toggle(
            &DeviceAction::VolumeLock,
            false,
            &DeviceId::from("dev1"),
            "Speaker",
            DeviceType::Output,
            &mut state,
            &backend,
        );

        let settings = state.devices.get("dev1").unwrap();
        assert!(device_settings_are_empty(settings));
    }
}