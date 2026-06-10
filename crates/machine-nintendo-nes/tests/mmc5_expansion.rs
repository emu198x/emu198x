//! #22: MMC5 register reads in the cartridge expansion area
//! (`$4020-$5FFF`) must reach the mapper on the real CPU bus.
//!
//! The machine used to return open bus for the whole `$4020-$5FFF`
//! range and never call the mapper there, so MMC5's IRQ-status,
//! multiplier and ExRAM reads were dead even though the mapper
//! implemented them. This drives an actual 6502 program through the
//! machine and asserts the value the CPU reads back — the bus output,
//! not mapper state.

use format_nintendo_nes_ines::Mmc5;
use machine_nintendo_nes::Nes;

/// Build an MMC5 machine whose reset program reads the 8×8 multiplier
/// result through `$5205`/`$5206` and stores it to zero page.
///
/// With MMC5's power-on PRG mode 3, only `$E000-$FFFF` is forced ROM,
/// so an 8 KiB PRG image maps there and the reset vector points at its
/// start.
fn multiplier_probe_machine() -> Nes {
    // LDA $5205 / STA $00 / LDA $5206 / STA $01 / JMP self.
    let program = [
        0xAD, 0x05, 0x52, // LDA $5205  (multiplier low)
        0x8D, 0x00, 0x00, // STA $0000
        0xAD, 0x06, 0x52, // LDA $5206  (multiplier high)
        0x8D, 0x01, 0x00, // STA $0001
        0x4C, 0x0C, 0xE0, // JMP $E00C  (self-loop)
    ];

    let mut prg = vec![0u8; 8192];
    prg[..program.len()].copy_from_slice(&program);
    prg[0x1FFC] = 0x00; // reset vector low  → $E000
    prg[0x1FFD] = 0xE0; // reset vector high

    let mut nes = Nes::new(Box::new(Mmc5::new(prg, vec![0u8; 8192])));
    // Seed the multiplier: 200 × 200 = 40000 = 0x9C40. Writes already
    // worked before the fix; the read-back path is what was broken.
    nes.mapper.cpu_write(0x5205, 200);
    nes.mapper.cpu_write(0x5206, 200);
    nes
}

#[test]
fn cpu_reads_mmc5_multiplier_through_the_expansion_bus() {
    let mut nes = multiplier_probe_machine();

    // Reset (7 cycles) + the ~19-cycle program complete well inside
    // 900 PPU dots (~300 CPU cycles).
    for _ in 0..900 {
        nes.tick();
    }

    assert_eq!(
        nes.peek(0x0000),
        0x40,
        "$5205 low byte of the product must reach the CPU, not open bus"
    );
    assert_eq!(
        nes.peek(0x0001),
        0x9C,
        "$5206 high byte of the product must reach the CPU, not open bus"
    );
}
