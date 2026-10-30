//! Headset radio state is distinct from the USB audio endpoint's connection.
//! Unknown observations never override Windows' availability decision.

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HeadsetPresence {
    #[default]
    Unknown = 0,
    Available = 1,
    Unavailable = 2,
}

pub type HidReportCallback = dyn Fn(&[u8]) + Send + Sync;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitialQuery {
    HidppAdcMeasurement {
        feature_id: u16,
        unavailable_error: Option<u8>,
    },
}

/// One verified HID interface and its device-specific status interpretation.
#[derive(Clone, Copy)]
pub struct HidPresenceProfile {
    #[allow(dead_code)]
    pub model: &'static str,
    pub name: &'static str,
    pub vendor_id: u16,
    pub product_id: u16,
    pub usage_page: u16,
    pub usage: u16,
    pub report_length: usize,
    pub decode: fn(&[u8]) -> Option<HeadsetPresence>,
    pub initial_query: Option<InitialQuery>,
}

impl HidPresenceProfile {
    pub fn matches_usb_device(&self, vendor_id: u16, product_id: u16) -> bool {
        self.vendor_id == vendor_id && self.product_id == product_id
    }

    pub fn matches(&self, vendor_id: u16, product_id: u16, usage_page: u16, usage: u16) -> bool {
        self.matches_usb_device(vendor_id, product_id)
            && self.usage_page == usage_page
            && self.usage == usage
    }
}

#[path = "presence/devices.rs"]
pub mod devices;

/// G533 and G733 broadcast ADC status at different feature indexes.
fn decode_adc_measurement_report(report: &[u8], feature_index: u8) -> Option<HeadsetPresence> {
    if report.len() != 20 || report.get(..4)? != [0x11, 0xff, feature_index, 0x00] {
        return None;
    }

    Some(decode_adc_flags(*report.get(6)?))
}

fn decode_adc_flags(flags: u8) -> HeadsetPresence {
    match flags {
        flags if flags & 0x01 == 0 => HeadsetPresence::Unavailable,
        0x01 | 0x03 | 0x07 => HeadsetPresence::Available,
        _ => HeadsetPresence::Unknown,
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::{HeadsetPresence, HidPresenceProfile, HidReportCallback, InitialQuery};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
    };
    use std::thread::{self, JoinHandle};
    use std::time::{Duration, Instant};

    const HIDPP_LONG_REPORT_ID: u8 = 0x11;
    const HIDPP_LONG_REPORT_LENGTH: usize = 20;
    const HIDPP_DIRECT_DEVICE_INDEX: u8 = 0xff;
    const HIDPP_SOFTWARE_ID: u8 = 0x01;

    enum HidppReply {
        Response([u8; HIDPP_LONG_REPORT_LENGTH]),
        Error(u8),
    }

    struct HidppReader<'a> {
        device: &'a hidapi::HidDevice,
        profile: HidPresenceProfile,
        state: &'a AtomicU8,
        on_change: &'a Arc<dyn Fn() + Send + Sync>,
        on_report: &'a Arc<HidReportCallback>,
    }

    fn make_hidpp_request(
        device_index: u8,
        feature_index: u8,
        function_sw_id: u8,
        parameters: [u8; 3],
    ) -> [u8; HIDPP_LONG_REPORT_LENGTH] {
        let mut report = [0; HIDPP_LONG_REPORT_LENGTH];
        report[..7].copy_from_slice(&[
            HIDPP_LONG_REPORT_ID,
            device_index,
            feature_index,
            function_sw_id,
            parameters[0],
            parameters[1],
            parameters[2],
        ]);
        report
    }

    fn parse_hidpp_reply(
        bytes: &[u8],
        device_index: u8,
        feature_index: u8,
        function_sw_id: u8,
    ) -> Option<HidppReply> {
        if bytes.len() != HIDPP_LONG_REPORT_LENGTH
            || bytes[0] != HIDPP_LONG_REPORT_ID
            || bytes[1] != device_index
        {
            return None;
        }
        if bytes[2] == feature_index && bytes[3] == function_sw_id {
            let mut response = [0; HIDPP_LONG_REPORT_LENGTH];
            response.copy_from_slice(bytes);
            return Some(HidppReply::Response(response));
        }
        if bytes[2] == 0xff && bytes[3] == feature_index && bytes[4] == function_sw_id {
            return Some(HidppReply::Error(bytes[5]));
        }
        None
    }

    impl HidppReader<'_> {
        fn exchange(
            &self,
            device_index: u8,
            feature_index: u8,
            function_sw_id: u8,
            parameters: [u8; 3],
        ) -> anyhow::Result<HidppReply> {
            let request =
                make_hidpp_request(device_index, feature_index, function_sw_id, parameters);
            let written = self.device.write(&request)?;
            anyhow::ensure!(
                written == request.len(),
                "HID++ request was only partially written ({written}/{} bytes)",
                request.len()
            );

            let deadline = Instant::now() + Duration::from_secs(2);
            let mut report = [0; 64];
            while Instant::now() < deadline {
                let count = self.device.read_timeout(&mut report, 500)?;
                if count == 0 {
                    continue;
                }
                let Some(bytes) = report.get(..count) else {
                    anyhow::bail!("HID++ response exceeds the read buffer");
                };
                (self.on_report)(bytes);
                if let Some(next) = (self.profile.decode)(bytes) {
                    publish(self.state, next, self.on_change);
                }
                if let Some(reply) =
                    parse_hidpp_reply(bytes, device_index, feature_index, function_sw_id)
                {
                    return Ok(reply);
                }
            }
            anyhow::bail!(
                "Timed out waiting for HID++ reply to feature {feature_index:#04x}, function {function_sw_id:#04x}"
            )
        }
    }

    fn query_initial_state(
        profile: HidPresenceProfile,
        query: InitialQuery,
        reader: &HidppReader<'_>,
    ) -> anyhow::Result<HeadsetPresence> {
        let InitialQuery::HidppAdcMeasurement {
            feature_id,
            unavailable_error,
        } = query;
        let feature_lookup = match reader.exchange(
            HIDPP_DIRECT_DEVICE_INDEX,
            0,
            HIDPP_SOFTWARE_ID,
            [(feature_id >> 8) as u8, feature_id as u8, 0],
        )? {
            HidppReply::Response(report) => report,
            HidppReply::Error(code) => {
                anyhow::bail!(
                    "{} HID++ feature lookup failed with error {code:#04x}",
                    profile.name
                )
            }
        };
        let feature_index = feature_lookup[4];
        anyhow::ensure!(
            feature_index != 0,
            "{} does not expose ADC feature {feature_id:#06x}",
            profile.name
        );

        let adc_reply = reader.exchange(
            HIDPP_DIRECT_DEVICE_INDEX,
            feature_index,
            HIDPP_SOFTWARE_ID,
            [0; 3],
        )?;
        decode_adc_reply(profile, unavailable_error, adc_reply)
    }

    fn decode_adc_reply(
        profile: HidPresenceProfile,
        unavailable_error: Option<u8>,
        reply: HidppReply,
    ) -> anyhow::Result<HeadsetPresence> {
        match reply {
            HidppReply::Response(report) => Ok(super::decode_adc_flags(report[6])),
            HidppReply::Error(code) if unavailable_error == Some(code) => {
                Ok(HeadsetPresence::Unavailable)
            }
            HidppReply::Error(code) => anyhow::bail!(
                "{} HID++ ADC measurement failed with error {code:#04x}",
                profile.name
            ),
        }
    }

    pub struct HidPresenceMonitor {
        stop: Arc<AtomicBool>,
        state: Arc<AtomicU8>,
        worker: Option<JoinHandle<()>>,
    }

    impl HidPresenceMonitor {
        pub fn start(
            profile: HidPresenceProfile,
            on_change: Arc<dyn Fn() + Send + Sync>,
            on_report: Arc<HidReportCallback>,
        ) -> std::io::Result<Self> {
            if !(1..=4096).contains(&profile.report_length) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "HID report length must be 1–4096 bytes",
                ));
            }
            let stop = Arc::new(AtomicBool::new(false));
            let state = Arc::new(AtomicU8::new(HeadsetPresence::Unknown as u8));
            let worker_stop = Arc::clone(&stop);
            let worker_state = Arc::clone(&state);
            let worker = thread::Builder::new()
                .name("headset-presence".into())
                .spawn(move || {
                    let mut last_error = None;
                    while !worker_stop.load(Ordering::Relaxed) {
                        let result =
                            listen(profile, &worker_stop, &worker_state, &on_change, &on_report);
                        // Losing the provider is not proof that the headset turned off.
                        publish(&worker_state, HeadsetPresence::Unknown, &on_change);
                        if let Err(error) = result {
                            let message = error.to_string();
                            if last_error.as_ref() != Some(&message) {
                                log::warn!("{} presence unavailable: {message}", profile.name);
                                last_error = Some(message);
                            }
                        }
                        for _ in 0..10 {
                            if worker_stop.load(Ordering::Relaxed) {
                                break;
                            }
                            thread::sleep(Duration::from_millis(200));
                        }
                    }
                })?;
            Ok(Self {
                stop,
                state,
                worker: Some(worker),
            })
        }

        pub fn presence(&self) -> HeadsetPresence {
            match self.state.load(Ordering::Acquire) {
                1 => HeadsetPresence::Available,
                2 => HeadsetPresence::Unavailable,
                _ => HeadsetPresence::Unknown,
            }
        }
    }

    impl Drop for HidPresenceMonitor {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }

    fn publish(state: &AtomicU8, next: HeadsetPresence, on_change: &Arc<dyn Fn() + Send + Sync>) {
        if state.swap(next as u8, Ordering::AcqRel) != next as u8 {
            log::info!("Headset radio presence: {next:?}");
            on_change();
        }
    }

    fn listen(
        profile: HidPresenceProfile,
        stop: &AtomicBool,
        state: &AtomicU8,
        on_change: &Arc<dyn Fn() + Send + Sync>,
        on_report: &Arc<HidReportCallback>,
    ) -> anyhow::Result<()> {
        let api = hidapi::HidApi::new()?;
        let mut candidates = api.device_list().filter(|device| {
            profile.matches(
                device.vendor_id(),
                device.product_id(),
                device.usage_page(),
                device.usage(),
            )
        });
        let candidate = candidates
            .next()
            .ok_or_else(|| anyhow::anyhow!("{} vendor HID interface not found", profile.name))?;
        anyhow::ensure!(
            candidates.next().is_none(),
            "Multiple {} HID interfaces; automatic association is ambiguous",
            profile.name
        );
        let device = candidate.open_device(&api)?;
        if let Some(query) = profile.initial_query {
            let reader = HidppReader {
                device: &device,
                profile,
                state,
                on_change,
                on_report,
            };
            match query_initial_state(profile, query, &reader) {
                Ok(initial_state) => publish(state, initial_state, on_change),
                Err(error) => {
                    log::warn!("Cannot query initial {} state: {error:#}", profile.name);
                }
            }
        }
        log::info!("Listening for {} status events", profile.name,);
        // After the one-time startup query, monitoring only reads input reports.
        observe_reports(
            profile,
            |report| device.read_timeout(report, 500).map_err(Into::into),
            stop,
            state,
            on_change,
            on_report.as_ref(),
        )
    }

    // The transport only exposes reads. Shared by every protocol adapter and
    // exercised with synthetic reports, timeouts and failures without hardware.
    fn observe_reports(
        profile: HidPresenceProfile,
        mut read: impl FnMut(&mut [u8]) -> anyhow::Result<usize>,
        stop: &AtomicBool,
        state: &AtomicU8,
        on_change: &Arc<dyn Fn() + Send + Sync>,
        mut on_report: impl FnMut(&[u8]),
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            (1..=4096).contains(&profile.report_length),
            "Invalid HID report length"
        );
        let mut report = vec![0_u8; profile.report_length];
        while !stop.load(Ordering::Relaxed) {
            let count = match read(&mut report) {
                Ok(count) => count,
                Err(error) => {
                    publish(state, HeadsetPresence::Unknown, on_change);
                    return Err(error);
                }
            };
            if count == 0 {
                continue;
            }
            let Some(bytes) = report.get(..count) else {
                publish(state, HeadsetPresence::Unknown, on_change);
                anyhow::bail!("HID input report exceeds read buffer");
            };
            on_report(bytes);
            if let Some(next) = (profile.decode)(bytes) {
                publish(state, next, on_change);
            }
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::super::devices::{A50X_PROFILE, G533_PROFILE, G733_PROFILE};
        use super::*;
        use std::sync::atomic::AtomicUsize;

        #[test]
        fn timeouts_are_never_decoded_as_device_reports() {
            let profile = HidPresenceProfile {
                decode: |_| panic!("a timeout is not a report"),
                ..A50X_PROFILE
            };
            let stop = AtomicBool::new(false);
            let state = AtomicU8::new(HeadsetPresence::Unavailable as u8);
            let callback: Arc<dyn Fn() + Send + Sync> =
                Arc::new(|| panic!("timeout changed presence"));
            observe_reports(
                profile,
                |_| {
                    stop.store(true, Ordering::Relaxed);
                    Ok(0)
                },
                &stop,
                &state,
                &callback,
                |_| {},
            )
            .unwrap();
            assert_eq!(
                state.load(Ordering::Acquire),
                HeadsetPresence::Unavailable as u8
            );
        }

        #[test]
        fn invalid_report_lengths_are_rejected_before_starting_a_worker() {
            for report_length in [0, 4097] {
                let profile = HidPresenceProfile {
                    report_length,
                    ..A50X_PROFILE
                };
                assert!(
                    HidPresenceMonitor::start(profile, Arc::new(|| {}), Arc::new(|_| {})).is_err()
                );
            }
        }

        #[test]
        fn timeouts_preserve_state_and_provider_failure_resets_to_unknown() {
            let stop = AtomicBool::new(false);
            let state = AtomicU8::new(HeadsetPresence::Unknown as u8);
            let changes = Arc::new(AtomicUsize::new(0));
            let observed = Arc::clone(&changes);
            let callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                observed.fetch_add(1, Ordering::Relaxed);
            });
            let mut step = 0;
            let result = observe_reports(
                A50X_PROFILE,
                |buffer| {
                    step += 1;
                    match step {
                        1 | 2 => {
                            buffer[..7].copy_from_slice(&[2, 12, 4, 0, 19, 0, 5]);
                            Ok(7)
                        }
                        3 => {
                            assert_eq!(
                                state.load(Ordering::Acquire),
                                HeadsetPresence::Unavailable as u8
                            );
                            Ok(0)
                        }
                        _ => {
                            assert_eq!(
                                state.load(Ordering::Acquire),
                                HeadsetPresence::Unavailable as u8
                            );
                            anyhow::bail!("disconnected")
                        }
                    }
                },
                &stop,
                &state,
                &callback,
                |_| {},
            );
            assert!(result.is_err());
            assert_eq!(
                state.load(Ordering::Acquire),
                HeadsetPresence::Unknown as u8
            );
            assert_eq!(
                changes.load(Ordering::Relaxed),
                2,
                "duplicate reports do not trigger routing"
            );
        }

        #[test]
        fn another_protocol_can_use_the_same_observer_without_a50x_decoding() {
            fn synthetic_decode(bytes: &[u8]) -> Option<HeadsetPresence> {
                match bytes {
                    [0x42, 1] => Some(HeadsetPresence::Available),
                    _ => None,
                }
            }
            let profile = HidPresenceProfile {
                model: "synthetic",
                name: "Synthetic test device",
                vendor_id: 1,
                product_id: 2,
                usage_page: 3,
                usage: 4,
                report_length: 8,
                decode: synthetic_decode,
                initial_query: None,
            };
            let stop = Arc::new(AtomicBool::new(false));
            let worker_stop = Arc::clone(&stop);
            let callback: Arc<dyn Fn() + Send + Sync> =
                Arc::new(move || worker_stop.store(true, Ordering::Relaxed));
            let state = AtomicU8::new(HeadsetPresence::Unknown as u8);
            let reports = Arc::new(std::sync::Mutex::new(Vec::new()));
            let captured_reports = Arc::clone(&reports);
            observe_reports(
                profile,
                |buffer| {
                    buffer[..2].copy_from_slice(&[0x42, 1]);
                    Ok(2)
                },
                &stop,
                &state,
                &callback,
                move |report| captured_reports.lock().unwrap().push(report.to_vec()),
            )
            .unwrap();
            assert_eq!(
                state.load(Ordering::Acquire),
                HeadsetPresence::Available as u8
            );
            assert_eq!(*reports.lock().unwrap(), vec![vec![0x42, 1]]);
            assert_eq!((A50X_PROFILE.decode)(&[0x42, 1]), None);
            assert!(profile.matches(1, 2, 3, 4));
            assert!(!profile.matches(1, 2, 3, 5));
        }

        #[test]
        fn initial_adc_queries_are_configured_per_device_profile() {
            assert!(matches!(
                G533_PROFILE.initial_query,
                Some(InitialQuery::HidppAdcMeasurement {
                    feature_id: 0x1f20,
                    unavailable_error: Some(0x05)
                })
            ));
            assert!(matches!(
                G733_PROFILE.initial_query,
                Some(InitialQuery::HidppAdcMeasurement {
                    feature_id: 0x1f20,
                    unavailable_error: Some(0x05)
                })
            ));
            assert_eq!(A50X_PROFILE.initial_query, None);
        }

        #[test]
        fn hidpp_requests_and_replies_use_the_long_report_format() {
            let request = make_hidpp_request(0xff, 0, 1, [0x1f, 0x20, 0]);
            assert_eq!(&request[..7], &[0x11, 0xff, 0, 1, 0x1f, 0x20, 0]);

            let mut response = [0; HIDPP_LONG_REPORT_LENGTH];
            response[..7].copy_from_slice(&[0x11, 0xff, 7, 1, 0x01, 0, 0x01]);
            assert!(matches!(
                parse_hidpp_reply(&response, 0xff, 7, 1),
                Some(HidppReply::Response(_))
            ));

            let mut error = [0; HIDPP_LONG_REPORT_LENGTH];
            error[..6].copy_from_slice(&[0x11, 0xff, 0xff, 7, 1, 0x05]);
            assert!(matches!(
                parse_hidpp_reply(&error, 0xff, 7, 1),
                Some(HidppReply::Error(0x05))
            ));
            assert!(parse_hidpp_reply(&error[..19], 0xff, 7, 1).is_none());
            assert!(parse_hidpp_reply(&response, 0xff, 8, 1).is_none());
        }

        #[test]
        fn initial_adc_response_and_unavailable_error_set_presence() {
            let mut response = [0; HIDPP_LONG_REPORT_LENGTH];
            response[6] = 0x01;
            assert_eq!(
                decode_adc_reply(G533_PROFILE, Some(0x05), HidppReply::Response(response)).unwrap(),
                HeadsetPresence::Available
            );
            assert_eq!(
                decode_adc_reply(G533_PROFILE, Some(0x05), HidppReply::Error(0x05)).unwrap(),
                HeadsetPresence::Unavailable
            );
            assert!(decode_adc_reply(G533_PROFILE, Some(0x05), HidppReply::Error(0x06)).is_err());
        }
    }
}

#[cfg(target_os = "windows")]
pub use windows::HidPresenceMonitor;

#[cfg(test)]
mod tests {
    use super::devices::{
        A50X_PROFILE, G533_PROFILE, G733_PROFILE, PROFILES, profile_by_model, profile_by_usb_device,
    };
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
                    .all(|other| !other.matches_usb_device(profile.vendor_id, profile.product_id))
            );
        }
        assert_eq!(
            profile_by_model("g533").map(|profile| profile.product_id),
            Some(0x0a66)
        );
        assert_eq!(
            profile_by_usb_device(0x046d, 0x0ab5).map(|profile| profile.model),
            Some("g733")
        );
    }

    #[test]
    fn audio_endpoint_selection_uses_vendor_and_product_ids() {
        assert!(A50X_PROFILE.matches_usb_device(0x046d, 0x0b0b));
        assert!(G533_PROFILE.matches_usb_device(0x046d, 0x0a66));
        assert!(!A50X_PROFILE.matches_usb_device(0x046d, 0x0a66));
        assert!(!G533_PROFILE.matches_usb_device(0x1234, 0x0a66));
    }

    #[test]
    fn g533_link_events_decode_only_observed_adc_status_reports() {
        let report = |event, flags| {
            let mut report = [0; 20];
            report[..7].copy_from_slice(&[0x11, 0xff, 0x07, event, 0x0f, 0x14, flags]);
            report
        };
        assert_eq!(
            (G533_PROFILE.decode)(&report(0x00, 0x00)),
            Some(HeadsetPresence::Unavailable)
        );
        for flags in [0x01, 0x03, 0x07] {
            assert_eq!(
                (G533_PROFILE.decode)(&report(0x00, flags)),
                Some(HeadsetPresence::Available)
            );
        }
        assert_eq!(
            (G533_PROFILE.decode)(&report(0x00, 0x0f)),
            Some(HeadsetPresence::Unknown)
        );
        assert_eq!((G533_PROFILE.decode)(&report(0x01, 0x00)), None);
        let mut unrelated = report(0x00, 0);
        unrelated[2] = 0x06;
        assert_eq!((G533_PROFILE.decode)(&unrelated), None);
        for length in 0..20 {
            assert_eq!((G533_PROFILE.decode)(&report(0x00, 0x01)[..length]), None);
        }
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
            (G733_PROFILE.decode)(&report(0x00)),
            Some(HeadsetPresence::Unavailable)
        );
        assert_eq!(
            (G733_PROFILE.decode)(&report(0x01)),
            Some(HeadsetPresence::Available)
        );
        assert_eq!(
            (G733_PROFILE.decode)(&report(0x03)),
            Some(HeadsetPresence::Available)
        );
        assert_eq!(
            (G733_PROFILE.decode)(&report(0x0f)),
            Some(HeadsetPresence::Unknown)
        );
        assert_eq!((G733_PROFILE.decode)(&report(0x01)[..19]), None);
        assert_eq!(
            (G733_PROFILE.decode)(&[0x11, 0xff, 0x07, 0x00, 0x0f, 0x41, 0x00]),
            None
        );
    }

    #[test]
    fn padded_reports_work_and_every_truncated_prefix_is_ignored() {
        let mut report = [0; 64];
        report[..7].copy_from_slice(&[2, 12, 4, 0, 19, 0, 5]);
        assert_eq!(
            (A50X_PROFILE.decode)(&report),
            Some(HeadsetPresence::Unavailable)
        );
        for len in 0..7 {
            assert_eq!((A50X_PROFILE.decode)(&report[..len]), None);
        }
    }

    #[test]
    fn captured_a50x_power_transitions_are_distinct() {
        assert_eq!(
            (A50X_PROFILE.decode)(&[2, 12, 4, 0, 19, 0, 0]),
            Some(HeadsetPresence::Available)
        );
        assert_eq!(
            (A50X_PROFILE.decode)(&[2, 12, 4, 0, 19, 0, 5]),
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
            assert_eq!((A50X_PROFILE.decode)(&report), None);
        }
        assert_eq!(
            (A50X_PROFILE.decode)(&[2, 12, 4, 0, 19, 0, 99]),
            Some(HeadsetPresence::Unknown)
        );
    }
}
