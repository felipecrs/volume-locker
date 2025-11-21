<img align="right" width="200" alt="Volume Locker" src="https://github.com/user-attachments/assets/80473f4a-f901-440b-9d7b-5003ef1c96c7" />

<p align="center">
⭐<b>Please star this project in GitHub if it helps you!</b>⭐
</p>

# Volume Locker

Tired of apps changing your microphone volume without your consent? Or Windows constantly switching to the wrong audio device?

Volume Locker is a tray icon app that keeps the volume of your audio devices locked and manages your default audio devices based on a priority list. It is a portable, ~1MB binary written in Rust for Windows.

Never worry about your microphone volume or default device again!

## Demo

https://github.com/user-attachments/assets/b7e47898-ee9f-42b4-a804-f107beac4e98

## Getting Started

Simply grab the executable from the [releases page](https://github.com/felipecrs/volume-locker/releases), place it somewhere like `C:\Apps\Volume Locker\Volume Locker.exe` and run it.

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

1.  **Output devices**: List of all active output devices.
2.  **Input devices**: List of all active input devices.
3.  **Default output device priority**: Manage the priority list for default output devices.
4.  **Default input device priority**: Manage the priority list for default input devices.
5.  **Temporary default device priority**: Temporarily override the default device priority.

### Locking Volume and Unmute State

To lock the volume or unmute state of a specific device:

1.  Navigate to **Output devices** or **Input devices**.
2.  Select the desired device.
3.  Check **Keep volume locked** to lock the volume at the current level.
4.  Check **Keep unmuted** to prevent the device from being muted.
5.  You can also enable notifications for these actions.

### Default Device Priority

Volume Locker can automatically switch your default audio device based on a priority list. This is useful if you have multiple devices (e.g., speakers and headphones) and want to ensure a specific one is always used when available.

1.  Navigate to **Default output device priority** or **Default input device priority**.
2.  Select **Add device** to add a device to the priority list.
3.  Use **Move Up** and **Move Down** to adjust the priority order. The device at the top has the highest priority.
4.  Volume Locker will monitor your devices and automatically switch the default device to the highest priority one available.
5.  Check **Notify on restore** to get a notification when the default device is switched.
6.  Check **Also switch default communication device** to also switch the default communication device.

### Temporary Default Device Priority

If you want to temporarily use a different device without changing your priority list (e.g., switching to speakers for a call while the headphones are connected), you can use the **Temporary default device priority** feature.

1.  Navigate to **Temporary default device priority**.
2.  Select **Output device** or **Input device**.
3.  Choose the device you want to use temporarily.
4.  This device will be treated as the highest priority device until you uncheck it or restart the application.

## Credits

Volume Locker started as my first Rust project, born from the dissatisfaction with existing solutions that relied on closed-source tools or lacked specific device locking capabilities. It has since evolved to include advanced features like default device priority management.

I used [wolfinabox/Windows-Mic-Volume-Locker](https://github.com/wolfinabox/Windows-Mic-Volume-Locker) for years before deciding to write my own solution.

I was inspired by [AntoineGS/teams-status-rs](https://github.com/AntoineGS/teams-status-rs) being so amazing in such a lightweight package. I wanted to do something similar, but for volume locking.

Special thanks to:

- [Kingloo/volume](https://github.com/Kingloo/volume) for the Windows volume control code foundation.
- [tauri-apps/tray-icon](https://github.com/tauri-apps/tray-icon) for the tray icon implementation.
- [GitHub Copilot](https://github.com/copilot/) for assisting with the code structure, syntax, and feature implementation.
- [@jmb](https://github.com/jmb) for the [great icon design](https://github.com/Templarian/MaterialDesign/issues/7714).

