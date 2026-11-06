//! Drives the app's tray icon and menu with real mouse input.
use std::time::Duration;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, TreeScope_Descendants,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, mouse_event,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetMenuItemCount, GetMenuItemRect, GetMenuStringW, HMENU, MF_BYPOSITION,
    MN_GETHMENU, SendMessageW, SetCursorPos,
};
use windows::core::{PCWSTR, w};

const OPEN_ATTEMPTS: usize = 10;

fn click(x: i32, y: i32) {
    unsafe {
        SetCursorPos(x, y).expect("move the mouse");
        std::thread::sleep(Duration::from_millis(200));
        mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
        mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
    }
}

fn click_element(element: &IUIAutomationElement) {
    let r = unsafe { element.CurrentBoundingRectangle() }.expect("read the element's position");
    click((r.left + r.right) / 2, (r.top + r.bottom) / 2);
}

/// The first on-screen element under a top-level window whose name matches.
fn find_visible(
    automation: &IUIAutomation,
    window_class: PCWSTR,
    name_part: &str,
) -> Option<IUIAutomationElement> {
    unsafe {
        let window = FindWindowW(window_class, None).ok()?;
        let root = automation.ElementFromHandle(window).ok()?;
        let all = root
            .FindAll(
                TreeScope_Descendants,
                &automation.CreateTrueCondition().ok()?,
            )
            .ok()?;
        (0..all.Length().ok()?)
            .filter_map(|i| all.GetElement(i).ok())
            .find(|e| {
                e.CurrentName()
                    .is_ok_and(|n| n.to_string().contains(name_part))
                    && e.CurrentIsOffscreen().is_ok_and(|off| !off.as_bool())
            })
    }
}

fn menu_window() -> Option<HWND> {
    unsafe { FindWindowW(w!("#32768"), None) }.ok()
}

/// The app's open tray menu.
pub struct Menu {
    window: HWND,
    handle: HMENU,
}

impl Menu {
    /// Clicks the app's tray icon, opening the hidden icons flyout when it is in there.
    pub fn open() -> Self {
        let automation: IUIAutomation =
            unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
                .expect("create UI Automation");
        for _ in 0..OPEN_ATTEMPTS {
            let icon = find_visible(
                &automation,
                w!("TopLevelWindowForOverflowXamlIsland"),
                "Volume Locker",
            )
            .or_else(|| find_visible(&automation, w!("Shell_TrayWnd"), "Volume Locker"));
            if let Some(icon) = icon {
                click_element(&icon);
            } else if let Some(more) =
                find_visible(&automation, w!("Shell_TrayWnd"), "Show Hidden Icons")
            {
                click_element(&more);
            }
            std::thread::sleep(Duration::from_secs(2));
            if let Some(window) = menu_window() {
                let reply = unsafe { SendMessageW(window, MN_GETHMENU, None, None) };
                return Self {
                    window,
                    handle: HMENU(reply.0 as *mut _),
                };
            }
        }
        panic!("the tray menu did not open");
    }

    pub fn item_names(&self) -> Vec<String> {
        (0..unsafe { GetMenuItemCount(Some(self.handle)) })
            .map(|i| {
                let mut name = [0u16; 256];
                let len = unsafe {
                    GetMenuStringW(self.handle, i as u32, Some(&mut name), MF_BYPOSITION)
                };
                String::from_utf16_lossy(&name[..len as usize])
            })
            .collect()
    }

    /// Clicks the item whose text contains `name_part`. It must be on screen.
    pub fn click(self, name_part: &str) {
        let names = self.item_names();
        let position = names
            .iter()
            .position(|name| name.contains(name_part))
            .unwrap_or_else(|| panic!("no menu item contains '{name_part}' in {names:?}"));
        let mut r = RECT::default();
        unsafe { GetMenuItemRect(Some(self.window), self.handle, position as u32, &mut r) }
            .expect("read the menu item's position");
        click((r.left + r.right) / 2, (r.top + r.bottom) / 2);
    }
}
