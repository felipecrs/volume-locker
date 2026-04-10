use crate::consts::{CURRENT_VERSION, GITHUB_RELEASE_ASSET, GITHUB_REPO_URL};
use crate::platform::{NotificationDuration, send_notification};
use crate::utils::{get_executable_path_str, log_and_notify_error};
use anyhow::Context;
use semver::Version;
use std::fs::File;
use std::io;
use std::os::windows::process::CommandExt;
use std::process::Command;
use ureq::config::Config;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};
use ureq::{Agent, ResponseExt};

fn create_agent() -> Agent {
    let config = Config::builder()
        .tls_config(
            TlsConfig::builder()
                .provider(TlsProvider::NativeTls)
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build();

    config.new_agent()
}

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub latest_version: String,
    pub download_url: String,
    pub release_url: String,
}

/// Extracts the version tag from a release URL like
/// `https://github.com/.../releases/tag/v1.2.3` and returns `("v1.2.3", "1.2.3")`.
fn extract_version_from_url(url: &str) -> anyhow::Result<(&str, &str)> {
    let tag = url
        .rsplit('/')
        .next()
        .context("could not extract version from redirect URL")?;
    let version = tag.trim_start_matches('v');
    Ok((tag, version))
}

/// Returns `true` if `latest` is newer than `current` by semver comparison.
fn is_newer_version(latest: &str, current: &str) -> bool {
    Version::parse(latest).ok() > Version::parse(current).ok()
}

fn check_for_updates() -> anyhow::Result<Option<UpdateInfo>> {
    log::info!("Checking for updates...");

    let agent = create_agent();
    let releases_url = format!("{GITHUB_REPO_URL}/releases/latest");
    let response = agent.head(&releases_url).call()?;
    let release_url = response.get_uri().to_string();

    let (latest_tag, latest_version) = extract_version_from_url(&release_url)?;

    log::info!("Current: {CURRENT_VERSION}, Latest: {latest_version}");

    if is_newer_version(latest_version, CURRENT_VERSION) {
        Ok(Some(UpdateInfo {
            latest_version: latest_version.to_string(),
            download_url: format!(
                "{GITHUB_REPO_URL}/releases/download/{latest_tag}/{GITHUB_RELEASE_ASSET}"
            ),
            release_url,
        }))
    } else {
        Ok(None)
    }
}

/// Checks for updates and optionally notifies the user.
/// If `manual_request` is true, shows notifications for all outcomes.
/// If `manual_request` is false, only logs errors without notifying.
pub fn check(manual_request: bool) -> anyhow::Result<Option<UpdateInfo>> {
    match check_for_updates() {
        Ok(Some(info)) => {
            log::info!("Update available: v{}", info.latest_version);
            if manual_request {
                let _ = send_notification(
                    "Update Available",
                    &format!(
                        "Version {} is available. Click 'Update' in the menu to install.",
                        info.latest_version
                    ),
                    NotificationDuration::Long,
                );
            }
            Ok(Some(info))
        }
        Ok(None) => {
            log::info!("No updates available");
            if manual_request {
                let _ = send_notification(
                    "No Updates Available",
                    "You are running the latest version of Volume Locker.",
                    NotificationDuration::Short,
                );
            }
            Ok(None)
        }
        Err(e) => {
            if manual_request {
                log_and_notify_error(
                    "Update Check Failed",
                    &format!("Failed to check for updates: {e:#}"),
                );
            } else {
                log::error!("Failed to check for updates: {e:#}");
            }
            Err(e)
        }
    }
}

/// Performs the update or shows error notification on failure.
/// Returns `true` if the application should exit (update launched successfully).
pub fn perform(update_info: &UpdateInfo) -> bool {
    log::info!("Starting update to {}", update_info.latest_version);

    match try_perform(update_info) {
        Ok(()) => true,
        Err(e) => {
            log_and_notify_error("Update Failed", &format!("Update failed: {e:#}"));
            false
        }
    }
}

fn try_perform(update_info: &UpdateInfo) -> anyhow::Result<()> {
    // Open release notes
    let _ = open::that_detached(&update_info.release_url);

    let exe_str = get_executable_path_str()?;
    let temp_download = format!("{exe_str}.download");

    log::info!("Downloading from {}", update_info.download_url);

    // Download the update
    let agent = create_agent();
    let mut response = agent.get(&update_info.download_url).call()?;

    // Write to temporary file
    let mut file = File::create(&temp_download)?;
    let mut reader = response.body_mut().as_reader();
    io::copy(&mut reader, &mut file)?;
    drop(file);

    log::info!("Download complete, launching post-update script");

    // Launch PowerShell script to complete the update (no window)
    Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-Command",
            "Start-Sleep -Seconds 2; Move-Item -Path $env:VL_TEMP_PATH -Destination $env:VL_EXE_PATH -Force; Start-Process $env:VL_EXE_PATH",
        ])
        .env("VL_TEMP_PATH", &temp_download)
        .env("VL_EXE_PATH", exe_str)
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .spawn()?;

    log::info!("Post-update script launched, exiting application...");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_version_from_release_url() {
        let (tag, version) = extract_version_from_url(
            "https://github.com/felipecrs/volume-locker/releases/tag/v1.2.3",
        )
        .unwrap();
        assert_eq!(tag, "v1.2.3");
        assert_eq!(version, "1.2.3");
    }

    #[test]
    fn extract_version_no_v_prefix() {
        let (tag, version) = extract_version_from_url(
            "https://github.com/felipecrs/volume-locker/releases/tag/1.0.0",
        )
        .unwrap();
        assert_eq!(tag, "1.0.0");
        assert_eq!(version, "1.0.0");
    }

    #[test]
    fn is_newer_detects_major() {
        assert!(is_newer_version("2.0.0", "1.0.0"));
    }

    #[test]
    fn is_newer_detects_minor() {
        assert!(is_newer_version("1.1.0", "1.0.0"));
    }

    #[test]
    fn is_newer_detects_patch() {
        assert!(is_newer_version("1.0.1", "1.0.0"));
    }

    #[test]
    fn is_newer_same_version() {
        assert!(!is_newer_version("1.0.0", "1.0.0"));
    }

    #[test]
    fn is_newer_older_version() {
        assert!(!is_newer_version("0.9.0", "1.0.0"));
    }

    #[test]
    fn is_newer_invalid_latest_returns_false() {
        assert!(!is_newer_version("not-a-version", "1.0.0"));
    }
}
