//! Minimal NES boot test — verify reset vector read and $2002 VBlank polling.
//!
//! Builds a minimal NROM (mapper 0) test ROM as a byte array. The code:
//! 1. SEI, CLD, LDX #$FF, TXS (standard init)
//! 2. Poll $2002 for VBlank flag (bit 7) — twice, per standard NES init
//! 3. JMP to self (infinite loop)
//!
//! If the CPU reaches the infinite loop within 3 frames, the NES boots.

use std::path::Path;

use emu_nes::{capture, Nes, NesConfig};

/// Build a minimal NROM iNES ROM (32K PRG, 8K CHR).
fn build_minimal_rom() -> Vec<u8> {
    // 32K PRG ROM (2 banks × 16K), 8K CHR ROM (1 bank)
    let prg_size = 32768usize;
    let chr_size = 8192usize;
    let mut rom = vec![0u8; 16 + prg_size + chr_size];

    // iNES header
    rom[0..4].copy_from_slice(b"NES\x1a");
    rom[4] = 2; // 2 × 16K PRG banks = 32K
    rom[5] = 1; // 1 × 8K CHR bank
    rom[6] = 0; // Mapper 0, horizontal mirroring
    rom[7] = 0;

    // Code starts at $8000 (offset 16 in file, since PRG maps to $8000-$FFFF)
    // Code layout:
    // $8000: 78       SEI
    // $8001: D8       CLD
    // $8002: A2 FF    LDX #$FF
    // $8004: 9A       TXS
    // $8005: AD 02 20 LDA $2002     (vblank1)
    // $8008: 10 FB    BPL $8005     (loop until VBlank)
    // $800A: AD 02 20 LDA $2002     (vblank2)
    // $800D: 10 FB    BPL $800A     (loop until VBlank)
    // $800F: 4C 0F 80 JMP $800F     (idle loop)
    let code: &[u8] = &[
        0x78,       // SEI
        0xD8,       // CLD
        0xA2, 0xFF, // LDX #$FF
        0x9A,       // TXS
        // First VBlank wait: poll $2002 bit 7
        0xAD, 0x02, 0x20, // vblank1: LDA $2002
        0x10, 0xFB,       //          BPL vblank1
        // Second VBlank wait
        0xAD, 0x02, 0x20, // vblank2: LDA $2002
        0x10, 0xFB,       //          BPL vblank2
        // Infinite loop — test checks PC lands here ($800F)
        0x4C, 0x0F, 0x80, // idle: JMP $800F
    ];

    // Place code at beginning of PRG (maps to $8000)
    rom[16..16 + code.len()].copy_from_slice(code);

    // Reset vector at $FFFC → $8000 (offset within 32K PRG: $7FFC)
    rom[16 + 0x7FFC] = 0x00; // Low byte
    rom[16 + 0x7FFD] = 0x80; // High byte

    // NMI vector at $FFFA → RTI at some safe location
    rom[16 + 0x7FFA] = 0x00;
    rom[16 + 0x7FFB] = 0x80; // Points to SEI (harmless)

    // IRQ/BRK vector at $FFFE → RTI at some safe location
    rom[16 + 0x7FFE] = 0x00;
    rom[16 + 0x7FFF] = 0x80;

    rom
}

#[test]
#[ignore] // Slow: runs 3 full frames
fn test_boot_minimal() {
    let rom_data = build_minimal_rom();
    let mut nes = Nes::new(&NesConfig { rom_data }).expect("Failed to parse minimal ROM");

    println!("Reset: PC=${:04X}", nes.cpu().regs.pc);
    assert_eq!(nes.cpu().regs.pc, 0x8000, "Reset vector should point to $8000");

    // The idle loop is JMP $800F at $800F (3 bytes: $800F-$8011).
    // PC can be sampled mid-instruction, so accept any address within the JMP.
    // Two VBlank waits need ~2 frames. Run 5 to be safe.
    let idle_range = 0x800Fu16..=0x8011u16;

    for frame in 0..5 {
        let ticks = nes.run_frame();
        let pc = nes.cpu().regs.pc;
        let sp = nes.cpu().regs.s;
        println!("Frame {frame}: PC=${pc:04X} SP=${sp:02X} ticks={ticks}");

        if idle_range.contains(&pc) {
            println!("Reached idle loop at frame {frame}!");
            return;
        }
    }

    let final_pc = nes.cpu().regs.pc;
    assert!(
        idle_range.contains(&final_pc),
        "NES did not reach idle loop ($800F-$8011) within 5 frames, stuck at ${final_pc:04X}"
    );
}

/// Build an NROM ROM that writes "HELLO NES" to the background.
///
/// PRG: standard init → 2× VBlank wait → load palette → write nametable →
/// reset scroll → enable rendering → idle.
/// CHR: 7 hand-drawn 8×8 tiles (space, H, E, L, O, N, S) in pattern table 0.
fn build_hello_rom() -> Vec<u8> {
    let prg_size = 32768usize;
    let chr_size = 8192usize;
    let mut rom = vec![0u8; 16 + prg_size + chr_size];

    // iNES header
    rom[0..4].copy_from_slice(b"NES\x1a");
    rom[4] = 2; // 2 × 16K PRG banks = 32K
    rom[5] = 1; // 1 × 8K CHR bank
    rom[6] = 0; // Mapper 0, horizontal mirroring
    rom[7] = 0;

    // 6502 code at $8000 (file offset 16).
    //
    // $8000: SEI / CLD / LDX #$FF / TXS          ; standard init
    // $8005: LDA #$00 / STA $2001                 ; disable rendering
    // $800A: LDA $2002 / BPL $800A               ; VBlank wait 1
    // $800F: LDA $2002 / BPL $800F               ; VBlank wait 2
    // $8014: LDA $2002                            ; reset address latch
    // $8017: LDA #$3F / STA $2006                ; PPU addr high
    // $801C: LDA #$00 / STA $2006                ; PPU addr low ($3F00)
    // $8021: LDX #$00
    // $8023: LDA $805A,X / STA $2007 / INX / CPX #$04 / BNE $8023
    // $802E: LDA #$21 / STA $2006                ; nametable addr high
    // $8033: LDA #$CC / STA $2006                ; nametable addr low ($21CC = row 14 col 12)
    // $8038: LDX #$00
    // $803A: LDA $805E,X / STA $2007 / INX / CPX #$09 / BNE $803A
    // $8045: LDA #$00 / STA $2005 / STA $2005    ; scroll = (0, 0)
    // $804D: LDA #$1E / STA $2001                ; enable BG + sprites
    // $8052: LDA #$80 / STA $2000                ; NMI on, pattern table 0
    // $8057: JMP $8057                            ; idle
    // $805A: palette data (4 bytes)
    // $805E: text data (9 bytes)
    // $8067: RTI                                  ; NMI/IRQ handler
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Standard init
        0x78,                   // $8000  SEI
        0xD8,                   // $8001  CLD
        0xA2, 0xFF,             // $8002  LDX #$FF
        0x9A,                   // $8004  TXS
        // Disable rendering during setup
        0xA9, 0x00,             // $8005  LDA #$00
        0x8D, 0x01, 0x20,       // $8007  STA $2001
        // VBlank wait 1
        0xAD, 0x02, 0x20,       // $800A  LDA $2002
        0x10, 0xFB,             // $800D  BPL $800A
        // VBlank wait 2
        0xAD, 0x02, 0x20,       // $800F  LDA $2002
        0x10, 0xFB,             // $8012  BPL $800F
        // Reset PPU address latch
        0xAD, 0x02, 0x20,       // $8014  LDA $2002
        // Set PPU address to $3F00 (palette)
        0xA9, 0x3F,             // $8017  LDA #$3F
        0x8D, 0x06, 0x20,       // $8019  STA $2006
        0xA9, 0x00,             // $801C  LDA #$00
        0x8D, 0x06, 0x20,       // $801E  STA $2006
        // Write 4 palette bytes
        0xA2, 0x00,             // $8021  LDX #$00
        0xBD, 0x5A, 0x80,       // $8023  LDA $805A,X  (palette_data)
        0x8D, 0x07, 0x20,       // $8026  STA $2007
        0xE8,                   // $8029  INX
        0xE0, 0x04,             // $802A  CPX #$04
        0xD0, 0xF5,             // $802C  BNE $8023
        // Set PPU address to $21CC (nametable 0, row 14, col 12)
        0xA9, 0x21,             // $802E  LDA #$21
        0x8D, 0x06, 0x20,       // $8030  STA $2006
        0xA9, 0xCC,             // $8033  LDA #$CC
        0x8D, 0x06, 0x20,       // $8035  STA $2006
        // Write 9 tile indices ("HELLO NES")
        0xA2, 0x00,             // $8038  LDX #$00
        0xBD, 0x5E, 0x80,       // $803A  LDA $805E,X  (text_data)
        0x8D, 0x07, 0x20,       // $803D  STA $2007
        0xE8,                   // $8040  INX
        0xE0, 0x09,             // $8041  CPX #$09
        0xD0, 0xF5,             // $8043  BNE $803A
        // Reset scroll to (0, 0)
        0xA9, 0x00,             // $8045  LDA #$00
        0x8D, 0x05, 0x20,       // $8047  STA $2005
        0x8D, 0x05, 0x20,       // $804A  STA $2005
        // Enable rendering: BG + sprites, no left-column clipping
        0xA9, 0x1E,             // $804D  LDA #$1E
        0x8D, 0x01, 0x20,       // $804F  STA $2001
        // PPUCTRL: NMI on VBlank, pattern table 0 for BG
        0xA9, 0x80,             // $8052  LDA #$80
        0x8D, 0x00, 0x20,       // $8054  STA $2000
        // Idle
        0x4C, 0x57, 0x80,       // $8057  JMP $8057
        // Palette: $0F=black, $30=white, $10=gray, $00=dark gray
        0x0F, 0x30, 0x10, 0x00, // $805A  palette_data
        // Text: H=1 E=2 L=3 L=3 O=4 _=0 N=5 E=2 S=6
        0x01, 0x02, 0x03, 0x03, 0x04, 0x00, 0x05, 0x02, 0x06, // $805E text_data
        // NMI/IRQ handler
        0x40,                   // $8067  RTI
    ];

    rom[16..16 + code.len()].copy_from_slice(code);

    // Reset vector → $8000
    rom[16 + 0x7FFC] = 0x00;
    rom[16 + 0x7FFD] = 0x80;
    // NMI vector → $8067 (RTI)
    rom[16 + 0x7FFA] = 0x67;
    rom[16 + 0x7FFB] = 0x80;
    // IRQ vector → $8067 (RTI)
    rom[16 + 0x7FFE] = 0x67;
    rom[16 + 0x7FFF] = 0x80;

    // CHR ROM: 7 tiles × 16 bytes at pattern table 0.
    // NES tiles are 8×8, 2 bitplanes: 8 bytes plane 0 then 8 bytes plane 1.
    // Plane 0 has the pixel pattern, plane 1 is all zeros → palette index 1.
    let chr_offset = 16 + prg_size;
    #[rustfmt::skip]
    let tiles: &[&[u8; 8]] = &[
        // Tile 0: space (blank)
        &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        // Tile 1: H
        &[0x88, 0x88, 0x88, 0xF8, 0x88, 0x88, 0x88, 0x00],
        // Tile 2: E
        &[0xF8, 0x80, 0x80, 0xF0, 0x80, 0x80, 0xF8, 0x00],
        // Tile 3: L
        &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0xF8, 0x00],
        // Tile 4: O
        &[0x70, 0x88, 0x88, 0x88, 0x88, 0x88, 0x70, 0x00],
        // Tile 5: N
        &[0x88, 0xC8, 0xA8, 0x98, 0x88, 0x88, 0x88, 0x00],
        // Tile 6: S
        &[0x70, 0x88, 0x80, 0x70, 0x08, 0x88, 0x70, 0x00],
    ];
    for (i, tile) in tiles.iter().enumerate() {
        let base = chr_offset + i * 16;
        rom[base..base + 8].copy_from_slice(&tile[..]); // Bitplane 0
        // Bitplane 1 stays zero (already initialized)
    }

    rom
}

#[test]
#[ignore] // Slow: runs 10 frames with rendering
fn test_background_rendering() {
    let rom_data = build_hello_rom();
    let mut nes = Nes::new(&NesConfig { rom_data }).expect("Failed to parse hello ROM");

    // Run 10 frames: 2 for VBlank waits, 1+ for setup, rest for rendering.
    for frame in 0..10 {
        let ticks = nes.run_frame();
        let pc = nes.cpu().regs.pc;
        println!("Frame {frame}: PC=${pc:04X} ticks={ticks}");
    }

    // Verify CPU reached idle loop at $8057
    let pc = nes.cpu().regs.pc;
    let idle_range = 0x8057u16..=0x8059;
    assert!(
        idle_range.contains(&pc),
        "Expected idle loop at $8057-$8059, got PC=${pc:04X}"
    );

    let fb = nes.framebuffer();
    let fb_w = nes.framebuffer_width() as usize;

    // Background colour at (0, 0): palette $0F = NES black = 0xFF000000.
    // Tile 0 (space) covers entire nametable except where we wrote text.
    let bg_pixel = fb[0 * fb_w + 0];
    println!("Pixel (0,0) = 0x{bg_pixel:08X}");
    assert_eq!(
        bg_pixel, 0xFF000000,
        "Top-left pixel should be NES black ($0F), got 0x{bg_pixel:08X}"
    );

    // Foreground pixel inside "H" tile: row 14 of tiles = pixel row 112,
    // col 12 = pixel col 96. Top-left of "H" ($88 = bit 7 set) → palette
    // index 1 → colour $30 → ARGB 0xFFFFFEFF.
    let h_pixel = fb[112 * fb_w + 96];
    println!("Pixel (96,112) = 0x{h_pixel:08X}");
    assert_eq!(
        h_pixel, 0xFFFFFEFF,
        "Top-left of 'H' tile should be white ($30 = 0xFFFFFEFF), got 0x{h_pixel:08X}"
    );

    // Save screenshot for visual inspection (repo root's test_output/).
    let output_dir = Path::new("../../test_output");
    std::fs::create_dir_all(output_dir).ok();
    let screenshot_path = output_dir.join("nes_hello.png");
    capture::save_screenshot(&nes, &screenshot_path).expect("Failed to save screenshot");
    println!("Screenshot saved to {}", screenshot_path.display());
}
