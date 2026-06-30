//! CI-running Centronics-printer proof for the Acorn Atom (#699).
//!
//! The base Atom wires a parallel printer through a 6522 VIA at `$B800` (MAME
//! `atom.cpp`): port A carries the data byte and CA2 is the `/STROBE` that latches
//! it. Like `keyboard_scan.rs`, this drives the real hardware path from a
//! hand-assembled **synthetic** ROM, so CI covers the printer wiring without the
//! copyrighted MOS (`test-rom-policy` Tier 3).

use machine_acorn_atom::AcornAtom;

/// A 24 KB ROM whose reset vector ($FFFC) points at a program at $D000 that sets
/// VIA port A to all-outputs, puts CA2 in pulse-output (`/STROBE`) mode, then
/// writes two bytes to the port-A output register — each write pulses `/STROBE`
/// and latches the byte to the printer — before looping.
fn printer_rom() -> Vec<u8> {
    let mut rom = vec![0xEAu8; 0x6000]; // NOP fill
    let program = [
        0xA9, 0xFF, // LDA #$FF
        0x8D, 0x03, 0xB8, // STA $B803      DDRA = all outputs
        0xA9, 0x0A, // LDA #$0A
        0x8D, 0x0C, 0xB8, // STA $B80C      PCR: CA2 = pulse output (/STROBE)
        0xA9, 0x48, // LDA #'H'
        0x8D, 0x01, 0xB8, // STA $B801      ORA -> data + strobe
        0xA9, 0x49, // LDA #'I'
        0x8D, 0x01, 0xB8, // STA $B801      ORA -> data + strobe
        0x4C, 0x14, 0xD0, // JMP $D014      loop forever
    ];
    rom[0x3000..0x3000 + program.len()].copy_from_slice(&program); // $D000
    // Reset vector at $FFFC maps to rom[0x3000 + ($FFFC - $D000)] = rom[0x5FFC].
    rom[0x5FFC] = 0x00; // -> $D000
    rom[0x5FFD] = 0xD0;
    rom
}

#[test]
fn cpu_program_prints_through_the_via() {
    let mut sys = AcornAtom::new(printer_rom(), 0x0A00);

    // Reset resolves to $D000, the program configures the VIA and strobes two
    // bytes out to the Centronics port.
    sys.run_frame();
    assert_eq!(
        sys.take_printer_output(),
        vec![b'H', b'I'],
        "the two strobed bytes reach the printer"
    );

    // Draining leaves the buffer empty; the idle loop strobes nothing more.
    sys.run_frame();
    assert!(
        sys.take_printer_output().is_empty(),
        "no further bytes are latched once the program is looping"
    );
}
