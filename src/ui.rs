use crate::audio::AudioBackend;
use crate::config::PersistentState;
use crate::consts::GITHUB_REPO_URL;
use crate::platform::{
    open_device_properties, open_device_settings, open_devices_list, open_sound_settings,
    open_volume_mixer,
};
use crate::types::{
    AppAction, DeviceAction, DeviceId, DeviceRole, DeviceSettings, DeviceType, MenuAction,
    MenuItemInfo, PreferenceAction, TemporaryPriorities,
};
use crate::update::UpdateInfo;
use crate::utils::{
    convert_float_to_percent, get_executable_directory, log_and_notify_error, open_path, open_url,
};
use std::collections::HashMap;
use tray_icon::menu::{
    CheckMenuItem, Menu, MenuId, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu,
};

pub enum UpdateAction {
    None,
    Check,
    Perform(UpdateInfo),
}

pub struct DeviceDisplayInfo<'a> {
    pub name: &'a str,
    pub volume_percent: f32,
    pub is_default: bool,
    pub is_locked: bool,
    pub is_muted: bool,
}

pub fn format_device_menu_label(info: &DeviceDisplayInfo) -> String {
    let default_indicator = if info.is_default { " · ☆" } else { "" };
    let locked_indicator = if info.is_locked { " · 🔒" } else { "" };
    let muted_indicator = if info.is_muted { " 🚫" } else { "" };
    format!(
        "{}{default_indicator} · {}%{muted_indicator}{locked_indicator}",
        info.name, info.volume_percent
    )
}

pub fn find_menu_item(menu: &Menu, id: &MenuId) -> Option<MenuItemKind> {
    find_in_items(&menu.items(), id)
}

fn get_check_item_state(menu: &Menu, id: &MenuId) -> Option<bool> {
    find_menu_item(menu, id).and_then(|item| item.as_check_menuitem().map(|c| c.is_checked()))
}

fn find_in_items(items: &[MenuItemKind], id: &MenuId) -> Option<MenuItemKind> {
    for item in items {
        if item.id() == id {
            return Some(item.clone());
        }
        if let Some(submenu) = item.as_submenu()
            && let Some(sub_item) = find_in_items(&submenu.items(), id)
        {
            return Some(sub_item);
        }
    }
    None
}

/// Creates a `MenuItem`, registers it in the device map, and appends it to the menu.
fn append_action_item(
    menu: &Menu,
    map: &mut HashMap<MenuId, MenuItemInfo>,
    label: &str,
    action: MenuAction,
) -> anyhow::Result<()> {
    let item = MenuItem::new(label, true, None);
    map.insert(
        item.id().clone(),
        MenuItemInfo {
            name: label.to_string(),
            action,
        },
    );
    menu.append(&item)?;
    Ok(())
}

/// Registers a menu item in the device map, associating it with a device and setting type.
fn register_menu_item(
    map: &mut HashMap<MenuId, MenuItemInfo>,
    menu_id: MenuId,
    action: DeviceAction,
    device_id: &str,
    name: &str,
    device_type: DeviceType,
) {
    map.insert(
        menu_id,
        MenuItemInfo {
            name: name.to_string(),
            action: MenuAction::Device {
                device_id: device_id.into(),
                device_type,
                action,
            },
        },
    );
}

fn lookup_device_name(
    device_id: &DeviceId,
    persistent_state: &PersistentState,
    backend: &impl AudioBackend,
) -> String {
    if let Some(settings) = persistent_state.devices.get::<str>(device_id) {
        settings.name.clone()
    } else {
        match backend.get_device_by_id(device_id) {
            Ok(d) => d.name(),
            Err(_) => "Unknown Device".to_string(),
        }
    }
}

pub struct TrayMenuItems<'a> {
    pub auto_launch_check_item: &'a CheckMenuItem,
    pub check_updates_on_launch_item: &'a CheckMenuItem,
    pub quit_item: &'a MenuItem,
    pub output_devices_heading_item: &'a MenuItem,
    pub input_devices_heading_item: &'a MenuItem,
}

pub struct MenuContext<'a, B: AudioBackend> {
    pub backend: &'a B,
    pub persistent_state: &'a PersistentState,
    pub temporary_priorities: &'a TemporaryPriorities,
    pub auto_launch_enabled: bool,
    pub update_info: &'a Option<UpdateInfo>,
}

pub fn rebuild_tray_menu(
    tray_menu: &Menu,
    ctx: &MenuContext<impl AudioBackend>,
    items: &TrayMenuItems,
) -> anyhow::Result<HashMap<MenuId, MenuItemInfo>> {
    // Clear the menu
    for _ in 0..tray_menu.items().len() {
        tray_menu.remove_at(0);
    }
    let mut menu_id_to_device: HashMap<MenuId, MenuItemInfo> = HashMap::new();

    // Devices section
    for (heading_item, device_type) in [
        (items.output_devices_heading_item, DeviceType::Output),
        (items.input_devices_heading_item, DeviceType::Input),
    ] {
        append_device_list_to_menu(
            tray_menu,
            heading_item,
            device_type,
            ctx.backend,
            ctx.persistent_state,
            &mut menu_id_to_device,
        )?;
    }

    // Sound shortcuts section
    append_action_item(
        tray_menu,
        &mut menu_id_to_device,
        "Sound settings...",
        MenuAction::App(AppAction::OpenSoundSettings),
    )?;
    append_action_item(
        tray_menu,
        &mut menu_id_to_device,
        "Volume mixer...",
        MenuAction::App(AppAction::OpenVolumeMixer),
    )?;
    tray_menu.append(&PredefinedMenuItem::separator())?;

    // Default device priority section
    for device_type in [DeviceType::Output, DeviceType::Input] {
        let temporary_priority = ctx.temporary_priorities.get(device_type);
        append_priority_list_to_menu(
            tray_menu,
            device_type,
            ctx.backend,
            ctx.persistent_state,
            temporary_priority,
            &mut menu_id_to_device,
        )?;
    }

    // Temporary priority section
    append_temporary_priority_section(
        tray_menu,
        ctx.backend,
        ctx.persistent_state,
        ctx.temporary_priorities,
        &mut menu_id_to_device,
    )?;

    // Preferences section
    append_preferences_section(
        tray_menu,
        ctx.auto_launch_enabled,
        ctx.persistent_state,
        items,
    )?;

    // Troubleshooting, updates, and quit section
    append_footer_section(tray_menu, &mut menu_id_to_device, ctx.update_info, items)?;

    Ok(menu_id_to_device)
}

pub fn sync_device_names(backend: &impl AudioBackend, persistent_state: &mut PersistentState) {
    for device_type in [DeviceType::Output, DeviceType::Input] {
        let devices = backend.get_devices(device_type).unwrap_or_default();
        for device in devices {
            if let Some(settings) = persistent_state.devices.get_mut(device.id()) {
                settings.name = device.name();
                settings.device_type = device_type;
            }
        }
    }
}

fn append_device_list_to_menu(
    tray_menu: &Menu,
    heading_item: &MenuItem,
    device_type: DeviceType,
    backend: &impl AudioBackend,
    persistent_state: &PersistentState,
    menu_id_to_device: &mut HashMap<MenuId, MenuItemInfo>,
) -> anyhow::Result<()> {
    tray_menu.append(heading_item)?;

    let devices = backend.get_devices(device_type).unwrap_or_default();

    // Get default device ID for Console role to mark it
    let default_device_id = backend
        .get_default_device(device_type, DeviceRole::Console)
        .map(|d| d.id().to_string())
        .ok();

    for device in devices {
        let name = device.name();
        let device_id = device.id();
        let volume = device.volume().unwrap_or(0.0);
        let volume_percent = convert_float_to_percent(volume);
        let is_muted = device.is_muted().unwrap_or(false);
        let is_default = default_device_id
            .as_ref()
            .is_some_and(|id| id == device_id);

        let (is_volume_locked, notify_on_volume_lock, is_unmute_locked, notify_on_unmute_lock) =
            if let Some(settings) = persistent_state.devices.get(device_id) {
                (
                    settings.volume_lock.is_locked,
                    settings.volume_lock.notify,
                    settings.unmute_lock.is_locked,
                    settings.unmute_lock.notify,
                )
            } else {
                (false, false, false, false)
            };

        let is_locked = is_volume_locked || is_unmute_locked;
        let label = format_device_menu_label(&DeviceDisplayInfo {
            name: &name,
            volume_percent,
            is_default,
            is_locked,
            is_muted,
        });

        let submenu = Submenu::new(&label, true);

        let volume_lock_item =
            CheckMenuItem::new("Keep volume locked", true, is_volume_locked, None);
        let volume_notify_item = CheckMenuItem::new(
            "Notify on volume restore",
            is_volume_locked,
            notify_on_volume_lock,
            None,
        );
        let unmute_lock_item = CheckMenuItem::new("Keep unmuted", true, is_unmute_locked, None);
        let unmute_notify_item = CheckMenuItem::new(
            "Notify on unmute",
            is_unmute_locked,
            notify_on_unmute_lock,
            None,
        );

        let mut register = |menu_id: MenuId, action: DeviceAction| {
            register_menu_item(
                menu_id_to_device,
                menu_id,
                action,
                &device_id,
                &name,
                device_type,
            );
        };
        register(volume_lock_item.id().clone(), DeviceAction::VolumeLock);
        register(
            volume_notify_item.id().clone(),
            DeviceAction::VolumeLockNotify,
        );
        register(unmute_lock_item.id().clone(), DeviceAction::UnmuteLock);
        register(
            unmute_notify_item.id().clone(),
            DeviceAction::UnmuteLockNotify,
        );

        submenu.append(&volume_lock_item)?;
        submenu.append(&unmute_lock_item)?;
        submenu.append(&PredefinedMenuItem::separator())?;
        submenu.append(&volume_notify_item)?;
        submenu.append(&unmute_notify_item)?;
        submenu.append(&PredefinedMenuItem::separator())?;

        let properties_item = MenuItem::new("Properties...", true, None);
        register(properties_item.id().clone(), DeviceAction::OpenProperties);
        submenu.append(&properties_item)?;

        let settings_item = MenuItem::new("Settings...", true, None);
        register(settings_item.id().clone(), DeviceAction::OpenSettings);
        submenu.append(&settings_item)?;

        tray_menu.append(&submenu)?;
    }

    let properties_label = match device_type {
        DeviceType::Output => "Playback devices...",
        DeviceType::Input => "Recording devices...",
    };
    append_action_item(
        tray_menu,
        menu_id_to_device,
        properties_label,
        MenuAction::Preference {
            device_type,
            action: PreferenceAction::OpenDevicesList,
        },
    )?;

    tray_menu.append(&PredefinedMenuItem::separator())?;

    Ok(())
}

fn append_temporary_priority_section(
    tray_menu: &Menu,
    backend: &impl AudioBackend,
    persistent_state: &PersistentState,
    temporary_priorities: &TemporaryPriorities,
    menu_id_to_device: &mut HashMap<MenuId, MenuItemInfo>,
) -> anyhow::Result<()> {
    tray_menu.append(&MenuItem::new(
        "Temporary default device priority",
        false,
        None,
    ))?;

    for device_type in [DeviceType::Output, DeviceType::Input] {
        let devices = backend.get_devices(device_type).unwrap_or_default();
        let available_devices: Vec<_> = devices.iter().map(|d| (d.id(), d.name())).collect();

        let temp_id_opt = temporary_priorities.get(device_type);

        let label_prefix = match device_type {
            DeviceType::Output => "Output device",
            DeviceType::Input => "Input device",
        };

        let submenu_label = if let Some(temp_id) = temp_id_opt {
            let device_name = lookup_device_name(temp_id, persistent_state, backend);
            format!("{}: {}", label_prefix, device_name)
        } else {
            label_prefix.to_string()
        };

        let submenu = Submenu::new(&submenu_label, true);

        for (id, name) in &available_devices {
            let is_checked = temp_id_opt.is_some_and(|t| *t == *id);
            let item = CheckMenuItem::new(name, true, is_checked, None);
            menu_id_to_device.insert(
                item.id().clone(),
                MenuItemInfo {
                    name: name.clone(),
                    action: MenuAction::Device {
                        device_id: (*id).into(),
                        device_type,
                        action: DeviceAction::SetTemporaryPriority,
                    },
                },
            );
            submenu.append(&item)?;
        }
        tray_menu.append(&submenu)?;
    }
    tray_menu.append(&PredefinedMenuItem::separator())?;

    Ok(())
}

fn append_preferences_section(
    tray_menu: &Menu,
    auto_launch_enabled: bool,
    persistent_state: &PersistentState,
    items: &TrayMenuItems,
) -> anyhow::Result<()> {
    tray_menu.append(&MenuItem::new("Preferences", false, None))?;

    items
        .auto_launch_check_item
        .set_checked(auto_launch_enabled);
    tray_menu.append(items.auto_launch_check_item)?;

    items
        .check_updates_on_launch_item
        .set_checked(persistent_state.check_updates_on_launch);
    tray_menu.append(items.check_updates_on_launch_item)?;
    tray_menu.append(&PredefinedMenuItem::separator())?;

    Ok(())
}

fn append_footer_section(
    tray_menu: &Menu,
    menu_id_to_device: &mut HashMap<MenuId, MenuItemInfo>,
    update_info: &Option<UpdateInfo>,
    items: &TrayMenuItems,
) -> anyhow::Result<()> {
    // Troubleshooting section
    tray_menu.append(&MenuItem::new("Troubleshooting", false, None))?;

    append_action_item(
        tray_menu,
        menu_id_to_device,
        "Open app folder...",
        MenuAction::App(AppAction::OpenAppDirectory),
    )?;

    // Update section
    tray_menu.append(&PredefinedMenuItem::separator())?;

    append_action_item(
        tray_menu,
        menu_id_to_device,
        "GitHub...",
        MenuAction::App(AppAction::OpenGitHubRepo),
    )?;

    let (label, action) = match update_info {
        Some(info) => (
            format!("Update to {}...", info.latest_version),
            AppAction::PerformUpdate,
        ),
        None => ("Check for updates".to_string(), AppAction::CheckForUpdates),
    };

    append_action_item(
        tray_menu,
        menu_id_to_device,
        &label,
        MenuAction::App(action),
    )?;

    // Quit section
    tray_menu.append(&PredefinedMenuItem::separator())?;
    tray_menu.append(items.quit_item)?;

    Ok(())
}

fn append_priority_list_to_menu(
    tray_menu: &Menu,
    device_type: DeviceType,
    backend: &impl AudioBackend,
    persistent_state: &PersistentState,
    temporary_priority: Option<&DeviceId>,
    menu_id_to_device: &mut HashMap<MenuId, MenuItemInfo>,
) -> anyhow::Result<()> {
    let priority_list = persistent_state.get_priority_list(device_type);
    let priority_label = match device_type {
        DeviceType::Output => "Default output device priority",
        DeviceType::Input => "Default input device priority",
    };

    let priority_header = MenuItem::new(priority_label, false, None);
    tray_menu.append(&priority_header)?;

    // Need available devices for "Add device"
    let devices = backend.get_devices(device_type).unwrap_or_default();
    let mut available_devices = Vec::new();
    for device in devices {
        available_devices.push((device.id().to_string(), device.name()));
    }

    for (index, device_id) in priority_list.iter().enumerate() {
        let device_name = lookup_device_name(device_id, persistent_state, backend);

        let label = format!("{}. {}", index + 1, device_name);
        let priority_submenu = Submenu::new(&label, true);

        let move_up_item = MenuItem::new("Move up", index > 0, None);
        if index > 0 {
            menu_id_to_device.insert(
                move_up_item.id().clone(),
                MenuItemInfo {
                    name: device_name.clone(),
                    action: MenuAction::Device {
                        device_id: device_id.clone(),
                        device_type,
                        action: DeviceAction::MovePriorityUp,
                    },
                },
            );
        }
        priority_submenu.append(&move_up_item)?;

        let move_down_item = MenuItem::new("Move down", index < priority_list.len() - 1, None);
        if index < priority_list.len() - 1 {
            menu_id_to_device.insert(
                move_down_item.id().clone(),
                MenuItemInfo {
                    name: device_name.clone(),
                    action: MenuAction::Device {
                        device_id: device_id.clone(),
                        device_type,
                        action: DeviceAction::MovePriorityDown,
                    },
                },
            );
        }
        priority_submenu.append(&move_down_item)?;
        priority_submenu.append(&PredefinedMenuItem::separator())?;

        let move_to_top_item = MenuItem::new("Move to top", index > 0, None);
        if index > 0 {
            menu_id_to_device.insert(
                move_to_top_item.id().clone(),
                MenuItemInfo {
                    name: device_name.clone(),
                    action: MenuAction::Device {
                        device_id: device_id.clone(),
                        device_type,
                        action: DeviceAction::MovePriorityToTop,
                    },
                },
            );
        }
        priority_submenu.append(&move_to_top_item)?;

        let move_to_bottom_item =
            MenuItem::new("Move to bottom", index < priority_list.len() - 1, None);
        if index < priority_list.len() - 1 {
            menu_id_to_device.insert(
                move_to_bottom_item.id().clone(),
                MenuItemInfo {
                    name: device_name.clone(),
                    action: MenuAction::Device {
                        device_id: device_id.clone(),
                        device_type,
                        action: DeviceAction::MovePriorityToBottom,
                    },
                },
            );
        }
        priority_submenu.append(&move_to_bottom_item)?;
        priority_submenu.append(&PredefinedMenuItem::separator())?;

        let remove_priority_item = MenuItem::new("Remove device", true, None);
        menu_id_to_device.insert(
            remove_priority_item.id().clone(),
            MenuItemInfo {
                name: device_name.clone(),
                action: MenuAction::Device {
                    device_id: device_id.clone(),
                    device_type,
                    action: DeviceAction::RemoveFromPriority,
                },
            },
        );
        priority_submenu.append(&remove_priority_item)?;

        tray_menu.append(&priority_submenu)?;
    }

    let mut devices_to_add = Vec::new();
    for (id, name) in &available_devices {
        if !priority_list.iter().any(|p| *p == *id) {
            devices_to_add.push((id, name));
        }
    }

    let add_device_submenu = Submenu::new("Add device", !devices_to_add.is_empty());
    for (id, name) in devices_to_add {
        let item = MenuItem::new(name, true, None);
        menu_id_to_device.insert(
            item.id().clone(),
            MenuItemInfo {
                name: name.clone(),
                action: MenuAction::Device {
                    device_id: id.clone().into(),
                    device_type,
                    action: DeviceAction::AddToPriority,
                },
            },
        );
        add_device_submenu.append(&item)?;
    }
    tray_menu.append(&add_device_submenu)?;

    let notify_on_restore = persistent_state.get_notify_on_priority_restore(device_type);

    let notify_item = CheckMenuItem::new(
        "Notify on priority restore",
        !priority_list.is_empty() || temporary_priority.is_some(),
        notify_on_restore,
        None,
    );

    menu_id_to_device.insert(
        notify_item.id().clone(),
        MenuItemInfo {
            name: "Priority Restore Notify".to_string(),
            action: MenuAction::Preference {
                device_type,
                action: PreferenceAction::PriorityRestoreNotify,
            },
        },
    );
    tray_menu.append(&notify_item)?;

    let switch_communication = persistent_state.get_switch_communication_device(device_type);

    let switch_comm_item = CheckMenuItem::new(
        "Also switch default communication device",
        !priority_list.is_empty() || temporary_priority.is_some(),
        switch_communication,
        None,
    );

    menu_id_to_device.insert(
        switch_comm_item.id().clone(),
        MenuItemInfo {
            name: "Switch Communication Device".to_string(),
            action: MenuAction::Preference {
                device_type,
                action: PreferenceAction::SwitchCommunicationDevice,
            },
        },
    );
    tray_menu.append(&switch_comm_item)?;

    tray_menu.append(&PredefinedMenuItem::separator())?;

    Ok(())
}

pub struct MenuEventResult {
    pub should_save: bool,
    pub devices_changed: bool,
    pub update_action: UpdateAction,
}

/// Returns `true` if the device has no active locks or notifications,
/// meaning its settings entry can be removed when not in a priority list.
fn device_settings_are_empty(settings: &DeviceSettings) -> bool {
    !settings.volume_lock.is_locked
        && !settings.unmute_lock.is_locked
        && !settings.volume_lock.notify
        && !settings.unmute_lock.notify
}

/// Applies a device lock/notify toggle and returns whether the device entry should be removed.
fn apply_device_lock_toggle(
    action: &DeviceAction,
    is_checked: bool,
    device_id: &DeviceId,
    device_name: &str,
    device_type: DeviceType,
    persistent_state: &mut PersistentState,
    backend: &impl AudioBackend,
) -> bool {
    let device_settings = persistent_state
        .devices
        .entry(device_id.clone())
        .or_insert_with(|| DeviceSettings::new(device_name.to_string(), device_type));

    match action {
        DeviceAction::VolumeLock => {
            if is_checked {
                if let Ok(device) = backend.get_device_by_id(device_id)
                    && let Ok(vol) = device.volume()
                {
                    device_settings.volume_lock.target_percent = convert_float_to_percent(vol);
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

    device_settings_are_empty(device_settings)
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
            let list = persistent_state.get_priority_list_mut(device_type);
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
            let list = persistent_state.get_priority_list_mut(device_type);
            if let Some(pos) = list.iter().position(|x| x == device_id) {
                list.remove(pos);
                if let Some(settings) = persistent_state.devices.get(device_id)
                    && device_settings_are_empty(settings)
                {
                    persistent_state.devices.remove(device_id);
                }
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
    let list = persistent_state.get_priority_list_mut(device_type);
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

pub fn handle_menu_event(
    event: &tray_icon::menu::MenuEvent,
    menu_info: &MenuItemInfo,
    tray_menu: &Menu,
    persistent_state: &mut PersistentState,
    backend: &impl AudioBackend,
    temporary_priorities: &mut TemporaryPriorities,
    update_info: &Option<UpdateInfo>,
) -> MenuEventResult {
    let mut should_save = false;
    let mut devices_changed = false;
    let mut update_action = UpdateAction::None;

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
                if let Some(is_checked) = get_check_item_state(tray_menu, &event.id) {
                    let should_remove = apply_device_lock_toggle(
                        action,
                        is_checked,
                        device_id,
                        &menu_info.name,
                        *device_type,
                        persistent_state,
                        backend,
                    );

                    if should_remove {
                        let is_in_priority =
                            persistent_state.output_priority_list.contains(device_id)
                                || persistent_state.input_priority_list.contains(device_id);

                        if !is_in_priority {
                            persistent_state.devices.remove(device_id);
                        }
                    }
                    should_save = true;
                }
            }
            DeviceAction::AddToPriority
            | DeviceAction::RemoveFromPriority
            | DeviceAction::MovePriorityUp
            | DeviceAction::MovePriorityDown
            | DeviceAction::MovePriorityToTop
            | DeviceAction::MovePriorityToBottom => {
                should_save = handle_priority_event(
                    action,
                    device_id,
                    *device_type,
                    &menu_info.name,
                    persistent_state,
                );
            }
            DeviceAction::SetTemporaryPriority => {
                let is_checked = get_check_item_state(tray_menu, &event.id).unwrap_or(false);

                temporary_priorities.set(
                    *device_type,
                    if is_checked {
                        Some(device_id.clone())
                    } else {
                        None
                    },
                );
                devices_changed = true;
            }
            DeviceAction::OpenProperties => {
                if let Err(e) = open_device_properties(device_id) {
                    log::error!("Failed to open device properties: {e:#}");
                }
            }
            DeviceAction::OpenSettings => {
                if let Err(e) = open_device_settings(device_id) {
                    log::error!("Failed to open device settings: {e:#}");
                }
            }
        },
        MenuAction::Preference {
            device_type,
            action,
        } => match action {
            PreferenceAction::PriorityRestoreNotify => {
                if let Some(is_checked) = get_check_item_state(tray_menu, &event.id) {
                    persistent_state.set_notify_on_priority_restore(*device_type, is_checked);
                    should_save = true;
                }
            }
            PreferenceAction::SwitchCommunicationDevice => {
                if let Some(is_checked) = get_check_item_state(tray_menu, &event.id) {
                    persistent_state.set_switch_communication_device(*device_type, is_checked);
                    should_save = true;
                }
            }
            PreferenceAction::OpenDevicesList => {
                if let Err(e) = open_devices_list(*device_type) {
                    log::error!("Failed to open devices list: {e:#}");
                }
            }
        },
        MenuAction::App(action) => match action {
            AppAction::OpenSoundSettings => {
                if let Err(e) = open_sound_settings() {
                    log::error!("Failed to open sound settings: {e:#}");
                }
            }
            AppAction::OpenVolumeMixer => {
                if let Err(e) = open_volume_mixer() {
                    log::error!("Failed to open volume mixer: {e:#}");
                }
            }
            AppAction::CheckForUpdates => {
                update_action = UpdateAction::Check;
            }
            AppAction::PerformUpdate => {
                if let Some(info) = update_info {
                    update_action = UpdateAction::Perform(info.clone());
                }
            }
            AppAction::OpenGitHubRepo => {
                if let Err(e) = open_url(GITHUB_REPO_URL) {
                    log::error!("Failed to open GitHub repo: {e:#}");
                }
            }
            AppAction::OpenAppDirectory => match get_executable_directory() {
                Ok(dir) => {
                    if let Err(e) = open_path(&dir) {
                        log::error!("Failed to open app directory: {e:#}");
                    }
                }
                Err(e) => log::error!("Failed to get executable directory: {e:#}"),
            },
        },
    }

    MenuEventResult {
        should_save,
        devices_changed,
        update_action,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DeviceAction, DeviceDisplayInfo, DeviceId, DeviceSettings, DeviceType, PersistentState,
        device_settings_are_empty, format_device_menu_label, handle_priority_event,
    };

    #[test]
    fn to_label_basic() {
        let label = format_device_menu_label(&DeviceDisplayInfo {
            name: "Speakers",
            volume_percent: 50.0,
            is_default: false,
            is_locked: false,
            is_muted: false,
        });
        assert_eq!(label, "Speakers · 50%");
    }

    #[test]
    fn to_label_default_device() {
        let label = format_device_menu_label(&DeviceDisplayInfo {
            name: "Speakers",
            volume_percent: 75.0,
            is_default: true,
            is_locked: false,
            is_muted: false,
        });
        assert_eq!(label, "Speakers · ☆ · 75%");
    }

    #[test]
    fn to_label_locked() {
        let label = format_device_menu_label(&DeviceDisplayInfo {
            name: "Speakers",
            volume_percent: 100.0,
            is_default: false,
            is_locked: true,
            is_muted: false,
        });
        assert_eq!(label, "Speakers · 100% · 🔒");
    }

    #[test]
    fn to_label_muted() {
        let label = format_device_menu_label(&DeviceDisplayInfo {
            name: "Mic",
            volume_percent: 0.0,
            is_default: false,
            is_locked: false,
            is_muted: true,
        });
        assert_eq!(label, "Mic · 0% 🚫");
    }

    #[test]
    fn to_label_all_indicators() {
        let label = format_device_menu_label(&DeviceDisplayInfo {
            name: "Headset",
            volume_percent: 42.0,
            is_default: true,
            is_locked: true,
            is_muted: true,
        });
        assert_eq!(label, "Headset · ☆ · 42% 🚫 · 🔒");
    }

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
        assert_eq!(state.output_priority_list, vec!["dev1"]);
        assert!(state.devices.contains_key("dev1"));
    }

    #[test]
    fn priority_add_duplicate_no_op() {
        let mut state = PersistentState::default();
        state.output_priority_list.push("dev1".into());
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
        state.output_priority_list.push("dev1".into());
        let changed = handle_priority_event(
            &DeviceAction::RemoveFromPriority,
            &DeviceId::from("dev1"),
            DeviceType::Output,
            "Speaker",
            &mut state,
        );
        assert!(changed);
        assert!(state.output_priority_list.is_empty());
        assert!(!state.devices.contains_key("dev1"));
    }

    #[test]
    fn priority_move_up() {
        let mut state = PersistentState {
            output_priority_list: vec!["a".into(), "b".into(), "c".into()],
            ..Default::default()
        };
        let changed = handle_priority_event(
            &DeviceAction::MovePriorityUp,
            &DeviceId::from("b"),
            DeviceType::Output,
            "B",
            &mut state,
        );
        assert!(changed);
        assert_eq!(state.output_priority_list, vec!["b", "a", "c"]);
    }

    #[test]
    fn priority_move_up_already_top() {
        let mut state = PersistentState {
            output_priority_list: vec!["a".into(), "b".into()],
            ..Default::default()
        };
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
        let mut state = PersistentState {
            output_priority_list: vec!["a".into(), "b".into(), "c".into()],
            ..Default::default()
        };
        let changed = handle_priority_event(
            &DeviceAction::MovePriorityDown,
            &DeviceId::from("b"),
            DeviceType::Output,
            "B",
            &mut state,
        );
        assert!(changed);
        assert_eq!(state.output_priority_list, vec!["a", "c", "b"]);
    }

    #[test]
    fn priority_move_to_top() {
        let mut state = PersistentState {
            output_priority_list: vec!["a".into(), "b".into(), "c".into()],
            ..Default::default()
        };
        let changed = handle_priority_event(
            &DeviceAction::MovePriorityToTop,
            &DeviceId::from("c"),
            DeviceType::Output,
            "C",
            &mut state,
        );
        assert!(changed);
        assert_eq!(state.output_priority_list, vec!["c", "a", "b"]);
    }

    #[test]
    fn priority_move_to_bottom() {
        let mut state = PersistentState {
            output_priority_list: vec!["a".into(), "b".into(), "c".into()],
            ..Default::default()
        };
        let changed = handle_priority_event(
            &DeviceAction::MovePriorityToBottom,
            &DeviceId::from("a"),
            DeviceType::Output,
            "A",
            &mut state,
        );
        assert!(changed);
        assert_eq!(state.output_priority_list, vec!["b", "c", "a"]);
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
        assert!(state.output_priority_list.is_empty());
        assert_eq!(state.input_priority_list, vec!["mic1"]);
    }
}
