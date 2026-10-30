# Experimental headset availability

Some wireless receivers keep their Windows audio endpoint active when the
headset is unavailable. This opt-in detector passively reads device status
events so the existing input/output priority lists can skip that endpoint.
Enable **Wireless headset detection (experimental)** in Preferences; supported
devices are identified automatically by USB VID/PID. It is disabled by
default. Configure a fallback device in the same priority list.

## Supported devices

| Device        | USB VID:PID | Observed signal                    |
| ------------- | ----------- | ---------------------------------- |
| Astro A50 X   | `046d:0b0b` | Power-state HID reports            |
| Logitech G533 | `046d:0a66` | ADC-measurement wireless-link flag |
| Logitech G733 | `046d:0ab5` | ADC-measurement wireless-link flag |

Unknown state defers to Windows. Available still requires an active Windows
endpoint; Unavailable excludes the matching endpoint. Each profile defines its
initial state until a valid report arrives. Monitor failures reset the state to
Unknown. A link-loss report may not distinguish power-off from out-of-range or
interference. Detection is experimental and may vary by firmware.

## Adding a device

Device profiles and the model registry live in
[`src/audio/presence/devices.rs`](../src/audio/presence/devices.rs). Adding a
device should not require changes to the monitor worker, Windows backend, or
diagnostic example:

1. Capture and verify real available/unavailable HID events for the exact model.
2. Define its profile and register it in `PROFILES` in `devices.rs`, reusing an
   existing decoder or adding a device-specific decoder that ignores
   unrelated reports and maps ambiguous values to Unknown.
3. Configure the profile's startup fallback when needed. Monitoring remains
   passive and never sends device commands.
4. Add captured-report fixtures in `presence.rs` and one supported-device row
   in this table. Validate routing with the hardware connected.

Profiles share the HID monitoring worker, not device assumptions or state.
Each audio endpoint is matched to its profile by USB VID/PID.
Battery, mute, silence, or a successful receiver request alone do not prove
headset availability.

## Diagnostics

The read-only example takes a registered model and an action (`list`, `watch`,
or `capture`). Watch and capture accept an optional duration in seconds
(default 30, maximum 300):

- `cargo run --locked --example headset_presence -- <model> <list|watch|capture> [seconds]`

Capture prints raw HID input reports to the terminal. Monitoring only reads
input reports and does not send device commands. Keep captured reports local;
they may contain device identifiers.
