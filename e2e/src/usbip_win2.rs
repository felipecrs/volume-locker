//! The usbip-win2 release the tests install. xtask compiles this file too, to cache the installer.

/// Renovate updates this (see renovate.json).
const VERSION: &str = "0.9.8.1";

pub fn installer_name() -> String {
    format!("USBip-{VERSION}-x64.exe")
}

pub fn installer_url() -> String {
    format!(
        "https://github.com/vadimgrn/usbip-win2/releases/download/v.{VERSION}/{}",
        installer_name()
    )
}
