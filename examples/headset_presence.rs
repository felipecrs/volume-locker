//! Read-only headset HID diagnostics. Choose a model and action explicitly.
//! cargo run --example headset_presence -- g533 watch 30
//! cargo run --example headset_presence -- g733 capture 30
//! cargo run --example headset_presence -- a50x list
#[path = "../src/audio/presence.rs"]
mod presence;

fn main() -> anyhow::Result<()> {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    let args: Vec<String> = std::env::args().skip(1).collect();
    let (model, action, seconds) = match args.as_slice() {
        [model, action] => (model.as_str(), action.as_str(), None),
        [model, action, seconds] => (model.as_str(), action.as_str(), Some(seconds)),
        _ => anyhow::bail!("{}", usage()),
    };
    let profile = *presence::devices::PROFILES
        .iter()
        .find(|profile| profile.model == model)
        .ok_or_else(|| anyhow::anyhow!("Unknown model '{model}'.\n{}", usage()))?;
    // The monitor logs raw reports at trace level.
    let level = if action == "capture" {
        log::LevelFilter::Trace
    } else {
        log::LevelFilter::Info
    };
    simplelog::SimpleLogger::init(level, simplelog::Config::default())?;

    if action == "list" {
        anyhow::ensure!(seconds.is_none(), "{}", usage());
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
            println!("No present {} HID collections found.", profile.model);
        }
        return Ok(());
    }

    anyhow::ensure!(
        matches!(action, "watch" | "capture"),
        "Unknown action '{action}'.\n{}",
        usage()
    );
    let seconds = seconds.map_or(Ok(30), |value| value.parse::<u64>())?;
    anyhow::ensure!((1..=300).contains(&seconds), "Choose 1–300 seconds");

    let started = Instant::now();
    let monitor = presence::HidPresenceMonitor::start(profile, Arc::new(|| {}))?;
    println!(
        "Read-only {} {action} for {seconds}s. Toggle headset power to observe status transitions.",
        profile.model
    );
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
