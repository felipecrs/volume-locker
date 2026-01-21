use crate::consts::{CURRENT_VERSION, DEVELOPMENT_VERSION, GITHUB_RELEASE_ASSET, GITHUB_REPO_URL};
use crate::platform::{NotificationDuration, send_notification};
use crate::utils::get_executable_path;
use semver::Version;
use std::fs::File;
use std::io;
use std::os::windows::process::CommandExt;
use std::process::Command;
use ureq::ResponseExt;

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub latest_version: String,
    pub download_url: String,
    pub release_url: String,
}

fn check_for_updates() -> Result<Option<UpdateInfo>, Box<dyn std::error::Error>> {
    log::info!("Checking for updates...");

    let releases_url = format!("{}/releases/latest", GITHUB_REPO_URL);
    let response = ureq::head(&releases_url).call()?;
    let release_url = response.get_uri().to_string();

    // Extract version from URL like: https://github.com/felipecrs/volume-locker/releases/tag/v1.2.3
    let latest_tag = release_url
        .rsplit('/')
        .next()
        .ok_or("Could not extract version from redirect URL")?;

    let latest_version = latest_tag.trim_start_matches('v');

    log::info!("Current: {}, Latest: {}", CURRENT_VERSION, latest_version);

    // Compare versions - if parsing fails, assume no update available
    if Version::parse(latest_version).ok() > Version::parse(CURRENT_VERSION).ok() {
        Ok(Some(UpdateInfo {
            latest_version: latest_version.to_string(),
            download_url: format!(
                "{}/releases/download/{}/{}",
                GITHUB_REPO_URL, latest_tag, GITHUB_RELEASE_ASSET
            ),
            release_url,
        }))
    } else {
        Ok(None)
    }
}

/// Checks for updates and optionally notifies the user
/// If `manual_request` is true, shows notifications for all outcomes
/// If `manual_request` is false, only shows notification when update is available
pub fn check(manual_request: bool) -> Option<UpdateInfo> {
    match check_for_updates() {
        Ok(Some(info)) => {
            log::info!("Update available: {}", info.latest_version);
            // Don't notify on initial check if running development version
            if manual_request || CURRENT_VERSION != DEVELOPMENT_VERSION {
                let _ = send_notification(
                    "Update Available",
                    &format!(
                        "Version {} is available. Click 'Update' in the menu to install.",
                        info.latest_version
                    ),
                    NotificationDuration::Long,
                );
            }
            Some(info)
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
            None
        }
        Err(e) => {
            log::error!("Failed to check for updates: {}", e);
            if manual_request {
                let _ = send_notification(
                    "Update Check Failed",
                    "Failed to check for updates. Please check your internet connection.",
                    NotificationDuration::Long,
                );
            }
            None
        }
    }
}

/// Performs the update or shows error notification on failure
pub fn perform(update_info: &UpdateInfo) {
    log::info!("Starting update to {}", update_info.latest_version);

    if let Err(e) = try_perform(update_info) {
        log::error!("Update failed: {}", e);
        let _ = send_notification(
            "Update Failed",
            "Please download the update manually from GitHub.",
            NotificationDuration::Long,
        );
    }
}

fn try_perform(update_info: &UpdateInfo) -> Result<(), Box<dyn std::error::Error>> {
    // Open release notes
    let _ = Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", &update_info.release_url])
        .spawn();

    let exe_path = get_executable_path().to_str().unwrap().to_string();
    let temp_download = format!("{}.download", exe_path);

    log::info!("Downloading from {}", update_info.download_url);

    // Download the update
    let mut response = ureq::get(&update_info.download_url).call()?;

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
        .env("VL_EXE_PATH", exe_path)
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .spawn()?;

    log::info!("Post-update script launched, exiting application...");
    std::process::exit(0);
}
