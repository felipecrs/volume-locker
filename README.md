<img align="right" width="200" alt="Volume Locker" src="https://github.com/user-attachments/assets/80473f4a-f901-440b-9d7b-5003ef1c96c7" />

<p align="center">
⭐<b>Please star this project in GitHub if it helps you!</b>⭐
</p>

# Volume Locker

Tired of apps changing your microphone volume without your consent? Or Windows constantly switching to the wrong audio device?

Volume Locker is a tray icon app that keeps the volume of your audio devices locked and manages your audio and communication devices based on priority lists. It also seamlessly preserves your volume locks, unmute settings, and priority lists across USB port changes, Bluetooth re-pairings, and driver updates. It is a portable, ~1MB binary written in Rust for Windows.

Never worry about your microphone volume or audio devices again!

## Demo

https://github.com/user-attachments/assets/b7e47898-ee9f-42b4-a804-f107beac4e98

## Getting Started

Simply grab the executable from the [releases page](https://github.com/felipecrs/volume-locker/releases), place it somewhere like `C:\Apps\Volume Locker\VolumeLocker.exe` and run it.

Or you can copy and paste this into _Windows PowerShell_, and execute:

```powershell
New-Item -ItemType Directory -Path 'C:\Apps\Volume Locker' -Force >$null; `
  Get-Process | Where-Object { $_.Path -eq 'C:\Apps\Volume Locker\VolumeLocker.exe' } | Stop-Process; `
  curl.exe --progress-bar --location --output 'C:\Apps\Volume Locker\VolumeLocker.exe' `
  'https://github.com/felipecrs/volume-locker/releases/latest/download/VolumeLocker.exe'; `
  Start-Process 'C:\Apps\Volume Locker\VolumeLocker.exe'
```

You can also use the snippet above to update the app, just run it again.

## Usage

Click on the Volume Locker tray icon to access the menu. The menu is organized into the following sections:

1.  **Output devices**: List of active output devices.
2.  **Input devices**: List of active input devices.
3.  **Output device priority**: Priorities for output and communication audio.
4.  **Input device priority**: Priorities for input and communication microphones.
5.  **Temporary priority**: Quick temporary overrides for any audio role.

### Locking Volume and Unmute State

To lock the volume or unmute state of a specific device:

1.  Navigate to **Output devices** or **Input devices**.
2.  Select the desired device.
3.  Check **Keep volume locked** to lock the volume at the current level.
4.  Check **Keep unmuted** to prevent the device from being muted.
5.  You can also enable notifications for these actions.

### Device Priority

Volume Locker can automatically switch your audio devices based on a priority list.

1.  Navigate to **Output device priority** or **Input device priority** in the tray menu. The active devices are shown under **Current: <device>**.
2.  Open **Device priority settings** to manage the priority list:
    - Check **Notify on restore** to receive a notification when switching devices based on priority.
    - Use **Move up** and **Move down** to adjust device order. The top device has highest priority.
    - Under **Add device**, select a device to **Add to top** or **Add to bottom**.
3.  Open **Communication priority settings** to configure communication routing.

### Communication Priority Settings

Output and input each support an independent communication priority list for voice calls and chat apps.

1.  Open **Communication priority settings** under output or input.
2.  By default, communication follows the main priority list (**Output device priority** or **Input device priority**).
3.  You can reorder priorities, add specific devices to take precedence over following the main list, or remove the main priority entry.
4.  Check **Notify on restore** inside communication priority settings to receive notifications when communication devices switch.

### Temporary Priority

Temporarily route audio to a specific device without altering your saved priority lists:

1.  Open **Temporary priority** in the tray menu.
2.  Select the desired role: **Output**, **Communication**, **Input**, or **Communication**. The submenu label displays the selected device when active.
3.  Select the device to use temporarily. It remains active until unchecked or when the app restarts. Temporary communication overrides take precedence over configured communication priorities.

### Seamless Device ID Migration

Windows frequently regenerates audio device endpoint IDs when you plug a device into a different USB port, unpair and re-pair a Bluetooth headset, or update audio drivers.

Volume Locker automatically detects physical hardware across ID changes and transfers your volume locks, unmute settings, priority lists to the new connection. See [docs/device-id-migration.md](docs/device-id-migration.md) for full architectural details.

### Preferences

Configure global application behavior under **Preferences**:

- **Auto-launch on startup**: Starts Volume Locker automatically when you sign in to Windows.
- **Check for updates on launch**: Automatically checks for new releases on startup.
- **Notify on device migration**: Displays a desktop notification whenever settings are transferred to a reconnected device.

## Credits

Volume Locker started as my first Rust project, born from the dissatisfaction with existing solutions that relied on closed-source tools or lacked specific device locking capabilities. It has since evolved to include advanced features like default device priority management.

I used [wolfinabox/Windows-Mic-Volume-Locker](https://github.com/wolfinabox/Windows-Mic-Volume-Locker) for years before deciding to write my own solution.

I was inspired by [AntoineGS/teams-status-rs](https://github.com/AntoineGS/teams-status-rs) being so amazing in such a lightweight package. I wanted to do something similar, but for volume locking.

Special thanks to:

- [Kingloo/volume](https://github.com/Kingloo/volume) for the Windows volume control code foundation.
- [tauri-apps/tray-icon](https://github.com/tauri-apps/tray-icon) for the tray icon implementation.
- [GitHub Copilot](https://github.com/copilot/) for assisting with the code structure, syntax, and feature implementation.
- [@jmb](https://github.com/jmb) for the [great icon design](https://github.com/Templarian/MaterialDesign/issues/7714).
