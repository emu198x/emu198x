//! CIA-A TOD timing for the fixed-sync PAL A500 profile.
//!
//! Unit-level tests (`cia::tests::tod_counter_increments_on_tick_…`)
//! cover `Cia::tick_tod()` in isolation. This integration test
//! verifies the machine wiring: the A500 connects CIA-A `TICK`
//! directly to active-low `/VSYNC`, so the counter advances after
//! `/VSYNC` deasserts rather than at line-zero `VERTB`.

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};

const PAL_CIA_A_TOD_LINE: u16 = 5;
const PAL_CIA_A_TOD_HPOS: u16 = 84;

fn run_until_position(amiga: &mut AmigaOcs, frame: u64, vpos: u16, hpos: u16) {
    for _ in 0..=PAL_FRAME_TICKS {
        if amiga.agnus().vbl_count == frame
            && amiga.agnus().vpos == vpos
            && amiga.agnus().hpos == hpos
        {
            return;
        }
        amiga.tick();
    }
    panic!("beam did not reach frame {frame}, position ({vpos},{hpos})");
}

#[test]
fn cia_a_tod_ticks_once_per_frame_after_pal_vsync_deasserts() {
    let mut amiga = AmigaOcs::new(vec![0; 512 * 1024]);

    // The line-zero VERTB event must not clock CIA-A. The current
    // fixed-sync approximation makes the delayed counter update
    // visible at (5,84), after the /VSYNC pin itself has risen.
    run_until_position(&mut amiga, 0, PAL_CIA_A_TOD_LINE, PAL_CIA_A_TOD_HPOS - 1);
    assert_eq!(
        amiga.cia_a().tod_counter(),
        0,
        "CIA-A TOD must remain unchanged before VSYNC deassertion"
    );
    run_until_position(&mut amiga, 0, PAL_CIA_A_TOD_LINE, PAL_CIA_A_TOD_HPOS);
    assert_eq!(
        amiga.cia_a().tod_counter(),
        1,
        "CIA-A TOD should advance at the delayed VSYNC event"
    );

    // No second increment is allowed before the next frame's event.
    run_until_position(&mut amiga, 1, PAL_CIA_A_TOD_LINE, PAL_CIA_A_TOD_HPOS - 1);
    assert_eq!(
        amiga.cia_a().tod_counter(),
        1,
        "CIA-A TOD should advance only once per PAL frame"
    );
    run_until_position(&mut amiga, 1, PAL_CIA_A_TOD_LINE, PAL_CIA_A_TOD_HPOS);
    assert_eq!(
        amiga.cia_a().tod_counter(),
        2,
        "next frame should produce one further TOD increment"
    );
}
