use crate::consts::{CURRENT_VERSION, GITHUB_RELEASE_ASSET, GITHUB_REPO_URL};
use crate::notification::log_and_notify_error;
use crate::platform::{NotificationDuration, send_notification};
use crate::utils::get_executable_path_str;
use anyhow::Context;
use semver::Version;
use std::fs::File;
use std::io;
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::time::Duration;
use ureq::config::Config;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};
use ureq::{Agent, ResponseExt};

fn create_download_agent() -> Agent {
    // No overall limit, since slow connections are fine as long as data keeps arriving.
    create_agent_with_timeouts(
        None,
        Some(Duration::from_secs(10)),
        Some(Duration::from_secs(30)),
    )
}

fn create_check_agent() -> Agent {
    create_agent_with_timeouts(
        Some(Duration::from_secs(10)),
        Some(Duration::from_secs(5)),
        None,
    )
}

fn create_agent_with_timeouts(
    timeout_global: Option<Duration>,
    timeout_connect: Option<Duration>,
    timeout_per_read: Option<Duration>,
) -> Agent {
    let config = Config::builder()
        .timeout_global(timeout_global)
        .timeout_connect(timeout_connect)
        .timeout_per_read(timeout_per_read)
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

pub fn fetch_update_info() -> anyhow::Result<Option<UpdateInfo>> {
    log::info!("Checking for updates...");

    let agent = create_check_agent();
    let latest_releases_url = format!("{GITHUB_REPO_URL}/releases/latest");
    let response = agent.head(&latest_releases_url).call()?;
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

/// Reports the result of an update check on the event-loop thread.
pub fn report_check_result(manual_request: bool, result: &Result<Option<UpdateInfo>, String>) {
    match result {
        Ok(Some(info)) => {
            log::info!("Update available: v{}", info.latest_version);
            if manual_request
                && let Err(e) = send_notification(
                    "Update Available",
                    &format!(
                        "Version {} is available. Click 'Update' in the menu to install.",
                        info.latest_version
                    ),
                    NotificationDuration::Long,
                )
            {
                log::error!("Failed to send update notification: {e:#}");
            }
        }
        Ok(None) => {
            log::info!("No updates available");
            if manual_request
                && let Err(e) = send_notification(
                    "No Updates Available",
                    "You are running the latest version of Volume Locker.",
                    NotificationDuration::Short,
                )
            {
                log::error!("Failed to send no-update notification: {e:#}");
            }
        }
        Err(e) => {
            if manual_request {
                log_and_notify_error(
                    "Update Check Failed",
                    &format!("Failed to check for updates: {e}"),
                );
            } else {
                log::error!("Failed to check for updates: {e}");
            }
        }
    }
}

fn download_to_file(agent: &Agent, url: &str, path: &str) -> anyhow::Result<()> {
    let mut response = agent.get(url).call()?;
    let mut file = File::create(path)?;
    let mut reader = response.body_mut().as_reader();
    if let Err(e) = io::copy(&mut reader, &mut file) {
        drop(file);
        if let Err(remove_error) = std::fs::remove_file(path) {
            log::warn!("Failed to remove partial download: {remove_error}");
        }
        return Err(e.into());
    }
    Ok(())
}

/// Downloads the update next to the executable. Blocks, so call it off the event loop.
pub fn download_update(update_info: &UpdateInfo) -> anyhow::Result<()> {
    log::info!("Starting update to {}", update_info.latest_version);

    if let Err(e) = crate::utils::open_url(&update_info.release_url) {
        log::warn!("Failed to open release URL: {e:#}");
    }

    let exe_str = get_executable_path_str()?;
    let temp_download = format!("{exe_str}.download");

    log::info!("Downloading from {}", update_info.download_url);

    download_to_file(
        &create_download_agent(),
        &update_info.download_url,
        &temp_download,
    )
}

/// Launches the script that installs a finished download. The application must
/// exit right after this succeeds, or the script cannot replace the executable.
pub fn launch_post_update_script() -> anyhow::Result<()> {
    let exe_str = get_executable_path_str()?;
    let temp_download = format!("{exe_str}.download");

    log::info!("Download complete, launching post-update script");

    // Spawns a detached process that waits for this app to exit, then replaces
    // the executable and relaunches it.
    Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-Command",
            "Start-Sleep -Seconds 2; Move-Item -Path $env:VL_TEMP_PATH -Destination $env:VL_EXE_PATH -Force; Start-Process $env:VL_EXE_PATH",
        ])
        .env("VL_TEMP_PATH", &temp_download)
        .env("VL_EXE_PATH", exe_str)
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()?;

    log::info!("Post-update script launched, exiting application...");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;

    /// Serves one response, then holds the connection open until the sender is dropped.
    fn serve_once(response: &'static str) -> (String, mpsc::Sender<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("should bind");
        let url = format!(
            "http://{}/",
            listener.local_addr().expect("should have addr")
        );
        let (hold, held) = mpsc::channel();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("should accept");
            let _ = stream.read(&mut [0; 1024]);
            stream
                .write_all(response.as_bytes())
                .expect("should write response");
            let _ = held.recv();
        });
        (url, hold)
    }

    fn short_read_timeout_agent() -> Agent {
        create_agent_with_timeouts(None, None, Some(Duration::from_millis(200)))
    }

    #[test]
    fn download_writes_complete_body() {
        let (url, _hold) = serve_once("HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nbody");
        let dir = tempfile::tempdir().expect("should create temp dir");
        let path = dir.path().join("update.download");
        let path = path.to_str().expect("path should be UTF-8");

        download_to_file(&short_read_timeout_agent(), &url, path).expect("should download");

        assert_eq!(std::fs::read(path).expect("should read file"), b"body");
    }

    #[test]
    fn stalled_download_fails_and_removes_partial_file() {
        let (url, _hold) = serve_once("HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\npart");
        let dir = tempfile::tempdir().expect("should create temp dir");
        let path = dir.path().join("update.download");
        let path = path.to_str().expect("path should be UTF-8");

        assert!(download_to_file(&short_read_timeout_agent(), &url, path).is_err());
        assert!(!std::path::Path::new(path).exists());
    }

    #[test]
    fn extract_version_from_release_url() {
        let (tag, version) = extract_version_from_url(
            "https://github.com/felipecrs/volume-locker/releases/tag/v1.2.3",
        )
        .expect("should extract version from URL");
        assert_eq!(tag, "v1.2.3");
        assert_eq!(version, "1.2.3");
    }

    #[test]
    fn extract_version_no_v_prefix() {
        let (tag, version) = extract_version_from_url(
            "https://github.com/felipecrs/volume-locker/releases/tag/1.0.0",
        )
        .expect("should extract version from URL");
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
