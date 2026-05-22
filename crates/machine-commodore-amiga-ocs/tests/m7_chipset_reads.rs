//! M7: chipset read fidelity.
//!
//! Per `knowledge/decisions/amiga-restart-plan.md`. Two specific
//! correctness fixes for register *reads*:
//!
//! 1. `VPOSR` (`$004`) and `VHPOSR` (`$006`) return the current beam
//!    position (vpos high byte; vpos low + hpos low). Boot reads
//!    these to time various waits.
//! 2. CIA-A `PRA` reads return the effective port-A line state:
//!    output bits = stored PRA, input bits = floating-high (1).
//!    Without this, the boot reads back zeroed input lines and
//!    misinterprets things like /CHNG, /TRK0, /RDY, /FIR0, /FIR1.

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_LINES, PAL_LINE_TICKS};
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
fn vhposr_reflects_beam_position() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);

    // Just past reset — beam should be at (0, 0).
    let vposr = amiga.read_word(0x00DFF004);
    let vhposr = amiga.read_word(0x00DFF006);
    // Both should be zero at reset.
    assert_eq!(vposr & 0xFF, 0, "VPOSR low byte (vpos[8]) should be 0");
    assert_eq!(vhposr, 0, "VHPOSR should be 0 at reset");

    // Tick part of a line. Agnus hpos advances 1 per CCK = 1 per
    // 2 master/4 ticks, so 100 ticks → hpos = 50.
    for _ in 0..100 {
        amiga.tick();
    }
    let vhposr = amiga.read_word(0x00DFF006);
    assert_eq!(vhposr & 0xFF, 50, "VHPOSR low byte (hpos) should be 50");
    assert_eq!(
        (vhposr >> 8) & 0xFF,
        0,
        "VHPOSR high byte (vpos low) should still be 0"
    );

    // Tick into the next line — one full line = PAL_LINE_TICKS ticks.
    let to_next_line_start = u64::from(PAL_LINE_TICKS) - 100;
    for _ in 0..to_next_line_start {
        amiga.tick();
    }
    let vhposr = amiga.read_word(0x00DFF006);
    assert_eq!(vhposr & 0xFF, 0, "after wrap, hpos should be 0");
    assert_eq!((vhposr >> 8) & 0xFF, 1, "vpos should be 1");

    // Tick to the middle of the frame — vpos should be at line N.
    let lines_to_advance = 100u64;
    for _ in 0..(lines_to_advance * u64::from(PAL_LINE_TICKS)) {
        amiga.tick();
    }
    let vhposr = amiga.read_word(0x00DFF006);
    assert_eq!(
        (vhposr >> 8) & 0xFF,
        101,
        "vpos should reach 101 ((1 + 100) line)"
    );

    // Run beyond 256 lines so the high bit of vpos goes into VPOSR.
    let lines_to_high = u64::from(PAL_FRAME_LINES) - 101 - 1;
    for _ in 0..(lines_to_high * u64::from(PAL_LINE_TICKS)) {
        amiga.tick();
    }
    let vposr = amiga.read_word(0x00DFF004);
    // VPOSR low bit (bit 0) is vpos[8]. We're at vpos = 311 (0x137 = 0b1_0011_0111).
    // So vpos[8] = 1.
    assert_eq!(
        vposr & 1,
        1,
        "VPOSR bit 0 (vpos[8]) should be 1 at vpos=311"
    );
}

#[test]
fn cia_a_pra_inputs_float_high() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);

    // Boot writes DDRA=$03 (bits 0+1 outputs) and PRA=$02 within
    // the first ~3M CCKs. Before that, all bits are inputs and
    // PRA reads the CIA-A input-pin default for an A500 with no
    // disk: $EB = 0b1110_1011 (/CHNG=0, /TK0=0; /RDY, /WPRO,
    // printer pins all high). See `AmigaOcs::with_slow_ram`.
    let pra_at_reset = amiga.read_word(0x00BFE001) & 0xFF;
    assert_eq!(
        pra_at_reset, 0xEB,
        "Before boot configures DDRA, PRA reads the empty-drive defaults"
    );

    // Drive past the boot's CIA-A setup.
    for _ in 0..3_000_000 {
        amiga.tick();
    }

    // Now DDRA=$03, PRA=$02 (boot's setup).
    // Effective read: bits 0+1 driven from PRA (binary 10), bits 2-7
    // from pa_input_lines = $EB (/CHNG=0, /TK0=0, others high).
    // (PRA & DDRA) | (input & !DDRA) = $02 | ($EB & $FC) = $02 | $E8 = $EA.
    let pra_after_setup = amiga.read_word(0x00BFE001) & 0xFF;
    assert_eq!(
        pra_after_setup, 0xEA,
        "After boot's DDRA=$03 + PRA=$02, effective read is $EA \
         (bits 0+1 = PRA, bits 2-7 = empty-drive input pins)"
    );
}
