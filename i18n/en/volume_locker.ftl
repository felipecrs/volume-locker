## Device list

output-devices = Output devices
input-devices = Input devices
keep-volume-locked = Keep volume locked
notify-on-volume-restore = Notify on volume restore
keep-unmuted = Keep unmuted
notify-on-unmute = Notify on unmute
device-properties = Properties...
device-settings = Settings...
playback-devices = Playback devices...
recording-devices = Recording devices...

## Windows shortcuts

sound-settings = Sound settings...
volume-mixer = Volume mixer...

## Priority lists

output-device-priority = Output device priority
input-device-priority = Input device priority
device-priority-settings = Device priority settings
communication-priority-settings = Communication priority settings
priority-list = Priority list
no-devices-configured = No devices configured
no-devices-available = No devices available
follow-output-device-priority = Follow output device priority
follow-input-device-priority = Follow input device priority
add-device = Add device
add-to-top = Add to top
add-to-bottom = Add to bottom
move-up = Move up
move-down = Move down
move-to-top = Move to top
move-to-bottom = Move to bottom
remove-device = Remove device
notify-on-restore = Notify on restore
current-device = Current: { $device }
no-current-device = None

## Temporary priority

temporary-priority = Temporary priority
temporary-output = Output
temporary-input = Input
temporary-communication = Communication
temporary-with-device = { $target }: { $device }

## Preferences

preferences = Preferences
settings-transfer = Settings transfer
transfer-device-settings = Automatically transfer device settings
transfer-device-name-and-icon = Also transfer device name and icon
notify-on-settings-transfer = Notify on settings transfer
skip-disconnected-wireless-headsets = Skip disconnected wireless headsets
auto-launch = Auto-launch on startup
check-updates-on-launch = Check for updates on launch

## Troubleshooting and updates

troubleshooting = Troubleshooting
open-app-folder = Open app folder...
check-for-updates = Check for updates
update-to-version = Update to { $version }...
downloading-update = Downloading update...
quit = Quit

## Notifications

volume-restored-title = Volume Restored
volume-restored = The volume of { $device } has been restored from { $from }% to { $to }%.
output-unmuted-title = Output Device Unmuted
input-unmuted-title = Input Device Unmuted
device-unmuted = { $device } was unmuted due to Keep unmuted setting.
output-restored-title = Output Device Restored
input-restored-title = Input Device Restored
output-communication-restored-title = Output Communication Device Restored
input-communication-restored-title = Input Communication Device Restored
switched-by-priority = Switched to { $device } based on priority list.
unknown-device = Unknown Device
settings-transferred-title = Device Settings Transferred
settings-transferred = Settings for '{ $device }' were transferred to its new connection.
update-available-title = Update Available
update-available = Version { $version } is available. Click 'Update' in the menu to install.
no-updates-title = No Updates Available
no-updates = You are running the latest version of Volume Locker.

## Errors. The error argument is technical detail and stays in its original language.

directory-not-writable-title = Volume Locker Directory Not Writable
directory-not-writable = Please move Volume Locker to a directory that is writable or fix the permissions of '{ $path }'.
save-state-failed-title = Failed to Save State
save-state-failed = Failed to save state: { $error }
save-state-after-update-failed = Failed to save state after device update: { $error }
update-failed-title = Update Failed
update-failed = Update failed: { $error }
update-check-failed-title = Update Check Failed
update-check-failed = Failed to check for updates: { $error }
auto-launch-failed-title = Failed to Toggle Auto-Launch
auto-launch-failed = Failed to toggle auto-launch: { $error }
rebuild-menu-failed-title = Failed to Rebuild Tray Menu
rebuild-menu-failed = Failed to rebuild tray menu: { $error }
lock-volume-failed-title = Failed to Lock Volume
lock-volume-failed = Failed to get volume for device { $device }, cannot lock.
