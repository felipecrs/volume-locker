# Windows Audio Endpoint Property Migration

This document describes how Volume Locker carries a user's Windows-assigned audio endpoint name and icon across an endpoint ID migration. It complements [Device ID Migration Architecture](./device-id-migration.md), which documents how the old and new endpoints are matched.

## Scope

Windows associates endpoint properties with an audio endpoint ID. When Windows creates a replacement endpoint for the same physical device, the new endpoint may have a different ID and default properties. Once the device-ID migration matcher has made an unambiguous match, Volume Locker attempts to carry over:

- The endpoint name from `PKEY_Device_DeviceDesc`. This is the property Windows edits when the user renames an endpoint in Sound settings; `PKEY_Device_FriendlyName` is computed as `"{DeviceDesc} ({InterfaceName})"` and is not stored.
- The endpoint icon path from `PKEY_DeviceClass_IconPath` (`{259abffc-50a7-47ce-af08-68c9a7d73366},12`), which is what Sound settings edits. `PKEY_AudioEndpointLogo_IconPath` is never populated on endpoints inspected.

The icon path is either a `.ico` file path or a resource reference such as `%windir%\system32\mmres.dll,-3014`; Volume Locker migrates that reference, not the image bytes or the driver resource itself.

## Data flow

```mermaid
flowchart LR
    OLD[Disconnected endpoint] -->|Stored DeviceFingerprint| CFG[Persistent device settings]
    OLD -->|Recent fingerprint| CACHE[In-memory device cache]
    CFG --> MATCH[Unambiguous ID migration match]
    CACHE --> MATCH
    MATCH -->|Name and icon path| NEW[New endpoint property store]
    MATCH -->|Volume, mute, and priority settings| STATE[Persistent state under new ID]
```

Volume Locker does not decide whether a name or logo is custom. Whenever a tracked device is seen, its current `PKEY_Device_DeviceDesc` (`hardware_desc`) and icon path are stored in its `DeviceFingerprint`, so the last seen values are always available. On migration they are restored as-is; a value is only written when it differs from the new endpoint's current value.

`DeviceFingerprint` holds the raw endpoint name and icon path in addition to the hardware identity used by the matcher. It is saved with each device's settings and also included in the in-memory device cache. Thus the source properties can still be available after Windows has removed the old endpoint, including when a Bluetooth device is unpaired. The existing recovery order remains unchanged: saved fingerprint, in-memory cache, then a property-store lookup for the old endpoint.

The endpoint name is kept in its raw Windows form for writing back; the existing cleaned `DeviceSettings.name` remains the user-facing name used by Volume Locker. Older state without a fingerprint, or without an icon path, remains valid because the fields are optional.

## Migration sequence

1. Device change handling runs the existing fingerprint and six-tier ID matcher.
2. The matcher rejects ambiguous pairs and excludes devices already claimed by current settings or priority lists.
3. For an accepted pair, migration asks the Windows audio backend to restore the last seen endpoint name and icon path that differ from the new endpoint's.
4. The backend uses the existing Windows `IPolicyConfig` endpoint configuration interface to set `PKEY_Device_DeviceDesc` and/or `PKEY_DeviceClass_IconPath` as `VT_LPWSTR` values.
5. The existing app-state migration moves device settings, persistent priorities, and temporary priorities to the new ID.

This property restoration does not participate in matching and cannot cause an ambiguous migration to be accepted.

## Failure behavior and platform boundary

Windows owns audio endpoint properties. Microsoft's Core Audio documentation says clients can read endpoint properties but should not set them; the existing `IPolicyConfig` COM interface used here is undocumented and Windows may reject a write or later replace a value. Restoration is best-effort: a property-write failure is logged with both endpoint IDs, while the independent Volume Locker settings and priority migration still completes. The app does not elevate privileges, edit the registry directly, or roll back a successful device-ID match because a property write failed.

The backend operation is deliberately limited to the two properties above. Driver descriptions, hardware identity, form factor, and other endpoint metadata are not copied from the old endpoint, so the new endpoint keeps its own hardware properties. A copied icon path is only useful if its referenced resource remains available on the system.

## References

- [Audio Endpoint Properties (Microsoft Learn)](https://learn.microsoft.com/en-us/windows/win32/coreaudio/audio-endpoint-properties) — endpoint properties are system-managed and clients should read, not set, them.
- [PKEY_Device_FriendlyName (Microsoft Learn)](https://learn.microsoft.com/en-us/windows/win32/coreaudio/pkey-device-friendlyname) — friendly-name property and `VT_LPWSTR` representation.
- Registry inspection of `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio` — shows `PKEY_DeviceClass_IconPath` holding built-in (`mmres.dll,N`) and custom (`.ico`) endpoint icons.
