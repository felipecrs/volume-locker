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

/// A device-specific, passive-input protocol adapter. New hardware needs its
/// own verified interface identity and decoder; no generic power byte is assumed.
#[derive(Clone, Copy)]
pub struct HidPresenceProfile {
    pub name: &'static str,
    pub vendor_id: u16,
    pub product_id: u16,
    pub usage_page: u16,
    pub usage: u16,
    pub report_length: usize,
    pub decode: fn(&[u8]) -> Option<HeadsetPresence>,
}

impl HidPresenceProfile {
    pub fn matches(&self, vendor_id: u16, product_id: u16, usage_page: u16, usage: u16) -> bool {
        self.vendor_id == vendor_id
            && self.product_id == product_id
            && self.usage_page == usage_page
            && self.usage == usage
    }
}

pub const A50X_PROFILE: HidPresenceProfile = HidPresenceProfile {
    name: "A50 X",
    vendor_id: 0x046d,
    product_id: 0x0b0b,
    usage_page: 0xff32,
    usage: 0x0074,
    report_length: 64,
    decode: decode_a50x_report,
};

pub const G533_PROFILE: HidPresenceProfile = HidPresenceProfile {
    name: "Logitech G533",
    vendor_id: 0x046d,
    product_id: 0x0a66,
    usage_page: 0xff43,
    usage: 0x0202,
    report_length: 20,
    decode: decode_g533_report,
};

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

/// G533's `0x1f20` ADC-measurement event uses flag bit 0 for the wireless link.
/// Its observed feature index is 7; replies to host requests are not events.
fn decode_g533_report(report: &[u8]) -> Option<HeadsetPresence> {
    if report.len() != G533_PROFILE.report_length || report[..4] != [0x11, 0xff, 0x07, 0x00] {
        return None;
    }

    match report[6] {
        flags if flags & 0x01 == 0 => Some(HeadsetPresence::Unavailable),
        0x01 | 0x03 | 0x07 => Some(HeadsetPresence::Available),
        _ => Some(HeadsetPresence::Unknown),
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::{HeadsetPresence, HidPresenceProfile};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
    };
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

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
            let worker = thread::Builder::new()
                .name("headset-presence".into())
                .spawn(move || {
                    let mut last_error = None;
                    while !worker_stop.load(Ordering::Relaxed) {
                        let result = listen(profile, &worker_stop, &worker_state, &on_change);
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
        log::info!(
            "Listening for {} power events; initial state is unknown",
            profile.name
        );
        // Passive input reports only: no HID output/feature writes or G HUB dependency.
        observe_reports(
            profile,
            |report| device.read_timeout(report, 500).map_err(Into::into),
            stop,
            state,
            on_change,
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
            if let Some(next) = (profile.decode)(bytes) {
                publish(state, next, on_change);
            }
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::super::A50X_PROFILE;
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
                name: "Synthetic test device",
                vendor_id: 1,
                product_id: 2,
                usage_page: 3,
                usage: 4,
                report_length: 8,
                decode: synthetic_decode,
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
            )
            .unwrap();
            assert_eq!(
                state.load(Ordering::Acquire),
                HeadsetPresence::Available as u8
            );
            assert_eq!((A50X_PROFILE.decode)(&[0x42, 1]), None);
            assert!(profile.matches(1, 2, 3, 4));
            assert!(!profile.matches(1, 2, 3, 5));
        }
    }
}

#[cfg(target_os = "windows")]
pub use windows::HidPresenceMonitor;

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
    fn g533_link_events_decode_only_observed_adc_status_reports() {
        let report = |event, flags| {
            let mut report = [0; 20];
            report[..7].copy_from_slice(&[0x11, 0xff, 0x07, event, 0x0f, 0x14, flags]);
            report
        };
        assert_eq!(
            decode_g533_report(&report(0x00, 0x00)),
            Some(HeadsetPresence::Unavailable)
        );
        for flags in [0x01, 0x03, 0x07] {
            assert_eq!(
                decode_g533_report(&report(0x00, flags)),
                Some(HeadsetPresence::Available)
            );
        }
        assert_eq!(
            decode_g533_report(&report(0x00, 0x0f)),
            Some(HeadsetPresence::Unknown)
        );
        assert_eq!(decode_g533_report(&report(0x01, 0x00)), None);
        let mut unrelated = report(0x00, 0);
        unrelated[2] = 0x06;
        assert_eq!(decode_g533_report(&unrelated), None);
        for length in 0..20 {
            assert_eq!(decode_g533_report(&report(0x00, 0x01)[..length]), None);
        }
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
            decode_a50x_report(&[2, 12, 4, 0, 19, 0, 0]),
            Some(HeadsetPresence::Available)
        );
        assert_eq!(
            decode_a50x_report(&[2, 12, 4, 0, 19, 0, 5]),
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
            assert_eq!(decode_a50x_report(&report), None);
        }
        assert_eq!(
            decode_a50x_report(&[2, 12, 4, 0, 19, 0, 99]),
            Some(HeadsetPresence::Unknown)
        );
    }
}
