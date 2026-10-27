# Release Notes

## What's New

### Seamless Audio Device Migration
- Volume Locker now tracks your audio devices across physical connection changes.
- Moving a USB headset, DAC, or microphone to a different USB port preserves all your volume locks, unmute locks, and priority positions.
- Unpairing and re-pairing Bluetooth devices no longer loses your configured settings or priority order.
- Audio driver updates (such as NVIDIA, AMD, or Realtek updates) no longer disconnect your saved device settings.

### Intelligent Multi-Channel & Profile Protection
- Dual-channel headsets (with separate Game and Chat channels) are safely distinguished—settings on your Game channel will never accidentally migrate to your Chat channel.
- Bluetooth dual profiles (Stereo Headphones vs. Hands-Free Headset) are tracked separately without cross-migrating.
- If you use multiple identical audio devices without unique serial numbers, Volume Locker prevents accidental settings swapping.

### Smart Rename & Reset Handling
- Renaming an audio device in Windows Sound Settings is now recognized immediately without needing to restart the app.
- If Windows resets a renamed device back to its driver default name after re-plugging, Volume Locker recognizes the underlying hardware and keeps your settings intact.
- Custom device names are strictly guarded so they will never accidentally match against generic default names from other devices.

### Instant Wake & Sleep Volume Clamping
- Volume Locker now immediately wakes when your computer resumes from sleep or Modern Standby.
- Volume locks and unmute states are re-enforced right upon wake, preventing audio drivers from resetting your volume levels during system sleep.

### Device Migration Notifications
- Added a new **Notify on device migration** option under the **Preferences** menu.
- When enabled, a desktop notification lets you know whenever settings are transferred to a reconnected device (*"Device Settings Transferred: Settings for '...' were transferred to its new connection"*).

### Purely Event-Driven Operation
- Device reconnections, renames, and migrations now happen instantly as Windows events occur.
- No interaction with the tray icon or application restarts needed to catch device changes.
