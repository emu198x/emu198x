//! Interrupt-source latching boundaries.
//!
//! Two scenarios are kept separate:
//!
//! 1. `VERTB` is a once-per-frame request at raster line zero. After
//!    software clears `INTREQ.VERTB`, the request stays clear until
//!    the next frame start; the vertical-blank interval is not itself
//!    a level-sensitive interrupt input.
//!
//! 2. CIA interrupt inputs are level-sensitive. `PORTS` and `EXTER`
//!    reassert after software clears them if the corresponding CIA
//!    still holds its shared active-low interrupt input asserted.
//!    Reading the CIA ICR releases the input and prevents reassertion.

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS, PAL_LINE_TICKS};

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

fn run_to_next_frame_start(amiga: &mut AmigaOcs) {
    let target = amiga.agnus().vbl_count + 1;
    for _ in 0..=PAL_FRAME_TICKS {
        if amiga.agnus().vbl_count == target {
            return;
        }
        amiga.tick();
    }
    assert_eq!(
        amiga.agnus().vbl_count,
        target,
        "beam should reach the next frame start"
    );
}

/// Clearing `VERTB` during vertical blank must hold until line zero of
/// the next frame.
#[test]
fn vertb_request_stays_clear_until_next_frame_start() {
    let mut amiga = AmigaOcs::new(rom_with_boot(&[]));

    // Arrange: enter line zero through a real frame wrap and observe
    // the once-per-frame request.
    run_to_next_frame_start(&mut amiga);
    assert_ne!(
        amiga.intreq() & 0x0020,
        0,
        "line zero should latch INTREQ.VERTB"
    );

    // Act: acknowledge it, then advance to line five while still
    // inside the PAL vertical-blank interval.
    amiga.poke_word(0x00DF_F09C, 0x0020);
    assert_eq!(amiga.intreq() & 0x0020, 0, "INTREQ.VERTB cleared by poke");
    for _ in 0..(5 * u64::from(PAL_LINE_TICKS)) {
        amiga.tick();
    }

    // Assert: vertical blank does not reassert the request. The next
    // frame start does.
    assert_eq!(
        amiga.intreq() & 0x0020,
        0,
        "VERTB must remain clear after acknowledgement"
    );
    run_to_next_frame_start(&mut amiga);
    assert_ne!(
        amiga.intreq() & 0x0020,
        0,
        "next line zero should latch VERTB"
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

fn tick_until_request(amiga: &mut AmigaOcs, mask: u16) {
    for _ in 0..20 {
        if amiga.intreq() & mask != 0 {
            return;
        }
        amiga.tick();
    }
    assert_ne!(
        amiga.intreq() & mask,
        0,
        "active interrupt input should set its Paula request"
    );
}

/// `PORTS` must reassert while CIA-A continues to hold `INT2*` low.
#[test]
fn cia_a_ports_reasserts_while_interrupt_input_remains_active() {
    let mut amiga = AmigaOcs::new(rom_with_boot(&[]));
    amiga.poke_byte(0x00BF_ED01, 0x88);
    amiga.cia_a_mut().receive_serial_byte(0x5A);
    tick_until_request(&mut amiga, 0x0008);

    amiga.poke_word(0x00DF_F09C, 0x0008);
    tick_until_request(&mut amiga, 0x0008);
}

/// Reading CIA-A ICR releases `INT2*`, so `PORTS` must stay clear.
#[test]
fn cia_a_icr_read_prevents_ports_reassertion() {
    let mut amiga = AmigaOcs::new(rom_with_boot(&[]));
    amiga.poke_byte(0x00BF_ED01, 0x88);
    amiga.cia_a_mut().receive_serial_byte(0x5A);
    tick_until_request(&mut amiga, 0x0008);

    let _ = amiga.cia_a_mut().read(0x0D);
    assert!(!amiga.cia_a().irq_active(), "ICR read should release INT2*");
    amiga.poke_word(0x00DF_F09C, 0x0008);
    for _ in 0..20 {
        amiga.tick();
    }
    assert_eq!(
        amiga.intreq() & 0x0008,
        0,
        "released INT2* must not reassert PORTS"
    );
}

/// `EXTER` must reassert while CIA-B continues to hold `INT6*` low.
#[test]
fn cia_b_exter_reasserts_while_interrupt_input_remains_active() {
    let mut amiga = AmigaOcs::new(rom_with_boot(&[]));
    amiga.poke_byte(0x00BF_DD00, 0x90);
    amiga.cia_b_mut().flag_falling_edge();
    tick_until_request(&mut amiga, 0x2000);

    amiga.poke_word(0x00DF_F09C, 0x2000);
    tick_until_request(&mut amiga, 0x2000);
}

/// Reading CIA-B ICR releases `INT6*`, so `EXTER` must stay clear.
#[test]
fn cia_b_icr_read_prevents_exter_reassertion() {
    let mut amiga = AmigaOcs::new(rom_with_boot(&[]));
    amiga.poke_byte(0x00BF_DD00, 0x90);
    amiga.cia_b_mut().flag_falling_edge();
    tick_until_request(&mut amiga, 0x2000);

    let _ = amiga.cia_b_mut().read(0x0D);
    assert!(!amiga.cia_b().irq_active(), "ICR read should release INT6*");
    amiga.poke_word(0x00DF_F09C, 0x2000);
    for _ in 0..20 {
        amiga.tick();
    }
    assert_eq!(
        amiga.intreq() & 0x2000,
        0,
        "released INT6* must not reassert EXTER"
    );
}
