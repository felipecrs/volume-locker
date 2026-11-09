//! The outside software the end-to-end tests run on, pinned in one place. Renovate updates the
//! versions here (see renovate.json). xtask compiles this file too.

/// Runs the VM. Pinned by tag and digest, so Renovate can follow new tags.
pub const DOCKUR_IMAGE: &str = "docker.io/dockurr/windows:6.06@sha256:16fd7c39a3e48bb946d29c59dc6015942e5b900a1fa5d7e5552053bfc48ee2ea";

/// Attaches the fake USB devices.
const USBIP_VERSION: &str = "0.9.8.1";

/// Who signs usbip-win2's installers and drivers. Checking the signer instead of a hash lets
/// Renovate update the version alone.
pub const USBIP_SIGNER: &str = "Cloudyne Systems (Scheibling Consulting AB)";

pub fn installer_name() -> String {
    format!("USBip-{USBIP_VERSION}-x64.exe")
}

pub fn installer_url() -> String {
    format!(
        "https://github.com/vadimgrn/usbip-win2/releases/download/v.{USBIP_VERSION}/{}",
        installer_name()
    )
}
