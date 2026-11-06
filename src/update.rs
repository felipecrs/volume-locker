use crate::bindings::CREATE_NO_WINDOW;
use crate::consts::{CURRENT_VERSION, GITHUB_RELEASE_ASSET, GITHUB_REPO_URL};
use crate::i18n::{t, t_en};
use crate::notification::notify_error;
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
    fetch_update_info_from(GITHUB_REPO_URL)
}

fn fetch_update_info_from(repo_url: &str) -> anyhow::Result<Option<UpdateInfo>> {
    log::info!("Checking for updates...");

    let agent = create_check_agent();
    let latest_releases_url = format!("{repo_url}/releases/latest");
    let response = agent.head(&latest_releases_url).call()?;
    let release_url = response.get_uri().to_string();

    let (latest_tag, latest_version) = extract_version_from_url(&release_url)?;

    log::info!("Current: {CURRENT_VERSION}, Latest: {latest_version}");

    if is_newer_version(latest_version, CURRENT_VERSION) {
        Ok(Some(UpdateInfo {
            latest_version: latest_version.to_string(),
            download_url: format!(
                "{repo_url}/releases/download/{latest_tag}/{GITHUB_RELEASE_ASSET}"
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
                    &t!("update-available-title"),
                    &t!("update-available", version = info.latest_version.as_str()),
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
                    &t!("no-updates-title"),
                    &t!("no-updates"),
                    NotificationDuration::Short,
                )
            {
                log::error!("Failed to send no-update notification: {e:#}");
            }
        }
        Err(e) => {
            if manual_request {
                notify_error!(
                    "update-check-failed-title",
                    "update-check-failed",
                    error = e.as_str()
                );
            } else {
                log::error!("{}", t_en!("update-check-failed", error = e.as_str()));
            }
        }
    }
}

fn temp_download_path(exe_str: &str) -> String {
    format!("{exe_str}.download")
}

fn download_to_file(agent: &Agent, url: &str, path: &str) -> anyhow::Result<()> {
    let mut response = agent.get(url).call()?;
    let mut file = File::create(path)?;
    let mut reader = response.body_mut().as_reader();
    if let Err(e) = io::copy(&mut reader, &mut file) {
        drop(file);
        remove_download(path);
        return Err(e.into());
    }
    Ok(())
}

fn remove_download(path: &str) {
    if let Err(e) = std::fs::remove_file(path)
        && e.kind() != io::ErrorKind::NotFound
    {
        log::warn!("Failed to remove partial download: {e}");
    }
}

/// Removes a download left behind by quitting or crashing partway through. Only
/// call this once holding the single-instance lock, or it could delete the file
/// another instance is still downloading.
pub fn remove_leftover_download() {
    match get_executable_path_str() {
        Ok(exe_str) => remove_download(&temp_download_path(&exe_str)),
        Err(e) => log::warn!("Failed to look for a leftover download: {e:#}"),
    }
}

/// Downloads the update next to the executable. Blocks, so call it off the event loop.
pub fn download_update(update_info: &UpdateInfo) -> anyhow::Result<()> {
    log::info!("Starting update to {}", update_info.latest_version);

    if let Err(e) = crate::utils::open_url(&update_info.release_url) {
        log::warn!("Failed to open release URL: {e:#}");
    }

    let temp_download = temp_download_path(&get_executable_path_str()?);

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
    let temp_download = temp_download_path(&exe_str);

    log::info!("Download complete, launching post-update script");

    post_update_command(&temp_download, &exe_str, std::process::id()).spawn()?;

    log::info!("Post-update script launched, exiting application...");
    Ok(())
}

/// A detached process that waits for process `app_pid` to exit, then replaces the executable and
/// relaunches it.
fn post_update_command(temp_download: &str, exe: &str, app_pid: u32) -> Command {
    let mut command = Command::new("powershell.exe");
    command
        .args([
            "-NoProfile",
            "-Command",
            "Wait-Process -Id $env:VL_APP_PID -Timeout 30 -ErrorAction SilentlyContinue; Move-Item -Path $env:VL_TEMP_PATH -Destination $env:VL_EXE_PATH -Force; Start-Process $env:VL_EXE_PATH",
        ])
        .env("VL_TEMP_PATH", temp_download)
        .env("VL_EXE_PATH", exe)
        .env("VL_APP_PID", app_pid.to_string())
        .creation_flags(CREATE_NO_WINDOW as u32);
    command
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

    fn read_timeout_agent(millis: u64) -> Agent {
        create_agent_with_timeouts(None, None, Some(Duration::from_millis(millis)))
    }

    fn temp_path() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().expect("should create temp dir");
        let path = dir.path().join("update.download");
        let path = path.to_str().expect("path should be UTF-8").to_owned();
        (dir, path)
    }

    #[test]
    fn download_writes_complete_body() {
        let (url, _hold) = serve_once("HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nbody");
        let (_dir, path) = temp_path();

        // Generous, so a busy machine can't fail this before the server answers.
        download_to_file(&read_timeout_agent(10_000), &url, &path).expect("should download");

        assert_eq!(std::fs::read(&path).expect("should read file"), b"body");
    }

    #[test]
    fn stalled_download_fails_and_removes_partial_file() {
        let (url, _hold) = serve_once("HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\npart");
        let (_dir, path) = temp_path();

        assert!(download_to_file(&read_timeout_agent(500), &url, &path).is_err());
        assert!(!std::path::Path::new(&path).exists());
    }

    #[test]
    fn remove_download_deletes_the_file_and_tolerates_a_missing_one() {
        let (_dir, path) = temp_path();
        std::fs::write(&path, "part").expect("should write file");

        remove_download(&path);
        assert!(!std::path::Path::new(&path).exists());

        remove_download(&path);
    }

    // A truncated file must never reach the script that replaces the executable.
    #[test]
    fn dropped_connection_fails_and_removes_partial_file() {
        let (url, hold) = serve_once("HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\npart");
        drop(hold);
        let (_dir, path) = temp_path();

        assert!(download_to_file(&read_timeout_agent(10_000), &url, &path).is_err());
        assert!(!std::path::Path::new(&path).exists());
    }

    /// Redirects `/releases/latest` to the release page of `tag`, as GitHub does.
    fn serve_latest_release(tag: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("should bind");
        let url = format!(
            "http://{}",
            listener.local_addr().expect("should have addr")
        );
        let location = format!("{url}/releases/tag/{tag}");
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.expect("should accept");
                let mut request = [0; 1024];
                let count = stream.read(&mut request).unwrap_or(0);
                let response = if request[..count].starts_with(b"HEAD /releases/latest ") {
                    format!(
                        "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                } else {
                    "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned()
                };
                let _ = stream.write_all(response.as_bytes());
            }
        });
        url
    }

    #[test]
    fn newer_release_is_reported_with_its_urls() {
        let url = serve_latest_release("v99.0.0");

        let info = fetch_update_info_from(&url)
            .expect("should check")
            .expect("should find an update");

        assert_eq!(info.latest_version, "99.0.0");
        assert_eq!(info.release_url, format!("{url}/releases/tag/v99.0.0"));
        assert_eq!(
            info.download_url,
            format!("{url}/releases/download/v99.0.0/{GITHUB_RELEASE_ASSET}")
        );
    }

    #[test]
    fn current_release_is_not_an_update() {
        let url = serve_latest_release(&format!("v{CURRENT_VERSION}"));

        assert!(
            fetch_update_info_from(&url)
                .expect("should check")
                .is_none()
        );
    }

    #[test]
    fn post_update_script_waits_for_the_app_then_replaces_the_executable() {
        let dir = tempfile::tempdir().expect("should create temp dir");
        let folder = dir.path().join("folder with spaces");
        std::fs::create_dir(&folder).expect("should create folder");
        let exe = folder.join("app.exe");
        let download = folder.join("app.exe.download");
        std::fs::write(&exe, "old").expect("should write file");
        std::fs::write(&download, "new").expect("should write file");

        let mut app = Command::new("ping.exe")
            .args(["-n", "2", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .creation_flags(CREATE_NO_WINDOW as u32)
            .spawn()
            .expect("should start a stand-in for the app");

        // Relaunching this fake executable fails, so the exit code is not checked.
        post_update_command(
            download.to_str().expect("path should be UTF-8"),
            exe.to_str().expect("path should be UTF-8"),
            app.id(),
        )
        .status()
        .expect("should run the script");

        assert!(app.try_wait().expect("should query the app").is_some());
        assert_eq!(std::fs::read(&exe).expect("should read file"), b"new");
        assert!(!download.exists());
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
