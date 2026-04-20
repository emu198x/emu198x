//! Paula 8364 Phase 2 machine-level integration tests.
//!
//! Verifies the port moved INTENA/INTREQ/ADKCON from the inline
//! chipset into Paula without behavioural regression. The chip-level
//! behaviour is covered in `crates/commodore-paula-8364-archive/tests/`;
//! these tests exercise the *wiring* through the Amiga custom-register
//! bus, the 68000 IPL line, and the CIA→Paula edge latches.

use machine_commodore_amiga_ocs::{AmigaOcs, IntSource};

fn zero_rom() -> Vec<u8> { vec![0; 512 * 1024] }

// ─── CPU custom-register access → Paula ────────────────────────────

#[test]
fn intena_write_lands_in_paula_and_intenar_read_round_trips() {
    let mut amiga = AmigaOcs::new(zero_rom());
    // Write INTENA at $DFF09A — SET INTEN + VERTB.
    amiga.poke_word(0x00DF_F09A, 0xC020);
    assert_eq!(amiga.intena(), 0x4020);

    // INTENAR at $DFF01C must read back the same value.
    assert_eq!(amiga.read_word(0x00DF_F01C), 0x4020);
}

#[test]
fn intreq_write_lands_in_paula_and_intreqr_read_round_trips() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F09C, 0x8040); // SET BLIT
    assert_eq!(amiga.intreq(), 0x0040);
    assert_eq!(amiga.read_word(0x00DF_F01E), 0x0040);
}

#[test]
fn adkcon_write_lands_in_paula_and_adkconr_read_round_trips() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F09E, 0x8100); // SET FAST
    assert_eq!(amiga.adkcon(), 0x0100);
    assert_eq!(amiga.read_word(0x00DF_F010), 0x0100);
}

// ─── INTENA clear-doesn't-touch-master guards read-modify-write ────

#[test]
fn intena_clear_of_one_source_leaves_master_untouched() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F09A, 0xC020); // SET INTEN + VERTB
    assert_eq!(amiga.intena(), 0x4020);
    amiga.poke_word(0x00DF_F09A, 0x0020); // CLEAR VERTB only
    assert_eq!(amiga.intena(), 0x4000, "master-enable must persist");
}

// ─── CIA /IRQ edges reach Paula's INTREQ ──────────────────────────

#[test]
fn cia_a_irq_edge_sets_intreq_ports() {
    let mut amiga = AmigaOcs::new(zero_rom());
    // Enable ICR.SP mask on CIA-A and inject a serial byte — that
    // raises CIA-A /IRQ level-sensitively. Paula edge-latches.
    amiga.poke_byte(0x00BF_ED01, 0x88); // ICR mask SET | SP bit
    amiga.cia_a_mut().receive_serial_byte(0);
    for _ in 0..20 {
        amiga.tick();
    }
    assert_ne!(amiga.intreq() & IntSource::Ports.mask(), 0);
}

#[test]
fn cia_b_flag_pulse_sets_intreq_exter() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_byte(0x00BF_DD00, 0x90); // CIA-B ICR: SET | FLG
    amiga.cia_b_mut().flag_falling_edge();
    for _ in 0..20 {
        amiga.tick();
    }
    assert_ne!(amiga.intreq() & IntSource::Exter.mask(), 0);
}

// ─── IPL reflects Paula state ──────────────────────────────────────

#[test]
fn master_enable_clear_keeps_cpu_ipl_at_zero() {
    let mut amiga = AmigaOcs::new(zero_rom());
    // Raise a high-priority source without enabling INTEN.
    amiga.poke_word(0x00DF_F09C, 0xA000); // SET EXTER
    // tick once to let compute_ipl propagate to the CPU.
    amiga.tick();
    assert_eq!(amiga.cpu().ipl, 0,
        "INTEN clear → IPL = 0 regardless of pending bits");
}

#[test]
fn master_enable_plus_source_raises_cpu_ipl_to_matching_level() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F09A, 0xE000); // SET INTEN + EXTER
    amiga.poke_word(0x00DF_F09C, 0xA000); // SET EXTER
    amiga.tick();
    assert_eq!(amiga.cpu().ipl, 6, "EXTER enabled + pending + INTEN → IPL 6");
}
