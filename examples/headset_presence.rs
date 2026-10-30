//! Read-only headset HID diagnostics. Choose a model and action explicitly.
//! cargo run --example headset_presence -- g533 watch 30
//! cargo run --example headset_presence -- g733 capture 30
//! cargo run --example headset_presence -- a50x list
#[allow(dead_code)]
#[path = "../src/audio/presence.rs"]
mod presence;

fn main() -> anyhow::Result<()> {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    simplelog::SimpleLogger::init(log::LevelFilter::Info, simplelog::Config::default())?;
    let mut args = std::env::args().skip(1);
    let model = args.next().ok_or_else(|| anyhow::anyhow!("{}", usage()))?;
    let profile = presence::devices::profile_by_model(&model)
        .copied()
        .ok_or_else(|| anyhow::anyhow!("Unknown model '{model}'.\n{}", usage()))?;
    let action = args.next().ok_or_else(|| anyhow::anyhow!("{}", usage()))?;

    if action == "list" {
        anyhow::ensure!(args.next().is_none(), "Usage: {}", usage());
        let api = hidapi::HidApi::new()?;
        let mut found = false;
        for device in api.device_list().filter(|device| {
            profile.matches(
                device.vendor_id(),
                device.product_id(),
                device.usage_page(),
                device.usage(),
            )
        }) {
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
            println!("No present {} HID collections found.", profile.name);
        }
        return Ok(());
    }

    anyhow::ensure!(
        matches!(action.as_str(), "watch" | "capture"),
        "Unknown action '{action}'.\n{}",
        usage()
    );
    let seconds = args.next().map_or(Ok(30), |value| value.parse::<u64>())?;
    anyhow::ensure!(args.next().is_none(), "Usage: {}", usage());
    anyhow::ensure!((1..=300).contains(&seconds), "Choose 1–300 seconds");

    let started = Instant::now();
    let on_report: Arc<presence::HidReportCallback> = if action == "capture" {
        Arc::new(move |report| {
            let hex = report
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<Vec<_>>()
                .join(" ");
            println!("{:.2}s: {hex}", started.elapsed().as_secs_f32());
        })
    } else {
        Arc::new(|_| {})
    };
    let monitor = presence::HidPresenceMonitor::start(profile, Arc::new(|| {}), on_report)?;
    println!(
        "Read-only {} {action} for {seconds}s. Toggle headset power to observe status transitions.",
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

fn usage() -> String {
    let models = presence::devices::PROFILES
        .iter()
        .map(|profile| profile.model)
        .collect::<Vec<_>>()
        .join("|");
    format!("Usage: headset_presence <{models}> <list|watch|capture> [seconds]")
}
