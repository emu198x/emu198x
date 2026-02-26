//! Minimal NES boot test — verify reset vector read and $2002 VBlank polling.
//!
//! Builds a minimal NROM (mapper 0) test ROM as a byte array. The code:
//! 1. SEI, CLD, LDX #$FF, TXS (standard init)
//! 2. Poll $2002 for VBlank flag (bit 7) — twice, per standard NES init
//! 3. JMP to self (infinite loop)
//!
//! If the CPU reaches the infinite loop within 3 frames, the NES boots.

use emu_nes::{Nes, NesConfig};

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
