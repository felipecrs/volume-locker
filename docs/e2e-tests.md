# End-to-end tests

The tests in `e2e/` run the real release build of the app in a Windows VM, with fake USB audio devices that Windows treats as real hardware. They check what the app does from the outside: default devices, volumes, mute state, its state file and its tray menu.

They are a separate Cargo package, so a plain `cargo test` doesn't build or run them. They change the default audio devices and move the mouse, so they refuse to run unless `VOLUME_LOCKER_E2E` is set. Only the VM sets it.

## How it works

- [dockur/windows](https://github.com/dockur/windows) runs Windows in a container, in a real console session. Audio endpoints aren't visible to desktop apps in Windows Sandbox, whose session works like a remote desktop.
- [usbip-win2](https://github.com/vadimgrn/usbip-win2) attaches USB devices over TCP using signed drivers, so the VM doesn't need test mode.
- `e2e/src/usbip.rs` is a USB/IP server inside the test process. It exports a speaker, a microphone, and a headset with both that also reports presence over HID. Tests decide what the headset reports.
- At each logon, `e2e/oem/logon.ps1` looks in a folder shared with the host. If the tests are there, it runs them and writes their output and exit code back. If not, it's building the image, so it powers off.
- `run.sh` boots the VM with QEMU's `-snapshot`, so whatever a run writes to disk is discarded and every run starts clean.

## Running them

You need Docker and KVM, which on Windows means Docker running inside WSL.

Build the base image once. This takes 10 to 30 minutes and needs about 7 GB of disk in `~/.cache/volume-locker-e2e` after the installer download. Each test run boots a VM with 4 GB of RAM.

```bash
mise run e2e:provision
```

It uses Windows 11 Enterprise LTSC. Every boot must pass dockur the same `VERSION`: if it changes, dockur moves the image to a `backups` folder and installs Windows again.

Then run the tests. This takes a few minutes, and you can watch the VM at <http://localhost:8006>.

```bash
mise run e2e
```

`e2e/run.ps1` builds the app, then runs `cargo test` from `e2e/`. There, `.cargo/config.toml` links with a static C runtime, since the VM doesn't have the Visual C++ runtime, and makes `run.sh` the test runner, so the tests run in the VM. Its arguments go to the test executable, so this runs only the tray test:

```bash
powershell -File e2e/run.ps1 tray
```

On Linux, build `VolumeLocker.exe` and the test executable for Windows with a static C runtime, then call `e2e/run.sh <VolumeLocker.exe> <test executable> [test arguments]`.

`run-tests.ps1` is copied into the VM on every run, so changing how tests run doesn't need a new image. Only `e2e/oem/` is part of the image.

## Adding a test

Add a `#[test]` to `e2e/tests/app.rs`. Start with `Machine::acquire()`, seed settings with `App::start()`, then use `wait_until()` rather than fixed sleeps. If a test fails, its output includes the app's log.

Tests can change devices with `plug_in()`, `unplug()` and `report_headset_presence()`, and use the tray with `Menu::open()`. Most tests check output and input in turn, with the speaker and the microphone as fallbacks for the headset.

## Limitations

- No test streams audio.
- `Menu::click()` can only click items that fit on the screen.
- The tests match the English menu text, and the VM's language is English.
- The base image uses a Windows evaluation licence. If tests start failing because Windows shuts itself down, provision again.
- Docker in WSL only stays up while a `wsl.exe` process is open. `run.ps1` and the `e2e:provision` task keep one open while they run, but a container started by hand dies shortly after the command returns.
- The tests don't run in CI yet.
