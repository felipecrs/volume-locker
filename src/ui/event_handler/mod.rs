use super::{AppAction, DeviceAction, MenuAction, MenuItemInfo, PreferenceAction};
use crate::audio::AudioBackend;
use crate::config::PersistentState;
use crate::consts::GITHUB_REPO_URL;
use crate::notification::log_and_notify_error;
use crate::platform::{
    open_device_settings, open_sound_control_panel, open_sound_settings, open_volume_mixer,
};
use crate::types::{DeviceId, DeviceType, PriorityKind, TemporaryPriorities};
use crate::update::UpdateInfo;
use crate::utils::{get_executable_directory, open_path, open_url};
use tray_icon::menu::Menu;

use super::find_menu_item;

/// mmsys.cpl tab index: Playback (0) for output, Recording (1) for input.
fn sound_panel_tab(device_type: DeviceType) -> &'static str {
    match device_type {
        DeviceType::Output => "0",
        DeviceType::Input => "1",
    }
}

fn get_check_item_state(menu: &Menu, id: &tray_icon::menu::MenuId) -> Option<bool> {
    find_menu_item(menu, id).and_then(|item| {
        item.as_check_menuitem()
            .map(tray_icon::menu::CheckMenuItem::is_checked)
    })
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
    let device_settings = persistent_state.ensure_device_settings(
        device_id.clone(),
        device_name.to_string(),
        device_type,
    );
    if device_settings.fingerprint.is_none()
        && let Ok(device) = backend.device_by_id(device_id)
    {
        device_settings.fingerprint = device.fingerprint();
    }

    match action {
        DeviceAction::VolumeLock => {
            if is_checked {
                if let Ok(device) = backend.device_by_id(device_id)
                    && let Ok(vol) = device.volume()
                {
                    let percent = vol.to_percent();
                    log::info!("User locked volume of '{device_name}' to {percent}%");
                    device_settings.volume_lock.target_percent = percent;
                    device_settings.volume_lock.is_locked = true;
                } else {
                    log_and_notify_error(
                        "Failed to Lock Volume",
                        &format!("Failed to get volume for device {device_name}, cannot lock."),
                    );
                    device_settings.volume_lock.is_locked = false;
                }
            } else {
                log::info!("User unlocked volume of '{device_name}'");
                device_settings.volume_lock.is_locked = false;
            }
        }
        DeviceAction::VolumeLockNotify => {
            log::info!("User set 'Notify on volume lock' for '{device_name}' to {is_checked}");
            device_settings.volume_lock.notify = is_checked;
        }
        DeviceAction::UnmuteLock => {
            log::info!("User set 'Keep unmuted' for '{device_name}' to {is_checked}");
            device_settings.unmute_lock.is_locked = is_checked;
        }
        DeviceAction::UnmuteLockNotify => {
            log::info!("User set 'Notify on unmute lock' for '{device_name}' to {is_checked}");
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
        DeviceAction::AddToPriorityTop(kind) => {
            let list = persistent_state.priority_list_of_mut(device_type, *kind);
            if list.iter().any(|x| *x == **device_id) {
                false
            } else {
                list.insert(0, device_id.clone());
                log::info!(
                    "User added '{device_name}' to top of {device_type} {kind} priority list"
                );
                if !device_id.is_main_priority() {
                    persistent_state.ensure_device_settings(
                        device_id.clone(),
                        device_name.to_string(),
                        device_type,
                    );
                }
                true
            }
        }
        DeviceAction::AddToPriorityBottom(kind) => {
            let list = persistent_state.priority_list_of_mut(device_type, *kind);
            if list.iter().any(|x| *x == **device_id) {
                false
            } else {
                list.push(device_id.clone());
                log::info!(
                    "User added '{device_name}' to bottom of {device_type} {kind} priority list"
                );
                if !device_id.is_main_priority() {
                    persistent_state.ensure_device_settings(
                        device_id.clone(),
                        device_name.to_string(),
                        device_type,
                    );
                }
                true
            }
        }
        DeviceAction::RemoveFromPriority(kind) => {
            let list = persistent_state.priority_list_of_mut(device_type, *kind);
            if let Some(pos) = list.iter().position(|x| x == device_id) {
                list.remove(pos);
                log::info!("User removed '{device_name}' from {device_type} {kind} priority list");
                persistent_state.remove_device_if_unused(device_id);
                true
            } else {
                false
            }
        }
        DeviceAction::MovePriorityUp(kind)
        | DeviceAction::MovePriorityDown(kind)
        | DeviceAction::MovePriorityToTop(kind)
        | DeviceAction::MovePriorityToBottom(kind) => {
            let moved = move_priority_item(action, device_id, device_type, *kind, persistent_state);
            if moved {
                log::info!("User reordered '{device_name}' in {device_type} {kind} priority list");
            }
            moved
        }
        _ => false,
    }
}

fn move_priority_item(
    action: &DeviceAction,
    device_id: &DeviceId,
    device_type: DeviceType,
    kind: PriorityKind,
    persistent_state: &mut PersistentState,
) -> bool {
    let list = persistent_state.priority_list_of_mut(device_type, kind);
    let Some(pos) = list.iter().position(|x| x == device_id) else {
        return false;
    };

    match action {
        DeviceAction::MovePriorityUp(_) if pos > 0 => {
            list.swap(pos, pos - 1);
            true
        }
        DeviceAction::MovePriorityDown(_) if pos < list.len() - 1 => {
            list.swap(pos, pos + 1);
            true
        }
        DeviceAction::MovePriorityToTop(_) if pos > 0 => {
            let id = list.remove(pos);
            list.insert(0, id);
            true
        }
        DeviceAction::MovePriorityToBottom(_) if pos < list.len() - 1 => {
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
        } => handle_device_event(event, action, device_id, *device_type, &menu_info.name, ctx),
        MenuAction::Preference {
            device_type,
            action,
        } => handle_preference_event(event, action, *device_type, ctx),
        MenuAction::App(action) => handle_app_event(event, action, ctx),
    }
}

fn handle_device_event(
    event: &tray_icon::menu::MenuEvent,
    action: &DeviceAction,
    device_id: &DeviceId,
    device_type: DeviceType,
    device_name: &str,
    ctx: &mut MenuEventContext<'_, impl AudioBackend>,
) -> MenuEventResult {
    match action {
        DeviceAction::VolumeLock
        | DeviceAction::VolumeLockNotify
        | DeviceAction::UnmuteLock
        | DeviceAction::UnmuteLockNotify => {
            if let Some(is_checked) = get_check_item_state(ctx.tray_menu, &event.id) {
                apply_device_lock_toggle(
                    action,
                    is_checked,
                    device_id,
                    device_name,
                    device_type,
                    ctx.persistent_state,
                    ctx.backend,
                );
                ctx.persistent_state.remove_device_if_unused(device_id);
                MenuEventResult::SaveConfig
            } else {
                MenuEventResult::NoChange
            }
        }
        DeviceAction::AddToPriorityTop(_)
        | DeviceAction::AddToPriorityBottom(_)
        | DeviceAction::RemoveFromPriority(_)
        | DeviceAction::MovePriorityUp(_)
        | DeviceAction::MovePriorityDown(_)
        | DeviceAction::MovePriorityToTop(_)
        | DeviceAction::MovePriorityToBottom(_) => {
            if handle_priority_event(
                action,
                device_id,
                device_type,
                device_name,
                ctx.persistent_state,
            ) {
                if let Some(s) = ctx.persistent_state.device_settings_mut(device_id)
                    && s.fingerprint.is_none()
                    && let Ok(dev) = ctx.backend.device_by_id(device_id)
                {
                    s.fingerprint = dev.fingerprint();
                }
                MenuEventResult::SaveConfig
            } else {
                MenuEventResult::NoChange
            }
        }
        DeviceAction::SetTemporaryPriority(kind) => {
            let is_checked = get_check_item_state(ctx.tray_menu, &event.id).unwrap_or(false);
            if is_checked {
                log::info!(
                    "User set temporary {device_type} {kind} priority: '{device_name}' ({device_id})"
                );
            } else {
                log::info!("User cleared temporary {device_type} {kind} priority");
            }
            ctx.temporary_priorities.set(
                device_type,
                *kind,
                if is_checked {
                    Some(device_id.clone())
                } else {
                    None
                },
            );
            MenuEventResult::DevicesChanged
        }
        DeviceAction::SetPresenceSource(source) => {
            ctx.persistent_state
                .ensure_device_settings(device_id.clone(), device_name.to_string(), device_type)
                .presence_source = *source;
            ctx.persistent_state.remove_device_if_unused(device_id);
            MenuEventResult::SaveConfig
        }
        DeviceAction::OpenProperties => {
            if let Err(e) = open_sound_control_panel(sound_panel_tab(device_type)) {
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
    }
}

fn handle_preference_event(
    event: &tray_icon::menu::MenuEvent,
    action: &PreferenceAction,
    device_type: DeviceType,
    ctx: &mut MenuEventContext<'_, impl AudioBackend>,
) -> MenuEventResult {
    match action {
        PreferenceAction::PriorityRestoreNotify(kind) => {
            with_check_state(ctx.tray_menu, &event.id, |checked| {
                log::info!(
                    "User toggled 'Notify on restore' for {device_type} {kind} to {checked}"
                );
                ctx.persistent_state
                    .set_notify_on_priority_restore_of(device_type, *kind, checked);
            })
        }
        PreferenceAction::OpenDevicesList => {
            if let Err(e) = open_sound_control_panel(sound_panel_tab(device_type)) {
                log::error!("Failed to open devices list: {e:#}");
            }
            MenuEventResult::NoChange
        }
    }
}

fn handle_app_event(
    event: &tray_icon::menu::MenuEvent,
    action: &AppAction,
    ctx: &mut MenuEventContext<'_, impl AudioBackend>,
) -> MenuEventResult {
    match action {
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
                log::info!("User toggled auto-launch to {checked}");
                MenuEventResult::ToggleAutoLaunch(checked)
            } else {
                MenuEventResult::NoChange
            }
        }
        AppAction::ToggleCheckUpdatesOnLaunch => {
            with_check_state(ctx.tray_menu, &event.id, |checked| {
                log::info!("User toggled 'Check updates on launch' to {checked}");
                ctx.persistent_state.check_updates_on_launch = checked;
            })
        }
        AppAction::ToggleNotifyOnDeviceMigration => {
            with_check_state(ctx.tray_menu, &event.id, |checked| {
                log::info!("User toggled 'Notify on device migration' to {checked}");
                ctx.persistent_state.notify_on_device_migration = checked;
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
    }
}

#[cfg(test)]
mod tests;
