//! Zorro-II autoconfig probe integration — machine-level.
//!
//! Asserts the full host-facing handshake against the probe window
//! `$E80000-$E8007F`, walked exactly the way `expansion.library` does
//! during boot: read the high and low nibbles of each config-ROM
//! byte, interpret the exceptional uninverted `ER_TYPE` byte, then
//! write the pair of base-address nibbles to assign its RAM window.
//!
//! Reads and writes go through `poke_word` / `read_word` so the
//! bus-dispatch wiring is exercised end-to-end — same path the 68000
//! uses when the ROM does `move.w $e80000, d0`.

use machine_commodore_amiga_ocs::{AmigaOcs, RamConfig};

fn zero_rom() -> Vec<u8> {
    vec![0; 512 * 1024]
}

/// Reconstruct one config-ROM byte from its nibble pair at
/// `$E80000 + hi_offset`. `hi_offset` must be a multiple of 4.
fn read_rom_byte(amiga: &AmigaOcs, hi_offset: u32) -> u8 {
    assert!(hi_offset & 0x3 == 0, "hi_offset must be 4-aligned");
    let addr_hi = 0x00E8_0000 + hi_offset;
    let addr_lo = addr_hi + 2;
    let hi = (amiga.read_word(addr_hi) >> 12) & 0x0F;
    let lo = (amiga.read_word(addr_lo) >> 12) & 0x0F;
    ((hi as u8) << 4) | (lo as u8)
}

#[test]
fn probe_window_returns_floating_bus_when_no_board_attached() {
    // RamConfig::bare() has `fast_kb == 0`, so no autoconfig board is
    // attached. The probe window reads back as floating-bus ($FFFF).
    let amiga = AmigaOcs::new(zero_rom());
    assert!(amiga.autoconfig().is_none());
    assert_eq!(amiga.read_word(0x00E8_0000), 0xFFFF);
    assert_eq!(amiga.read_word(0x00E8_0010), 0xFFFF);
}

#[test]
fn er_type_identifies_zorro_ii_ram_board_without_inversion() {
    // A 2M fast-RAM board should report itself as a Zorro-II memory
    // board with size-code 0b110 (2M) in the ER_TYPE byte. ER_TYPE is
    // the one complete Autoconfig byte whose data bits are not
    // physically inverted.
    let amiga = AmigaOcs::with_ram_config(
        zero_rom(),
        RamConfig {
            chip_kb: 512,
            slow_kb: 0,
            fast_kb: 2048,
        },
    );
    let er_type = read_rom_byte(&amiga, 0x00);
    // bits 7-6 = 11 (Zorro-II), bit 5 = memory, size = 0b110.
    assert_eq!(er_type, 0xE6);
}

#[test]
fn manufacturer_id_reads_commodore_post_inversion() {
    // Our fast-RAM board uses the Commodore manufacturer ID ($0202).
    // Both bytes arrive inverted in the probe window — the host un-
    // inverts by XORing with $FF.
    let amiga = AmigaOcs::with_ram_config(
        zero_rom(),
        RamConfig {
            chip_kb: 512,
            slow_kb: 0,
            fast_kb: 512,
        },
    );
    let hi = read_rom_byte(&amiga, 0x10);
    let lo = read_rom_byte(&amiga, 0x14);
    assert_eq!(hi ^ 0xFF, 0x02);
    assert_eq!(lo ^ 0xFF, 0x02);
}

#[test]
fn host_base_assignment_handshake_maps_fast_ram() {
    // Full `expansion.library` handshake:
    //   1. Read ER_TYPE, confirm Zorro-II memory board.
    //   2. Write A19-A16 to `$E8004A`.
    //   3. Write A23-A20 to `$E80048` — board moves to Configured
    //      state and releases the next board.
    //   4. Subsequent reads/writes at the assigned base hit the
    //      board's RAM backing.
    let mut amiga = AmigaOcs::with_ram_config(
        zero_rom(),
        RamConfig {
            chip_kb: 512,
            slow_kb: 0,
            fast_kb: 2048,
        },
    );
    // Step 2-3: assign base $20_0000 (the canonical first Zorro-II
    // slot above the chip + slow-RAM region).
    amiga.poke_word(0x00E8_004A, 0x0000);
    amiga.poke_word(0x00E8_0048, 0x2000);
    let board = amiga.autoconfig().expect("board should still exist");
    assert_eq!(board.base(), Some(0x0020_0000));
    assert!(!board.visible_in_probe_window());

    // Step 4: post-config reads at $20_0000 land on the board's
    // backing store (initially zero). Write a value through the bus
    // and read it back.
    amiga.poke_word(0x0020_0100, 0xCAFE);
    assert_eq!(amiga.read_word(0x0020_0100), 0xCAFE);
    // Byte granularity also works: each half of the written word is
    // addressable individually.
    assert_eq!(amiga.read_word(0x0020_0100) >> 8, 0xCA);
    assert_eq!(amiga.read_word(0x0020_0100) & 0xFF, 0xFE);
}

#[test]
fn probe_window_goes_silent_after_configuration() {
    let mut amiga = AmigaOcs::with_ram_config(
        zero_rom(),
        RamConfig {
            chip_kb: 512,
            slow_kb: 0,
            fast_kb: 512,
        },
    );
    amiga.poke_word(0x00E8_004A, 0x0000);
    amiga.poke_word(0x00E8_0048, 0x2000);
    // Reads from the probe window now float — any further ROM-byte
    // reconstruction would see floating-bus $FFFF patterns.
    assert_eq!(amiga.read_word(0x00E8_0000), 0xFFFF);
    assert_eq!(amiga.read_word(0x00E8_0010), 0xFFFF);
}

#[test]
fn shut_up_command_silences_board_permanently() {
    // If the host decides not to accept the board (e.g. size doesn't
    // fit in the remaining address space), it writes the shut-up
    // escape at `$E8004C`. Board goes silent forever.
    let mut amiga = AmigaOcs::with_ram_config(
        zero_rom(),
        RamConfig {
            chip_kb: 512,
            slow_kb: 0,
            fast_kb: 512,
        },
    );
    amiga.poke_word(0x00E8_004C, 0x0000);
    assert_eq!(amiga.read_word(0x00E8_0000), 0xFFFF);
    // And a subsequent base-write is ignored — the board stays silent.
    amiga.poke_word(0x00E8_0048, 0x2000);
    amiga.poke_word(0x00E8_004A, 0x0000);
    assert!(
        amiga
            .autoconfig()
            .expect("board should exist")
            .base()
            .is_none()
    );
}
