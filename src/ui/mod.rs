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