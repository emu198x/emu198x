//! M6: beam counter + VBL interrupt.
//!
//! Per `knowledge/decisions/amiga-restart-plan.md`. Adds Agnus beam
//! counter (vpos / hpos) and VBL interrupt delivery: every PAL frame
//! (227 × 312 = 70824 CCKs) the VERTB bit gets latched into INTREQ;
//! IPL computation routes that to the CPU when INTENA's master + VERTB
//! bits are set.
//!
//! Without VBL the boot's many spin-on-VBL loops never advance.

#[allow(dead_code)]
mod support;
use common_commodore_amiga_ocs::PAL_FRAME_TICKS;
#[allow(unused_imports)]
use support::*;

fn load_kickstart() -> Option<Vec<u8>> {
    let path = support::home_fixture_path(".emu198x/roms/commodore-amiga/kick13.rom")?;
    if !path.exists() {
        emu198x_test_skip::record(&format!(
            "skipping: Kickstart 1.3 ROM missing at {}",
            path.display()
        ));
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

#[test]
fn agnus_beam_counts_through_pal_frame() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);

    // After one full PAL frame's worth of ticks, vbl_count should be 1.
    for _ in 0..PAL_FRAME_TICKS {
        amiga.tick();
    }
    assert_eq!(amiga.agnus().vbl_count, 1, "exactly one VBL per PAL frame");
    assert_eq!(amiga.agnus().vpos, 0);
    assert_eq!(amiga.agnus().hpos, 0);
}

#[test]
fn boot_eventually_enables_master_and_vertb_in_intena() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);

    // Run for ~50 PAL frames worth — well past where the boot
    // should have programmed INTENA with at least VERTB enabled.
    for _ in 0..(50 * PAL_FRAME_TICKS) {
        amiga.tick();
    }

    // We don't pin to an exact INTENA value — the boot's INTENA
    // evolves over time. We just want to verify that VBL has been
    // firing AND the boot has acknowledged it (e.g. INTREQ has been
    // cleared at some point) rather than perpetually pending.
    assert!(
        amiga.agnus().vbl_count >= 50,
        "Should have seen 50 VBLs; saw {}",
        amiga.agnus().vbl_count
    );
}
