use super::{
    HIDPP_DIRECT_DEVICE_INDEX, HIDPP_LONG_REPORT_ID, HIDPP_LONG_REPORT_LENGTH, HeadsetPresence,
    HidPresenceProfile, InitialQuery, decode_adc_flags,
};

pub const A50X_PROFILE: HidPresenceProfile = HidPresenceProfile {
    model: "a50x",
    vendor_id: 0x046d,
    product_id: 0x0b0b,
    usage_page: 0xff32,
    usage: 0x0074,
    report_length: 64,
    decode: |report, _| decode_a50x_report(report),
    initial_query: None,
};

pub const G533_PROFILE: HidPresenceProfile = HidPresenceProfile {
    model: "g533",
    vendor_id: 0x046d,
    product_id: 0x0a66,
    usage_page: 0xff43,
    usage: 0x0202,
    report_length: HIDPP_LONG_REPORT_LENGTH,
    decode: decode_adc_measurement_report,
    initial_query: Some(InitialQuery::HidppAdcMeasurement {
        feature_id: 0x1f20,
        unavailable_error: 0x05,
    }),
};

pub const G733_PROFILE: HidPresenceProfile = HidPresenceProfile {
    model: "g733",
    vendor_id: 0x046d,
    product_id: 0x0ab5,
    usage_page: 0xff43,
    usage: 0x0202,
    report_length: HIDPP_LONG_REPORT_LENGTH,
    decode: decode_adc_measurement_report,
    initial_query: Some(InitialQuery::HidppAdcMeasurement {
        feature_id: 0x1f20,
        unavailable_error: 0x05,
    }),
};

pub const PROFILES: &[HidPresenceProfile] = &[A50X_PROFILE, G533_PROFILE, G733_PROFILE];

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

/// Firmware assigns the feature index, so these events can only be read once a
/// startup query has discovered it.
fn decode_adc_measurement_report(
    report: &[u8],
    feature_index: Option<u8>,
) -> Option<HeadsetPresence> {
    let feature_index = feature_index?;
    (report.len() == HIDPP_LONG_REPORT_LENGTH
        && report[..4]
            == [
                HIDPP_LONG_REPORT_ID,
                HIDPP_DIRECT_DEVICE_INDEX,
                feature_index,
                0x00,
            ])
    .then(|| decode_adc_flags(report[6]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_requires_the_complete_vendor_interface_identity() {
        assert!(A50X_PROFILE.matches(0x046d, 0x0b0b, 0xff32, 0x0074));
        for identity in [
            (0x1234, 0x0b0b, 0xff32, 0x0074),
            (0x046d, 0x9999, 0xff32, 0x0074),
            (0x046d, 0x0b0b, 0xff00, 0x0074),
            (0x046d, 0x0b0b, 0xff32, 0x0001),
        ] {
            assert!(!A50X_PROFILE.matches(identity.0, identity.1, identity.2, identity.3));
        }
    }

    #[test]
    fn g533_profile_requires_its_vendor_status_collection() {
        assert!(G533_PROFILE.matches(0x046d, 0x0a66, 0xff43, 0x0202));
        assert!(!G533_PROFILE.matches(0x046d, 0x0a66, 0xff00, 0x0001));
        assert!(!G533_PROFILE.matches(0x046d, 0x0a66, 0xff43, 0x0201));
    }

    #[test]
    fn profiles_have_unique_model_and_usb_selectors() {
        for (index, profile) in PROFILES.iter().enumerate() {
            assert!(
                PROFILES[..index]
                    .iter()
                    .all(|other| other.model != profile.model)
            );
            assert!(
                PROFILES[..index]
                    .iter()
                    .all(|other| (other.vendor_id, other.product_id)
                        != (profile.vendor_id, profile.product_id))
            );
        }
    }

    #[test]
    fn g533_link_events_decode_only_observed_adc_status_reports() {
        let report = |event, flags| {
            let mut report = [0; 20];
            report[..7].copy_from_slice(&[0x11, 0xff, 0x07, event, 0x0f, 0x14, flags]);
            report
        };
        assert_eq!(
            (G533_PROFILE.decode)(&report(0x00, 0x00), Some(7)),
            Some(HeadsetPresence::Unavailable)
        );
        for flags in [0x01, 0x03, 0x07] {
            assert_eq!(
                (G533_PROFILE.decode)(&report(0x00, flags), Some(7)),
                Some(HeadsetPresence::Available)
            );
        }
        assert_eq!(
            (G533_PROFILE.decode)(&report(0x00, 0x0f), Some(7)),
            Some(HeadsetPresence::Unknown)
        );
        assert_eq!((G533_PROFILE.decode)(&report(0x01, 0x00), Some(7)), None);
        let mut unrelated = report(0x00, 0);
        unrelated[2] = 0x06;
        assert_eq!((G533_PROFILE.decode)(&unrelated, Some(7)), None);
        for length in 0..20 {
            assert_eq!(
                (G533_PROFILE.decode)(&report(0x00, 0x01)[..length], Some(7)),
                None
            );
        }
    }

    #[test]
    fn status_events_need_a_discovered_feature_index() {
        let report = |index| {
            let mut report = [0; 20];
            report[..7].copy_from_slice(&[0x11, 0xff, index, 0x00, 0x0f, 0x14, 0x00]);
            report
        };
        assert_eq!(
            (G533_PROFILE.decode)(&report(9), Some(9)),
            Some(HeadsetPresence::Unavailable)
        );
        assert_eq!((G533_PROFILE.decode)(&report(7), Some(9)), None);
        assert_eq!((G533_PROFILE.decode)(&report(7), None), None);
    }

    #[test]
    fn g733_profile_matches_its_usb_and_hid_identity() {
        assert!(G733_PROFILE.matches(0x046d, 0x0ab5, 0xff43, 0x0202));
        let report = |flags| {
            let mut report = [0; 20];
            report[..7].copy_from_slice(&[0x11, 0xff, 0x08, 0x00, 0x0f, 0x41, flags]);
            report
        };
        assert_eq!(
            (G733_PROFILE.decode)(&report(0x00), Some(8)),
            Some(HeadsetPresence::Unavailable)
        );
        assert_eq!(
            (G733_PROFILE.decode)(&report(0x01), Some(8)),
            Some(HeadsetPresence::Available)
        );
        assert_eq!(
            (G733_PROFILE.decode)(&report(0x03), Some(8)),
            Some(HeadsetPresence::Available)
        );
        assert_eq!(
            (G733_PROFILE.decode)(&report(0x0f), Some(8)),
            Some(HeadsetPresence::Unknown)
        );
        assert_eq!((G733_PROFILE.decode)(&report(0x01)[..19], Some(8)), None);
        assert_eq!(
            (G733_PROFILE.decode)(&[0x11, 0xff, 0x07, 0x00, 0x0f, 0x41, 0x00], Some(8)),
            None
        );
    }

    #[test]
    fn padded_reports_work_and_every_truncated_prefix_is_ignored() {
        let mut report = [0; 64];
        report[..7].copy_from_slice(&[2, 12, 4, 0, 19, 0, 5]);
        assert_eq!(
            (A50X_PROFILE.decode)(&report, None),
            Some(HeadsetPresence::Unavailable)
        );
        for len in 0..7 {
            assert_eq!((A50X_PROFILE.decode)(&report[..len], None), None);
        }
    }

    #[test]
    fn captured_a50x_power_transitions_are_distinct() {
        assert_eq!(
            (A50X_PROFILE.decode)(&[2, 12, 4, 0, 19, 0, 0], None),
            Some(HeadsetPresence::Available)
        );
        assert_eq!(
            (A50X_PROFILE.decode)(&[2, 12, 4, 0, 19, 0, 5], None),
            Some(HeadsetPresence::Unavailable)
        );
    }

    #[test]
    fn unrelated_reports_and_timeouts_do_not_infer_power_off() {
        for report in [
            vec![],
            vec![2, 12, 4, 0, 19, 0],
            vec![2, 12, 4, 0, 10, 0, 6],
            vec![2, 12, 6, 0, 6, 0, 90, 90, 0],
        ] {
            assert_eq!((A50X_PROFILE.decode)(&report, None), None);
        }
        assert_eq!(
            (A50X_PROFILE.decode)(&[2, 12, 4, 0, 19, 0, 99], None),
            Some(HeadsetPresence::Unknown)
        );
    }
}
