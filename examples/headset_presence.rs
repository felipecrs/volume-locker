//! Observe the experimental detector without changing any Windows audio settings.
//! cargo run --example headset_presence -- 45
#[allow(dead_code)]
#[path = "../src/audio/presence.rs"]
mod presence;

fn main() -> anyhow::Result<()> {
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    simplelog::SimpleLogger::init(log::LevelFilter::Info, simplelog::Config::default())?;
    let seconds = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "30".into())
        .parse::<u64>()?;
    anyhow::ensure!((1..=300).contains(&seconds), "Choose 1–300 seconds");
    let monitor = presence::HidPresenceMonitor::start(presence::A50X_PROFILE, Arc::new(|| {}))?;
    println!(
        "Read-only observer running for {seconds}s. Toggle headset power to observe transitions."
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
