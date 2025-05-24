<p align="center">
⭐<b>Please star this project in GitHub if it helps you!</b>⭐
</p>

# Volume Locker

<img align="right" width="200" alt="Volume Locker" src="https://github.com/user-attachments/assets/20dbba8d-f86f-4f88-b72c-088180ecbe30" />

Tired of apps changing your microphone volume without your consent?

Volume Locker is a tray icon app that keeps the volume of your audio devices locked. It is a portable, less than 1MB binary written in Rust for Windows.

Never worry about your microphone volume again!

## Demo

> [!IMPORTANT]
> This demo is outdated, since v0.6.0, Volume Locker no longer takes some seconds to restore the volume. It is now instantaneous.

https://github.com/user-attachments/assets/772af810-0353-4db0-99ec-ab39c6cd6aab

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

When you want to lock the volume of your audio output or input devices:

1. Adjust the volume to the desired level
2. Click on the Volume Locker tray icon
3. Select the device you want to lock the volume for
4. Try messing with the volume of the device. It should return to the locked level shortly after.

## Credits

This is my first Rust project, with no prior experience or knowledge of the language. Don't expect the code to be examplary, but I think it's overall optimised.

I was dissastisfied with any existing solution because they were either relying on `nircmd.exe`, which is not open source, and also didn't allow to lock the volume of specific devices, only the current default one.

In fact, I used [wolfinabox/Windows-Mic-Volume-Locker](https://github.com/wolfinabox/Windows-Mic-Volume-Locker) for years before deciding to write my own solution.

I was inspired by [AntoineGS/teams-status-rs](https://github.com/AntoineGS/teams-status-rs) being so amazing in such a lightweight package. I wanted to do something similar, but for volume locking.

Thanks [Kingloo/volume](https://github.com/Kingloo/volume) for the Windows volume control code. I copied pretty much everything from there.

The barebones of the tray icon code were taken from [tauri-apps/tray-icon](https://github.com/tauri-apps/tray-icon)'s [tao example](https://github.com/tauri-apps/tray-icon/blob/97723fd207add9c3bb0511cb0e4d04d8652a0027/examples/tao.rs)

[GitHub Copilot](https://github.com/copilot/) helped me a lot to glue everything together, and to get the syntax going.

Finally, thanks to [@jmb](https://github.com/jmb) for the [great icon design](https://github.com/Templarian/MaterialDesign/issues/7714).
