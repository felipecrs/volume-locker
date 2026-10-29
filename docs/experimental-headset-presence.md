# Experimental headset availability

Some wireless receivers keep their Windows audio endpoint active when the
headset is unavailable. This opt-in detector reads passive HID input events so
the existing input/output priority lists can skip that endpoint. **Windows
connection** remains the default; configure a fallback device in the same list.

Choose a source under **Output/Input devices > device > Availability**:

| Device | Source | Observed signal |
| --- | --- | --- |
| Astro A50 X | A50 X power events | Power-state HID reports |
| Logitech G533 | G533 link events | ADC-measurement wireless-link flag |

Unknown state defers to Windows. Available still requires an active Windows
endpoint; Unavailable excludes the opted-in endpoint. Startup stays Unknown
until an event arrives, and link loss does not distinguish power-off from
out-of-range or interference. Detection is experimental and may vary by
firmware.

## Adding a device

Add one row to the table above and follow the existing source pattern:

1. Capture and verify real available/unavailable HID events for the exact model.
2. Add a device profile and decoder; ignore unrelated reports and map ambiguous
   values to Unknown.
3. Give the source its own setting, monitor, and state. Keep detection passive:
   do not send HID requests or output reports.
4. Add captured-report fixtures, document device-specific limitations, and
   validate routing with the hardware connected.

Profiles share the passive HID transport, not device assumptions or state.
Battery, mute, silence, or a successful receiver request alone do not prove
headset availability.

## Diagnostics

- A50 X: `cargo run --locked --example headset_presence -- 30`
- G533: `cargo run --locked --example headset_presence -- --watch-g533 30`
- List G533 collections: `cargo run --locked --example headset_presence -- --list-g533`

These commands only observe HID input. Keep any captures local; they may contain
device identifiers.
