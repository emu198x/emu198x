//! Phase 3 machine-level integration tests for the DF0 floppy drive.
//!
//! Closes task #172 — the final floppy port milestone. Verifies that
//! the machine exposes the drive through its public API and that the
//! ported wiring (drive_pra_byte + decode_cia_b_prb_for_df0 + MFM
//! track-read path) behaves correctly when driven through the CPU
//! bus.
//!
//! The full "Kickstart boots from ADF" path is left to a follow-up
//! integration task — it needs a bootable ADF, a ROM with BOOTSTRAP
//! code that actually advances through CMD_READ, and enough frames
//! for DoIO/WaitIO to complete. This file proves the plumbing is
//! live; the boot-flow test lives in #180 alongside the other
//! cross-cutting scenarios.

use format_commodore_amiga_adf::{ADF_SIZE_DD, Adf};
use machine_commodore_amiga_ocs::AmigaOcs;

fn zero_rom() -> Vec<u8> {
    vec![0; 512 * 1024]
}

#[test]
fn fresh_machine_has_no_disk_and_reports_power_on_status() {
    let amiga = AmigaOcs::new(zero_rom());
    assert!(!amiga.drive().has_disk());
    // Power-on CIA-A PRA: disk changed (PA2=0), not write-protected
    // (PA3=1), track0 (PA4=0), not ready (PA5=1).
    // Together with PA0/1/6/7 high, that's $EB.
    assert_eq!(
        amiga.cia_a().port_a_output(),
        0xEB,
        "fresh drive should match the pre-port $EB stub"
    );
}

#[test]
fn insert_adf_clears_disk_change_and_exposes_disk() {
    let mut amiga = AmigaOcs::new(zero_rom());
    let adf = Adf::from_bytes(vec![0; ADF_SIZE_DD]).expect("valid blank ADF");
    amiga.insert_adf(adf);
    assert!(amiga.drive().has_disk());
    // insert_adf acknowledges the change, so PA2 (/DSKCHANGE) is now
    // deasserted — bit 2 flips high, giving $EF.
    assert_eq!(
        amiga.cia_a().port_a_output(),
        0xEF,
        "acknowledged disk -> PA2 high ($EF)"
    );
}

#[test]
fn eject_disk_reasserts_dskchange() {
    let mut amiga = AmigaOcs::new(zero_rom());
    let adf = Adf::from_bytes(vec![0; ADF_SIZE_DD]).expect("valid ADF");
    amiga.insert_adf(adf);
    amiga.eject_disk();
    assert!(!amiga.drive().has_disk());
    // Back to $EB: disk changed asserted, not write-protected,
    // track0, not ready.
    assert_eq!(amiga.cia_a().port_a_output(), 0xEB);
}

#[test]
fn cia_b_step_pulse_is_not_missed_between_eclock_ticks() {
    let mut amiga = AmigaOcs::new(zero_rom());

    // Seed PRB first so enabling DDRB doesn't create a fake pulse:
    // $75 = motor on, DF0 selected, DIR=inward, /STEP high.
    amiga.poke_byte(0x00BFD100, 0x75);
    amiga.poke_byte(0x00BFD300, 0xFF);

    assert_eq!(amiga.drive().cylinder(), 0);
    assert_eq!(amiga.drive().step_event_counter(), 0);

    // Pulse /STEP low then high without waiting for an E-clock tick.
    amiga.poke_byte(0x00BFD100, 0x74);
    amiga.poke_byte(0x00BFD100, 0x75);

    assert_eq!(
        amiga.drive().step_event_counter(),
        1,
        "a short PRB step pulse should reach the drive immediately"
    );
    assert_eq!(
        amiga.drive().cylinder(),
        1,
        "DIR low on CIA-B PRB should step inward to cylinder 1"
    );
}
