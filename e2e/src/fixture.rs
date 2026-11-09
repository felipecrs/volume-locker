//! Sets up what a test needs: the fake devices plugged in and the app running on seeded settings.
use crate::audio::{self, Endpoint};
use crate::tray;
use crate::usbip::{self, Device};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, MutexGuard, Once};
use std::time::{Duration, Instant};

/// Set on CI runners, and by oem/logon.cmd in the VM.
const GUARD_VARIABLE: &str = "CI";
const USBIP: &str = r"C:\Program Files\USBip\usbip.exe";
/// Renovate updates this (see renovate.json).
const USBIP_VERSION: &str = "0.9.8.1";
/// Who signs usbip-win2's installers and drivers. Checking the signer instead of a hash
/// lets Renovate update the version alone.
const USBIP_SIGNER: &str = "Cloudyne Systems (Scheibling Consulting AB)";
const STATE_FILE: &str = "VolumeLockerState.json";
const LOG_FILE: &str = "VolumeLocker.log";
const DEVICE_TIMEOUT: Duration = Duration::from_secs(60);
const APP_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Tests share the machine's audio state, so only one may run at a time.
static MACHINE: Mutex<()> = Mutex::new(());

/// Polls until `done` holds, and fails the test with `what` if it never does.
#[track_caller]
pub fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    wait_up_to(APP_TIMEOUT, what, &mut done);
}

#[track_caller]
fn wait_up_to(timeout: Duration, what: &str, done: &mut impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < timeout, "timed out waiting: {what}");
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Runs a program to completion and returns its output, failing the test if it fails.
fn run(program: impl AsRef<std::ffi::OsStr>, args: &[&str]) -> String {
    let program = program.as_ref();
    let output = Command::new(program)
        .args(args)
        // PowerShell 7 (CI's shell) sets this, and Windows PowerShell then can't load its own modules.
        .env_remove("PSModulePath")
        .stdin(Stdio::null())
        .output()
        .unwrap_or_else(|e| panic!("run {program:?}: {e}"));
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "{program:?} {args:?} failed: {stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
}

/// Starts Windows Audio, which Windows Server turns off, and installs usbip-win2 if it's
/// missing. Test runs in the VM always need the install, since every run starts clean.
fn prepare_windows() {
    static PREPARED: Once = Once::new();
    PREPARED.call_once(|| {
        run("sc.exe", &["config", "audiosrv", "start=", "auto"]);
        // Fails when the service is already running, which is fine.
        let _ = Command::new("sc.exe").args(["start", "audiosrv"]).output();
        if Path::new(USBIP).exists() {
            return;
        }
        let installer = std::env::temp_dir().join("USBip-x64.exe");
        let installer_path = installer.to_str().expect("the temp path is UTF-8");
        let curl_args = [
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--output",
        ];
        let url = format!(
            "https://github.com/vadimgrn/usbip-win2/releases/download/v.{USBIP_VERSION}/USBip-{USBIP_VERSION}-x64.exe"
        );
        run(
            "curl.exe",
            &[&curl_args[..], &[installer_path, &url]].concat(),
        );
        // Prints the signer's name only if the signature is valid.
        let script = format!(
            "$s = Get-AuthenticodeSignature -LiteralPath '{installer_path}'; \
             if ($s.Status -eq 'Valid') {{ $s.SignerCertificate.GetNameInfo('SimpleName', $false) }}"
        );
        let signer = run("powershell.exe", &["-NoProfile", "-Command", &script]);
        assert_eq!(
            signer.trim(),
            USBIP_SIGNER,
            "the usbip-win2 installer's signer"
        );
        run(
            &installer,
            &["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"],
        );
        assert!(Path::new(USBIP).exists(), "usbip-win2 did not install");
    });
}

fn usbip(args: &[&str]) -> String {
    let output = Command::new(USBIP)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("run usbip");
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The virtual port a device is attached to.
fn attached_port(device: Device) -> Option<String> {
    let suffix = format!("/{}", device.busid);
    let listing = usbip(&["port"]);
    let mut port = None;
    for line in listing.lines() {
        if let Some(rest) = line.strip_prefix("Port ") {
            port = rest.split(':').next().map(str::to_owned);
        } else if line.trim_end().ends_with(&suffix) {
            return port;
        }
    }
    None
}

fn endpoints_present(device: Device) -> usize {
    device
        .flows()
        .into_iter()
        .filter(|&flow| Endpoint::find(flow, device.product).is_some())
        .count()
}

/// Plugs a fake device in and waits for Windows to offer all its audio endpoints.
pub fn plug_in(device: Device) {
    if attached_port(device).is_none() {
        wait_up_to(DEVICE_TIMEOUT, "usbip to attach a device", &mut || {
            usbip(&["attach", "-r", "localhost", "-b", device.busid]).contains("successfully")
        });
    }
    wait_up_to(DEVICE_TIMEOUT, device.product, &mut || {
        endpoints_present(device) == device.flows().len()
    });
}

/// Unplugs a fake device and waits for its audio endpoints to go away.
pub fn unplug(device: Device) {
    let port = attached_port(device).expect("the device is attached");
    usbip(&["detach", "-p", &port]);
    wait_up_to(DEVICE_TIMEOUT, device.product, &mut || {
        endpoints_present(device) == 0
    });
}

/// Exclusive use of the machine, with every fake device plugged in.
pub struct Machine {
    _guard: MutexGuard<'static, ()>,
}

impl Machine {
    pub fn acquire() -> Self {
        assert!(
            std::env::var_os(GUARD_VARIABLE).is_some(),
            "these tests change the default audio devices and move the mouse, \
             so they only run in CI or the test VM. See docs/e2e-tests.md."
        );
        let guard = MACHINE.lock().unwrap_or_else(|e| e.into_inner());
        prepare_windows();
        audio::init_com();
        usbip::start();
        plug_in(usbip::SPEAKER);
        plug_in(usbip::HEADSET);
        plug_in(usbip::MICROPHONE);
        Self { _guard: guard }
    }
}

/// The app running from its own folder, so each test gets a fresh state file and log.
pub struct App {
    child: Child,
    folder: PathBuf,
}

impl App {
    pub fn start(test_name: &str, state: &Value) -> Self {
        // In the VM the app sits next to the tests; elsewhere, use the release build.
        let next_to_tests = std::env::current_exe()
            .expect("find the test executable")
            .with_file_name("VolumeLocker.exe");
        let source = if next_to_tests.exists() {
            next_to_tests
        } else {
            Path::new(env!("CARGO_MANIFEST_DIR")).join(r"..\target\release\VolumeLocker.exe")
        };
        let folder = std::env::temp_dir()
            .join("volume-locker-e2e")
            .join(test_name);
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("create the app folder");
        let exe = folder.join("VolumeLocker.exe");
        std::fs::copy(&source, &exe).expect("copy the app");
        std::fs::write(folder.join(STATE_FILE), state.to_string()).expect("seed the state file");
        let child = Command::new(exe)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start the app");
        Self { child, folder }
    }

    /// Pins the app's tray icon to the taskbar, so `Menu::open()` can find it.
    pub fn promote_tray_icon(&self) {
        let folder = self.folder.file_name().expect("the app folder has a name");
        tray::promote_icon(&format!(r"\{}\VolumeLocker.exe", folder.to_string_lossy()));
    }

    /// The state file the app last saved.
    pub fn saved_state(&self) -> Value {
        let text = std::fs::read_to_string(self.folder.join(STATE_FILE)).expect("read state file");
        serde_json::from_str(&text).expect("parse state file")
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if std::thread::panicking() {
            let log = std::fs::read_to_string(self.folder.join(LOG_FILE)).unwrap_or_default();
            println!("--- app log\n{log}--- end of app log");
        }
    }
}
