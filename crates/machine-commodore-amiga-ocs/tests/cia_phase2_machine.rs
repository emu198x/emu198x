//! CIA-8520 Phase 2 machine-level integration tests.
//!
//! Covers tasks #111–#115 — the port-the-full-chip-into-the-machine
//! phase. Phase 1 characterisation tests (in `mos-cia-8520/tests`)
//! already exercise the chip in isolation. These tests exercise the
//! *wiring* — how the machine routes CIA state through the custom-
//! chip bus, the memory overlay line, and Paula's INTREQ latches.
//!
//! Kickstart ROM is loaded on the TOD alarm boot test; everything
//! else builds an `AmigaOcs` with no ROM dependency by pointing it at
//! an all-zero image (`AmigaOcs::new(vec![0; 512 * 1024])`).

use std::path::PathBuf;

use machine_commodore_amiga_ocs::{AmigaOcs, CiaExt, PAL_FRAME_TICKS};

fn zero_rom() -> Vec<u8> {
    vec![0; 512 * 1024]
}

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

// ─── #114 CIA-A wiring ────────────────────────────────────────────

#[test]
fn ovl_defaults_true_at_reset_mapping_rom_into_low_memory() {
    let amiga = AmigaOcs::new(zero_rom());
    assert!(amiga.memory().overlay(),
        "reset overlay must be high: DDRA bit 0 = input, PRA floats high");
    assert!(amiga.cia_a().ovl(), "CIA-A OVL() helper matches");
}

#[test]
fn ovl_drops_when_cpu_writes_ddra_output_and_pra_low() {
    let mut amiga = AmigaOcs::new(zero_rom());
    // Drive DDRA bit 0 as output.
    amiga.poke_byte(0x00BF_E201, 0x03);
    // Write PRA bit 0 = 0 → OVL asserts low → chip RAM at $0.
    amiga.poke_byte(0x00BF_E001, 0x00);
    assert!(!amiga.memory().overlay(), "OVL low must map chip RAM at $0");
    assert!(!amiga.cia_a().ovl());
}

#[test]
fn ovl_rises_again_when_cpu_writes_pra_bit0_high() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_byte(0x00BF_E201, 0x03); // DDRA = output
    amiga.poke_byte(0x00BF_E001, 0x00); // OVL low
    assert!(!amiga.memory().overlay());
    amiga.poke_byte(0x00BF_E001, 0x01); // OVL high again
    assert!(amiga.memory().overlay(),
        "overlay must track CIA-A PRA bit 0 after every write");
}

#[test]
fn cia_a_pra_reads_eb_on_reset_for_empty_drive_sense() {
    // HRM disk-subsystem sense pins: /RDY, /TK0, /WPRO, /CHNG on
    // PA5..PA2; defaults chosen so trackdisk reports "disk changed /
    // empty". PRA read (via memory) returns effective_port = latched
    // output bits OR floating input bits → $EB on reset.
    let amiga = AmigaOcs::with_slow_ram(zero_rom(), 0);
    let pa = amiga.cia_a().peek_register(0);
    assert_eq!(pa, 0xEB,
        "CIA-A PRA effective byte should be $EB so trackdisk sees /CHNG asserted");
}

#[test]
fn cia_a_sdr_receive_raises_sp_and_paula_ports_intreq() {
    // CIA-A SP pin is wired to keyboard in the Amiga; SDR-received
    // latches ICR bit 3 (SP). With ICR mask bit 3 set, /IRQ asserts.
    // Paula rising-edge-latches CIA-A /IRQ into INTREQ.PORTS (bit 3).
    let mut amiga = AmigaOcs::new(zero_rom());
    // Enable ICR.SP mask (bit 7 = SET, bit 3 = SP).
    amiga.poke_byte(0x00BF_ED01, 0x88);
    // Receive a byte on the shift register.
    amiga.cia_a_mut().receive_serial_byte(0x5A);
    // Step enough master/4 ticks to let the E-clock Paula-edge-latch
    // fire (one E-clock = 10 ticks).
    for _ in 0..20 {
        amiga.tick();
    }
    assert_ne!(
        amiga.intreq() & 0x0008,
        0,
        "SP-caused CIA-A /IRQ must latch INTREQ.PORTS (bit 3)"
    );
}

// ─── #111 TOD counter + alarm ─────────────────────────────────────

#[test]
fn cia_a_tod_alarm_latches_icr_when_counter_matches() {
    // Arm alarm at TOD=3 via CRB.ALARM=1 path, then pulse TOD three
    // times; ICR bit 2 should be set and (with mask) /IRQ asserted.
    let mut amiga = AmigaOcs::new(zero_rom());
    // CRB: set ALARM bit (bit 7) so TOD writes target alarm regs.
    amiga.poke_byte(0x00BF_EF01, 0x80);
    // Write alarm = $000003 (HI, MID, LO). 8520 order: writing HI halts
    // the counter; writing LO restarts it. Writing *alarm* regs does
    // not halt or restart.
    amiga.poke_byte(0x00BF_EA01, 0x00); // ALARM-HI
    amiga.poke_byte(0x00BF_E901, 0x00); // ALARM-MID
    amiga.poke_byte(0x00BF_E801, 0x03); // ALARM-LO
    // Clear CRB.ALARM so future TOD writes go to the counter.
    amiga.poke_byte(0x00BF_EF01, 0x00);
    // Arm ICR.ALARM mask (bit 2).
    amiga.poke_byte(0x00BF_ED01, 0x84);
    // Pulse TOD three times.
    amiga.cia_a_mut().tod_pulse();
    amiga.cia_a_mut().tod_pulse();
    amiga.cia_a_mut().tod_pulse();
    assert_eq!(amiga.cia_a().tod_counter(), 3);
    assert!(amiga.cia_a().irq_active(),
        "TOD match with ICR.ALARM unmasked must assert /IRQ");
    // ICR read clears; peek to see flags.
    assert_ne!(amiga.cia_a().peek_register(0x0D) & 0x04, 0,
        "ICR.ALARM flag must be latched");
}

#[test]
fn cia_a_tod_writes_to_msb_halt_the_counter() {
    // HRM: any TOD write to MSB halts; 8520 also halts on MID. Only a
    // write to LSB restarts.
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_byte(0x00BF_EA01, 0x00); // TODHI → halt
    amiga.cia_a_mut().tod_pulse();
    amiga.cia_a_mut().tod_pulse();
    assert_eq!(amiga.cia_a().tod_counter(), 0,
        "TOD is halted after TODHI write; pulses are ignored");
}

// ─── #112 SDR serial + SP/CNT ─────────────────────────────────────

#[test]
fn cia_a_spmode_bit_persists_through_cra_write() {
    // CRA bit 6 = SPMODE (0 = input, 1 = output). Machine must pass
    // the bit through to the chip unmodified via the memory decoder.
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_byte(0x00BF_EE01, 0x40); // CRA.SPMODE = 1
    assert_eq!(amiga.cia_a().cra() & 0x40, 0x40);
    amiga.poke_byte(0x00BF_EE01, 0x00);
    assert_eq!(amiga.cia_a().cra() & 0x40, 0);
}

#[test]
fn cia_a_sdr_write_and_read_back_roundtrips() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_byte(0x00BF_EC01, 0xA5); // SDR write
    assert_eq!(amiga.cia_a().sdr(), 0xA5,
        "SDR value must round-trip through the machine memory decoder");
}

// ─── #113 ICR edge cases ──────────────────────────────────────────

#[test]
fn cia_a_icr_set_clear_semantics_via_machine_bus() {
    let mut amiga = AmigaOcs::new(zero_rom());
    // SET: bit 7 = 1 → set bits 0 and 1 (TA, TB).
    amiga.poke_byte(0x00BF_ED01, 0x83);
    assert_eq!(amiga.cia_a().icr_mask() & 0x1F, 0x03);
    // CLEAR: bit 7 = 0 → clear bit 0 (TA).
    amiga.poke_byte(0x00BF_ED01, 0x01);
    assert_eq!(amiga.cia_a().icr_mask() & 0x1F, 0x02,
        "clear must remove only the masked bit");
}

#[test]
fn cia_a_icr_read_clears_status_flags() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.cia_a_mut().receive_serial_byte(0);
    // Reading $0D via the CIA bus must clear flags.
    let _ = amiga.cia_a_mut().read(0x0D);
    assert_eq!(amiga.cia_a().icr_status(), 0,
        "ICR read must clear latched status bits");
}

// ─── #115 CIA-B wiring ────────────────────────────────────────────

#[test]
fn cia_b_at_bfd000_even_bytes_isolated_from_cia_a() {
    let mut amiga = AmigaOcs::new(zero_rom());
    // CIA-B PRA write at $BFD000 (even) must land in CIA-B.
    amiga.poke_byte(0x00BF_D000, 0x42);
    assert_eq!(amiga.cia_b().port_a_latch(), 0x42);
    assert_eq!(amiga.cia_a().port_a_latch(), 0xFF,
        "CIA-A must be untouched — different decode space");
}

#[test]
fn cia_b_prb_output_drives_pin_read_back() {
    // Task #115: PRB is the floppy-control output byte. When DDRB is
    // fully output, every PRB write propagates to the read-back.
    let mut amiga = AmigaOcs::new(zero_rom());
    // DDRB = $FF (all output).
    amiga.poke_byte(0x00BF_D300, 0xFF);
    amiga.poke_byte(0x00BF_D100, 0x5A);
    let prb = amiga.cia_b().peek_register(1);
    assert_eq!(prb, 0x5A,
        "all-output DDRB means PRB write is visible on the read-back pin value");
}

#[test]
fn cia_b_flag_pin_falling_edge_latches_icr_and_drives_exter() {
    // FLAG on CIA-B is the floppy index pulse. A negative edge latches
    // ICR bit 4 (FLG). With the ICR mask set, CIA-B /IRQ asserts, and
    // Paula edge-latches it into INTREQ.EXTER (bit 13).
    let mut amiga = AmigaOcs::new(zero_rom());
    // Enable ICR.FLG mask on CIA-B (bit 7 = SET, bit 4 = FLG).
    amiga.poke_byte(0x00BF_DD00, 0x90);
    amiga.cia_b_mut().flag_falling_edge();
    for _ in 0..20 {
        amiga.tick();
    }
    assert_ne!(
        amiga.intreq() & 0x2000,
        0,
        "FLAG → ICR.FLG → CIA-B /IRQ → INTREQ.EXTER bit 13"
    );
}

// ─── #116 CIA-8520 Phase 3 — real-ROM integration proof ───────────

#[test]
fn tod_alarm_fires_during_a_long_real_kickstart_run() {
    // Phase-3-lite smoke test: run the real Kickstart, seed TOD + alarm
    // deterministically, and confirm VBL → TOD pulse → alarm latch
    // end-to-end on top of a real boot. We can't rely on TOD counting
    // from reset because Kickstart halts it during timer.device init
    // (HRM-correct: any TODHI/TODMID write halts the counter; only
    // TODLO restarts it).
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);
    for _ in 0..(120 * PAL_FRAME_TICKS) {
        amiga.tick();
    }

    // Arm the alarm at $000010 and TOD at $000000 — order matters:
    // write alarm regs first (CRB.ALARM=1), then clear CRB.ALARM and
    // seed TOD so the final write lands on TODLO which restarts it.
    amiga.cia_a_mut().write(0x0F, 0x80); // CRB.ALARM = 1
    amiga.cia_a_mut().write(0x0A, 0x00); // ALARM-HI
    amiga.cia_a_mut().write(0x09, 0x00); // ALARM-MID
    amiga.cia_a_mut().write(0x08, 0x10); // ALARM-LO (= $10)
    amiga.cia_a_mut().write(0x0F, 0x00); // CRB.ALARM = 0 → future writes hit counter
    amiga.cia_a_mut().write(0x0A, 0x00); // TOD-HI (halts)
    amiga.cia_a_mut().write(0x09, 0x00); // TOD-MID
    amiga.cia_a_mut().write(0x08, 0x00); // TOD-LO (restarts at 0)
    let _ = amiga.cia_a_mut().read(0x0D);    // clear any pending ICR
    amiga.cia_a_mut().write(0x0D, 0x84); // enable ICR.ALARM mask (bit 2)

    // Run 20 frames — 20 VBLs pulse TOD 20 times → exceeds the $10
    // match. Alarm flag must latch before the window closes.
    for _ in 0..(20 * PAL_FRAME_TICKS) {
        amiga.tick();
        if amiga.cia_a().peek_register(0x0D) & 0x04 != 0 {
            return;
        }
    }
    panic!(
        "CIA-A TOD alarm should have latched ICR.ALARM within 20 VBLs; \
         current TOD=${:06X}",
        amiga.cia_a().tod_counter()
    );
}
