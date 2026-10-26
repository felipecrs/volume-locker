use super::register_menu_item;
use crate::config::PersistentState;
use crate::types::{CommunicationRoute, DeviceId, DeviceType};
use crate::ui::{DeviceAction, MenuIdMap};
use tray_icon::menu::{CheckMenuItem, PredefinedMenuItem, Submenu};

/// Settings live inside each existing device entry, not in another top-level list.
pub(super) fn append_routing_settings(
    parent: &Submenu,
    device_id: &DeviceId,
    name: &str,
    device_type: DeviceType,
    state: &PersistentState,
    map: &mut MenuIdMap,
    output_choices: &[(DeviceId, String)],
) -> anyhow::Result<()> {
    let settings = state.device_settings(device_id);
    if device_type == DeviceType::Output {
        let route = settings.map_or(&CommunicationRoute::Inherited, |s| &s.communication_route);
        let label = match route {
            CommunicationRoute::Inherited => "Voice output: use global setting".into(),
            CommunicationRoute::SameDevice => "Voice output: same device".into(),
            CommunicationRoute::Paired(id) => {
                // Known names are retained even while the paired device is unplugged.
                let target_name = state
                    .device_settings(id)
                    .map_or("Unavailable device", |s| s.name.as_str());
                format!("Voice output: {target_name}")
            }
        };
        let submenu = Submenu::new(label, true);
        let mut choices = vec![
            (
                "Use global voice settings".to_string(),
                CommunicationRoute::Inherited,
            ),
            (
                "Same as this output".to_string(),
                CommunicationRoute::SameDevice,
            ),
        ];
        let mut targets: Vec<_> = output_choices
            .iter()
            .filter(|(id, _)| id != device_id)
            .map(|(id, name)| (name.clone(), CommunicationRoute::Paired(id.clone())))
            .collect();
        targets.sort_by(|a, b| a.0.cmp(&b.0));
        choices.extend(targets);
        for (index, (label, choice)) in choices.into_iter().enumerate() {
            if index == 2 {
                submenu.append(&PredefinedMenuItem::separator())?;
            }
            let item = CheckMenuItem::new(label, true, route == &choice, None);
            register_menu_item(
                map,
                item.id().clone(),
                DeviceAction::SetCommunicationRoute(choice),
                device_id,
                name,
                device_type,
            );
            submenu.append(&item)?;
        }
        if let CommunicationRoute::Paired(id) = route
            && !output_choices.iter().any(|(candidate, _)| candidate == id)
        {
            let target_name = state
                .device_settings(id)
                .map_or("Paired device", |s| s.name.as_str());
            submenu.append(&CheckMenuItem::new(
                format!("{target_name} (unavailable)"),
                false,
                true,
                None,
            ))?;
        }
        parent.append(&submenu)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::MenuAction;

    #[test]
    fn unconfigured_available_outputs_can_be_paired_inside_existing_device_entry() {
        let state = PersistentState::default();
        let parent = Submenu::new("Game", true);
        let mut map = MenuIdMap::new();
        append_routing_settings(
            &parent,
            &"game".into(),
            "Game",
            DeviceType::Output,
            &state,
            &mut map,
            &[
                ("game".into(), "Game".into()),
                ("voice".into(), "Voice".into()),
            ],
        )
        .unwrap();
        assert_eq!(parent.items().len(), 1, "one routing submenu");
        assert!(map.values().any(|entry| matches!(&entry.action,
            MenuAction::Device {device_id, action: DeviceAction::SetCommunicationRoute(CommunicationRoute::Paired(target)), ..}
            if device_id == "game" && target == "voice")));
        assert!(!map.values().any(|entry| matches!(&entry.action,
            MenuAction::Device {action: DeviceAction::SetCommunicationRoute(CommunicationRoute::Paired(target)), ..}
            if target == "game")));
    }

    #[test]
    fn unavailable_pair_remains_visible_and_selected() {
        let mut state = PersistentState::default();
        state
            .ensure_device_settings("game".into(), "Game".into(), DeviceType::Output)
            .communication_route = CommunicationRoute::Paired("voice".into());
        state.ensure_device_settings("voice".into(), "Voice".into(), DeviceType::Output);
        let parent = Submenu::new("Game", true);
        append_routing_settings(
            &parent,
            &"game".into(),
            "Game",
            DeviceType::Output,
            &state,
            &mut MenuIdMap::new(),
            &[],
        )
        .unwrap();
        let route = parent.items()[0].as_submenu().unwrap().clone();
        let items = route.items();
        let unavailable = items.last().unwrap().as_check_menuitem().unwrap();
        assert_eq!(unavailable.text(), "Voice (unavailable)");
        assert!(unavailable.is_checked());
        assert!(!unavailable.is_enabled());
    }
}
