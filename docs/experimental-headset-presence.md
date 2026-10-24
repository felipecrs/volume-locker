# Experimental headset availability

Some wireless base stations keep their Windows audio endpoints active while the
headset is off. This opt-in detector reads device-specific HID input events and
lets the existing input/output priority lists skip unavailable headsets. It does
not require paired-output routing or independent communication lists.

Select **Output/Input devices > device > Availability > A50 X power events
(experimental)** for each affected endpoint. Configure a second device in its
normal priority list for fallback. **Windows connection** remains the default.

The provider has three states: Unknown defers to Windows; Available still requires
an active Windows endpoint; Unavailable excludes opted-in endpoints, including
temporary priority choices. Startup, an unrecognized power value, and provider
failure are Unknown. A read timeout preserves the last observation.

## A50 X evidence and limits

The earlier fork investigation observed a Logitech/Astro A50 X base with firmware
1.3.21 and USB identity `046d:0b0b`, usage page `ff32`, usage `0074`. Passive reports
began with `02 0c 04 00 13 00 00` on power-on and `02 0c 04 00 13 00 05` on power-off,
padded to 64 bytes. Chatmix, battery, volume and unrelated reports are ignored.
Automated tests replay these observations; they do not establish
compatibility with other firmware or headset models.

- Passive startup cannot identify an already-off headset. Toggle its power once
  after enabling or restarting the observer. A current-state query remains research.
- Docking, sleep, out-of-range behavior and running with G HUB exited need physical
  captures and a routing soak before stable release.
- Multiple matching interfaces are refused because endpoint association would be
  ambiguous. Only one matching headset is supported.
- The worker sends no HID output or feature reports and makes no G HUB requests.
  Reads and reconnects run off the event loop; dropping it joins the worker.

## Adding another device

`HidPresenceProfile` separates the interface identity and report decoder from
`HidPresenceMonitor`, the shared passive transport, reconnect loop and shutdown.
Profiles specify an input report length and must match vendor ID, product ID,
usage page and usage. A decoder returns
Available/Unavailable only for verified observations, Unknown for recognized but
uninterpretable status values, and None for unrelated or incomplete reports.

To support another model, capture its actual on/off behavior, implement a profile
and decoder, add a `PresenceSource` option and menu entry, and map that source to
its monitor in the Windows backend. Add fixtures for real transitions and false
positives, then physically validate the endpoint association and firmware. The
synthetic second-protocol test demonstrates transport reuse, not hardware support.
Additional sources need separate monitors/states rather than sharing A50 X state.

No universal USB-receiver power flag is assumed. Battery level, mute, silence and
successful base-station requests do not establish radio availability.

## Read-only diagnostics

Run `cargo run --locked --example headset_presence -- 30` to observe power events
without changing Windows defaults. `scripts/Capture-HidReports.py --help` describes
passive capture for a selected interface. `Inspect-HeadsetPresence.ps1` and
`GHub.ReadOnly.psm1` compare Windows and read-only G HUB observations. Captures can
contain identifiers; keep local data under ignored `target/presence-research/`.
