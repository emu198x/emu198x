//! 68000 autovectored interrupts on the Amiga.
//!
//! The Amiga uses the 68000's autovector mechanism: during the
//! InterruptAck bus cycle Gary asserts /VPA, and the CPU computes
//! the exception vector from the interrupted IPL as `24 + IPL`. The
//! resulting addresses are:
//!
//!   IPL 1 → vector 25 ($64) — TBE / DSKBLK / SOFT
//!   IPL 2 → vector 26 ($68) — PORTS  (CIA-A)
//!   IPL 3 → vector 27 ($6C) — COPER / VERTB / BLIT
//!   IPL 4 → vector 28 ($70) — AUDx
//!   IPL 5 → vector 29 ($74) — RBF / DSKSYN
//!   IPL 6 → vector 30 ($78) — EXTER  (CIA-B)
//!   IPL 7 → vector 31 ($7C) — NMI    (unused on stock A500)
//!
//! This test builds a synthetic 256 KiB ROM that:
//!   - Sets reset SSP/PC.
//!   - Provides a tiny boot routine that drops OVL (so chip RAM
//!     becomes addressable at low memory), populates the IPL-3
//!     autovector at $6C with the address of a handler, enables the
//!     master + VERTB bits in INTENA, fires INTREQ.VERTB, and then
//!     spins.
//!   - Provides a handler that increments a counter at $1000 and
//!     RTEs back.
//!
//! Then we tick the machine for a few CCKs and assert that the
//! counter has been bumped — proving the autovector returned the
//! right vector (without the fix this would land on $60 spurious
//! and the counter would stay at zero).

use machine_commodore_amiga_ocs::AmigaOcs;

/// Pack a u16 into a byte slice as big-endian at the given offset.
fn put_w(buf: &mut [u8], at: usize, val: u16) {
    buf[at] = (val >> 8) as u8;
    buf[at + 1] = val as u8;
}

/// Pack a u32 into a byte slice as big-endian at the given offset.
fn put_l(buf: &mut [u8], at: usize, val: u32) {
    buf[at] = (val >> 24) as u8;
    buf[at + 1] = (val >> 16) as u8;
    buf[at + 2] = (val >> 8) as u8;
    buf[at + 3] = val as u8;
}

/// Build the synthetic ROM. Returns a 256 KiB image mapped at
/// $FC0000-$FFFFFF (since AmigaOcs mirrors a 256 K image to fill the
/// 512 K window). ROM offset $0 corresponds to address $FC0000.
fn build_test_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 256 * 1024];

    // Reset vectors at the very start of the ROM (address $0 with
    // OVL=1 is read from ROM offset $0).
    put_l(&mut rom, 0x0000, 0x0000_8000); // initial SSP = $8000 (chip RAM)
    put_l(&mut rom, 0x0004, 0x00FC_0100); // initial PC = boot routine

    // Boot routine at ROM $100 / address $FC0100.
    let mut at = 0x0100;

    // 1. Drop OVL: write to CIA-A DDRA bit 0 (output) and PRA bit 0 = 0.
    //
    //    MOVE.B #$01, $00BFE201   ; DDRA bit 0 = output
    //      $13FC 0001 00BF E201   (10 bytes)
    put_w(&mut rom, at, 0x13FC); at += 2;
    put_w(&mut rom, at, 0x0001); at += 2;
    put_l(&mut rom, at, 0x00BF_E201); at += 4;

    //    MOVE.B #$00, $00BFE001   ; PRA bit 0 = 0 → OVL clears
    //      $13FC 0000 00BF E001
    put_w(&mut rom, at, 0x13FC); at += 2;
    put_w(&mut rom, at, 0x0000); at += 2;
    put_l(&mut rom, at, 0x00BF_E001); at += 4;

    // 2. Write the IPL-3 autovector to chip RAM at $6C. Once OVL
    //    clears, the exception-lookup read at $6C targets chip RAM,
    //    so we must install the handler address there before firing
    //    the interrupt.
    //
    //    MOVE.L #$00FC0200, $0000006C
    //      $23FC 00FC 0200 0000 006C   (10 bytes)
    put_w(&mut rom, at, 0x23FC); at += 2;
    put_l(&mut rom, at, 0x00FC_0200); at += 4;
    put_l(&mut rom, at, 0x0000_006C); at += 4;

    // 3. Lower SR interrupt mask so IPL ≥ 1 will fire.
    //    ANDI.W #$F8FF, SR
    //      $027C F8FF
    put_w(&mut rom, at, 0x027C); at += 2;
    put_w(&mut rom, at, 0xF8FF); at += 2;

    // 4. Enable INTENA master + VERTB ($C020 = SET, INTEN, VERTB).
    //    MOVE.W #$C020, $DFF09A
    //      $33FC C020 00DF F09A
    put_w(&mut rom, at, 0x33FC); at += 2;
    put_w(&mut rom, at, 0xC020); at += 2;
    put_l(&mut rom, at, 0x00DF_F09A); at += 4;

    // 5. Fire INTREQ.VERTB (IPL 3).
    //    MOVE.W #$8020, $DFF09C
    //      $33FC 8020 00DF F09C
    put_w(&mut rom, at, 0x33FC); at += 2;
    put_w(&mut rom, at, 0x8020); at += 2;
    put_l(&mut rom, at, 0x00DF_F09C); at += 4;

    // 6. Loop forever waiting for the handler to bump the counter.
    //    BRA.S * (i.e., branch to self)
    //      $60FE
    put_w(&mut rom, at, 0x60FE);

    // Handler at ROM $200 / address $FC0200.
    let mut at = 0x0200;

    // 1. Increment the long at $1000 (chip RAM, accessible after OVL
    //    cleared).
    //    ADDQ.L #1, $00001000
    //      $5279 0000 1000   (ADDQ.L #1, abs.L)
    //    Actually 5279 is ADDQ.W to (xxx).L. We need ADDQ.L which is
    //    52B9. Let me use that.
    //      $52B9 0000 1000
    put_w(&mut rom, at, 0x52B9); at += 2;
    put_l(&mut rom, at, 0x0000_1000); at += 4;

    // 2. Acknowledge the interrupt: clear INTREQ.VERTB.
    //    MOVE.W #$0020, $DFF09C   (CLEAR write — bit 15 = 0)
    put_w(&mut rom, at, 0x33FC); at += 2;
    put_w(&mut rom, at, 0x0020); at += 2;
    put_l(&mut rom, at, 0x00DF_F09C); at += 4;

    // 3. RTE.
    //    $4E73
    put_w(&mut rom, at, 0x4E73);

    rom
}

fn read_chip_long(amiga: &AmigaOcs, addr: u32) -> u32 {
    let b0 = u32::from(amiga.read_chip_ram_byte(addr));
    let b1 = u32::from(amiga.read_chip_ram_byte(addr + 1));
    let b2 = u32::from(amiga.read_chip_ram_byte(addr + 2));
    let b3 = u32::from(amiga.read_chip_ram_byte(addr + 3));
    (b0 << 24) | (b1 << 16) | (b2 << 8) | b3
}

#[test]
fn vertb_interrupt_lands_on_correct_autovector() {
    let rom = build_test_rom();
    let mut amiga = AmigaOcs::new(rom);

    // Plenty of CCKs for boot routine to set up + interrupt to fire +
    // handler to run + return.
    for _ in 0..10_000 {
        amiga.tick_cck();
    }

    // The handler increments the long at $0001000. With the autovector
    // fix returning vector (24 + IPL) the handler runs and bumps the
    // counter. Without the fix, IPL 3 returned vector 24 ($60 =
    // spurious interrupt), the wrong handler ran, and the counter
    // stays at zero.
    let counter = read_chip_long(&amiga, 0x1000);
    assert!(
        counter >= 1,
        "VERTB handler at autovector $6C should have run \
         (counter at $1000 = ${counter:08X})",
    );
}

/// Build a variant ROM that tests IPL 6 (EXTER, vector 30 = $78).
/// Identical structure to the VERTB test but fires EXTER via
/// INTREQ.EXTER ($2000) and installs the handler at $78 instead.
fn build_exter_test_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 256 * 1024];

    put_l(&mut rom, 0x0000, 0x0000_8000);
    put_l(&mut rom, 0x0004, 0x00FC_0100);

    let mut at = 0x0100usize;

    // Drop OVL (same as above).
    put_w(&mut rom, at, 0x13FC); at += 2;
    put_w(&mut rom, at, 0x0001); at += 2;
    put_l(&mut rom, at, 0x00BF_E201); at += 4;
    put_w(&mut rom, at, 0x13FC); at += 2;
    put_w(&mut rom, at, 0x0000); at += 2;
    put_l(&mut rom, at, 0x00BF_E001); at += 4;

    // Install IPL-6 autovector in chip RAM at $78.
    // MOVE.L #$00FC0200, $00000078
    put_w(&mut rom, at, 0x23FC); at += 2;
    put_l(&mut rom, at, 0x00FC_0200); at += 4;
    put_l(&mut rom, at, 0x0000_0078); at += 4;

    // Lower SR mask to allow IPL ≤ 6 (mask = 5).
    // ANDI.W #$F8FF, SR (mask = 0; easier than computing exact mask).
    put_w(&mut rom, at, 0x027C); at += 2;
    put_w(&mut rom, at, 0xF8FF); at += 2;

    // Enable INTENA master + EXTER ($E000 = SET + INTEN + EXTER).
    put_w(&mut rom, at, 0x33FC); at += 2;
    put_w(&mut rom, at, 0xE000); at += 2;
    put_l(&mut rom, at, 0x00DF_F09A); at += 4;

    // Fire INTREQ.EXTER ($A000 = SET + EXTER).
    put_w(&mut rom, at, 0x33FC); at += 2;
    put_w(&mut rom, at, 0xA000); at += 2;
    put_l(&mut rom, at, 0x00DF_F09C); at += 4;

    // Spin.
    put_w(&mut rom, at, 0x60FE);

    // Handler at ROM $200 — same as the VERTB handler, but it clears
    // INTREQ.EXTER before RTEing.
    let mut at = 0x0200usize;
    put_w(&mut rom, at, 0x52B9); at += 2;
    put_l(&mut rom, at, 0x0000_1000); at += 4;
    put_w(&mut rom, at, 0x33FC); at += 2;
    put_w(&mut rom, at, 0x2000); at += 2; // CLEAR EXTER
    put_l(&mut rom, at, 0x00DF_F09C); at += 4;
    put_w(&mut rom, at, 0x4E73);

    rom
}

#[test]
fn exter_interrupt_lands_on_correct_autovector() {
    let rom = build_exter_test_rom();
    let mut amiga = AmigaOcs::new(rom);

    for _ in 0..10_000 {
        amiga.tick_cck();
    }

    let counter = read_chip_long(&amiga, 0x1000);
    assert!(
        counter >= 1,
        "EXTER handler at autovector $78 should have run \
         (counter at $1000 = ${counter:08X})",
    );
}
