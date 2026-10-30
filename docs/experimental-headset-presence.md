# Experimental headset availability

Some wireless receivers keep their Windows audio endpoint active when the
headset is unavailable. This opt-in detector uses a one-time read-only startup
query when supported, then listens for device status events so the existing
input/output priority lists can skip that endpoint.
Enable **Wireless headset detection (experimental)** in Preferences; supported
devices are identified automatically by USB VID/PID. It is disabled by
default. Configure a fallback device in the same priority list.

## Supported devices

| Device        | USB VID:PID | Observed signal                    | Startup status query              |
| ------------- | ----------- | ---------------------------------- | --------------------------------- |
| Astro A50 X   | `046d:0b0b` | Power-state HID reports            | None                              |
| Logitech G533 | `046d:0a66` | ADC-measurement wireless-link flag | ADC; error `0x05` means Unavailable |
| Logitech G733 | `046d:0ab5` | ADC-measurement wireless-link flag | ADC; error `0x05` means Unavailable |

Unknown state defers to Windows. Available still requires an active Windows
endpoint; Unavailable excludes the matching endpoint. State starts Unknown and
changes when a configured startup query succeeds or a valid status report
arrives. Query failures do not imply Unavailable; if no status is known, the
state remains Unknown. Monitor failures reset the state to Unknown. A link-loss
report may not distinguish power-off from out-of-range or interference.
Detection is experimental and may vary by firmware.

## Adding a device

Device profiles and the model registry live in
[`src/audio/presence/devices.rs`](../src/audio/presence/devices.rs). Adding a
device should not require changes to the monitor worker, Windows backend, or
diagnostic example:

1. Capture and verify real available/unavailable HID events for the exact model.
2. Define its profile and register it in `PROFILES` in `devices.rs`, reusing an
   existing decoder or adding a device-specific decoder that ignores
   unrelated reports and maps ambiguous values to Unknown.
3. Configure an optional read-only startup query in the profile. Without a
   query, state remains Unknown until the first valid report; ongoing monitoring
   listens for input reports.
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

Capture prints raw HID input reports to the terminal. Models with a configured
startup query issue read-only requests to retrieve an initial status; no
settings are changed. Keep captured reports local; they may contain device
identifiers.
