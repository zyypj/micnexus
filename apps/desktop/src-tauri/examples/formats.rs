//! Prints the shared-mode mix format of every audio endpoint.
//!
//!   cargo run --example formats
fn main() {
    #[cfg(windows)]
    {
        use micnexus_desktop_lib::audio::devices;
        if let Some(rate) = std::env::args().nth(1).and_then(|r| r.parse::<u32>().ok()) {
            // `formats <rate>`: set the best cable's capture side to <rate> (debugging).
            let outs = devices::outputs().unwrap();
            let cable = devices::best_cable(&outs).expect("no cable");
            let id = cable.capture_id.clone().expect("no capture twin");
            println!(
                "set {} -> {rate}: {:?}",
                cable.capture_name.clone().unwrap_or_default(),
                devices::set_rate(&id, rate)
            );
        }
        for e in devices::outputs().unwrap() {
            println!(
                "render  {:<55} {:?}  cable={:?}",
                e.name,
                devices::mix_format(&e.id),
                e.cable
            );
            if let Some(c) = &e.capture_id {
                println!(
                    "  twin  {:<55} {:?}",
                    e.capture_name.clone().unwrap_or_default(),
                    devices::mix_format(c)
                );
            }
        }
    }
}
