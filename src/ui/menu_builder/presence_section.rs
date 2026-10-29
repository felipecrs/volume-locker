use super::register_menu_item;
use crate::config::PersistentState;
use crate::types::{DeviceId, DeviceType, PresenceSource};
use crate::ui::{DeviceAction, MenuIdMap};
use tray_icon::menu::{CheckMenuItem, MenuItem, PredefinedMenuItem, Submenu};

pub(super) fn append_presence_settings(
    parent: &Submenu,
    device_id: &DeviceId,
    name: &str,
    device_type: DeviceType,
    state: &PersistentState,
    map: &mut MenuIdMap,
) -> anyhow::Result<()> {
    let settings = state.device_settings(device_id);
    let source = settings.map_or(PresenceSource::Windows, |s| s.presence_source);
    let submenu = Submenu::new("Availability", true);
    for (label, choice) in [
        ("Windows connection", PresenceSource::Windows),
        (
            "A50 X power events (experimental)",
            PresenceSource::AstroA50X,
        ),
        (
            "Logitech G533 link events (experimental)",
            PresenceSource::LogitechG533,
        ),
    ] {
        let item = CheckMenuItem::new(label, true, source == choice, None);
        register_menu_item(
            map,
            item.id().clone(),
            DeviceAction::SetPresenceSource(choice),
            device_id,
            name,
            device_type,
        );
        submenu.append(&item)?;
    }
    if source != PresenceSource::Windows {
        submenu.append(&PredefinedMenuItem::separator())?;
        submenu.append(&MenuItem::new(
            "After enabling, toggle headset power once",
            false,
            None,
        ))?;
    }
    parent.append(&submenu)?;
    Ok(())
}
