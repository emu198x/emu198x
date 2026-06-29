//! CI-running boot + keyboard proof for the Acorn Atom (#370).
//!
//! The real-MOS boot/keyboard/cassette tests need a copyrighted 24 KB ROM and so
//! stay `#[ignore]` (per the `test-rom-policy` Tier 3 rule — copyrighted firmware
//! is never bundled, and CI provisions no ROM). This test exercises the *same*
//! path — reset, the 6502 executes from ROM, drives the 8255 to scan the
//! keyboard, and reads a pressed key — with a hand-assembled **synthetic** ROM,
//! so CI covers the boot + keyboard wiring without any ROM placement.

use machine_acorn_atom::{AcornAtom, AtomKey};

/// A 24 KB ROM whose reset vector ($FFFC) points at a tiny program at $D000 that
/// loops: drive keyboard column 6 on port A ($B000), read the row lines back on
/// port B ($B001), and store them at $0080. Column 6 is where `A` (matrix (6,3))
/// and the modifiers (SHIFT on port B bit 7) report.
fn scan_rom() -> Vec<u8> {
    let mut rom = vec![0xEAu8; 0x6000]; // NOP fill
    let program = [
        0xA9, 0x06, // LDA #$06       column 6
        0x8D, 0x00, 0xB0, // STA $B000      drive port A (column select)
        0xAD, 0x01, 0xB0, // LDA $B001      read port B (rows, active-low)
        0x85, 0x80, // STA $80        store the scan result
        0x4C, 0x00, 0xD0, // JMP $D000      loop forever
    ];
    rom[0x3000..0x3000 + program.len()].copy_from_slice(&program); // $D000
    // Reset vector at $FFFC maps to rom[0x3000 + ($FFFC - $D000)] = rom[0x5FFC].
    rom[0x5FFC] = 0x00; // -> $D000
    rom[0x5FFD] = 0xD0;
    rom
}

#[test]
fn cpu_program_scans_the_keyboard_through_the_8255() {
    let mut sys = AcornAtom::new(scan_rom(), 0x0A00);

    // Boots: the reset vector resolves to $D000 and the scan loop runs. With no
    // key down, every row line is high (active-low), so port B reads 0xFF.
    sys.run_frame();
    assert_eq!(sys.peek(0x0080), 0xFF, "idle keyboard scans all-high");

    // 'A' is at matrix (row 6, col 3): with column 6 driven, port B bit 3 drops.
    sys.press_key(AtomKey::A);
    sys.run_frame();
    assert_eq!(sys.peek(0x0080) & 0x08, 0, "'A' pulls port B bit 3 low");

    // SHIFT is read on port B bit 7, common to every column.
    sys.press_key(AtomKey::Shift);
    sys.run_frame();
    assert_eq!(sys.peek(0x0080) & 0x80, 0, "SHIFT pulls port B bit 7 low");

    // Releasing restores both lines high on the next scan.
    sys.release_key(AtomKey::A);
    sys.release_key(AtomKey::Shift);
    sys.run_frame();
    assert_eq!(sys.peek(0x0080), 0xFF, "release restores an all-high scan");
}
