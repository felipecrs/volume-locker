<img align="right" width="200" alt="Volume Locker" src="https://github.com/user-attachments/assets/80473f4a-f901-440b-9d7b-5003ef1c96c7" />

<p align="center">
⭐<b>Please star this project in GitHub if it helps you!</b>⭐
</p>

# Volume Locker

Tired of apps changing your microphone volume without your consent? Of Windows constantly switching to the wrong audio device? Or of losing your device names and icons every time you change USB ports?

Volume Locker is a tray app that locks the volume of your audio devices and switches your audio and communication devices based on priority lists. When you change USB ports, re-pair a Bluetooth device, or update a driver, it transfers your settings, along with the device's name and icon, to the device's new Windows entry. It is a portable, ~2MB binary written in Rust for Windows.

## Demo

https://github.com/user-attachments/assets/b7e47898-ee9f-42b4-a804-f107beac4e98

## Getting started

Paste this into _Windows PowerShell_ to install or update:

```powershell
New-Item -ItemType Directory -Path 'C:\Apps\Volume Locker' -Force >$null; `
  Get-Process | Where-Object { $_.Path -eq 'C:\Apps\Volume Locker\VolumeLocker.exe' } | Stop-Process; `
  curl.exe --progress-bar --location --output 'C:\Apps\Volume Locker\VolumeLocker.exe' `
  'https://github.com/felipecrs/volume-locker/releases/latest/download/VolumeLocker.exe'; `
  Start-Process 'C:\Apps\Volume Locker\VolumeLocker.exe'
```

Or download the executable from the [releases page](https://github.com/felipecrs/volume-locker/releases), place it somewhere like `C:\Apps\Volume Locker\VolumeLocker.exe`, and run it.

## Usage

Click the Volume Locker tray icon to open the menu.

### Lock volume and unmute

1.  Open **Output devices** or **Input devices**, then select a device.
2.  Check **Keep volume locked** to hold the volume at its current level.
3.  Check **Keep unmuted** to prevent the device from being muted.

**Notify on volume restore** and **Notify on unmute** show a notification when Volume Locker acts.

### Set device priority

Volume Locker switches your default device to the highest-priority one available.

1.  Open **Output device priority** or **Input device priority**.
2.  Open **Device priority settings**.
3.  Under **Add device**, select a device, then **Add to top** or **Add to bottom**.
4.  Select a device in the **Priority list** to move or remove it.

**Communication priority settings** works the same way for voice calls and chat apps. By default, it follows the device priority list.

Check **Notify on restore** in either list to get a notification when Volume Locker switches devices.

### Use a device temporarily

To override your priority lists without changing them:

1.  Open **Temporary priority**.
2.  Select **Output**, **Input**, or the **Communication** role under either one.
3.  Select the device.

It stays active until you uncheck it or restart the app.

### Skip wireless headsets that are off

Some wireless receivers stay connected while the headset is off or out of range. Volume Locker skips supported headsets in that state, so put a fallback device below the headset in your priority list. See [Headset availability](docs/headset-availability.md).

### Keep your settings, names, and icons

Windows sometimes treats your device as new when you change USB ports, re-pair it, or update a driver. You would normally lose its custom name and icon, along with anything tied to it.

Volume Locker recognizes the device and transfers its volume lock, unmute lock, priority list entries, name, and icon.

See [Settings transfer](docs/settings-transfer.md) and [Transferring names and icons](docs/device-names-and-icons.md).

### Use your language

Volume Locker follows the Windows display language, and English is the only language so far. To add yours, see [Localization](docs/localization.md).

### Preferences

| Preference                                                 | Default | What it does                                                   |
| ---------------------------------------------------------- | ------- | -------------------------------------------------------------- |
| Settings transfer > Automatically transfer device settings | On      | Transfers settings when Windows treats a device as new         |
| Settings transfer > Also transfer device name and icon     | On      | Transfers the device's Windows name and icon with its settings |
| Settings transfer > Notify on settings transfer            | Off     | Shows a notification when settings transfer                    |
| Skip disconnected wireless headsets                        | On      | Skips supported headsets that are off or out of range          |
| Auto-launch on startup                                     | Off     | Starts Volume Locker when you sign in to Windows               |
| Check for updates on launch                                | On      | Checks for a new release when the app starts                   |

## Credits

Volume Locker started as my first Rust project, born from the dissatisfaction with existing solutions that relied on closed-source tools or lacked specific device locking capabilities. It has since grown to include default device priority management.

I used [wolfinabox/Windows-Mic-Volume-Locker](https://github.com/wolfinabox/Windows-Mic-Volume-Locker) for years before deciding to write my own solution.

I was inspired by [AntoineGS/teams-status-rs](https://github.com/AntoineGS/teams-status-rs) being so amazing in such a lightweight package. I wanted to do something similar, but for volume locking.

Special thanks to:

- [Kingloo/volume](https://github.com/Kingloo/volume) for the Windows volume control code foundation.
- [tauri-apps/tray-icon](https://github.com/tauri-apps/tray-icon) for the tray icon implementation.
- [GitHub Copilot](https://github.com/copilot/) for assisting with the code structure, syntax, and feature implementation.
- [@jmb](https://github.com/jmb) for the [great icon design](https://github.com/Templarian/MaterialDesign/issues/7714).
