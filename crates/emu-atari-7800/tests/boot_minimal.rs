//! Minimal Atari 7800 boot test.
//!
//! Loads a tiny in-memory test ROM (no BIOS needed) that sets the
//! MARIA background colour to blue, sets up a minimal DLL/DL in RAM,
//! enables DMA, and loops forever. Verifies the framebuffer contains
//! non-black pixels and that timing is correct.

use emu_atari_7800::{Atari7800, Atari7800Config, Atari7800Region};
use emu_core::Tickable;

/// Build a minimal 32KB test ROM as a byte array.
///
/// The ROM is placed at $8000-$FFFF. The program:
///
///   1. Builds a DLL in main RAM at $1900 (24 entries of 3 bytes each,
///      all pointing to an empty DL at $1A00).
///   2. Sets MARIA BACKGRND ($0020) to blue ($94).
///   3. Sets MARIA CTRL ($003C) to $80 (DMA enabled).
///   4. Sets MARIA DPPH ($002C) and DPPL ($0030) to point at $1900.
///   5. Infinite JMP loop.
fn minimal_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 32768]; // 32KB

    // Code at $8000 (offset 0 in ROM).
    #[rustfmt::skip]
    let code: &[u8] = &[
        // --- Build DLL in RAM at $1900 ---
        // Each DLL entry is 3 bytes: [control, DL_addr_hi, DL_addr_lo]
        // Control: bits 6-4 = height-1, bits 3-0 = offset, bit 7 = DLI.
        // $70 = height 8 (raw 7=0b111), no DLI, offset 0.
        // DL at $1A00 will be zeros (end marker: byte0=0, byte1 & 0x5F = 0).

        0xA2, 0x00,             // LDX #0              ; $8000
        // loop:
        0xA9, 0x70,             // LDA #$70            ; $8002: height=8, no DLI
        0x9D, 0x00, 0x19,      // STA $1900,X         ; $8004
        0xA9, 0x1A,             // LDA #$1A            ; $8007: DL addr high
        0x9D, 0x01, 0x19,      // STA $1901,X         ; $8009
        0xA9, 0x00,             // LDA #$00            ; $800C: DL addr low
        0x9D, 0x02, 0x19,      // STA $1902,X         ; $800E
        0xE8, 0xE8, 0xE8,      // INX * 3             ; $8011-$8013
        0xE0, 0x48,             // CPX #72             ; $8014: 24 entries * 3
        0xD0, 0xEA,             // BNE $8002           ; $8016

        // --- Set BACKGRND = blue ($94) ---
        0xA9, 0x94,             // LDA #$94            ; $8018
        0x8D, 0x20, 0x00,      // STA $0020           ; $801A (MARIA BACKGRND)

        // --- Set CTRL = $80 (DMA on) ---
        0xA9, 0x80,             // LDA #$80            ; $801D
        0x8D, 0x3C, 0x00,      // STA $003C           ; $801F (MARIA CTRL)

        // --- Set DLL pointer = $1900 ---
        0xA9, 0x19,             // LDA #$19            ; $8022
        0x8D, 0x2C, 0x00,      // STA $002C           ; $8024 (DPPH)
        0xA9, 0x00,             // LDA #$00            ; $8027
        0x8D, 0x30, 0x00,      // STA $0030           ; $8029 (DPPL)

        // --- Infinite loop ---
        0x4C, 0x2C, 0x80,      // JMP $802C           ; $802C
    ];
    rom[..code.len()].copy_from_slice(code);

    // NMI handler: RTI at $8100 (offset $0100 in ROM).
    rom[0x0100] = 0x40; // RTI

    // Reset vector at $FFFC (offset $7FFC in 32KB ROM) -> $8000.
    rom[0x7FFC] = 0x00;
    rom[0x7FFD] = 0x80;

    // NMI vector at $FFFA -> $8100.
    rom[0x7FFA] = 0x00;
    rom[0x7FFB] = 0x81;

    // IRQ vector at $FFFE -> $8100.
    rom[0x7FFE] = 0x00;
    rom[0x7FFF] = 0x81;

    rom
}

#[test]
fn boot_minimal_produces_blue_background() {
    let config = Atari7800Config {
        rom_data: minimal_rom(),
        region: Atari7800Region::Ntsc,
    };
    let mut system = Atari7800::new(&config).expect("ROM should load");
    system.run_frame();

    // After one frame the CPU should have set BACKGRND and enabled MARIA DMA.
    // The framebuffer should contain non-black pixels in the visible area.
    let fb = system.framebuffer();
    let mid = 120 * 320 + 160; // Line 120, pixel 160
    assert_ne!(
        fb[mid], 0,
        "framebuffer should have non-black pixels after one frame"
    );
}

#[test]
fn run_frame_ntsc_clock_count() {
    let config = Atari7800Config {
        rom_data: minimal_rom(),
        region: Atari7800Region::Ntsc,
    };
    let mut system = Atari7800::new(&config).expect("ROM should load");
    let clocks = system.run_frame();
    // NTSC: 263 lines x 228 colour clocks = 59,964
    assert_eq!(clocks, 228 * 263);
}

#[test]
fn run_frame_pal_clock_count() {
    let config = Atari7800Config {
        rom_data: minimal_rom(),
        region: Atari7800Region::Pal,
    };
    let mut system = Atari7800::new(&config).expect("ROM should load");
    let clocks = system.run_frame();
    // PAL: 313 lines x 228 colour clocks = 71,364
    assert_eq!(clocks, 228 * 313);
}

#[test]
fn master_clock_advances_one_per_tick() {
    let config = Atari7800Config {
        rom_data: minimal_rom(),
        region: Atari7800Region::Ntsc,
    };
    let mut system = Atari7800::new(&config).expect("ROM should load");

    assert_eq!(system.master_clock(), 0);
    system.tick();
    assert_eq!(system.master_clock(), 1);
}

#[test]
fn cpu_reaches_loop() {
    let config = Atari7800Config {
        rom_data: minimal_rom(),
        region: Atari7800Region::Ntsc,
    };
    let mut system = Atari7800::new(&config).expect("ROM should load");
    system.run_frame();

    // The infinite loop is at $802C (JMP $802C). The NMI handler (RTI)
    // is at $8100. After one frame the PC should be in the ROM region.
    let pc = system.cpu().regs.pc;
    assert!(
        (0x8000..=0xFFFF).contains(&pc),
        "CPU should be executing from cartridge ROM ($8000-$FFFF), got ${pc:04X}"
    );
}
