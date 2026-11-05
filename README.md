<img align="right" width="200" alt="Volume Locker" src="https://github.com/user-attachments/assets/80473f4a-f901-440b-9d7b-5003ef1c96c7" />

<p align="center">
⭐<b>Please star this project in GitHub if it helps you!</b>⭐
</p>

# Volume Locker

Tired of apps changing your microphone volume without your consent? Or Windows constantly switching to the wrong audio device?

Volume Locker is a tray app that locks the volume of your audio devices and switches your audio and communication devices based on priority lists. Your volume locks, unmute settings, and priority lists carry over when you change USB ports, re-pair Bluetooth devices, or update drivers. It is a portable, ~1MB binary written in Rust for Windows.

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

Click the Volume Locker tray icon to open the menu. It has these sections:

1.  **Output devices**: List of active output devices.
2.  **Input devices**: List of active input devices.
3.  **Output device priority**: Priorities for output and communication audio.
4.  **Input device priority**: Priorities for input and communication microphones.
5.  **Temporary priority**: Quick temporary overrides for any audio role.

### Lock volume and unmute state

1.  Open **Output devices** or **Input devices**.
2.  Select the device.
3.  Check **Keep volume locked** to lock the volume at the current level.
4.  Check **Keep unmuted** to prevent the device from being muted.
5.  Optionally, enable notifications for these actions.

### Device priority

Volume Locker switches your audio devices based on a priority list.

1.  Open **Output device priority** or **Input device priority** in the tray menu. The active devices are shown under **Current: <device>**.
2.  Open **Device priority settings** to manage the priority list:
    - Check **Notify on restore** to receive a notification when switching devices based on priority.
    - Use **Move up** and **Move down** to adjust device order. The top device has highest priority.
    - Under **Add device**, select a device to **Add to top** or **Add to bottom**.
3.  Open **Communication priority settings** to configure communication routing.

### Communication priority settings

Output and input each support an independent communication priority list for voice calls and chat apps.

1.  Open **Communication priority settings** under output or input.
2.  By default, communication follows the main priority list (**Follow output device priority** or **Follow input device priority**).
3.  You can reorder priorities, add specific devices to take precedence over following the main list, or remove the main priority entry.
4.  Check **Notify on restore** inside communication priority settings to receive notifications when communication devices switch.

### Temporary priority

Route audio to a specific device without changing your saved priority lists:

1.  Open **Temporary priority** in the tray menu.
2.  Select the role: **Output**, **Communication**, **Input**, or **Communication**. The submenu label displays the selected device when active.
3.  Select the device to use temporarily. It remains active until unchecked or when the app restarts. Temporary communication overrides take precedence over configured communication priorities.

### Wireless headset availability

Wireless receivers can stay connected while their headset is off or out of range. Volume Locker skips supported headsets in that state, so put a fallback device below the headset in your priority list.

Supported headsets are detected automatically by USB VID/PID, and detection is on by default. Devices that support a startup status query report their state right away; others stay unknown until their first status event.

See [headset availability](docs/headset-presence.md) for supported devices, diagnostics, and how to add a device.

### Device ID migration

Windows often regenerates audio endpoint IDs when you plug a device into a different USB port, unpair and re-pair a Bluetooth headset, or update audio drivers.

Volume Locker recognizes the physical device across these changes and moves your volume locks, unmute settings, and priority lists to the new ID. See [docs/device-id-migration.md](docs/device-id-migration.md) for how it works.

### Preferences

- **Auto-launch on startup**: Starts Volume Locker when you sign in to Windows.
- **Check for updates on launch**: Checks for new releases on startup.
- **Notify on device migration**: Shows a desktop notification when settings move to a reconnected device.
- **Skip disconnected wireless headsets**: Skips supported wireless headsets that are off or out of range. On by default.

## Credits

Volume Locker started as my first Rust project, born from the dissatisfaction with existing solutions that relied on closed-source tools or lacked specific device locking capabilities. It has since grown to include default device priority management.

I used [wolfinabox/Windows-Mic-Volume-Locker](https://github.com/wolfinabox/Windows-Mic-Volume-Locker) for years before deciding to write my own solution.

I was inspired by [AntoineGS/teams-status-rs](https://github.com/AntoineGS/teams-status-rs) being so amazing in such a lightweight package. I wanted to do something similar, but for volume locking.

Special thanks to:

- [Kingloo/volume](https://github.com/Kingloo/volume) for the Windows volume control code foundation.
- [tauri-apps/tray-icon](https://github.com/tauri-apps/tray-icon) for the tray icon implementation.
- [GitHub Copilot](https://github.com/copilot/) for assisting with the code structure, syntax, and feature implementation.
- [@jmb](https://github.com/jmb) for the [great icon design](https://github.com/Templarian/MaterialDesign/issues/7714).
