mod event_handler;
mod menu_builder;

pub use event_handler::{MenuEventContext, MenuEventResult, handle_menu_event};
pub use menu_builder::{MenuContext, TrayMenuItems, rebuild_tray_menu};

use crate::types::MenuItemInfo;
use std::collections::HashMap;
use tray_icon::menu::{Menu, MenuId, MenuItemKind};

pub type MenuIdMap = HashMap<MenuId, MenuItemInfo>;

pub fn find_menu_item(menu: &Menu, id: &MenuId) -> Option<MenuItemKind> {
    find_in_items(&menu.items(), id)
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tray_icon::menu::{MenuItem, Submenu};

    #[test]
    fn find_menu_item_top_level() {
        let menu = Menu::new();
        let item = MenuItem::new("Test", true, None);
        let target_id = item.id().clone();
        menu.append(&item).unwrap();

        let found = find_menu_item(&menu, &target_id);
        assert!(found.is_some());
    }

    #[test]
    fn find_menu_item_in_submenu() {
        let menu = Menu::new();
        let submenu = Submenu::new("Sub", true);
        let item = MenuItem::new("Nested", true, None);
        let target_id = item.id().clone();
        submenu.append(&item).unwrap();
        menu.append(&submenu).unwrap();

        let found = find_menu_item(&menu, &target_id);
        assert!(found.is_some());
    }

    #[test]
    fn find_menu_item_missing_returns_none() {
        let menu = Menu::new();
        let item = MenuItem::new("Test", true, None);
        menu.append(&item).unwrap();

        let bogus_id = MenuId::new("nonexistent");
        assert!(find_menu_item(&menu, &bogus_id).is_none());
    }
}