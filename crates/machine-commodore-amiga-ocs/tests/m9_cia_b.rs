//! M9: CIA-B stub.
//!
//! Per `wiki/decisions/amiga-restart-plan.md`. CIA-B is the second
//! 8520 CIA on the Amiga, accessed at `$BFD000+` even bytes. It
//! handles disk control (PRB drives /STEP, /SIDE, /DIR, /SEL0-3,
//! /MTR), the parallel port, and provides the second timer pair.
//! Its /IRQ wires to Paula's EXTER (INTREQ bit 13, level 6).
//!
//! M9 is a minimal stub: same Cia chip behavior as CIA-A, decoded at
//! the CIA-B address space, IRQ routed to EXTER. No disk control
//! peripheral wiring yet.

use machine_commodore_amiga_ocs::AmigaOcs;
use std::path::PathBuf;

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
fn cia_b_at_bfd000_even_bytes() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);

    // Write to CIA-B PRA ($BFD000) — should land in cia_b, not cia_a.
    // CIA-A PRA reset value is $FF (HRM: port latches reset to all-1s
    // so pull-ups float high); the Amiga ROM then writes bits 0-1
    // (OVL, LED) via DDRA soon after, but not before we run the test.
    amiga.poke_byte(0x00BFD000, 0x42);
    assert_eq!(amiga.cia_b().port_a_latch(), 0x42);
    assert_eq!(
        amiga.cia_a().port_a_latch(),
        0xFF,
        "CIA-A unchanged, still at reset default"
    );
}

#[test]
fn cia_b_irq_sets_intreq_exter() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);

    // Programme + start CIA-B timer A (same shape as CIA-A's test).
    amiga.poke_byte(0x00BFD400, 0x00); // TALO
    amiga.poke_byte(0x00BFD500, 0x01); // TAHI → latch = 256
    amiga.poke_byte(0x00BFDD00, 0x81); // ICR mask: enable TA
    amiga.poke_byte(0x00BFDE00, 0x19); // CRA: START | one-shot | LOAD

    for _ in 0..3000 {
        amiga.tick();
    }

    assert_ne!(
        amiga.intreq() & 0x2000,
        0,
        "INTREQ.EXTER (bit 13) should latch when CIA-B asserts /IRQ"
    );
}
