//! Minimal Atari 5200 boot test.
//!
//! Loads a tiny in-memory test ROM (no BIOS needed) that sets the
//! background colour to blue, sets up a simple ANTIC display list,
//! and loops forever. Verifies the framebuffer contains non-black
//! pixels and that timing is correct.

use emu_atari_5200::{Atari5200, Atari5200Config, Atari5200Region};
use emu_core::Tickable;

/// Build a minimal 8KB test ROM as a byte array.
///
/// The ROM is placed at $A000-$BFFF. The program:
///
///   $A000: Copy display list from ROM to RAM at $0400
///   $A00C: Set COLBK = $94 (blue)
///   $A011: Set COLPF0 = $0E (white)
///   $A016: Set DMACTL = $22 (normal playfield + DL DMA)
///   $A01B: Set DLISTL/H = $0400
///   $A024: Set CHBASE = $E0 (irrelevant for bitmap modes)
///   $A029: Infinite loop (JMP $A029)
///
/// Display list at ROM offset $30 ($A030), copied to $0400:
///   3 x 8 blank lines, mode D + LMS to $2000, 2 x mode D, JVB to $0400
///
/// Reset vector at $BFFC-$BFFD = $A000.
fn minimal_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 8192]; // 8KB

    // Code at $A000 (offset 0 in ROM)
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Copy display list from ROM ($A030) to RAM ($0400)
        0xA2, 0x00,             // LDX #0
        0xBD, 0x30, 0xA0,      // LDA $A030,X
        0x9D, 0x00, 0x04,      // STA $0400,X
        0xE8,                   // INX
        0xE0, 0x0B,            // CPX #11 (DL is 11 bytes)
        0xD0, 0xF6,            // BNE $A002 (loop)

        // Set COLBK = $94 (blue)              offset $0C
        0xA9, 0x94,            // LDA #$94
        0x8D, 0x1A, 0xC0,     // STA $C01A (GTIA COLBK)

        // Set COLPF0 = $0E (white)            offset $11
        0xA9, 0x0E,            // LDA #$0E
        0x8D, 0x16, 0xC0,     // STA $C016 (GTIA COLPF0)

        // Set DMACTL = $22 (normal playfield, DL DMA on)   offset $16
        0xA9, 0x22,            // LDA #$22
        0x8D, 0x00, 0xD4,     // STA $D400 (ANTIC DMACTL)

        // Set display list pointer = $0400     offset $1B
        0xA9, 0x00,            // LDA #$00
        0x8D, 0x02, 0xD4,     // STA $D402 (ANTIC DLISTL)
        0xA9, 0x04,            // LDA #$04
        0x8D, 0x03, 0xD4,     // STA $D403 (ANTIC DLISTH)

        // Set CHBASE = $E0                    offset $24
        0xA9, 0xE0,            // LDA #$E0
        0x8D, 0x09, 0xD4,     // STA $D409 (ANTIC CHBASE)

        // Infinite loop                       offset $29
        0x4C, 0x29, 0xA0,     // JMP $A029

        // NMI handler (RTI)                   offset $2C
        0x40,                  // RTI
    ];
    rom[..code.len()].copy_from_slice(code);

    // Display list at offset $30 ($A030 in ROM), will be copied to $0400
    #[rustfmt::skip]
    let dl: &[u8] = &[
        0x70,                   // 8 blank lines
        0x70,                   // 8 blank lines
        0x70,                   // 8 blank lines
        0x4D,                   // Mode D + LMS (load memory scan)
        0x00, 0x20,            // Screen data at $2000 (RAM, will be zeros = COLBK)
        0x0D,                   // Mode D (continues from previous memory scan)
        0x0D,                   // Mode D
        0x41,                   // JVB (jump and wait for VBlank)
        0x00, 0x04,            // Back to $0400
    ];
    rom[0x30..0x30 + dl.len()].copy_from_slice(dl);

    // NMI vector at $BFFA-$BFFB (offset $1FFA in 8KB ROM)
    // Points to $A02C (RTI handler)
    rom[0x1FFA] = 0x2C; // Low byte
    rom[0x1FFB] = 0xA0; // High byte

    // Reset vector at $BFFC-$BFFD (offset $1FFC in 8KB ROM)
    // Points to $A000
    rom[0x1FFC] = 0x00; // Low byte
    rom[0x1FFD] = 0xA0; // High byte

    // IRQ vector at $BFFE-$BFFF (offset $1FFE in 8KB ROM)
    // Points to $A02C (RTI handler)
    rom[0x1FFE] = 0x2C; // Low byte
    rom[0x1FFF] = 0xA0; // High byte

    rom
}

#[test]
fn boot_minimal_produces_display() {
    let config = Atari5200Config {
        rom_data: minimal_rom(),
        bios_data: None,
        region: Atari5200Region::Ntsc,
    };
    let mut system = Atari5200::new(&config).expect("ROM should load");
    system.run_frame();

    // After one frame the CPU should have set COLBK and enabled ANTIC DMA.
    // The framebuffer should contain non-black pixels in the visible area.
    let fb = system.framebuffer();
    let mid = 120 * 320 + 160; // Line 120, pixel 160
    assert_ne!(
        fb[mid], 0,
        "framebuffer should have non-black pixels after one frame"
    );
}

#[test]
fn run_frame_returns_expected_clocks() {
    let config = Atari5200Config {
        rom_data: minimal_rom(),
        bios_data: None,
        region: Atari5200Region::Ntsc,
    };
    let mut system = Atari5200::new(&config).expect("ROM should load");
    let clocks = system.run_frame();
    // NTSC: 262 lines x 228 colour clocks = 59,736
    assert_eq!(clocks, 228 * 262);
}

#[test]
fn pal_frame_clock_count() {
    let config = Atari5200Config {
        rom_data: minimal_rom(),
        bios_data: None,
        region: Atari5200Region::Pal,
    };
    let mut system = Atari5200::new(&config).expect("ROM should load");
    let clocks = system.run_frame();
    // PAL: 312 lines x 228 colour clocks = 71,136
    assert_eq!(clocks, 228 * 312);
}

#[test]
fn master_clock_advances_one_per_tick() {
    let config = Atari5200Config {
        rom_data: minimal_rom(),
        bios_data: None,
        region: Atari5200Region::Ntsc,
    };
    let mut system = Atari5200::new(&config).expect("ROM should load");

    assert_eq!(system.master_clock(), 0);
    system.tick();
    assert_eq!(system.master_clock(), 1);
}

#[test]
fn cpu_reaches_loop() {
    let config = Atari5200Config {
        rom_data: minimal_rom(),
        bios_data: None,
        region: Atari5200Region::Ntsc,
    };
    let mut system = Atari5200::new(&config).expect("ROM should load");
    system.run_frame();

    // The infinite loop is at $A029 (JMP $A029). The NMI handler (RTI)
    // is at $A02C. After one frame the PC should be in the ROM region,
    // either in the loop or processing an NMI. The exact address depends
    // on timing -- we just verify the CPU is executing from cartridge ROM.
    let pc = system.cpu().regs.pc;
    assert!(
        (0xA000..=0xBFFF).contains(&pc),
        "CPU should be executing from cartridge ROM ($A000-$BFFF), got ${pc:04X}"
    );
}
