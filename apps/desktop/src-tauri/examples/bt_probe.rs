//! Bluetooth radio check (debugging): prints what the listener would report.
fn main() {
    let core = micnexus_desktop_lib::core::Core::new(
        micnexus_desktop_lib::settings::Store::new(&std::env::temp_dir().join("micnexus-bt-probe")),
        "probe",
        |e| {
            if let micnexus_desktop_lib::core::Event::State(s) = e {
                println!("bluetooth: {} {:?}", s.bluetooth.state, s.bluetooth.error);
            }
        },
    );
    micnexus_desktop_lib::bluetooth::start(core.clone());
    std::thread::sleep(std::time::Duration::from_secs(2));
    let s = core.snapshot();
    println!("final: {} {:?}", s.bluetooth.state, s.bluetooth.error);
}
