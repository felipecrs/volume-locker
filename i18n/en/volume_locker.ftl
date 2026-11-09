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

## Default device order

default-output-device-order = Default output device order
default-input-device-order = Default input device order
device-order = Device order
communication-device-order = Communication device order
current-order = Current order
no-devices-added = No devices added
no-devices-to-add = No devices to add
follow-default-output-device = Follow default output device
follow-default-input-device = Follow default input device
add-device = Add device
add-to-top = Add to top
add-to-bottom = Add to bottom
move-up = Move up
move-down = Move down
move-to-top = Move to top
move-to-bottom = Move to bottom
remove-device = Remove device
notify-on-switch = Notify on switch
current-device = Current device: { $device }
no-current-device = None

## Temporary default device

temporary-default-device = Temporary default device
no-devices-available = No devices available
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
output-switched-title = Default Output Device Switched
input-switched-title = Default Input Device Switched
output-communication-switched-title = Default Communication Output Device Switched
input-communication-switched-title = Default Communication Input Device Switched
switched-by-order = Switched to { $device } based on device order.
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
