//! Tatung Einstein: Ctrl-BREAK starts loading a real CPCEMU `.DSK` image (the
//! `einstein_flop` / TOSEC "Tatung Einstein TC-01" set).
//!
//! Gated on ROM and disk assets: set `EMU198X_EINSTEIN_BIOS` and
//! `EMU198X_EINSTEIN_DISK`, then run with `--ignored`. The parser's read-back
//! correctness remains covered by an in-module unit test against a synthetic
//! DSK.

use std::env;
use std::fs;

use machine_tatung_einstein::{Einstein, EinsteinRegion, Modifier};

#[test]
#[ignore = "FIXTURE: needs the Einstein MOS and a bootable CPCEMU .dsk — run with --ignored"]
fn ctrl_break_loads_from_real_disk() {
    let Ok(bios_path) = env::var("EMU198X_EINSTEIN_BIOS") else {
        emu198x_test_skip::skip!(
            "not staged: set EMU198X_EINSTEIN_BIOS to the 8 KB Einstein MOS ROM"
        );
    };
    let Ok(disk_path) = env::var("EMU198X_EINSTEIN_DISK") else {
        emu198x_test_skip::skip!("not staged: set EMU198X_EINSTEIN_DISK to a bootable CPCEMU .dsk");
    };
    let bios = fs::read(&bios_path).expect("read BIOS");
    let dsk = fs::read(&disk_path).expect("read .dsk");

    let mut sys = Einstein::new(bios, EinsteinRegion::Pal);
    sys.insert_cpc_dsk(0, &dsk)
        .unwrap_or_else(|e| panic!("parse {disk_path}: {e}"));
    for _ in 0..300 {
        sys.run_frame();
    }

    sys.start_io_trace();
    sys.set_modifier(Modifier::Control, true);
    sys.press_key(0, 0);
    for frame in 0..300 {
        sys.run_frame();
        if frame == 5 {
            // The MOS debounces BREAK by waiting for its release before it
            // samples the separate modifier byte at $20. Keep CTRL held.
            sys.release_key(0, 0);
        }
    }
    sys.set_modifier(Modifier::Control, false);
    let trace = sys.take_io_trace();
    let control_sample = trace
        .iter()
        .find(|event| !event.write && event.port == 0x20)
        .expect("MOS should sample the modifier keys after BREAK is released");
    assert_eq!(
        control_sample.value & 0x40,
        0,
        "CTRL must remain held until the MOS samples port $20"
    );

    let fdc_commands = trace
        .iter()
        .filter(|event| event.write && event.port == 0x18)
        .count();
    let disk_bytes = trace
        .iter()
        .filter(|event| !event.write && event.port == 0x1b)
        .count();
    assert!(fdc_commands > 0, "Ctrl-BREAK should issue WD1770 commands");
    assert!(
        disk_bytes >= 512,
        "Ctrl-BREAK should transfer at least one complete disk sector; got {disk_bytes} bytes"
    );
}
