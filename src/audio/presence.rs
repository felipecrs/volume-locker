//! Headset radio state is distinct from the USB audio endpoint's connection.
//! Unknown observations never override Windows' availability decision.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HeadsetPresence {
    Unknown = 0,
    Available = 1,
    Unavailable = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitialQuery {
    HidppAdcMeasurement {
        feature_id: u16,
        unavailable_error: u8,
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
    /// Takes the report and the feature index a startup query discovered, if any.
    pub decode: fn(&[u8], Option<u8>) -> Option<HeadsetPresence>,
    pub initial_query: Option<InitialQuery>,
}

impl HidPresenceProfile {
    pub fn matches(&self, vendor_id: u16, product_id: u16, usage_page: u16, usage: u16) -> bool {
        self.vendor_id == vendor_id
            && self.product_id == product_id
            && self.usage_page == usage_page
            && self.usage == usage
    }
}

const HIDPP_LONG_REPORT_ID: u8 = 0x11;
const HIDPP_LONG_REPORT_LENGTH: usize = 20;
const HIDPP_DIRECT_DEVICE_INDEX: u8 = 0xff;

#[path = "presence/devices.rs"]
pub mod devices;

fn decode_adc_flags(flags: u8) -> HeadsetPresence {
    match flags {
        flags if flags & 0x01 == 0 => HeadsetPresence::Unavailable,
        0x01 | 0x03 | 0x07 => HeadsetPresence::Available,
        _ => HeadsetPresence::Unknown,
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::{
        HIDPP_DIRECT_DEVICE_INDEX, HIDPP_LONG_REPORT_ID, HIDPP_LONG_REPORT_LENGTH, HeadsetPresence,
        HidPresenceProfile, InitialQuery,
    };
    use std::cell::Cell;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc,
    };
    use std::thread::{self, JoinHandle};
    use std::time::{Duration, Instant};

    // Replies echo this ID; avoid the low IDs other HID++ clients are likeliest to use.
    const HIDPP_SOFTWARE_ID: u8 = 0x0b;
    // shortcut: a slower query can still route once to a dead headset, raise this if that is seen.
    const INITIAL_QUERY_WAIT: Duration = Duration::from_millis(500);

    enum HidppReply {
        Response([u8; HIDPP_LONG_REPORT_LENGTH]),
        Error(u8),
    }

    struct HidppReader<'a> {
        write: &'a dyn Fn(&[u8]) -> anyhow::Result<usize>,
        read: &'a dyn Fn(&mut [u8]) -> anyhow::Result<usize>,
        profile: HidPresenceProfile,
        stop: &'a AtomicBool,
        state: &'a AtomicU8,
        on_change: &'a Arc<dyn Fn() + Send + Sync>,
        discovered_index: Cell<Option<u8>>,
    }

    fn make_hidpp_request(
        feature_index: u8,
        parameters: [u8; 3],
    ) -> [u8; HIDPP_LONG_REPORT_LENGTH] {
        let mut report = [0; HIDPP_LONG_REPORT_LENGTH];
        report[..7].copy_from_slice(&[
            HIDPP_LONG_REPORT_ID,
            HIDPP_DIRECT_DEVICE_INDEX,
            feature_index,
            HIDPP_SOFTWARE_ID,
            parameters[0],
            parameters[1],
            parameters[2],
        ]);
        report
    }

    fn parse_hidpp_reply(bytes: &[u8], feature_index: u8) -> Option<HidppReply> {
        if bytes.len() != HIDPP_LONG_REPORT_LENGTH
            || bytes[0] != HIDPP_LONG_REPORT_ID
            || bytes[1] != HIDPP_DIRECT_DEVICE_INDEX
        {
            return None;
        }
        if bytes[2] == feature_index && bytes[3] == HIDPP_SOFTWARE_ID {
            let mut response = [0; HIDPP_LONG_REPORT_LENGTH];
            response.copy_from_slice(bytes);
            return Some(HidppReply::Response(response));
        }
        if bytes[2] == 0xff && bytes[3] == feature_index && bytes[4] == HIDPP_SOFTWARE_ID {
            return Some(HidppReply::Error(bytes[5]));
        }
        None
    }

    impl HidppReader<'_> {
        fn exchange(&self, feature_index: u8, parameters: [u8; 3]) -> anyhow::Result<HidppReply> {
            let request = make_hidpp_request(feature_index, parameters);
            let written = (self.write)(&request)?;
            anyhow::ensure!(
                written == request.len(),
                "HID++ request was only partially written ({written}/{} bytes)",
                request.len()
            );

            let deadline = Instant::now() + Duration::from_secs(2);
            let mut report = [0; 64];
            while !self.stop.load(Ordering::Relaxed) && Instant::now() < deadline {
                let count = (self.read)(&mut report)?;
                if count == 0 {
                    continue;
                }
                let bytes = &report[..count];
                log::trace!("{}: {bytes:02x?}", self.profile.name);
                if let Some(next) = (self.profile.decode)(bytes, self.discovered_index.get()) {
                    publish(self.profile.name, self.state, next, self.on_change);
                }
                if let Some(reply) = parse_hidpp_reply(bytes, feature_index) {
                    return Ok(reply);
                }
            }
            anyhow::bail!("No HID++ reply to feature {feature_index:#04x}")
        }
    }

    fn query_initial_state(
        query: InitialQuery,
        reader: &HidppReader<'_>,
    ) -> anyhow::Result<HeadsetPresence> {
        let InitialQuery::HidppAdcMeasurement {
            feature_id,
            unavailable_error,
        } = query;
        let feature_lookup =
            match reader.exchange(0, [(feature_id >> 8) as u8, feature_id as u8, 0])? {
                HidppReply::Response(report) => report,
                HidppReply::Error(code) => {
                    anyhow::bail!("HID++ feature lookup failed with error {code:#04x}")
                }
            };
        let feature_index = feature_lookup[4];
        anyhow::ensure!(
            feature_index != 0,
            "ADC feature {feature_id:#06x} is not exposed"
        );
        reader.discovered_index.set(Some(feature_index));

        decode_adc_reply(unavailable_error, reader.exchange(feature_index, [0; 3])?)
    }

    fn decode_adc_reply(
        unavailable_error: u8,
        reply: HidppReply,
    ) -> anyhow::Result<HeadsetPresence> {
        match reply {
            HidppReply::Response(report) => Ok(super::decode_adc_flags(report[6])),
            HidppReply::Error(code) if code == unavailable_error => {
                Ok(HeadsetPresence::Unavailable)
            }
            HidppReply::Error(code) => {
                anyhow::bail!("HID++ ADC measurement failed with error {code:#04x}")
            }
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
            let (query_done, query_finished) = mpsc::channel();
            let worker = thread::Builder::new()
                .name("headset-presence".into())
                .spawn(move || {
                    let mut last_error = None;
                    while !worker_stop.load(Ordering::Relaxed) {
                        let result = listen(
                            profile,
                            &worker_stop,
                            &worker_state,
                            &on_change,
                            &query_done,
                        );
                        // Don't make the caller wait if listen failed before querying.
                        let _ = query_done.send(());
                        // Losing the provider is not proof that the headset turned off.
                        if !worker_stop.load(Ordering::Relaxed) {
                            publish(
                                profile.name,
                                &worker_state,
                                HeadsetPresence::Unknown,
                                &on_change,
                            );
                        }
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
            // Callers apply priorities right after this, so give the query a chance
            // to answer first rather than routing to a headset that is off.
            if profile.initial_query.is_some() {
                let _ = query_finished.recv_timeout(INITIAL_QUERY_WAIT);
            }
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

    fn publish(
        name: &str,
        state: &AtomicU8,
        next: HeadsetPresence,
        on_change: &Arc<dyn Fn() + Send + Sync>,
    ) {
        if state.swap(next as u8, Ordering::AcqRel) != next as u8 {
            log::info!("{name} radio presence: {next:?}");
            on_change();
        }
    }

    fn listen(
        profile: HidPresenceProfile,
        stop: &AtomicBool,
        state: &AtomicU8,
        on_change: &Arc<dyn Fn() + Send + Sync>,
        query_done: &mpsc::Sender<()>,
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
        let write = |request: &[u8]| -> anyhow::Result<usize> { Ok(device.write(request)?) };
        let read =
            |report: &mut [u8]| -> anyhow::Result<usize> { Ok(device.read_timeout(report, 500)?) };
        let mut feature_index = None;
        if let Some(query) = profile.initial_query {
            let reader = HidppReader {
                write: &write,
                read: &read,
                profile,
                stop,
                state,
                on_change,
                discovered_index: Cell::new(None),
            };
            match query_initial_state(query, &reader) {
                Ok(initial_state) => publish(profile.name, state, initial_state, on_change),
                Err(error) => {
                    log::warn!("Cannot query initial {} state: {error:#}", profile.name);
                }
            }
            feature_index = reader.discovered_index.get();
            let _ = query_done.send(());
        }
        log::info!("Listening for {} status events", profile.name);
        // After the status query on connect, monitoring only reads input reports.
        observe_reports(profile, read, stop, state, on_change, feature_index)
    }

    // The transport only exposes reads. Shared by every protocol adapter and
    // exercised with synthetic reports, timeouts and failures without hardware.
    fn observe_reports(
        profile: HidPresenceProfile,
        mut read: impl FnMut(&mut [u8]) -> anyhow::Result<usize>,
        stop: &AtomicBool,
        state: &AtomicU8,
        on_change: &Arc<dyn Fn() + Send + Sync>,
        feature_index: Option<u8>,
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
                    publish(profile.name, state, HeadsetPresence::Unknown, on_change);
                    return Err(error);
                }
            };
            if count == 0 {
                continue;
            }
            let Some(bytes) = report.get(..count) else {
                publish(profile.name, state, HeadsetPresence::Unknown, on_change);
                anyhow::bail!("HID input report exceeds read buffer");
            };
            log::trace!("{}: {bytes:02x?}", profile.name);
            if let Some(next) = (profile.decode)(bytes, feature_index) {
                publish(profile.name, state, next, on_change);
            }
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::super::devices::{A50X_PROFILE, G533_PROFILE};
        use super::*;
        use std::cell::RefCell;
        use std::collections::VecDeque;
        use std::sync::atomic::AtomicUsize;

        #[test]
        fn timeouts_are_never_decoded_as_device_reports() {
            let profile = HidPresenceProfile {
                decode: |_, _| panic!("a timeout is not a report"),
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
                None,
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
                assert!(HidPresenceMonitor::start(profile, Arc::new(|| {})).is_err());
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
                None,
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
            fn synthetic_decode(bytes: &[u8], _: Option<u8>) -> Option<HeadsetPresence> {
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
            observe_reports(
                profile,
                |buffer| {
                    buffer[..2].copy_from_slice(&[0x42, 1]);
                    Ok(2)
                },
                &stop,
                &state,
                &callback,
                None,
            )
            .unwrap();
            assert_eq!(
                state.load(Ordering::Acquire),
                HeadsetPresence::Available as u8
            );
            assert_eq!((A50X_PROFILE.decode)(&[0x42, 1], None), None);
            assert!(profile.matches(1, 2, 3, 4));
            assert!(!profile.matches(1, 2, 3, 5));
        }

        #[test]
        fn hidpp_requests_and_replies_use_the_long_report_format() {
            let request = make_hidpp_request(0, [0x1f, 0x20, 0]);
            assert_eq!(
                &request[..7],
                &[0x11, 0xff, 0, HIDPP_SOFTWARE_ID, 0x1f, 0x20, 0]
            );

            let mut response = [0; HIDPP_LONG_REPORT_LENGTH];
            response[..7].copy_from_slice(&[0x11, 0xff, 7, HIDPP_SOFTWARE_ID, 0x01, 0, 0x01]);
            assert!(matches!(
                parse_hidpp_reply(&response, 7),
                Some(HidppReply::Response(_))
            ));

            let mut error = [0; HIDPP_LONG_REPORT_LENGTH];
            error[..6].copy_from_slice(&[0x11, 0xff, 0xff, 7, HIDPP_SOFTWARE_ID, 0x05]);
            assert!(matches!(
                parse_hidpp_reply(&error, 7),
                Some(HidppReply::Error(0x05))
            ));
            assert!(parse_hidpp_reply(&error[..19], 7).is_none());
            assert!(parse_hidpp_reply(&response, 8).is_none());
        }

        #[test]
        fn initial_adc_response_and_unavailable_error_set_presence() {
            let mut response = [0; HIDPP_LONG_REPORT_LENGTH];
            response[6] = 0x01;
            assert_eq!(
                decode_adc_reply(0x05, HidppReply::Response(response)).unwrap(),
                HeadsetPresence::Available
            );
            assert_eq!(
                decode_adc_reply(0x05, HidppReply::Error(0x05)).unwrap(),
                HeadsetPresence::Unavailable
            );
            assert!(decode_adc_reply(0x05, HidppReply::Error(0x06)).is_err());
        }

        fn hidpp_report(bytes: &[u8]) -> Vec<u8> {
            let mut report = vec![0; HIDPP_LONG_REPORT_LENGTH];
            report[..bytes.len()].copy_from_slice(bytes);
            report
        }

        /// Runs the G533 query against canned replies; returns the result,
        /// the written requests and the discovered feature index.
        fn run_query(
            replies: Vec<Vec<u8>>,
            stopped: bool,
        ) -> (anyhow::Result<HeadsetPresence>, Vec<Vec<u8>>, Option<u8>) {
            let replies = RefCell::new(VecDeque::from(replies));
            let written = RefCell::new(Vec::new());
            let write = |request: &[u8]| -> anyhow::Result<usize> {
                written.borrow_mut().push(request.to_vec());
                Ok(request.len())
            };
            let read = |buffer: &mut [u8]| -> anyhow::Result<usize> {
                let reply = replies.borrow_mut().pop_front().expect("unexpected read");
                buffer[..reply.len()].copy_from_slice(&reply);
                Ok(reply.len())
            };
            let stop = AtomicBool::new(stopped);
            let state = AtomicU8::new(HeadsetPresence::Unknown as u8);
            let callback: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
            let reader = HidppReader {
                write: &write,
                read: &read,
                profile: G533_PROFILE,
                stop: &stop,
                state: &state,
                on_change: &callback,
                discovered_index: Cell::new(None),
            };
            let result = query_initial_state(G533_PROFILE.initial_query.unwrap(), &reader);
            let discovered = reader.discovered_index.get();
            (result, written.into_inner(), discovered)
        }

        #[test]
        fn query_looks_up_the_feature_then_reads_status_at_the_discovered_index() {
            let (result, written, discovered) = run_query(
                vec![
                    hidpp_report(&[0x11, 0xff, 0, HIDPP_SOFTWARE_ID, 9]),
                    hidpp_report(&[0x11, 0xff, 9, HIDPP_SOFTWARE_ID, 0x0f, 0x14, 0x01]),
                ],
                false,
            );
            assert_eq!(result.unwrap(), HeadsetPresence::Available);
            assert_eq!(discovered, Some(9));
            assert_eq!(
                written[0][..7],
                [0x11, 0xff, 0, HIDPP_SOFTWARE_ID, 0x1f, 0x20, 0]
            );
            assert_eq!(written[1][..4], [0x11, 0xff, 9, HIDPP_SOFTWARE_ID]);
        }

        #[test]
        fn query_maps_the_configured_error_to_unavailable_but_keeps_the_index() {
            let (result, _, discovered) = run_query(
                vec![
                    hidpp_report(&[0x11, 0xff, 0, HIDPP_SOFTWARE_ID, 9]),
                    hidpp_report(&[0x11, 0xff, 0xff, 9, HIDPP_SOFTWARE_ID, 0x05]),
                ],
                false,
            );
            assert_eq!(result.unwrap(), HeadsetPresence::Unavailable);
            assert_eq!(discovered, Some(9));
        }

        #[test]
        fn query_stops_without_reading_or_sending_more_once_stopped() {
            let (result, written, discovered) = run_query(Vec::new(), true);
            assert!(result.is_err());
            assert_eq!(written.len(), 1);
            assert_eq!(discovered, None);
        }
    }
}

#[cfg(target_os = "windows")]
pub use windows::HidPresenceMonitor;
