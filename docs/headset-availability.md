# Headset availability

To use this, put a fallback device below the headset in the same priority list. It is on by default. To turn it off, uncheck **Skip disconnected wireless headsets** in **Preferences**.

Some wireless receivers keep their Windows audio endpoint active while the headset is off or out of range. Volume Locker reads the status of supported headsets over HID, and the priority lists skip the endpoint while the headset is unavailable. Supported headsets are identified by USB VID/PID.

## Supported headsets

| Device        | USB VID:PID | Observed signal                    | Startup status query                |
| ------------- | ----------- | ---------------------------------- | ----------------------------------- |
| Astro A50 X   | `046d:0b0b` | Power-state HID reports            | None                                |
| Logitech G533 | `046d:0a66` | ADC-measurement wireless-link flag | ADC; error `0x05` means Unavailable |
| Logitech G733 | `046d:0ab5` | ADC-measurement wireless-link flag | ADC; error `0x05` means Unavailable |

## How the state changes

| State       | What happens                                   |
| ----------- | ---------------------------------------------- |
| Unknown     | Windows decides whether the endpoint is usable |
| Available   | The endpoint must still be active in Windows   |
| Unavailable | The endpoint is skipped                        |

The state starts as Unknown. It changes when a status query succeeds or a valid status report arrives.

- **Status query:** read-only, and runs each time the monitor connects to the device. A failed query does not mean Unavailable, unless it returns the error a profile maps to Unavailable.
- **Monitor failure:** resets the state to Unknown.
- **Feature index:** some devices assign status events a feature index that only the startup query can discover. If that lookup fails, their status events are ignored and the state stays Unknown until the monitor reconnects.
- **Limits:** a link-loss report may not distinguish power-off from out-of-range or interference, and detection may vary by firmware.

## Adding a headset

Device profiles and the model registry are in [`src/audio/presence/devices.rs`](../src/audio/presence/devices.rs). A new headset should not need changes to the monitor worker, Windows backend, or diagnostic example.

1. Capture real available and unavailable HID events for the exact model. Battery, mute, silence, or a successful receiver request alone do not prove the headset is available.
2. Add its profile to `PROFILES` in `devices.rs`. Reuse a decoder, or add one that ignores unrelated reports and maps ambiguous values to Unknown.
3. Optionally, configure a read-only startup query in the profile. Without one, the state stays Unknown until the first valid report.
4. Add captured-report fixtures in `devices.rs` and a row in the supported headsets table.
5. Validate routing with the hardware connected.

Each audio endpoint is matched to its profile by USB VID/PID. Profiles share the HID monitoring worker, but not device assumptions or state.

## Diagnostics

```bash
cargo run --locked --example headset_presence -- <model> <list|watch|capture> [seconds]
```

- `list` shows the matching HID interfaces.
- `watch` prints state changes.
- `capture` also prints raw HID input reports. The app logs these reports too, but only at trace level.

`watch` and `capture` run for 30 seconds by default, up to 300.

Models with a startup query send read-only requests to get their initial status, and no settings change. Keep captured reports local, because they may contain device identifiers.
