# Experimental headset availability

Some wireless receivers keep their Windows audio endpoint active when the
headset is unavailable. This opt-in detector reads device status events so the
existing input/output priority lists can skip that endpoint. G533 and G733 also
get a read-only HID++ ADC query when monitoring starts.
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
endpoint; Unavailable excludes the matching endpoint. G533 and G733 start with
a read-only HID++ ADC query. For both, the receiver's ADC error `0x05` is
observed when the headset is unlinked and maps to Unavailable; other query
failures stay Unknown until an event arrives. G533 alone starts as Unavailable
until its first valid status; subsequent monitor failures become Unknown.
A50 X and G733 start Unknown until a status is available. Link loss does not
distinguish power-off from out-of-range or interference. Detection is
experimental and may vary by firmware.

## Adding a device

Add one row to the table above and follow the same profile pattern:

1. Capture and verify real available/unavailable HID events for the exact model.
2. Add a device profile and decoder; ignore unrelated reports and map ambiguous
   values to Unknown.
3. Add the supported device's USB VID/PID mapping to its profile. Keep any
   active HID communication read-only and device-specific.
4. Add captured-report fixtures, document device-specific limitations, and
   validate routing with the hardware connected.

Profiles share the HID monitoring worker, not device assumptions or state.
Each audio endpoint is matched to its profile by USB VID/PID.
Battery, mute, silence, or a successful receiver request alone do not prove
headset availability.

## Diagnostics

The read-only example requires both a model (`a50x`, `g533`, or `g733`) and an
action (`list`, `watch`, or `capture`). Watch and capture accept an optional
duration in seconds (default 30, maximum 300):

- `cargo run --locked --example headset_presence -- a50x list`
- `cargo run --locked --example headset_presence -- g533 watch 30`
- `cargo run --locked --example headset_presence -- g733 capture 30`

Capture prints raw HID input reports to the terminal. G533/G733 also send two
read-only HID++ requests to query their initial link state; they do not change
device settings. Keep captured reports local; they may contain device
identifiers.
