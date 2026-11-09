//! Project tasks, run through mise.
//!
//! `cargo xtask e2e [test arguments]` builds the app and the end-to-end tests, then runs the tests
//! in a disposable Windows VM, building its image first if there isn't one. See docs/e2e-tests.md.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use std::{env, fs, thread};
use xshell::{Cmd, Shell, cmd};

type Result<T = (), E = Box<dyn std::error::Error>> = std::result::Result<T, E>;

const IMAGE: &str = "docker.io/dockurr/windows@sha256:16fd7c39a3e48bb946d29c59dc6015942e5b900a1fa5d7e5552053bfc48ee2ea";
const CONTAINER: &str = "volume-locker-e2e";
const INSTALL_TIMEOUT: Duration = Duration::from_secs(7200);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(300);
const BOOT_ATTEMPTS: u32 = 3;
const BOOT_TIMEOUT: Duration = Duration::from_secs(300);
const TEST_TIMEOUT: Duration = Duration::from_secs(1200);
const POLL_INTERVAL: Duration = Duration::from_secs(5);

fn main() -> Result {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("e2e") => {
            let code = e2e(&args.collect::<Vec<_>>())?;
            std::process::exit(code)
        }
        _ => Err("usage: cargo xtask e2e [test arguments]".into()),
    }
}

/// Returns the tests' exit code.
fn e2e(test_args: &[String]) -> Result<i32> {
    let sh = Shell::new()?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    sh.change_dir(root);
    // The VM runs Windows on x64, so build for it from anywhere else too.
    let target: &[&str] = if cfg!(all(windows, target_arch = "x86_64")) {
        &[]
    } else {
        &["--target", "x86_64-pc-windows-msvc"]
    };
    let app = built_executable(
        cmd!(
            sh,
            "cargo build --package volume-locker --release --locked {target...}"
        ),
        "VolumeLocker",
    )?;
    let tests = built_executable(
        cmd!(
            sh,
            "cargo test --package volume-locker-e2e --locked --no-run {target...}"
        ),
        "app",
    )?;

    let linux = Linux::new(&sh)?;
    let cache = format!("{}/.cache/volume-locker-e2e", linux.home);
    let storage = format!("{cache}/windows");
    linux.cmd("mkdir").args(["-p", &cache]).run()?;
    let shared = linux
        .cmd("mktemp")
        .args(["-d", &format!("{cache}/shared.XXXXXX")])
        .read()?;
    let vm = Vm {
        linux: &linux,
        storage: &storage,
        shared: &shared,
    };

    // To rebuild the image, delete the cache folder.
    if !linux.host_path(&format!("{storage}/windows.boot")).exists() {
        println!(
            "Building the Windows image first, which takes 10 to 30 minutes. Watch it at http://localhost:8006"
        );
        linux.cmd("rm").args(["-rf", &storage]).run()?;
        linux.cmd("mkdir").args(["-p", &storage]).run()?;
        let oem = linux.linux_path(&root.join("e2e").join("oem"))?;
        // Dockur copies oem/ to C:\OEM and runs install.bat at the end of setup.
        vm.boot(&["--volume", &format!("{oem}:/oem:ro")])?;
        // oem/logon.cmd writes this and powers off at the first logon, since no tests are in the shared folder.
        vm.wait_for_file("provisioned", INSTALL_TIMEOUT)?;
        wait_until("the VM to power off", SHUTDOWN_TIMEOUT, || {
            Ok(!vm.running()?)
        })?;
        fs::remove_file(vm.file("provisioned"))?;
    }

    fs::copy(&tests, vm.file("e2e.exe"))?;
    fs::copy(&app, vm.file("VolumeLocker.exe"))?;
    // One line, since logon.cmd reads only the first.
    fs::write(vm.file("test-args"), test_args.join(" ") + "\n")?;
    // The VM's Samba server reads and writes these as another user.
    linux.cmd("chmod").args(["-R", "a+rwX", &shared]).run()?;

    // A boot occasionally hangs before reaching the desktop, so try again.
    for attempt in 1..=BOOT_ATTEMPTS {
        // -snapshot throws away everything the VM writes to its disk, so every run starts clean.
        vm.boot(&["--env", "ARGUMENTS=-snapshot"])?;
        if let Err(e) = vm.wait_for_file("started", BOOT_TIMEOUT) {
            eprintln!("Boot attempt {attempt} failed: {e}");
            continue;
        }
        let finished = vm.wait_for_file("exit-code", TEST_TIMEOUT);
        if let Ok(log) = fs::read_to_string(vm.file("tests.log")) {
            print!("{}", log.replace("\r\n", "\n"));
        }
        finished?;
        let code = fs::read_to_string(vm.file("exit-code"))?;
        return Ok(code.trim().parse()?);
    }
    Err(format!("The VM did not boot after {BOOT_ATTEMPTS} attempts.").into())
}

/// Runs a cargo build and returns where it put the executable of the target named `name`.
fn built_executable(cargo: Cmd, name: &str) -> Result<PathBuf> {
    let messages = cargo.arg("--message-format=json").read()?;
    messages
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|message| message["target"]["name"] == name)
        .find_map(|message| message["executable"].as_str().map(PathBuf::from))
        .ok_or_else(|| format!("cargo didn't build {name}").into())
}

/// Polls `done` until it returns true.
fn wait_until(what: &str, timeout: Duration, mut done: impl FnMut() -> Result<bool>) -> Result {
    let start = Instant::now();
    while !done()? {
        if start.elapsed() > timeout {
            return Err(
                format!("Timed out after {}s waiting for {what}.", timeout.as_secs()).into(),
            );
        }
        thread::sleep(POLL_INTERVAL);
    }
    Ok(())
}

/// Where Docker and the VM's files live: this machine on Linux, or WSL on Windows.
struct Linux<'a> {
    sh: &'a Shell,
    home: String,
    /// On Windows, how this machine reaches WSL's files.
    wsl_root: Option<PathBuf>,
    /// WSL powers off when its last `wsl.exe` exits, taking Docker with it.
    _keep_alive: Option<KillOnDrop>,
}

impl<'a> Linux<'a> {
    fn new(sh: &'a Shell) -> Result<Self> {
        if !cfg!(windows) {
            return Ok(Self {
                sh,
                home: env::var("HOME")?,
                wsl_root: None,
                _keep_alive: None,
            });
        }
        let keep_alive = Command::new("wsl.exe")
            .args(["--exec", "sleep", "infinity"])
            .stdin(Stdio::null())
            .spawn()?;
        let mut linux = Self {
            sh,
            home: String::new(),
            wsl_root: None,
            _keep_alive: Some(KillOnDrop(keep_alive)),
        };
        let vars = linux
            .cmd("printenv")
            .args(["WSL_DISTRO_NAME", "HOME"])
            .read()?;
        let (distro, home) = vars.split_once('\n').ok_or("WSL didn't report its home")?;
        linux.wsl_root = Some(PathBuf::from(format!(r"\\wsl.localhost\{distro}")));
        linux.home = home.trim().to_owned();
        Ok(linux)
    }

    /// A command that runs on the Linux side. `--exec` skips WSL's shell, which would mangle arguments.
    fn cmd(&self, program: &str) -> Cmd<'a> {
        match self.wsl_root {
            Some(_) => self.sh.cmd("wsl.exe").args(["--exec", program]),
            None => self.sh.cmd(program),
        }
    }

    /// How this machine reaches a Linux path.
    fn host_path(&self, linux_path: &str) -> PathBuf {
        match &self.wsl_root {
            Some(root) => root.join(linux_path.trim_start_matches('/')),
            None => PathBuf::from(linux_path),
        }
    }

    /// How Linux reaches a path on this machine.
    fn linux_path(&self, path: &Path) -> Result<String> {
        match self.wsl_root {
            Some(_) => Ok(self.cmd("wslpath").arg("-u").arg(path).read()?),
            None => Ok(path.to_str().ok_or("the path is not UTF-8")?.to_owned()),
        }
    }
}

struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

/// The VM's container and its shared folder, both removed on drop.
struct Vm<'a> {
    linux: &'a Linux<'a>,
    storage: &'a str,
    shared: &'a str,
}

impl Vm<'_> {
    fn docker(&self) -> Cmd<'_> {
        self.linux.cmd("docker")
    }

    /// Creates and boots the VM. `extra` goes to `docker run`.
    fn boot(&self, extra: &[&str]) -> Result {
        self.remove_container();
        // Every boot must pass the same VERSION, or dockur moves the image to backups/ and reinstalls.
        let (storage, shared) = (self.storage, self.shared);
        self.docker()
            .args(["run", "--detach", "--name", CONTAINER])
            .args(["--env", "VERSION=11l", "--env", "RAM_SIZE=4G"])
            .args(["--env", "CPU_CORES=4", "--env", "DISK_SIZE=32G"])
            .args(["--publish", "127.0.0.1:8006:8006"])
            .args([
                "--device",
                "/dev/kvm",
                "--device",
                "/dev/net/tun",
                "--cap-add",
                "NET_ADMIN",
            ])
            .args(["--volume", &format!("{storage}:/storage")])
            .args(["--volume", &format!("{shared}:/shared")])
            .args(["--stop-timeout", "120"])
            .args(extra)
            .arg(IMAGE)
            .ignore_stdout()
            .run()?;
        Ok(())
    }

    fn running(&self) -> Result<bool> {
        let state = self
            .docker()
            .args(["inspect", "--format", "{{.State.Running}}", CONTAINER])
            .quiet()
            .read()?;
        Ok(state == "true")
    }

    /// A file in the shared folder, as this machine reaches it.
    fn file(&self, name: &str) -> PathBuf {
        self.linux.host_path(&format!("{}/{name}", self.shared))
    }

    /// Waits until the VM writes `name` to the shared folder. Fails if the VM stops first.
    fn wait_for_file(&self, name: &str, timeout: Duration) -> Result {
        wait_until(&format!("the VM to write {name}"), timeout, || {
            if self.file(name).exists() {
                return Ok(true);
            }
            if !self.running()? {
                let logs = self
                    .docker()
                    .args(["logs", "--tail", "20", CONTAINER])
                    .quiet()
                    .ignore_status()
                    .output()?;
                return Err(format!(
                    "The VM stopped before writing {name}. Its last log lines:\n{}{}",
                    String::from_utf8_lossy(&logs.stdout),
                    String::from_utf8_lossy(&logs.stderr)
                )
                .into());
            }
            Ok(false)
        })
    }

    fn remove_container(&self) {
        let _ = self
            .docker()
            .args(["rm", "--force", CONTAINER])
            .quiet()
            .ignore_stdout()
            .ignore_stderr()
            .run();
    }
}

impl Drop for Vm<'_> {
    fn drop(&mut self) {
        self.remove_container();
        let _ = self
            .linux
            .cmd("rm")
            .args(["-rf", self.shared])
            .quiet()
            .run();
    }
}
