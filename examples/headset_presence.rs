//! Observe the experimental detector without changing any Windows audio settings.
//! cargo run --example headset_presence -- 45
//! cargo run --example headset_presence -- --watch-g533 45
//! cargo run --example headset_presence -- --list-g533
#[allow(dead_code)]
#[path = "../src/audio/presence.rs"]
mod presence;

fn main() -> anyhow::Result<()> {
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    simplelog::SimpleLogger::init(log::LevelFilter::Info, simplelog::Config::default())?;
    let mut args = std::env::args().skip(1);
    let first = args.next();
    if first.as_deref() == Some("--list-g533") {
        let api = hidapi::HidApi::new()?;
        let mut found = false;
        for device in api
            .device_list()
            .filter(|device| device.vendor_id() == 0x046d && device.product_id() == 0x0a66)
        {
            found = true;
            println!(
                "usage={:04x}:{:04x} interface={} path={}",
                device.usage_page(),
                device.usage(),
                device.interface_number(),
                device.path().to_string_lossy()
            );
        }
        if !found {
            println!("No present G533 HID collections found.");
        }
        return Ok(());
    }
    let watch_g533 = first.as_deref() == Some("--watch-g533");
    let profile = if watch_g533 {
        presence::G533_PROFILE
    } else {
        presence::A50X_PROFILE
    };
    let seconds = (if watch_g533 { args.next() } else { first })
        .unwrap_or_else(|| "30".into())
        .parse::<u64>()?;
    anyhow::ensure!((1..=300).contains(&seconds), "Choose 1–300 seconds");
    let monitor = presence::HidPresenceMonitor::start(profile, Arc::new(|| {}))?;
    println!(
        "Read-only {} observer running for {seconds}s. Toggle headset power to observe transitions.",
        profile.name
    );
    let started = Instant::now();
    let mut previous = None;
    while started.elapsed() < Duration::from_secs(seconds) {
        let current = monitor.presence();
        if previous != Some(current) {
            println!("{:.2}s: {current:?}", started.elapsed().as_secs_f32());
            previous = Some(current);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
