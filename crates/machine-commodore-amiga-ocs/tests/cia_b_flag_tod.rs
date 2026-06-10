//! #29: CIA-B FLAG (floppy /INDEX) + TOD (/HSYNC) wiring.
//!
//! On the Amiga, CIA-B's FLAG pin is the floppy index pulse and its TOD
//! pin is /HSYNC. Both were unwired: the drive's index pulse from
//! `drive.tick()` was discarded, and CIA-B TOD never ticked. These tests
//! pin the two connections.

use format_commodore_amiga_adf::{ADF_SIZE_DD, Adf};
use machine_commodore_amiga_ocs::AmigaOcs;

const ICR_FLAG: u8 = 0x10; // CIA ICR bit 4 = FLAG.

/// A ROM whose reset vector parks the CPU in a `BRA.S *` self-loop, so it
/// never writes CIA registers (which would halt TOD or clear ICR) while
/// we observe the chip-driven wiring.
fn parked_cpu_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 256 * 1024];
    rom[0..4].copy_from_slice(&0x0008_0000u32.to_be_bytes()); // initial SSP
    rom[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes()); // initial PC
    rom[8] = 0x60; // BRA.S
    rom[9] = 0xFE; // -2 → branch to self
    rom
}

#[test]
fn cia_b_tod_ticks_once_per_scanline() {
    let mut amiga = AmigaOcs::new(parked_cpu_rom());
    assert_eq!(amiga.cia_b().tod_counter(), 0, "TOD starts at zero");

    // Independently count scanline starts (the beam wrapping hpos to 0)
    // and confirm CIA-B TOD — wired to /HSYNC — tracks them exactly.
    let mut lines = 0u32;
    let mut prev_hpos = amiga.agnus().hpos;
    for _ in 0..20_000 {
        amiga.tick();
        let hpos = amiga.agnus().hpos;
        if hpos == 0 && prev_hpos != 0 {
            lines += 1;
        }
        prev_hpos = hpos;
    }

    assert!(
        lines > 10,
        "should have crossed several scanlines (got {lines})"
    );
    assert_eq!(
        amiga.cia_b().tod_counter(),
        lines,
        "CIA-B TOD must tick once per /HSYNC scanline"
    );
}

#[test]
#[ignore = "spins the drive a full revolution (~5M machine ticks); run with --include-ignored"]
fn cia_b_flag_raised_by_floppy_index_pulse() {
    let mut amiga = AmigaOcs::new(parked_cpu_rom());
    amiga.insert_adf(Adf::from_bytes(vec![0; ADF_SIZE_DD]).expect("valid blank ADF"));
    // CIA-B drive-control pins are outputs; the OS sets DDRB = $FF. Then
    // PRB ($BFD100) = $75 → motor on + DF0 selected. A parked CPU leaves
    // both set.
    amiga.poke_byte(0x00BF_D300, 0xFF);
    amiga.poke_byte(0x00BF_D100, 0x75);

    assert_eq!(
        amiga.cia_b().icr_status() & ICR_FLAG,
        0,
        "FLAG starts clear"
    );

    // Run until the spinning drive emits its first /INDEX pulse, which the
    // wiring routes to CIA-B FLAG. Motor spin-up + one full revolution.
    let mut fired = false;
    for _ in 0..6_000_000u32 {
        amiga.tick();
        if amiga.cia_b().icr_status() & ICR_FLAG != 0 {
            fired = true;
            break;
        }
    }
    assert!(
        fired,
        "floppy /INDEX pulse must latch CIA-B FLAG (ICR bit 4)"
    );
}
