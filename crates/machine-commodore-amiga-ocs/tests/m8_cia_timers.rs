//! M8: CIA-A timers + ICR + CIA→Paula IRQ.
//!
//! Per `knowledge/decisions/amiga-restart-plan.md`. The KS 1.3 boot
//! stalls at INTENA=$202C (no master) at PC=$FC3132 because the
//! exec scheduler / library init waits for CIA timer interrupts
//! that never fire without timer behavior.
//!
//! CIA-A clock = E clock = master/10 = 1 tick per 10 CCKs.
//!
//! Timer A behaviour:
//!   - CRA bit 0 (START) = 1 → counter decrements on each CIA tick
//!   - Underflow → ICR bit 0 (TA) is set
//!   - Continuous mode (CRA bit 3 = 0) → reload from latch on
//!     underflow; one-shot (bit 3 = 1) → stop after underflow
//!
//! ICR ($D):
//!   - Read returns IDR (data) | $80-if-IR-pending; CLEARS IDR
//!   - Write programs IMR (mask): bit 7 = SET if 1, CLEAR if 0
//!     bits 0-4 are the bits to set/clear in the mask
//!
//! When (IDR & IMR) != 0, the CIA asserts /IRQ → Paula INTREQ.PORTS
//! (bit 3) latches.

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
fn timer_a_one_shot_underflow_fires_icr() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);

    // Programme timer A: latch = 256 ($0100), enable TA in IMR,
    // then start in one-shot mode + force load.
    amiga.poke_byte(0x00BFE401, 0x00); // TALO = 0
    amiga.poke_byte(0x00BFE501, 0x01); // TAHI = 1 → latch = $0100 = 256
    amiga.poke_byte(0x00BFED01, 0x81); // ICR write: SET bit 0 (TA mask)
    amiga.poke_byte(0x00BFEE01, 0x19); // CRA: START | RUNMODE=one-shot | LOAD

    // Tick enough CCKs for 256 CIA ticks (= 2560 CCKs).
    for _ in 0..3000 {
        amiga.tick();
    }

    // ICR read should now return bit 0 (TA) set + bit 7 (IR pending).
    // Reading clears the IDR, so subsequent read returns 0.
    let icr_first = amiga.cpu_read_word(0x00BFED01) & 0xFF;
    assert_eq!(
        icr_first, 0x81,
        "ICR should report TA underflow (bit 0) + IR pending (bit 7)"
    );
    let icr_second = amiga.cpu_read_word(0x00BFED01) & 0xFF;
    assert_eq!(icr_second, 0, "ICR should clear after read");
}

#[test]
fn cia_a_irq_sets_intreq_ports_bit() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);

    // Programme + start timer A as above.
    amiga.poke_byte(0x00BFE401, 0x00);
    amiga.poke_byte(0x00BFE501, 0x01);
    amiga.poke_byte(0x00BFED01, 0x81);
    amiga.poke_byte(0x00BFEE01, 0x19);

    // Run timer to underflow.
    for _ in 0..3000 {
        amiga.tick();
    }

    // INTREQ.PORTS (bit 3) should be set.
    assert_ne!(
        amiga.intreq() & 0x0008,
        0,
        "INTREQ.PORTS should latch when CIA-A asserts /IRQ"
    );
}

#[test]
fn cia_a_timer_b_underflow_sets_intreq_ports() {
    // Timer B variant of the IRQ-chain test. This is the path
    // timer.device's UNIT_MICROHZ uses: CIA-A Timer B in one-shot
    // mode, ICR mask bit 1 (TB) set. When the underflow happens,
    // CIA /IRQ → Paula level-sensitive input → INTREQ.PORTS.
    //
    // If this test passes, our CIA→Paula→INTREQ chain is correct
    // and the MICROHZ-never-fires problem is entirely in what the
    // ROM writes (not in how we respond to those writes).
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);

    amiga.poke_byte(0x00BFE601, 0x00); // TBLO = 0
    amiga.poke_byte(0x00BFE701, 0x01); // TBHI = 1 → latch = $0100 = 256
    amiga.poke_byte(0x00BFED01, 0x82); // ICR write: SET bit 1 (TB mask)
    amiga.poke_byte(0x00BFEF01, 0x19); // CRB: START | ONE-SHOT | LOAD

    for _ in 0..3000 {
        amiga.tick();
    }

    assert_ne!(
        amiga.intreq() & 0x0008,
        0,
        "INTREQ.PORTS should latch on Timer B underflow too"
    );
}
