//! Minimal Atari 2600 boot test.
//!
//! Loads a tiny in-memory test ROM that sets the background colour to
//! blue and does WSYNC per line for one frame, then verifies the
//! framebuffer contains blue pixels.

use emu_atari_2600::{Atari2600, Atari2600Config, Atari2600Region};
use emu_core::Tickable;

/// Build a minimal 4KB test ROM as a byte array.
///
/// The program:
///   $F000: LDA #$9A      ; blue colour (NTSC palette)
///   $F002: STA $09        ; COLUBK = blue
///   $F004: STA $02        ; WSYNC (halt until end of line)
///   $F006: JMP $F004      ; loop: WSYNC every line
///
/// Reset vector at $FFFC = $F000.
fn minimal_rom() -> Vec<u8> {
    let mut rom = vec![0; 4096];

    // Code at offset $000 (maps to $F000 in last bank)
    let code: &[u8] = &[
        0xA9, 0x9A, // LDA #$9A
        0x85, 0x09, // STA $09 (COLUBK — zeropage mirrors TIA)
        0x85, 0x02, // STA $02 (WSYNC)
        0x4C, 0x04, 0xF0, // JMP $F004
    ];
    rom[..code.len()].copy_from_slice(code);

    // Reset vector at $FFFC (offset $0FFC in 4KB ROM)
    rom[0x0FFC] = 0x00; // Low byte → $F000
    rom[0x0FFD] = 0xF0; // High byte

    rom
}

#[test]
fn boot_minimal_produces_blue_screen() {
    let config = Atari2600Config {
        rom_data: minimal_rom(),
        region: Atari2600Region::Ntsc,
    };
    let mut system = Atari2600::new(&config).expect("ROM should load");

    // Run one frame (262 lines × 228 clocks = 59,736 colour clocks).
    system.run_frame();

    // After one frame the CPU should have executed and set COLUBK.
    // Check that some visible pixels are blue.
    let fb = system.framebuffer();

    // The ROM doesn't set VSYNC/VBLANK, so all lines get the background colour.
    // Pick a pixel in the middle of the visible area.
    let mid = 130 * 160 + 80; // Line 130, pixel 80
    let pixel = fb[mid];

    // NTSC palette index $9A >> 1 = $4D = 77
    let expected = atari_tia::NTSC_PALETTE[0x9A >> 1];
    assert_eq!(
        pixel, expected,
        "Expected blue (${expected:08X}), got ${pixel:08X} at framebuffer index {mid}"
    );
}

#[test]
fn master_clock_advances_one_per_tick() {
    let config = Atari2600Config {
        rom_data: minimal_rom(),
        region: Atari2600Region::Ntsc,
    };
    let mut system = Atari2600::new(&config).expect("ROM should load");

    assert_eq!(system.master_clock(), 0);
    system.tick();
    assert_eq!(system.master_clock(), 1);
}

#[test]
fn run_frame_returns_expected_clock_count() {
    let config = Atari2600Config {
        rom_data: minimal_rom(),
        region: Atari2600Region::Ntsc,
    };
    let mut system = Atari2600::new(&config).expect("ROM should load");

    let clocks = system.run_frame();
    // NTSC: 262 lines × 228 clocks = 59,736
    assert_eq!(clocks, 262 * 228);
}

#[test]
fn pal_frame_clock_count() {
    let config = Atari2600Config {
        rom_data: minimal_rom(),
        region: Atari2600Region::Pal,
    };
    let mut system = Atari2600::new(&config).expect("ROM should load");

    let clocks = system.run_frame();
    // PAL: 312 lines × 228 clocks = 71,136
    assert_eq!(clocks, 312 * 228);
}
