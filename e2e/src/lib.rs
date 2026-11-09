//! End-to-end test support. See `docs/e2e-tests.md`.
#![cfg(target_os = "windows")]

pub mod audio;
pub mod fixture;
pub mod tray;
pub mod usbip;
mod usbip_win2;
