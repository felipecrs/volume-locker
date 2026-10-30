# Experimental headset availability

Some wireless receivers keep their Windows audio endpoint active when the
headset is unavailable. This opt-in detector reads passive HID input events so
the existing input/output priority lists can skip that endpoint. Enable
**Wireless headset detection (experimental)** in Preferences; supported devices
are identified automatically by USB VID/PID. It is disabled by default.
Configure a fallback device in the same priority list.

## Supported devices

| Device        | USB VID:PID | Observed signal                    |
| ------------- | ----------- | ---------------------------------- |
| Astro A50 X   | `046d:0b0b` | Power-state HID reports            |
| Logitech G533 | `046d:0a66` | ADC-measurement wireless-link flag |
| Logitech G733 | `046d:0ab5` | ADC-measurement wireless-link flag |

Unknown state defers to Windows. Available still requires an active Windows
endpoint; Unavailable excludes the matching endpoint. Startup stays Unknown
until an event arrives, and link loss does not distinguish power-off from
out-of-range or interference. Detection is experimental and may vary by
firmware.

## Adding a device

Add one row to the table above and follow the same profile pattern:

1. Capture and verify real available/unavailable HID events for the exact model.
2. Add a device profile and decoder; ignore unrelated reports and map ambiguous
   values to Unknown.
3. Add the supported device's USB VID/PID mapping to its profile. Keep detection
   passive: do not send HID requests or output reports.
4. Add captured-report fixtures, document device-specific limitations, and
   validate routing with the hardware connected.

Profiles share the passive HID transport, not device assumptions or state. Each
audio endpoint is matched to its profile by USB VID/PID.
Battery, mute, silence, or a successful receiver request alone do not prove
headset availability.

## Diagnostics

The read-only example requires both a model (`a50x`, `g533`, or `g733`) and an
action (`list`, `watch`, or `capture`). Watch and capture accept an optional
duration in seconds (default 30, maximum 300):

- `cargo run --locked --example headset_presence -- a50x list`
- `cargo run --locked --example headset_presence -- g533 watch 30`
- `cargo run --locked --example headset_presence -- g733 capture 30`

Capture prints raw HID input reports to the terminal. It sends no HID output or
feature reports. Keep captured reports local; they may contain device
identifiers.
