use crate::config::PersistentState;
use crate::types::{
    DeviceBusType, DeviceFingerprint, DeviceId, DeviceSettings, DeviceType, PriorityKind,
    TemporaryPriorities,
};
use regex_lite::Regex;
use std::sync::LazyLock;

use super::AudioBackend;

pub(crate) fn clean_name_component(s: &str) -> String {
    static PREFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^\d+\s*-\s*").unwrap_or_else(|_| unreachable!("constant regex pattern"))
    });
    static CLONE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\s*\(\d+\)$").unwrap_or_else(|_| unreachable!("constant regex pattern"))
    });
    let s = PREFIX_RE.replace(s, "");
    let s = CLONE_RE.replace(&s, "");
    s.trim().to_string()
}

pub(crate) fn clean_device_name(name: &str) -> String {
    static NAME_SPLITTER: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?P<friendlyName>.+)\s\([\d\s\-|]*(?P<deviceName>.+)\)")
            .unwrap_or_else(|_| unreachable!("constant regex pattern"))
    });
    static NAME_CLEANER: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\s?\(\d\)|^\d+\s?-\s?")
            .unwrap_or_else(|_| unreachable!("constant regex pattern"))
    });

    if let Some(captures) = NAME_SPLITTER.captures(name) {
        let friendly_name = captures.name("friendlyName").map_or("", |m| m.as_str());
        let device_name = captures.name("deviceName").map_or("", |m| m.as_str());

        let cleaned_friendly = NAME_CLEANER.replace_all(friendly_name, "");
        let cleaned_friendly = cleaned_friendly.trim();

        format!("{cleaned_friendly} ({device_name})")
    } else {
        name.to_string()
    }
}

pub(crate) fn parse_bus_type(
    enumerator: Option<&str>,
    pnp_iface: Option<&str>,
) -> Option<DeviceBusType> {
    if let Some(e) = enumerator {
        let upper = e.to_uppercase();
        if upper.contains("USB") {
            return Some(DeviceBusType::Usb);
        }
        if upper.contains("BTH") || upper.contains("BLUETOOTH") {
            return Some(DeviceBusType::Bluetooth);
        }
        if upper.contains("HDAUDIO") || upper.contains("PCI") {
            return Some(DeviceBusType::HdAudio);
        }
        if upper.contains("ROOT") || upper.contains("SWD") {
            return Some(DeviceBusType::RootVirtual);
        }
    }
    if let Some(p) = pnp_iface {
        let upper = p.to_uppercase();
        if upper.contains("USB") {
            return Some(DeviceBusType::Usb);
        }
        if upper.contains("BTH") || upper.contains("BLUETOOTH") {
            return Some(DeviceBusType::Bluetooth);
        }
        if upper.contains("HDAUDIO") || upper.contains("PCI") {
            return Some(DeviceBusType::HdAudio);
        }
        if upper.contains("ROOT") {
            return Some(DeviceBusType::RootVirtual);
        }
    }
    Some(DeviceBusType::Unknown)
}

pub(crate) fn parse_mac_address(pnp_iface: &str) -> Option<String> {
    static BTH_MAC_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)&([0-9a-f]{12})(?:_|[\\/]|\b|$)")
            .unwrap_or_else(|_| unreachable!("constant regex pattern"))
    });
    BTH_MAC_RE.captures(pnp_iface).map(|c| c[1].to_uppercase())
}

pub(crate) fn parse_usb_vid_pid(pnp_iface: &str) -> Option<(u16, u16)> {
    static VID_PID_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)VID_([0-9a-f]{4})&PID_([0-9a-f]{4})")
            .unwrap_or_else(|_| unreachable!("constant regex pattern"))
    });
    VID_PID_RE.captures(pnp_iface).and_then(|c| {
        let vid = u16::from_str_radix(&c[1], 16).ok()?;
        let pid = u16::from_str_radix(&c[2], 16).ok()?;
        Some((vid, pid))
    })
}

pub(crate) fn parse_interface_index(pnp_iface: &str) -> Option<u8> {
    static MI_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)&MI_([0-9a-f]{2})")
            .unwrap_or_else(|_| unreachable!("constant regex pattern"))
    });
    MI_RE
        .captures(pnp_iface)
        .and_then(|c| u8::from_str_radix(&c[1], 16).ok())
}

pub(crate) fn parse_usb_serial(bus_type: Option<DeviceBusType>, pnp_iface: &str) -> Option<String> {
    if bus_type != Some(DeviceBusType::Usb) {
        return None;
    }
    pnp_iface.rsplit('\\').next().and_then(|seg| {
        let trimmed = seg.trim();
        // If the instance segment contains '&', Windows synthesized it from the port topology.
        // A true hardware serial number contains no '&' and is non-empty.
        if !trimmed.is_empty() && !trimmed.contains('&') {
            Some(trimmed.to_string())
        } else {
            None
        }
    })
}

pub(crate) fn build_fingerprint(
    friendly_name: Option<String>,
    hardware_desc: Option<String>,
    interface_name: Option<String>,
    enumerator_name: Option<String>,
    container_id: Option<String>,
    form_factor: Option<u32>,
    pnp_interface: Option<String>,
) -> DeviceFingerprint {
    let bus_type = parse_bus_type(enumerator_name.as_deref(), pnp_interface.as_deref());
    let mac_address = pnp_interface.as_deref().and_then(parse_mac_address);
    let usb_vid_pid = pnp_interface.as_deref().and_then(parse_usb_vid_pid);
    let interface_index = pnp_interface.as_deref().and_then(parse_interface_index);
    let usb_serial = parse_usb_serial(bus_type, pnp_interface.as_deref().unwrap_or(""));

    let is_custom_name = match (&friendly_name, &hardware_desc, &interface_name) {
        (Some(friendly), Some(desc), Some(iface)) => {
            let default_template = format!("{desc} ({iface})");
            friendly != &default_template
        }
        (Some(friendly), Some(desc), None) => friendly != desc,
        _ => false,
    };

    let clean_desc = hardware_desc.as_deref().map(clean_name_component);
    let clean_iface = interface_name.as_deref().map(clean_name_component);

    let clean_original_name = match (clean_desc, clean_iface) {
        (Some(d), Some(i)) if !d.is_empty() && !i.is_empty() => Some(format!("{d} ({i})")),
        (Some(d), _) if !d.is_empty() => Some(d),
        (_, Some(i)) if !i.is_empty() => Some(i),
        _ => None,
    };

    DeviceFingerprint {
        bus_type,
        hardware_desc,
        interface_name,
        clean_original_name,
        is_custom_name,
        mac_address,
        usb_vid_pid,
        usb_serial,
        interface_index,
        container_id,
        form_factor,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchTier {
    ExactHardwareId = 1,
    HardwareProfileAndInterface = 2,
    InternalHardwareProfile = 3,
    CustomToCustomName = 4,
    NormalizedOriginalName = 5,
    HeuristicFallback = 6,
}

impl std::fmt::Display for MatchTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExactHardwareId => write!(f, "Tier 1 (Exact Hardware ID)"),
            Self::HardwareProfileAndInterface => write!(f, "Tier 2 (Hardware Profile & Interface)"),
            Self::InternalHardwareProfile => write!(f, "Tier 3 (Internal Hardware Profile)"),
            Self::CustomToCustomName => write!(f, "Tier 4 (Custom-to-Custom Name)"),
            Self::NormalizedOriginalName => write!(f, "Tier 5 (Normalized Original Name)"),
            Self::HeuristicFallback => write!(f, "Tier 6 (Heuristic Fallback)"),
        }
    }
}

#[derive(Clone)]
struct MissingDevice {
    id: DeviceId,
    settings: DeviceSettings,
    fingerprint: Option<DeviceFingerprint>,
}

#[derive(Clone)]
struct ActiveCandidate {
    id: DeviceId,
    name: String,
    fingerprint: Option<DeviceFingerprint>,
}

fn matches_tier(missing: &MissingDevice, candidate: &ActiveCandidate, tier: MatchTier) -> bool {
    let m_fp = missing.fingerprint.as_ref();
    let c_fp = candidate.fingerprint.as_ref();

    // FormFactor compatibility: if both specify form_factor, they must match.
    if let (Some(m), Some(c)) = (
        m_fp.and_then(|f| f.form_factor),
        c_fp.and_then(|f| f.form_factor),
    ) && m != c
    {
        return false;
    }

    // BusType compatibility: if both specify bus_type, they must match.
    if let (Some(m), Some(c)) = (m_fp.and_then(|f| f.bus_type), c_fp.and_then(|f| f.bus_type))
        && m != c
    {
        return false;
    }

    // USB VID/PID compatibility: if both specify usb_vid_pid, they must match.
    if let (Some(m), Some(c)) = (
        m_fp.and_then(|f| f.usb_vid_pid),
        c_fp.and_then(|f| f.usb_vid_pid),
    ) && m != c
    {
        return false;
    }

    match tier {
        MatchTier::ExactHardwareId => {
            // Bluetooth MAC match
            let mac_match = match (
                m_fp.and_then(|f| f.mac_address.as_deref()),
                c_fp.and_then(|f| f.mac_address.as_deref()),
            ) {
                (Some(m), Some(c)) => m == c,
                _ => false,
            };
            if mac_match {
                return true;
            }

            // USB Serial Number match (requires non-empty serial and matching VID/PID)
            match (
                m_fp.and_then(|f| f.usb_serial.as_deref()),
                c_fp.and_then(|f| f.usb_serial.as_deref()),
                m_fp.and_then(|f| f.usb_vid_pid),
                c_fp.and_then(|f| f.usb_vid_pid),
            ) {
                (Some(ms), Some(cs), Some(mvp), Some(cvp)) => ms == cs && mvp == cvp,
                _ => false,
            }
        }
        MatchTier::HardwareProfileAndInterface => {
            let Some(m) = m_fp else { return false };
            let Some(c) = c_fp else { return false };

            if m.bus_type != Some(DeviceBusType::Usb) || c.bus_type != Some(DeviceBusType::Usb) {
                return false;
            }

            let vid_pid_matches = match (m.usb_vid_pid, c.usb_vid_pid) {
                (Some(m_vp), Some(c_vp)) => m_vp == c_vp,
                _ => false,
            };
            if !vid_pid_matches {
                return false;
            }

            if let (Some(m_mi), Some(c_mi)) = (m.interface_index, c.interface_index)
                && m_mi != c_mi
            {
                return false;
            }

            if let (Some(m_desc), Some(c_desc)) =
                (m.hardware_desc.as_deref(), c.hardware_desc.as_deref())
                && m_desc != c_desc
            {
                return false;
            }

            true
        }
        MatchTier::InternalHardwareProfile => {
            let Some(m) = m_fp else { return false };
            let Some(c) = c_fp else { return false };

            let hd_audio_match = m.bus_type == Some(DeviceBusType::HdAudio)
                && c.bus_type == Some(DeviceBusType::HdAudio);
            let container_match = match (m.container_id.as_deref(), c.container_id.as_deref()) {
                (Some(mc), Some(cc)) => mc == cc,
                _ => false,
            };

            if !hd_audio_match && !container_match {
                return false;
            }

            match (m.hardware_desc.as_deref(), c.hardware_desc.as_deref()) {
                (Some(m_desc), Some(c_desc)) if !m_desc.is_empty() => m_desc == c_desc,
                _ => false,
            }
        }
        MatchTier::CustomToCustomName => {
            let m_custom = m_fp.is_some_and(|f| f.is_custom_name);
            let c_custom = c_fp.is_some_and(|f| f.is_custom_name);

            if m_custom && c_custom {
                missing.settings.name == candidate.name
            } else {
                false
            }
        }
        MatchTier::NormalizedOriginalName => {
            // Anti-cross-matching: Do not match a custom name with an original name!
            let m_custom = m_fp.is_some_and(|f| f.is_custom_name);
            let c_custom = c_fp.is_some_and(|f| f.is_custom_name);
            if m_custom != c_custom {
                return false;
            }

            let Some(m) = m_fp else { return false };
            let Some(c) = c_fp else { return false };

            let orig_matches = match (
                m.clean_original_name.as_deref(),
                c.clean_original_name.as_deref(),
            ) {
                (Some(m_orig), Some(c_orig)) if !m_orig.is_empty() => m_orig == c_orig,
                _ => false,
            };
            if !orig_matches {
                return false;
            }

            if let (Some(m_bus), Some(c_bus)) = (m.bus_type, c.bus_type)
                && m_bus != c_bus
            {
                return false;
            }

            true
        }
        MatchTier::HeuristicFallback => {
            // Anti-cross-matching: Do not match if either has a custom name
            let m_custom = m_fp.is_some_and(|f| f.is_custom_name);
            let c_custom = c_fp.is_some_and(|f| f.is_custom_name);
            if m_custom || c_custom {
                return false;
            }

            // Reject cross-bus matching when bus types are known and differ
            if let (Some(m_bus), Some(c_bus)) =
                (m_fp.and_then(|f| f.bus_type), c_fp.and_then(|f| f.bus_type))
                && m_bus != c_bus
            {
                return false;
            }

            // Reject conflicting USB VID/PID when both are known and differ
            if let (Some(m_vp), Some(c_vp)) = (
                m_fp.and_then(|f| f.usb_vid_pid),
                c_fp.and_then(|f| f.usb_vid_pid),
            ) && m_vp != c_vp
            {
                return false;
            }

            if clean_device_name(&missing.settings.name) == clean_device_name(&candidate.name) {
                return true;
            }

            match (
                m_fp.and_then(|f| f.usb_vid_pid),
                c_fp.and_then(|f| f.usb_vid_pid),
            ) {
                (Some(m_vp), Some(c_vp)) => m_vp == c_vp,
                _ => false,
            }
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MigrationResult {
    pub persistent_changed: bool,
    pub migrated_device_names: Vec<String>,
}

impl MigrationResult {
    #[cfg(test)]
    pub fn has_migrations(&self) -> bool {
        !self.migrated_device_names.is_empty()
    }
}

fn is_device_active(backend: &impl AudioBackend, id: &DeviceId) -> bool {
    backend
        .device_by_id(id)
        .is_ok_and(|d| d.is_active().unwrap_or(false))
}

pub fn migrate_device_ids(
    backend: &impl AudioBackend,
    persistent_state: &mut PersistentState,
    temporary_priorities: &mut TemporaryPriorities,
    device_cache: &std::collections::HashMap<
        DeviceId,
        (String, DeviceType, Option<DeviceFingerprint>),
    >,
) -> MigrationResult {
    let mut persistent_changed = false;
    let mut migrated_device_names: Vec<String> = Vec::new();
    let mut devices_to_update: Vec<(DeviceId, DeviceSettings)> = Vec::new();
    let mut missing_by_type: Vec<MissingDevice> = Vec::new();

    // Collect first, then mutate — avoids borrowing `persistent_state.devices`
    // while iterating over it.
    for (device_id, device_settings) in persistent_state.devices_iter() {
        if is_device_active(backend, device_id) {
            let Ok(device) = backend.device_by_id(device_id) else {
                continue;
            };
            let current_name = device.name();
            let current_fp = device.fingerprint();
            let mut updated_settings = device_settings.clone();
            let mut changed = false;

            if current_name != device_settings.name {
                log::info!(
                    "Device '{}' ({device_id}) renamed to '{current_name}'",
                    device_settings.name,
                );
                updated_settings.name = current_name;
                changed = true;
            }

            if current_fp.is_some() && updated_settings.fingerprint != current_fp {
                updated_settings.fingerprint = current_fp;
                changed = true;
            }

            if changed {
                devices_to_update.push((device_id.clone(), updated_settings));
            }
        } else {
            // Attempt to recover fingerprint from saved settings, device cache, or Windows
            let recovered_fp = device_settings
                .fingerprint
                .clone()
                .or_else(|| {
                    device_cache
                        .get(device_id)
                        .and_then(|(_, _, fp)| fp.clone())
                })
                .or_else(|| backend.device_fingerprint(device_id));

            missing_by_type.push(MissingDevice {
                id: device_id.clone(),
                settings: device_settings.clone(),
                fingerprint: recovered_fp,
            });
        }
    }

    for (device_id, updated_settings) in devices_to_update {
        persistent_state.insert_device(device_id, updated_settings);
        persistent_changed = true;
    }

    // Also include any disconnected devices in priority lists not tracked in persistent_state.devices
    for device_type in [DeviceType::Output, DeviceType::Input] {
        for kind in [PriorityKind::Main, PriorityKind::Communication] {
            for priority_id in persistent_state
                .priority_list_of(device_type, kind)
                .to_vec()
            {
                if !priority_id.is_main_priority()
                    && !missing_by_type.iter().any(|m| m.id == priority_id)
                    && !is_device_active(backend, &priority_id)
                {
                    let cached = device_cache.get(&priority_id);
                    let recovered_fp = cached
                        .and_then(|(_, _, fp)| fp.clone())
                        .or_else(|| backend.device_fingerprint(&priority_id));
                    let name = cached
                        .map(|(n, _, _)| n.clone())
                        .or_else(|| {
                            recovered_fp
                                .as_ref()
                                .and_then(|f| f.clean_original_name.clone())
                        })
                        .or_else(|| recovered_fp.as_ref().and_then(|f| f.hardware_desc.clone()))
                        .unwrap_or_else(|| "Priority Device".to_string());

                    missing_by_type.push(MissingDevice {
                        id: priority_id.clone(),
                        settings: DeviceSettings::new(name, device_type),
                        fingerprint: recovered_fp,
                    });
                }
            }
        }
    }

    // Also include any disconnected devices in temporary priorities not tracked in persistent_state.devices
    for device_type in [DeviceType::Output, DeviceType::Input] {
        for kind in [PriorityKind::Main, PriorityKind::Communication] {
            if let Some(temp_id) = temporary_priorities.get(device_type, kind)
                && !missing_by_type.iter().any(|m| m.id == *temp_id)
                && !is_device_active(backend, temp_id)
            {
                let cached = device_cache.get(temp_id);
                let recovered_fp = cached
                    .and_then(|(_, _, fp)| fp.clone())
                    .or_else(|| backend.device_fingerprint(temp_id));
                let name = cached
                    .map(|(n, _, _)| n.clone())
                    .or_else(|| {
                        recovered_fp
                            .as_ref()
                            .and_then(|f| f.clean_original_name.clone())
                    })
                    .or_else(|| recovered_fp.as_ref().and_then(|f| f.hardware_desc.clone()))
                    .unwrap_or_else(|| "Temporary Device".to_string());

                missing_by_type.push(MissingDevice {
                    id: temp_id.clone(),
                    settings: DeviceSettings::new(name, device_type),
                    fingerprint: recovered_fp,
                });
            }
        }
    }

    if missing_by_type.is_empty() {
        return MigrationResult {
            persistent_changed,
            migrated_device_names,
        };
    }

    for device_type in [DeviceType::Output, DeviceType::Input] {
        let mut missing_devices: Vec<MissingDevice> = missing_by_type
            .iter()
            .filter(|m| m.settings.device_type == device_type)
            .cloned()
            .collect();

        if missing_devices.is_empty() {
            continue;
        }

        let active_devices = match backend.devices(device_type) {
            Ok(devs) => devs,
            Err(e) => {
                log::warn!("Failed to query {device_type:?} devices for migration: {e:#}");
                continue;
            }
        };

        // Filter out active devices that are already registered under their current ID,
        // or already actively present in priority lists or temporary priorities.
        let mut candidates: Vec<ActiveCandidate> = active_devices
            .into_iter()
            .filter(|d| {
                !persistent_state.devices.contains_key(d.id())
                    && !persistent_state
                        .priority_list_of(device_type, PriorityKind::Main)
                        .contains(d.id())
                    && !persistent_state
                        .priority_list_of(device_type, PriorityKind::Communication)
                        .contains(d.id())
                    && temporary_priorities.get(device_type, PriorityKind::Main) != Some(d.id())
                    && temporary_priorities.get(device_type, PriorityKind::Communication)
                        != Some(d.id())
            })
            .map(|d| ActiveCandidate {
                id: d.id().clone(),
                name: d.name(),
                fingerprint: d.fingerprint(),
            })
            .collect();

        for tier in [
            MatchTier::ExactHardwareId,
            MatchTier::HardwareProfileAndInterface,
            MatchTier::InternalHardwareProfile,
            MatchTier::CustomToCustomName,
            MatchTier::NormalizedOriginalName,
            MatchTier::HeuristicFallback,
        ] {
            if missing_devices.is_empty() || candidates.is_empty() {
                break;
            }

            // Find mutually unambiguous matches for this tier
            loop {
                let mut matched_pair: Option<(usize, usize)> = None;

                for (m_idx, m) in missing_devices.iter().enumerate() {
                    let matching_candidates: Vec<usize> = candidates
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| matches_tier(m, c, tier))
                        .map(|(c_idx, _)| c_idx)
                        .collect();

                    if matching_candidates.len() == 1 {
                        let c_idx = matching_candidates[0];
                        let c = &candidates[c_idx];

                        // Ensure candidate matches ONLY this missing device
                        let candidate_matching_missing: Vec<usize> = missing_devices
                            .iter()
                            .enumerate()
                            .filter(|(_, other_m)| matches_tier(other_m, c, tier))
                            .map(|(other_m_idx, _)| other_m_idx)
                            .collect();

                        if candidate_matching_missing.len() == 1 {
                            matched_pair = Some((m_idx, c_idx));
                            break;
                        }
                    }
                }

                let Some((m_idx, c_idx)) = matched_pair else {
                    break;
                };

                let missing = missing_devices.remove(m_idx);
                let candidate = candidates.remove(c_idx);

                let old_device_id = missing.id;
                let new_device_id = candidate.id;
                let mut new_settings = missing.settings.clone();
                new_settings.name = candidate.name;
                if candidate.fingerprint.is_some() {
                    new_settings.fingerprint = candidate.fingerprint;
                }

                if persistent_state.devices.contains_key(&old_device_id) {
                    persistent_state.remove_device(&old_device_id);
                    persistent_state.insert_device(new_device_id.clone(), new_settings);
                    persistent_changed = true;
                }

                for kind in [PriorityKind::Main, PriorityKind::Communication] {
                    let priority_list = persistent_state.priority_list_of_mut(device_type, kind);
                    if let Some(pos) = priority_list.iter().position(|id| id == &old_device_id) {
                        priority_list[pos] = new_device_id.clone();
                        persistent_changed = true;
                    }
                }

                temporary_priorities.migrate_device_id(device_type, &old_device_id, &new_device_id);

                log::info!(
                    "Migrated {device_type} device '{}' from ID {old_device_id} to {new_device_id} via {tier}",
                    missing.settings.name
                );
                migrated_device_names.push(missing.settings.name.clone());
            }
        }

        for m in missing_devices {
            if persistent_state.devices.contains_key(&m.id) {
                log::info!(
                    "Tracked device '{}' ({}) is currently disconnected, keeping settings in case it returns",
                    m.settings.name,
                    m.id
                );
            }
        }
    }

    MigrationResult {
        persistent_changed,
        migrated_device_names,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::tests::{MockAudioBackend, MockDevice, make_device_settings};

    fn migrate_device_ids(
        backend: &impl AudioBackend,
        state: &mut PersistentState,
    ) -> MigrationResult {
        super::migrate_device_ids(
            backend,
            state,
            &mut TemporaryPriorities::default(),
            &std::collections::HashMap::new(),
        )
    }

    #[test]
    fn clean_name_component_strips_prefixes_and_clones() {
        assert_eq!(
            clean_name_component("2- Logitech Webcam C930e"),
            "Logitech Webcam C930e"
        );
        assert_eq!(clean_name_component("1 - Speakers"), "Speakers");
        assert_eq!(
            clean_name_component("Realtek(R) Audio (1)"),
            "Realtek(R) Audio"
        );
        assert_eq!(clean_name_component("  Buds2 Pro  "), "Buds2 Pro");
    }

    #[test]
    fn parse_mac_address_extracts_bluetooth_mac() {
        let iface = r"{1}.BTHENUM\{0000110B-0000-1000-8000-00805F9B34FB}_VID&00010075_PID&A013\A&1AF4467A&0&A0AC69494DD3_C00000000";
        assert_eq!(parse_mac_address(iface), Some("A0AC69494DD3".to_string()));

        let non_bth = r"{1}.USB\VID_046D&PID_0A66&MI_00\8&11E19EAD&0&0000";
        assert_eq!(parse_mac_address(non_bth), None);
    }

    #[test]
    fn parse_usb_vid_pid_and_interface_index() {
        let iface = r"{1}.USB\VID_046D&PID_0A66&MI_02\8&11E19EAD&0&0000";
        assert_eq!(parse_usb_vid_pid(iface), Some((0x046d, 0x0a66)));
        assert_eq!(parse_interface_index(iface), Some(2));
    }

    #[test]
    fn parse_usb_serial_distinguishes_real_serial_from_topology_hash() {
        let topology = r"{1}.USB\VID_046D&PID_0A66&MI_00\8&11E19EAD&0&0000";
        assert_eq!(parse_usb_serial(Some(DeviceBusType::Usb), topology), None);

        let real_serial = r"{1}.USB\VID_1234&PID_5678\0123456789ABCDEF";
        assert_eq!(
            parse_usb_serial(Some(DeviceBusType::Usb), real_serial),
            Some("0123456789ABCDEF".to_string())
        );
    }

    #[test]
    fn build_fingerprint_detects_custom_names_and_cleans_original() {
        let fp_default = build_fingerprint(
            Some("Speakers (Realtek(R) Audio)".to_string()),
            Some("Speakers".to_string()),
            Some("Realtek(R) Audio".to_string()),
            Some("HDAUDIO".to_string()),
            Some("{GUID}".to_string()),
            Some(1),
            Some(r"{1}.HDAUDIO\FUNC_01...".to_string()),
        );
        assert!(!fp_default.is_custom_name);
        assert_eq!(
            fp_default.clean_original_name.as_deref(),
            Some("Speakers (Realtek(R) Audio)")
        );

        let fp_custom = build_fingerprint(
            Some("My Custom Monitors".to_string()),
            Some("Speakers".to_string()),
            Some("Realtek(R) Audio".to_string()),
            Some("HDAUDIO".to_string()),
            Some("{GUID}".to_string()),
            Some(1),
            Some(r"{1}.HDAUDIO\FUNC_01...".to_string()),
        );
        assert!(fp_custom.is_custom_name);
        assert_eq!(
            fp_custom.clean_original_name.as_deref(),
            Some("Speakers (Realtek(R) Audio)")
        );
    }

    #[test]
    fn migrate_no_changes_when_devices_match() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("id1", "Speakers", true),
            MockDevice::new("id2", "Headphones", true),
        ]);
        let mut state = PersistentState::default();
        state.devices.insert(
            "id1".into(),
            make_device_settings("Speakers", DeviceType::Output),
        );
        state.devices.insert(
            "id2".into(),
            make_device_settings("Headphones", DeviceType::Output),
        );

        let result = migrate_device_ids(&backend, &mut state);
        assert!(!result.persistent_changed);
        assert!(state.devices.contains_key("id1"));
        assert!(state.devices.contains_key("id2"));
    }

    #[test]
    fn migrate_updates_name_and_fingerprint_when_device_renamed() {
        let fp = DeviceFingerprint {
            bus_type: Some(DeviceBusType::Usb),
            hardware_desc: Some("Headphones".to_string()),
            ..Default::default()
        };
        let dev = MockDevice::new("id1", "New Speaker Name", true).with_fingerprint(fp.clone());
        let backend = MockAudioBackend::new(vec![dev]);
        let mut state = PersistentState::default();
        state.devices.insert(
            "id1".into(),
            make_device_settings("Old Speaker Name", DeviceType::Output),
        );

        let result = migrate_device_ids(&backend, &mut state);
        assert!(result.persistent_changed);
        assert_eq!(state.devices["id1"].name, "New Speaker Name");
        assert_eq!(state.devices["id1"].fingerprint, Some(fp));
    }

    #[test]
    fn migrate_moves_device_when_id_changes() {
        let backend = MockAudioBackend::new(vec![MockDevice::new("id_new", "Speakers", true)]);
        let mut state = PersistentState::default();
        state.devices.insert(
            "id_old".into(),
            make_device_settings("Speakers", DeviceType::Output),
        );
        state.output.priority_list = vec!["id_old".into()];

        let result = migrate_device_ids(&backend, &mut state);
        assert!(result.persistent_changed);
        assert_eq!(result.migrated_device_names, vec!["Speakers"]);
        assert!(!state.devices.contains_key("id_old"));
        assert!(state.devices.contains_key("id_new"));
        assert_eq!(state.output.priority_list, vec!["id_new".to_string()]);
    }

    #[test]
    fn migrate_tier1_bluetooth_mac_match() {
        let mac = "A0AC69494DD3".to_string();
        let old_fp = DeviceFingerprint {
            bus_type: Some(DeviceBusType::Bluetooth),
            mac_address: Some(mac.clone()),
            form_factor: Some(3), // Headphones
            ..Default::default()
        };
        let new_fp = DeviceFingerprint {
            bus_type: Some(DeviceBusType::Bluetooth),
            mac_address: Some(mac),
            form_factor: Some(3),
            ..Default::default()
        };

        let new_dev = MockDevice::new("bth_new_id", "Buds2 Pro", true).with_fingerprint(new_fp);
        let backend = MockAudioBackend::new(vec![new_dev]);

        let mut state = PersistentState::default();
        let mut settings = make_device_settings("Old Buds2 Name", DeviceType::Output);
        settings.fingerprint = Some(old_fp);
        state.devices.insert("bth_old_id".into(), settings);
        state.output.priority_list = vec!["bth_old_id".into()];

        let result = migrate_device_ids(&backend, &mut state);
        assert!(result.persistent_changed);
        assert_eq!(result.migrated_device_names, vec!["Old Buds2 Name"]);
        assert!(!state.devices.contains_key("bth_old_id"));
        assert!(state.devices.contains_key("bth_new_id"));
        assert_eq!(state.output.priority_list, vec!["bth_new_id".to_string()]);
    }

    #[test]
    fn migrate_tier1_bluetooth_respects_form_factor_disambiguation() {
        let mac = "A0AC69494DD3".to_string();
        // Missing stereo headphones (form_factor = 3)
        let old_headphones_fp = DeviceFingerprint {
            bus_type: Some(DeviceBusType::Bluetooth),
            mac_address: Some(mac.clone()),
            form_factor: Some(3),
            ..Default::default()
        };
        // Active hands-free headset (form_factor = 5) on the same MAC
        let active_headset_fp = DeviceFingerprint {
            bus_type: Some(DeviceBusType::Bluetooth),
            mac_address: Some(mac),
            form_factor: Some(5),
            ..Default::default()
        };

        let new_dev = MockDevice::new("bth_hfp_id", "Hands-Free AG Audio", true)
            .with_fingerprint(active_headset_fp);
        let backend = MockAudioBackend::new(vec![new_dev]);

        let mut state = PersistentState::default();
        let mut settings = make_device_settings("Buds Stereo", DeviceType::Output);
        settings.fingerprint = Some(old_headphones_fp);
        state.devices.insert("bth_a2dp_old".into(), settings);

        let result = migrate_device_ids(&backend, &mut state);
        // Should NOT migrate stereo headphones onto hands-free headset!
        assert!(!result.persistent_changed);
        assert!(state.devices.contains_key("bth_a2dp_old"));
        assert!(!state.devices.contains_key("bth_hfp_id"));
    }

    #[test]
    fn migrate_tier2_usb_vid_pid_and_interface_match() {
        let old_fp = DeviceFingerprint {
            bus_type: Some(DeviceBusType::Usb),
            usb_vid_pid: Some((0x046d, 0x0a66)),
            interface_index: Some(0), // Game output
            hardware_desc: Some("Logitech G533".to_string()),
            form_factor: Some(1),
            ..Default::default()
        };
        let new_fp = DeviceFingerprint {
            bus_type: Some(DeviceBusType::Usb),
            usb_vid_pid: Some((0x046d, 0x0a66)),
            interface_index: Some(0), // Game output
            hardware_desc: Some("Logitech G533".to_string()),
            form_factor: Some(1),
            ..Default::default()
        };

        let new_dev = MockDevice::new("usb_new_port_id", "Logitech G533 Gaming Headset", true)
            .with_fingerprint(new_fp);
        let backend = MockAudioBackend::new(vec![new_dev]);

        let mut state = PersistentState::default();
        let mut settings = make_device_settings("Logitech G533", DeviceType::Output);
        settings.fingerprint = Some(old_fp);
        state.devices.insert("usb_old_port_id".into(), settings);

        let result = migrate_device_ids(&backend, &mut state);
        assert!(result.persistent_changed);
        assert_eq!(result.migrated_device_names, vec!["Logitech G533"]);
        assert!(!state.devices.contains_key("usb_old_port_id"));
        assert!(state.devices.contains_key("usb_new_port_id"));
    }

    #[test]
    fn migrate_tier5_normalized_original_name_with_port_prefix() {
        let old_fp = DeviceFingerprint {
            bus_type: Some(DeviceBusType::Usb),
            hardware_desc: Some("Logitech C930e".to_string()),
            interface_name: Some("Logitech Webcam C930e".to_string()),
            clean_original_name: Some("Logitech C930e (Logitech Webcam C930e)".to_string()),
            form_factor: Some(4),
            ..Default::default()
        };
        // On new port, Windows injected "2- " into interface name, but clean_original_name is normalized
        let new_fp = DeviceFingerprint {
            bus_type: Some(DeviceBusType::Usb),
            hardware_desc: Some("Logitech C930e".to_string()),
            interface_name: Some("2- Logitech Webcam C930e".to_string()),
            clean_original_name: Some("Logitech C930e (Logitech Webcam C930e)".to_string()),
            form_factor: Some(4),
            ..Default::default()
        };

        let mut new_dev = MockDevice::new(
            "mic_port2",
            "Logitech C930e (2- Logitech Webcam C930e)",
            true,
        )
        .with_fingerprint(new_fp);
        new_dev.device_type = DeviceType::Input;
        let backend = MockAudioBackend::new(vec![new_dev]);

        let mut state = PersistentState::default();
        let mut settings = make_device_settings("Logitech C930e", DeviceType::Input);
        settings.fingerprint = Some(old_fp);
        state.devices.insert("mic_port1".into(), settings);
        state.input.priority_list = vec!["mic_port1".into()];

        let result = migrate_device_ids(&backend, &mut state);
        assert!(result.persistent_changed);
        assert_eq!(result.migrated_device_names, vec!["Logitech C930e"]);
        assert!(!state.devices.contains_key("mic_port1"));
        assert!(state.devices.contains_key("mic_port2"));
        assert_eq!(state.input.priority_list, vec!["mic_port2".to_string()]);
    }

    #[test]
    fn anti_cross_matching_prevents_custom_matching_original_name() {
        // User custom-renamed internal jack to "Speakers"
        let old_custom_fp = DeviceFingerprint {
            bus_type: Some(DeviceBusType::HdAudio),
            is_custom_name: true,
            clean_original_name: Some("Headphones (Realtek Audio)".to_string()),
            ..Default::default()
        };
        // New USB Soundbar with default original name "Speakers"
        let new_soundbar_fp = DeviceFingerprint {
            bus_type: Some(DeviceBusType::Usb),
            is_custom_name: false,
            clean_original_name: Some("Speakers (USB Audio Device)".to_string()),
            ..Default::default()
        };

        let new_dev = MockDevice::new("usb_soundbar", "Speakers (USB Audio Device)", true)
            .with_fingerprint(new_soundbar_fp);
        let backend = MockAudioBackend::new(vec![new_dev]);

        let mut state = PersistentState::default();
        let mut settings = make_device_settings("Speakers", DeviceType::Output);
        settings.fingerprint = Some(old_custom_fp);
        state.devices.insert("internal_jack".into(), settings);

        let result = migrate_device_ids(&backend, &mut state);
        assert!(!result.persistent_changed);
        assert!(state.devices.contains_key("internal_jack"));
        assert!(!state.devices.contains_key("usb_soundbar"));
    }

    #[test]
    fn collision_prevention_rejects_identical_ambiguous_devices() {
        let old_mic_fp = DeviceFingerprint {
            bus_type: Some(DeviceBusType::Usb),
            usb_vid_pid: Some((0x1234, 0x5678)),
            hardware_desc: Some("Generic Mic".to_string()),
            clean_original_name: Some("Generic Mic".to_string()),
            ..Default::default()
        };

        // Two identical active mics on system
        let mut mic1 = MockDevice::new("mic_active_1", "Generic Mic", true)
            .with_fingerprint(old_mic_fp.clone());
        mic1.device_type = DeviceType::Input;
        let mut mic2 = MockDevice::new("mic_active_2", "Generic Mic", true)
            .with_fingerprint(old_mic_fp.clone());
        mic2.device_type = DeviceType::Input;

        let backend = MockAudioBackend::new(vec![mic1, mic2]);

        let mut state = PersistentState::default();
        let mut settings = make_device_settings("Generic Mic", DeviceType::Input);
        settings.fingerprint = Some(old_mic_fp);
        state.devices.insert("mic_old_id".into(), settings);

        let result = migrate_device_ids(&backend, &mut state);
        // Ambiguous match (1 missing vs 2 candidates) must be rejected
        assert!(!result.persistent_changed);
        assert!(state.devices.contains_key("mic_old_id"));
    }

    #[test]
    fn on_the_fly_recovery_of_disconnected_device_fingerprint() {
        let backend = MockAudioBackend::new(vec![
            MockDevice::new("new_bth_id", "Galaxy Buds", true).with_fingerprint(
                DeviceFingerprint {
                    bus_type: Some(DeviceBusType::Bluetooth),
                    mac_address: Some("112233445566".to_string()),
                    form_factor: Some(3),
                    ..Default::default()
                },
            ),
        ]);

        // Old device in state has NO fingerprint (legacy config), but backend can recover it
        backend.disconnected_fingerprints.borrow_mut().insert(
            "old_bth_id".into(),
            DeviceFingerprint {
                bus_type: Some(DeviceBusType::Bluetooth),
                mac_address: Some("112233445566".to_string()),
                form_factor: Some(3),
                ..Default::default()
            },
        );

        let mut state = PersistentState::default();
        let settings = make_device_settings("Galaxy Buds", DeviceType::Output);
        state.devices.insert("old_bth_id".into(), settings);

        let result = migrate_device_ids(&backend, &mut state);
        assert!(result.persistent_changed);
        assert_eq!(result.migrated_device_names, vec!["Galaxy Buds"]);
        assert!(!state.devices.contains_key("old_bth_id"));
        assert!(state.devices.contains_key("new_bth_id"));
    }

    #[test]
    fn migrate_keeps_missing_device_when_no_match() {
        let backend = MockAudioBackend::new(vec![MockDevice::new("id_other", "Microphone", true)]);
        let mut state = PersistentState::default();
        state.devices.insert(
            "id_gone".into(),
            make_device_settings("Speakers", DeviceType::Output),
        );

        let result = migrate_device_ids(&backend, &mut state);
        assert!(!result.persistent_changed);
        assert!(state.devices.contains_key("id_gone"));
    }

    #[test]
    fn migrate_updates_input_priority_list() {
        let mut mic = MockDevice::new("mic_new", "Microphone", true);
        mic.device_type = DeviceType::Input;
        let backend = MockAudioBackend::new(vec![mic]);
        let mut state = PersistentState::default();
        state.devices.insert(
            "mic_old".into(),
            make_device_settings("Microphone", DeviceType::Input),
        );
        state.input.priority_list = vec!["mic_old".into()];

        let result = migrate_device_ids(&backend, &mut state);
        assert!(result.persistent_changed);
        assert!(state.devices.contains_key("mic_new"));
        assert_eq!(state.input.priority_list, vec!["mic_new"]);
    }

    #[test]
    fn migrate_updates_communication_priority_list() {
        let backend = MockAudioBackend::new(vec![MockDevice::new("id_new", "Headset", true)]);
        let mut state = PersistentState::default();
        state.devices.insert(
            "id_old".into(),
            make_device_settings("Headset", DeviceType::Output),
        );
        state.output.priority_list = vec!["id_old".into()];
        state.output.communication_priority_list = vec!["id_old".into()];

        let result = migrate_device_ids(&backend, &mut state);
        assert!(result.persistent_changed);
        assert_eq!(state.output.priority_list, vec!["id_new".to_string()]);
        assert_eq!(
            state.output.communication_priority_list,
            vec!["id_new".to_string()]
        );
    }

    #[test]
    fn migrate_updates_temporary_priorities() {
        let backend = MockAudioBackend::new(vec![MockDevice::new("id_new", "Headset", true)]);
        let mut state = PersistentState::default();
        state.devices.insert(
            "id_old".into(),
            make_device_settings("Headset", DeviceType::Output),
        );
        let mut temp = TemporaryPriorities::default();
        temp.set(
            DeviceType::Output,
            PriorityKind::Main,
            Some("id_old".into()),
        );
        temp.set(
            DeviceType::Output,
            PriorityKind::Communication,
            Some("id_old".into()),
        );

        let result = super::migrate_device_ids(
            &backend,
            &mut state,
            &mut temp,
            &std::collections::HashMap::new(),
        );
        assert!(result.persistent_changed);
        assert_eq!(
            temp.get(DeviceType::Output, PriorityKind::Main),
            Some(&DeviceId::from("id_new"))
        );
        assert_eq!(
            temp.get(DeviceType::Output, PriorityKind::Communication),
            Some(&DeviceId::from("id_new"))
        );
    }

    #[test]
    fn migrate_temporary_priority_when_not_in_persistent_devices() {
        let fp = DeviceFingerprint {
            bus_type: Some(DeviceBusType::Usb),
            usb_vid_pid: Some((0x046d, 0x0a66)),
            interface_index: Some(0),
            hardware_desc: Some("Logitech G533".to_string()),
            ..Default::default()
        };
        let new_dev =
            MockDevice::new("g533_new_port", "Logitech G533", true).with_fingerprint(fp.clone());
        let backend = MockAudioBackend::new(vec![new_dev]);
        backend
            .disconnected_fingerprints
            .borrow_mut()
            .insert("g533_old_port".into(), fp);

        let mut state = PersistentState::default();
        assert!(state.devices.is_empty());

        let mut temp = TemporaryPriorities::default();
        temp.set(
            DeviceType::Output,
            PriorityKind::Main,
            Some("g533_old_port".into()),
        );

        let result = super::migrate_device_ids(
            &backend,
            &mut state,
            &mut temp,
            &std::collections::HashMap::new(),
        );
        assert!(!result.persistent_changed);
        assert!(result.has_migrations());
        assert_eq!(
            temp.get(DeviceType::Output, PriorityKind::Main),
            Some(&DeviceId::from("g533_new_port"))
        );
        assert!(state.devices.is_empty());
    }

    #[test]
    fn test_live_g533_exact_strings() {
        let old_fp = build_fingerprint(
            Some("Speakers (5- Logitech G533 Gaming Headset)".to_string()),
            Some("Speakers".to_string()),
            Some("5- Logitech G533 Gaming Headset".to_string()),
            Some("USB".to_string()),
            Some("{B34D6196-D262-11F1-AFAE-64BC581DA843}".to_string()),
            Some(1),
            Some(r"{1}.USB\VID_046D&PID_0A66&MI_00\9&247E1D72&0&0000".to_string()),
        );
        let new_fp = build_fingerprint(
            Some("Speakers (2- Logitech G533 Gaming Headset)".to_string()),
            Some("Speakers".to_string()),
            Some("2- Logitech G533 Gaming Headset".to_string()),
            Some("USB".to_string()),
            Some("{55C790CC-D25A-11F1-AFAE-64BC581DA843}".to_string()),
            Some(1),
            Some(r"{1}.USB\VID_046D&PID_0A66&MI_00\9&279C0F32&0&0000".to_string()),
        );

        println!("OLD FP: {old_fp:#?}");
        println!("NEW FP: {new_fp:#?}");

        let old_inactive_dev = MockDevice::new(
            "{0.0.0.00000000}.{ce699537-6ac9-4b42-a0f2-dd7965a73d4b}",
            "Speakers (5- Logitech G533 Gaming Headset)",
            false,
        )
        .with_fingerprint(old_fp);

        let new_dev = MockDevice::new(
            "{0.0.0.00000000}.{06caeba9-45a3-4c97-a1d3-25fb7b2871fe}",
            "Speakers (2- Logitech G533 Gaming Headset)",
            true,
        )
        .with_fingerprint(new_fp);
        let backend = MockAudioBackend::new(vec![old_inactive_dev, new_dev]);

        let mut state = PersistentState::default();
        let mut temp = TemporaryPriorities::default();
        temp.set(
            DeviceType::Output,
            PriorityKind::Main,
            Some("{0.0.0.00000000}.{ce699537-6ac9-4b42-a0f2-dd7965a73d4b}".into()),
        );

        let result = super::migrate_device_ids(
            &backend,
            &mut state,
            &mut temp,
            &std::collections::HashMap::new(),
        );
        assert!(result.has_migrations(), "Migration should have succeeded!");
    }

    #[test]
    fn test_live_bluetooth_unpair_re_pair_migration() {
        let mac = "A0AC69494DD3".to_string();
        let old_id = DeviceId::from("{0.0.0.00000000}.{e801d086-a873-4852-9c6b-66d7df7be3d0}");
        let new_id = DeviceId::from("{0.0.0.00000000}.{8e89cde4-58c0-432c-a776-bce5eff55f2b}");

        let old_fp = DeviceFingerprint {
            bus_type: Some(DeviceBusType::Bluetooth),
            mac_address: Some(mac.clone()),
            form_factor: Some(3),
            hardware_desc: Some("Headphones".to_string()),
            clean_original_name: Some("Headphones (Buds2 Pro de Felipe)".to_string()),
            ..Default::default()
        };

        let new_fp = DeviceFingerprint {
            bus_type: Some(DeviceBusType::Bluetooth),
            mac_address: Some(mac),
            form_factor: Some(3),
            hardware_desc: Some("Headphones".to_string()),
            clean_original_name: Some("Headphones (Buds2 Pro de Felipe)".to_string()),
            ..Default::default()
        };

        // When unpaired, the old device is completely DELETED from Windows (not in backend.devices)
        let new_dev = MockDevice::new(&new_id, "Headphones (Buds2 Pro de Felipe)", true)
            .with_fingerprint(new_fp);
        let backend = MockAudioBackend::new(vec![new_dev]);

        // device_cache remembered the device while it was active
        let mut cache = std::collections::HashMap::new();
        cache.insert(
            old_id.clone(),
            (
                "Headphones (Buds2 Pro de Felipe)".to_string(),
                DeviceType::Output,
                Some(old_fp),
            ),
        );

        let mut state = PersistentState::default();
        let mut temp = TemporaryPriorities::default();
        temp.set(DeviceType::Output, PriorityKind::Main, Some(old_id));

        let result = super::migrate_device_ids(&backend, &mut state, &mut temp, &cache);
        assert!(
            result.has_migrations(),
            "Bluetooth migration via cached fingerprint should succeed!"
        );
        assert_eq!(
            temp.get(DeviceType::Output, PriorityKind::Main),
            Some(&new_id)
        );
    }

    #[test]
    fn tier6_rejects_cross_bus_false_match() {
        let old_hd_fp = DeviceFingerprint {
            bus_type: Some(DeviceBusType::HdAudio),
            clean_original_name: Some("Speakers (Realtek Audio)".to_string()),
            ..Default::default()
        };
        let new_usb_fp = DeviceFingerprint {
            bus_type: Some(DeviceBusType::Usb),
            clean_original_name: Some("Speakers (USB Audio Device)".to_string()),
            ..Default::default()
        };

        let new_dev =
            MockDevice::new("usb_speakers", "Speakers", true).with_fingerprint(new_usb_fp);
        let backend = MockAudioBackend::new(vec![new_dev]);

        let mut state = PersistentState::default();
        let mut settings = make_device_settings("Speakers", DeviceType::Output);
        settings.fingerprint = Some(old_hd_fp);
        state
            .devices
            .insert("internal_hd_speakers".into(), settings);

        let result = migrate_device_ids(&backend, &mut state);
        assert!(!result.persistent_changed);
        assert!(state.devices.contains_key("internal_hd_speakers"));
        assert!(!state.devices.contains_key("usb_speakers"));
    }

    #[test]
    fn tier6_rejects_conflicting_vid_pid() {
        let old_usb_fp = DeviceFingerprint {
            bus_type: Some(DeviceBusType::Usb),
            usb_vid_pid: Some((0x1234, 0x5678)),
            clean_original_name: Some("USB Audio Device".to_string()),
            ..Default::default()
        };
        let new_usb_fp = DeviceFingerprint {
            bus_type: Some(DeviceBusType::Usb),
            usb_vid_pid: Some((0x9999, 0x8888)),
            clean_original_name: Some("USB Audio Device".to_string()),
            ..Default::default()
        };

        let new_dev =
            MockDevice::new("usb_dev2", "USB Audio Device", true).with_fingerprint(new_usb_fp);
        let backend = MockAudioBackend::new(vec![new_dev]);

        let mut state = PersistentState::default();
        let mut settings = make_device_settings("USB Audio Device", DeviceType::Output);
        settings.fingerprint = Some(old_usb_fp);
        state.devices.insert("usb_dev1".into(), settings);

        let result = migrate_device_ids(&backend, &mut state);
        assert!(!result.persistent_changed);
        assert!(state.devices.contains_key("usb_dev1"));
        assert!(!state.devices.contains_key("usb_dev2"));
    }

    #[test]
    fn migration_does_not_claim_active_priority_or_temporary_devices() {
        // "active_priority_dev" is currently active and in output priority list,
        // but has no locks so it is not in state.devices.
        let active_dev = MockDevice::new("active_priority_dev", "Headset", true);
        let backend = MockAudioBackend::new(vec![active_dev]);

        let mut state = PersistentState::default();
        state.devices.insert(
            "old_disconnected_headset".into(),
            make_device_settings("Headset", DeviceType::Output),
        );
        state.output.priority_list = vec!["active_priority_dev".into()];

        let mut temp = TemporaryPriorities::default();
        let result = super::migrate_device_ids(
            &backend,
            &mut state,
            &mut temp,
            &std::collections::HashMap::new(),
        );

        assert!(!result.persistent_changed);
        assert!(state.devices.contains_key("old_disconnected_headset"));
        assert!(!state.devices.contains_key("active_priority_dev"));
    }
}
