//! Level-sensitive interrupt inputs + Paula edge latches.
//!
//! Two scenarios this covers, both previously "conservative/latent"
//! gaps:
//!
//! 1. VBL re-latch. Agnus drives `/VERTB` high for the whole
//!    vertical blanking interval (vpos 0..VBL_END_LINE). Paula
//!    edge-latches the rising edge into INTREQ.VERTB. If a VBL
//!    handler clears INTREQ.VERTB while blanking is still active,
//!    the bit re-latches on the next tick — real hardware sees the
//!    level still high and the edge detector wasn't reset because
//!    the input never went low.
//!
//! 2. CIA /IRQ re-latch suppression. CIA::irq_pending is
//!    level-sensitive (true while any unmasked ICR flag is set).
//!    Paula uses rising-edge detection to latch INTREQ.PORTS /
//!    INTREQ.EXTER. A handler that clears INTREQ.PORTS without
//!    reading the CIA ICR *should not* retrigger an interrupt: the
//!    CIA line stays low (flag still set), so no rising edge, so no
//!    new latch. Our previous edge-in-CIA model incorrectly
//!    re-latched; this test pins the Paula-edge behaviour.

use machine_commodore_amiga_ocs::AmigaOcs;

fn put_w(buf: &mut [u8], at: usize, val: u16) {
    buf[at] = (val >> 8) as u8;
    buf[at + 1] = val as u8;
}

fn put_l(buf: &mut [u8], at: usize, val: u32) {
    buf[at] = (val >> 24) as u8;
    buf[at + 1] = (val >> 16) as u8;
    buf[at + 2] = (val >> 8) as u8;
    buf[at + 3] = val as u8;
}

/// Synthetic ROM: reset vectors + a minimal boot that drops OVL,
/// configures the requested init, then spins.
fn rom_with_boot(configure: &[u16]) -> Vec<u8> {
    let mut rom = vec![0u8; 256 * 1024];
    put_l(&mut rom, 0x0000, 0x0000_8000);
    put_l(&mut rom, 0x0004, 0x00FC_0100);

    let mut at = 0x0100usize;

    // Drop OVL: DDRA bit 0 = output; PRA bit 0 = 0.
    put_w(&mut rom, at, 0x13FC);
    at += 2;
    put_w(&mut rom, at, 0x0001);
    at += 2;
    put_l(&mut rom, at, 0x00BF_E201);
    at += 4;
    put_w(&mut rom, at, 0x13FC);
    at += 2;
    put_w(&mut rom, at, 0x0000);
    at += 2;
    put_l(&mut rom, at, 0x00BF_E001);
    at += 4;

    // Emit the caller's configuration instructions.
    for &w in configure {
        put_w(&mut rom, at, w);
        at += 2;
    }

    // Spin forever.
    put_w(&mut rom, at, 0x60FE);

    rom
}

/// Run the Amiga until it reaches the spin loop at $FC01xx.
/// Returns false if PC never parked inside that range.
fn run_until_parked(amiga: &mut AmigaOcs, max_ticks: u64) -> bool {
    let mut last_pc = 0u32;
    let mut same_pc_count = 0u64;
    for _ in 0..max_ticks {
        amiga.tick();
        let pc = amiga.cpu().regs.pc;
        if pc == last_pc {
            same_pc_count += 1;
            if same_pc_count > 4 {
                return true;
            }
        } else {
            same_pc_count = 0;
            last_pc = pc;
        }
    }
    false
}

/// VBL re-latch: after the CPU clears INTREQ.VERTB during the
/// blanking window, the bit re-asserts because Agnus's /VERTB level
/// is still high.
#[test]
fn vbl_intreq_relatches_when_cleared_mid_blanking() {
    let mut amiga = AmigaOcs::new(rom_with_boot(&[]));

    // Tick until we're firmly inside blanking (vpos = 5, well before
    // VBL_END_LINE = 25). One PAL line is 454 ticks; 5 lines = 2270.
    for _ in 0..2270 {
        amiga.tick();
    }
    // Expect INTREQ.VERTB set by the latch.
    assert_ne!(
        amiga.intreq() & 0x0020,
        0,
        "INTREQ.VERTB should be latched during blanking"
    );

    // Clear INTREQ.VERTB as a handler would (write $0020 with bit
    // 15 = 0 → CLEAR semantics).
    amiga.poke_word(0x00DF_F09C, 0x0020);
    assert_eq!(amiga.intreq() & 0x0020, 0, "INTREQ.VERTB cleared by poke");

    // Advance a few more ticks — still inside blanking — and expect
    // the bit to re-latch on the next CCK boundary.
    for _ in 0..8 {
        amiga.tick();
    }
    assert_ne!(
        amiga.intreq() & 0x0020,
        0,
        "INTREQ.VERTB should re-latch while still inside blanking"
    );
}

/// Outside blanking, INTREQ.VERTB stays where the handler left it.
/// Once the handler clears it, the level (now low) prevents
/// re-latching until the next frame.
#[test]
fn vbl_intreq_stays_cleared_outside_blanking() {
    let mut amiga = AmigaOcs::new(rom_with_boot(&[]));

    // Tick past blanking (vpos > 25). 30 lines is safe.
    for _ in 0..(30 * 454) {
        amiga.tick();
    }
    // Any previously latched VBL should be there (from the frame
    // start); clear it now.
    amiga.poke_word(0x00DF_F09C, 0x0020);
    assert_eq!(amiga.intreq() & 0x0020, 0);

    // Tick for 100 more ticks (still mid-frame, not blanking).
    // INTREQ.VERTB must remain cleared — no edge available.
    for _ in 0..100 {
        amiga.tick();
    }
    assert_eq!(
        amiga.intreq() & 0x0020,
        0,
        "no re-latch outside blanking window"
    );
}

/// CIA /IRQ Paula edge latch: clearing INTREQ.PORTS without reading
/// the CIA's ICR must NOT cause the bit to reappear, because the CIA
/// line is still low (level sensitive), so Paula sees no new edge.
#[test]
fn cia_a_intreq_does_not_relatch_without_icr_read() {
    // Arm CIA-A Timer A to fire continuously, enable its ICR mask.
    let config = [
        // MOVE.B #$81, $00BFED01  — ICR write: SET timer-A enable.
        0x13FC, 0x0081, 0x00BF, 0xED01,
        // MOVE.B #$02, $00BFE401  — Timer A latch low = 2
        0x13FC, 0x0002, 0x00BF, 0xE401,
        // MOVE.B #$00, $00BFE501  — Timer A latch high = 0
        0x13FC, 0x0000, 0x00BF, 0xE501,
        // MOVE.B #$01, $00BFEE01  — CRA: START, continuous mode
        0x13FC, 0x0001, 0x00BF, 0xEE01,
    ];
    let mut amiga = AmigaOcs::new(rom_with_boot(&config));

    // Let boot complete and CIA fire at least once.
    assert!(run_until_parked(&mut amiga, 200_000));

    // Run extra ticks to guarantee at least one CIA underflow edge
    // has been latched into INTREQ.PORTS.
    let mut latched = false;
    for _ in 0..1000 {
        amiga.tick();
        if amiga.intreq() & 0x0008 != 0 {
            latched = true;
            break;
        }
    }
    assert!(
        latched,
        "INTREQ.PORTS should latch on first CIA-A /IRQ edge"
    );

    // Now clear INTREQ.PORTS WITHOUT reading the CIA ICR. The CIA
    // flag stays set, so /IRQ stays low, so no new rising edge.
    amiga.poke_word(0x00DF_F09C, 0x0008);
    assert_eq!(amiga.intreq() & 0x0008, 0, "INTREQ.PORTS cleared by poke");

    // Tick a bunch. INTREQ.PORTS must stay cleared until something
    // resets the CIA edge (e.g. handler reads ICR, or flag is
    // otherwise dismissed).
    for _ in 0..2000 {
        amiga.tick();
    }
    assert_eq!(
        amiga.intreq() & 0x0008,
        0,
        "INTREQ.PORTS must not relatch — CIA line never went high"
    );
}
