# Device ID migration

When Windows changes an audio endpoint ID, Volume Locker finds the same physical device and moves its volume locks, unmute locks, and priority list entries to the new ID.

## The problem

Windows Core Audio (WASAPI) identifies audio endpoints using ephemeral GUIDs:

- Output: `{0.0.0.00000000}.{GUID}`
- Input: `{0.0.1.00000000}.{GUID}`

The Windows `AudioEndpointBuilder` service often regenerates these GUIDs:

```mermaid
flowchart LR
    A[Physical Audio Device] -->|Move to another USB port| B[New Endpoint GUID]
    A -->|Unpair & re-pair Bluetooth| B
    A -->|Driver or Windows Update| B
    A -->|Windows resets friendly name| B
    B -->|Without Migration| C[❌ Lost volume locks & priority order]
    B -->|With Volume Locker| D[✅ Settings seamlessly transferred]
```

## Overview

Volume Locker handles device ID changes in three stages:

```mermaid
graph TB
    subgraph Detection["1. Event Triggers"]
        IMM[IMMNotificationClient<br/>Device Added / Removed / State / Prop Change]
        PWR[RegisterSuspendResumeNotification<br/>System Wake / Resume]
    end

    subgraph Recovery["2. Metadata & Fingerprint Recovery"]
        STORE[Windows COM Property Store<br/>GetDevice on disconnected endpoint]
        CACHE[Saved DeviceFingerprint<br/>Config fallback]
    end

    subgraph Matching["3. 6-Tier Migration Cascade"]
        T1[Tier 1: Hardware ID - MAC / Serial]
        T2[Tier 2: USB Profile - VID/PID + Interface]
        T3[Tier 3: Internal Audio Profile]
        T4[Tier 4: Custom Name Match]
        T5[Tier 5: Normalized Original Name]
        T6[Tier 6: Fallback VID/PID]
    end

    Detection --> Recovery
    Recovery --> Matching
    Matching -->|Match Found| APPLIED[Migrate Settings & Notify User]
    Matching -->|Ambiguity / Collision| REJECT[Safely Reject - No guessing]
```

## 1. Recovering the fingerprint

When a USB device is unplugged or re-enumerated, Windows moves the old endpoint to `DEVICE_STATE_NOTPRESENT` (`4`). Audio streams can no longer be activated on it, but Windows keeps the complete COM property store in the registry.

When a Bluetooth device is unpaired in Windows Settings, Windows deletes the endpoint (`0x80070490 Element not found`). To cover this, Volume Locker keeps an in-memory `device_cache` of all active endpoints:

```mermaid
sequenceDiagram
    autonumber
    participant APP as Volume Locker
    participant MM as Windows MMDevice API
    participant MEM as In-Memory Device Cache
    participant REG as Windows Registry (MMDevices)
    participant CFG as PersistentState (Config)

    alt Disconnected USB endpoint (DEVICE_STATE_NOTPRESENT)
        APP->>MM: IMMDeviceEnumerator::GetDevice(old_id)
        MM->>REG: OpenPropertyStore(STGM_READ)
        REG-->>APP: Return PKEY properties (Hardware ID, Names, FormFactor)
        Note over APP: Fingerprint recovered on the fly!
    else Unpaired Bluetooth endpoint (Element not found)
        APP->>MEM: Query in-memory device cache
        MEM-->>APP: Return cached fingerprint (Bluetooth MAC address)
        Note over APP: Fingerprint recovered from memory cache!
    else Offline fallback (Cold start / App restart)
        APP->>CFG: Read saved fingerprint from config
        CFG-->>APP: Return persistent fingerprint fallback
    end
```

Settings can therefore migrate across Bluetooth unpairing and USB port changes.

## 2. Hardware fingerprints

`DeviceFingerprint` holds hardware descriptors extracted from the endpoint's `IPropertyStore`:

```mermaid
classDiagram
    class DeviceFingerprint {
        +DeviceBusType bus_type
        +String hardware_desc
        +String interface_name
        +String clean_original_name
        +bool is_custom_name
        +String mac_address
        +Option~(u16, u16)~ usb_vid_pid
        +String usb_serial
        +u8 interface_index
        +String container_id
        +u32 form_factor
    }
```

- **Bluetooth (`BTHENUM`)**: 48-bit MAC address (`BD_ADDR`) from the PnP interface string. It persists across unpairing and re-pairing.
- **USB (`USB`)**: Vendor ID, Product ID, sub-interface index (`MI_xx`), and serial number (distinguished from synthetic port topology hashes).
- **Display / Motherboard (`HDAUDIO`, `PCI`)**: Driver subsystem ID, monitor description, and form factor.
- **Form factor (`PKEY_AudioEndpoint_FormFactor`)**: Prevents migrating between different profiles (e.g. A2DP Stereo Headphones vs. HFP Hands-Free Headset, or Game vs. Chat audio channels).

## 3. Name normalization and cross-matching

Windows derives the default friendly name from two driver properties:
$$\text{Friendly Name} = \text{DeviceDesc} + \text{" ("} + \text{DeviceInterface\_FriendlyName} + \text{")"}$$

When a user renames a device in Windows Sound Settings, Windows changes only `PKEY_Device_FriendlyName`. The underlying descriptors stay the same:

```mermaid
flowchart TD
    subgraph Inspection[Custom Name Detection]
        FN[PKEY_Device_FriendlyName]
        DD[PKEY_Device_DeviceDesc]
        IF[PKEY_DeviceInterface_FriendlyName]
        COMP{FriendlyName == DeviceDesc + ' (' + InterfaceName + ')'?}
        FN & DD & IF --> COMP
        COMP -->|No| CUSTOM[is_custom_name = true]
        COMP -->|Yes| DEFAULT[is_custom_name = false]
    end
```

### Safety rules

1. **Cross-matching**: A custom friendly name is never matched against a driver's default name.
   - _Prevents:_ A custom-named `"Speakers"` endpoint incorrectly matching a newly plugged `"Speakers (USB Audio Device)"`.
2. **Normalized original names**: Windows PnP adds port prefixes (e.g. `"2- "`) and clone counters (e.g. `" (1)"`) into `DeviceInterface_FriendlyName` when devices are moved across USB ports. Normalization strips these before matching:

```mermaid
flowchart LR
    RAW["2- Logitech USB Headset (1)"] -->|Strip '^\d+\s*-\s*'| S1["Logitech USB Headset (1)"]
    S1 -->|Strip '\s*\(\d+\)$'| CLEAN["Logitech USB Headset"]
```

3. **Bus type and VID/PID**: Endpoints on different hardware buses (e.g. motherboard `HDAUDIO` vs. `USB` or `Bluetooth`) or with conflicting USB VID/PIDs never match, in any tier.
   - _Prevents:_ Motherboard speakers named `"Speakers"` from cross-migrating onto a newly attached USB or Bluetooth speaker.
4. **Devices in use**: Active endpoints already in a priority list or an active temporary priority are excluded from the candidates.
   - _Prevents:_ Devices already assigned from being overwritten as migration targets.

## 4. The 6-tier migration cascade

Volume Locker compares missing registered devices against unclaimed active devices in six tiers, highest confidence first:

```mermaid
flowchart TD
    START([Unclaimed Active & Missing Devices]) --> T1

    T1{"Tier 1: Exact Hardware ID<br/>• Bluetooth MAC OR USB Serial<br/>• DeviceType & FormFactor match"}
    T1 -->|Match| MIGRATE[Migrate Device ID]
    T1 -->|No Match| T2

    T2{"Tier 2: USB Profile<br/>• USB VID/PID + MI_xx + Desc + FormFactor<br/>• Unambiguous 1:1 match"}
    T2 -->|Match| MIGRATE
    T2 -->|No Match| T3

    T3{"Tier 3: Internal Audio Profile<br/>• HDAUDIO Subsystem + Desc + FormFactor"}
    T3 -->|Match| MIGRATE
    T3 -->|No Match| T4

    T4{"Tier 4: Custom Name Match<br/>• Both devices have is_custom_name = true<br/>• friendly_name == friendly_name<br/>• DeviceType match"}
    T4 -->|Match| MIGRATE
    T4 -->|No Match| T5

    T5{"Tier 5: Normalized Original Name<br/>• clean_original == clean_original<br/>• BusType + FormFactor + DeviceType<br/>• Unambiguous 1:1 match"}
    T5 -->|Match| MIGRATE
    T5 -->|No Match| T6

    T6{"Tier 6: Heuristic Fallback<br/>• Unclaimed VID/PID or clean name<br/>• BusType + FormFactor compatibility<br/>• Strictly 1 missing to 1 active pair"}
    T6 -->|Match| MIGRATE
    T6 -->|No Match| NONE([No Migration / Safe Abort])
```

### Collision rejection

USB devices without a serial, or devices sharing an original name (e.g. two identical microphones or generic audio dongles), can look the same. If several missing or active candidates qualify for Tiers 2, 5, or 6, the migration is rejected, so settings are never swapped between identical devices.

## 5. Event triggers and power resume

Device ID migrations and volume locks are event-driven, with no periodic polling:

```mermaid
flowchart LR
    subgraph Sources[Event Sources]
        WIN_NOTIF[IMMNotificationClient<br/>• OnDeviceAdded<br/>• OnDeviceRemoved<br/>• OnDeviceStateChanged<br/>• OnDefaultDeviceChanged<br/>• OnPropertyValueChanged]
        WIN_PWR[RegisterSuspendResumeNotification<br/>• PBT_APMRESUMEAUTOMATIC<br/>• PBT_APMRESUMESUSPEND]
    end

    subgraph AppLoop[Volume Locker Event Loop]
        EVENT[UserEvent::DevicesChanged]
        MIG[migrate_device_ids_if_needed]
        LOCK[Synchronously enforce volume/unmute locks]
    end

    WIN_NOTIF -->|Immediate callback| EVENT
    WIN_PWR -->|Wake from sleep| EVENT
    EVENT --> MIG
    MIG --> LOCK
```

- **`OnPropertyValueChanged`**: Filtered to `PKEY_Device_FriendlyName`, so it only acts when a user renames an endpoint in Windows Sound Settings. Driver telemetry (volume, battery, routing, peak meters) is ignored.
- **Event coalescing**: Bursts of hardware events while a device connects (e.g. `OnDeviceAdded`, `OnDeviceStateChanged`, `OnDefaultDeviceChanged`) become a single event loop dispatch.
- **Event attribution**: Windows callbacks pass the affected `DeviceId` and state transition to the event loop, so logs can name them without blocking COM threads.
- **Power resume**: On wake from modern standby or system sleep, volume levels are enforced synchronously, in case audio drivers reset endpoint volumes.

## 6. Notifications

When a migration occurs and the user has enabled **Notify on device migration** under `Preferences`:

```mermaid
flowchart LR
    MIG[Device ID Migrated] --> PREF{Notify on device migration enabled?}
    PREF -->|Yes| TOAST["Desktop Notification<br/><b>Device Settings Transferred</b><br/>Settings for '{name}' were transferred to its new connection."]
    PREF -->|No| SILENT[Silent update in config]
```
