//! CIA-A TOD tick rate is driven by Agnus's /VERTB signal.
//!
//! Unit-level tests (`cia::tests::tod_counter_increments_on_tick_…`)
//! cover `Cia::tick_tod()` in isolation. This integration test
//! verifies that the *wiring* is right: running the full AmigaOcs
//! for N PAL frames should produce exactly N TOD increments on
//! CIA-A (the TOD pin on a real A500 is tied to /VSYNC).

use std::path::PathBuf;
use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        eprintln!("skipping: Kickstart 1.3 ROM missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

#[test]
fn cia_a_tod_ticks_once_per_pal_frame() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);

    // Before the first VBL: TOD should be 0 (and stay 0 for most of
    // the frame since the pin only rises at vpos = 0).
    for _ in 0..(PAL_FRAME_TICKS / 2) {
        amiga.tick();
    }
    assert_eq!(
        amiga.cia_a().tod_counter,
        0,
        "TOD should not have ticked before the first VBL edge"
    );

    // Finish the first frame — that rising edge should tick TOD once.
    for _ in 0..(PAL_FRAME_TICKS / 2 + 1) {
        amiga.tick();
    }
    assert_eq!(
        amiga.cia_a().tod_counter,
        1,
        "TOD should have ticked exactly once on first VBL"
    );

    // Run 49 more frames — 50 total, 50 VBL edges, TOD counter should
    // reach 50.
    for _ in 0..(49 * PAL_FRAME_TICKS) {
        amiga.tick();
    }
    assert_eq!(
        amiga.cia_a().tod_counter,
        50,
        "TOD should advance exactly once per PAL frame"
    );
}
