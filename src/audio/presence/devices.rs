use super::{HeadsetPresence, HidPresenceProfile, InitialQuery};

pub const A50X_PROFILE: HidPresenceProfile = HidPresenceProfile {
    model: "a50x",
    name: "A50 X",
    vendor_id: 0x046d,
    product_id: 0x0b0b,
    usage_page: 0xff32,
    usage: 0x0074,
    report_length: 64,
    decode: decode_a50x_report,
    initial_query: None,
};

pub const G533_PROFILE: HidPresenceProfile = HidPresenceProfile {
    model: "g533",
    name: "Logitech G533",
    vendor_id: 0x046d,
    product_id: 0x0a66,
    usage_page: 0xff43,
    usage: 0x0202,
    report_length: 20,
    decode: decode_g533_report,
    initial_query: Some(InitialQuery::HidppAdcMeasurement {
        feature_id: 0x1f20,
        unavailable_error: Some(0x05),
    }),
};

pub const G733_PROFILE: HidPresenceProfile = HidPresenceProfile {
    model: "g733",
    name: "Logitech G733",
    vendor_id: 0x046d,
    product_id: 0x0ab5,
    usage_page: 0xff43,
    usage: 0x0202,
    report_length: 20,
    decode: decode_g733_report,
    initial_query: Some(InitialQuery::HidppAdcMeasurement {
        feature_id: 0x1f20,
        unavailable_error: Some(0x05),
    }),
};

pub const PROFILES: &[HidPresenceProfile] = &[A50X_PROFILE, G533_PROFILE, G733_PROFILE];

#[allow(dead_code)]
pub fn profile_by_model(model: &str) -> Option<&'static HidPresenceProfile> {
    PROFILES.iter().find(|profile| profile.model == model)
}

pub fn profile_by_usb_device(
    vendor_id: u16,
    product_id: u16,
) -> Option<&'static HidPresenceProfile> {
    PROFILES
        .iter()
        .find(|profile| profile.matches_usb_device(vendor_id, product_id))
}

/// Observed on A50 X 046d:0b0b, firmware 1.3.21, during a real on/off cycle.
/// Ignore chatmix (command 0x0a): its center value also appears during power-off.
fn decode_a50x_report(report: &[u8]) -> Option<HeadsetPresence> {
    if !report.starts_with(&[0x02, 0x0c, 0x04, 0x00, 0x13, 0x00]) {
        return None;
    }
    match report.get(6)? {
        0x00 => Some(HeadsetPresence::Available),
        0x05 => Some(HeadsetPresence::Unavailable),
        _ => Some(HeadsetPresence::Unknown),
    }
}

fn decode_g533_report(report: &[u8]) -> Option<HeadsetPresence> {
    super::decode_adc_measurement_report(report, 7)
}

fn decode_g733_report(report: &[u8]) -> Option<HeadsetPresence> {
    super::decode_adc_measurement_report(report, 8)
}
