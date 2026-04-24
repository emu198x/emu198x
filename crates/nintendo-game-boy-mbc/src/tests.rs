//! MBC integration tests.

use super::*;

/// Build a test ROM where each 16 KiB bank's first byte equals the
/// bank index (and the second byte equals `bank ^ 0x55`, so we can
/// distinguish reads inside the bank from spurious bank-0 reads).
fn build_rom(banks: usize) -> Vec<u8> {
    let mut rom = vec![0xFF; banks * 0x4000];
    for bank in 0..banks {
        let base = bank * 0x4000;
        rom[base] = bank as u8;
        rom[base + 1] = (bank as u8) ^ 0x55;
    }
    rom
}

fn install_mbc1m_logos(rom: &mut [u8]) {
    const LOGO_OFFSET: usize = 0x0104;
    const REGION_SIZE: usize = 0x40000;
    const LOGO: [u8; 0x30] = [
        0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B, 0x03, 0x73, 0x00, 0x83, 0x00, 0x0C, 0x00,
        0x0D, 0x00, 0x08, 0x11, 0x1F, 0x88, 0x89, 0x00, 0x0E, 0xDC, 0xCC, 0x6E, 0xE6, 0xDD, 0xDD,
        0xD9, 0x99, 0xBB, 0xBB, 0x67, 0x63, 0x6E, 0x0E, 0xEC, 0xCC, 0xDD, 0xDC, 0x99, 0x9F, 0xBB,
        0xB9, 0x33, 0x3E,
    ];

    for region in 0..4 {
        let offset = region * REGION_SIZE + LOGO_OFFSET;
        rom[offset..offset + LOGO.len()].copy_from_slice(&LOGO);
    }
}

// -- ROM only ---------------------------------------------------------

#[test]
fn rom_only_reads_passthrough() {
    let rom = build_rom(2);
    let cart = Cartridge::new(rom, CartType::RomOnly, 0);
    assert_eq!(cart.read_rom(0x0000), 0x00);
    assert_eq!(cart.read_rom(0x4000), 0x01);
    assert_eq!(cart.read_rom(0x4001), 0x01 ^ 0x55);
}

#[test]
fn rom_only_writes_have_no_effect() {
    let rom = build_rom(2);
    let mut cart = Cartridge::new(rom, CartType::RomOnly, 0);
    cart.write_rom(0x2000, 0xFF);
    assert_eq!(cart.read_rom(0x4000), 0x01, "no banking on ROM-only carts");
}

#[test]
fn rom_only_with_ram_round_trips() {
    let mut cart = Cartridge::new(build_rom(2), CartType::RomOnly, 0x2000);
    cart.write_ram(0xA000, 0x42);
    assert_eq!(cart.read_ram(0xA000), 0x42);
}

// -- MBC1 -------------------------------------------------------------

#[test]
fn mbc1_default_bank_one_at_4000_window() {
    let cart = Cartridge::new(
        build_rom(8),
        CartType::Mbc1 {
            ram: true,
            battery: false,
        },
        0x2000,
    );
    assert_eq!(cart.read_rom(0x0000), 0x00, "fixed bank at $0000");
    assert_eq!(cart.read_rom(0x4000), 0x01, "default switchable bank is 1");
}

#[test]
fn mbc1_bank_zero_writes_become_one() {
    let mut cart = Cartridge::new(
        build_rom(8),
        CartType::Mbc1 {
            ram: true,
            battery: false,
        },
        0x2000,
    );
    cart.write_rom(0x2000, 0x00);
    assert_eq!(cart.read_rom(0x4000), 0x01, "writing 0 is treated as 1");
}

#[test]
fn mbc1_selects_low_5_bits_of_bank() {
    let mut cart = Cartridge::new(
        build_rom(8),
        CartType::Mbc1 {
            ram: true,
            battery: false,
        },
        0x2000,
    );
    cart.write_rom(0x2000, 0x05);
    assert_eq!(cart.read_rom(0x4000), 0x05);
}

#[test]
fn mbc1_secondary_bits_extend_rom_bank_in_rom_mode() {
    // 64 banks (1 MiB) ROM. Pick bank 0x21 (low 5 = 0x01, high 2 = 0b01).
    let mut cart = Cartridge::new(
        build_rom(64),
        CartType::Mbc1 {
            ram: true,
            battery: false,
        },
        0x2000,
    );
    cart.write_rom(0x2000, 0x01);
    cart.write_rom(0x4000, 0b01);
    assert_eq!(cart.read_rom(0x4000), 0x21);
}

#[test]
fn mbc1_large_fixed_window_wraps_to_physical_rom_size() {
    let mut cart = Cartridge::new(
        build_rom(8),
        CartType::Mbc1 {
            ram: false,
            battery: false,
        },
        0,
    );
    cart.write_rom(0x6000, 0x01); // advanced mode maps high bits into $0000 window
    cart.write_rom(0x4000, 0b01); // bank 32, which wraps to bank 0 on an 8-bank ROM
    assert_eq!(cart.read_rom(0x0000), 0x00);
}

#[test]
fn mbc1_advanced_mode_still_extends_switchable_rom_bank() {
    let mut cart = Cartridge::new(
        build_rom(64),
        CartType::Mbc1 {
            ram: true,
            battery: false,
        },
        0x8000,
    );
    cart.write_rom(0x6000, 0x01);
    cart.write_rom(0x2000, 0x01);
    cart.write_rom(0x4000, 0b01);
    assert_eq!(cart.read_rom(0x4000), 0x21);
}

#[test]
fn mbc1m_multicart_shifts_secondary_rom_bits_by_four() {
    let mut rom = build_rom(64);
    install_mbc1m_logos(&mut rom);
    let mut cart = Cartridge::new(
        rom,
        CartType::Mbc1 {
            ram: false,
            battery: false,
        },
        0,
    );

    cart.write_rom(0x2000, 0x01);
    cart.write_rom(0x4000, 0b01);
    assert_eq!(cart.read_rom(0x4000), 0x11);

    cart.write_rom(0x6000, 0x01);
    assert_eq!(cart.read_rom(0x0000), 0x10);
}

#[test]
fn mbc1_ram_disabled_reads_high() {
    let mut cart = Cartridge::new(
        build_rom(2),
        CartType::Mbc1 {
            ram: true,
            battery: false,
        },
        0x2000,
    );
    cart.write_ram(0xA000, 0x42); // disabled — write ignored
    assert_eq!(cart.read_ram(0xA000), 0xFF);
}

#[test]
fn mbc1_ram_enable_unlocks_round_trip() {
    let mut cart = Cartridge::new(
        build_rom(2),
        CartType::Mbc1 {
            ram: true,
            battery: false,
        },
        0x2000,
    );
    cart.write_rom(0x0000, 0x0A); // enable RAM
    cart.write_ram(0xA000, 0x42);
    assert_eq!(cart.read_ram(0xA000), 0x42);
    cart.write_rom(0x0000, 0x00); // disable
    assert_eq!(cart.read_ram(0xA000), 0xFF);
}

#[test]
fn mbc1_advanced_mode_picks_ram_bank() {
    let mut cart = Cartridge::new(
        build_rom(2),
        CartType::Mbc1 {
            ram: true,
            battery: false,
        },
        0x8000,
    );
    cart.write_rom(0x0000, 0x0A); // enable RAM
    cart.write_rom(0x6000, 0x01); // advanced (RAM-mode) banking
    cart.write_rom(0x4000, 0x02); // RAM bank 2
    cart.write_ram(0xA000, 0xDE);
    cart.write_rom(0x4000, 0x00);
    assert_eq!(
        cart.read_ram(0xA000),
        0xFF,
        "switching back to bank 0 hides the write"
    );
    cart.write_rom(0x4000, 0x02);
    assert_eq!(cart.read_ram(0xA000), 0xDE);
}

// -- MBC2 -------------------------------------------------------------

#[test]
fn mbc2_selects_four_bit_rom_bank() {
    let mut cart = Cartridge::new(build_rom(8), CartType::Mbc2 { battery: false }, 0);
    cart.write_rom(0x2100, 0x03); // A8=1: ROM bank select
    assert_eq!(cart.read_rom(0x4000), 0x03);
    cart.write_rom(0x2100, 0x00);
    assert_eq!(cart.read_rom(0x4000), 0x01, "bank 0 reads as bank 1");
}

#[test]
fn mbc2_ram_enable_uses_address_bit_8() {
    let mut cart = Cartridge::new(build_rom(8), CartType::Mbc2 { battery: false }, 0);
    cart.write_rom(0x0100, 0x0A); // A8=1: bank select, not RAM enable
    cart.write_ram(0xA000, 0x05);
    assert_eq!(cart.read_ram(0xA000), 0xFF);

    cart.write_rom(0x0000, 0x0A); // A8=0: RAM enable
    cart.write_ram(0xA000, 0x05);
    assert_eq!(cart.read_ram(0xA000), 0xF5);
}

#[test]
fn mbc2_internal_ram_uses_low_nibble_and_9_address_bits() {
    let mut cart = Cartridge::new(build_rom(8), CartType::Mbc2 { battery: false }, 0);
    cart.write_rom(0x0000, 0x0A);
    cart.write_ram(0xA1FF, 0xAB);
    assert_eq!(cart.read_ram(0xA1FF), 0xFB);
    assert_eq!(
        cart.read_ram(0xA3FF),
        0xFB,
        "MBC2 RAM mirrors every $200 bytes"
    );
}

// -- MBC3 -------------------------------------------------------------

#[test]
fn mbc3_selects_seven_bit_rom_bank() {
    let mut cart = Cartridge::new(
        build_rom(128),
        CartType::Mbc3 {
            ram: true,
            battery: false,
            rtc: false,
        },
        0x2000,
    );
    cart.write_rom(0x2000, 0x42);
    assert_eq!(cart.read_rom(0x4000), 0x42);
}

#[test]
fn mbc3_bank_zero_writes_become_one() {
    let mut cart = Cartridge::new(
        build_rom(8),
        CartType::Mbc3 {
            ram: true,
            battery: false,
            rtc: false,
        },
        0x2000,
    );
    cart.write_rom(0x2000, 0x00);
    assert_eq!(cart.read_rom(0x4000), 0x01);
}

#[test]
fn mbc3_rom_bank_wraps_to_physical_rom_size() {
    let mut cart = Cartridge::new(
        build_rom(8),
        CartType::Mbc3 {
            ram: false,
            battery: false,
            rtc: false,
        },
        0,
    );
    cart.write_rom(0x2000, 0x42);
    assert_eq!(cart.read_rom(0x4000), 0x02);
}

#[test]
fn mbc3_ram_bank_select() {
    let mut cart = Cartridge::new(
        build_rom(2),
        CartType::Mbc3 {
            ram: true,
            battery: false,
            rtc: false,
        },
        0x8000,
    );
    cart.write_rom(0x0000, 0x0A);
    cart.write_rom(0x4000, 0x02);
    cart.write_ram(0xA000, 0x99);
    cart.write_rom(0x4000, 0x00);
    assert_eq!(cart.read_ram(0xA000), 0xFF);
    cart.write_rom(0x4000, 0x02);
    assert_eq!(cart.read_ram(0xA000), 0x99);
}

#[test]
fn mbc3_rtc_register_round_trip() {
    let mut cart = Cartridge::new(
        build_rom(2),
        CartType::Mbc3 {
            ram: true,
            battery: true,
            rtc: true,
        },
        0x2000,
    );
    cart.write_rom(0x0000, 0x0A);
    cart.write_rom(0x4000, 0x08); // RTC seconds register
    cart.write_ram(0xA000, 30);
    // Latch the live values into the latched copy.
    cart.write_rom(0x6000, 0);
    cart.write_rom(0x6000, 1);
    assert_eq!(cart.read_ram(0xA000), 30);
}

#[test]
fn mbc3_without_rtc_ignores_rtc_bank_selects() {
    let mut cart = Cartridge::new(
        build_rom(2),
        CartType::Mbc3 {
            ram: true,
            battery: false,
            rtc: false,
        },
        0x2000,
    );
    cart.write_rom(0x0000, 0x0A);
    cart.write_rom(0x4000, 0x08); // would be RTC seconds — ignored
    cart.write_ram(0xA000, 0xAB);
    assert_eq!(cart.read_ram(0xA000), 0xFF);
}

// -- MBC5 -------------------------------------------------------------

#[test]
fn mbc5_low_byte_then_high_bit_for_512_banks() {
    let mut cart = Cartridge::new(
        build_rom(0x200),
        CartType::Mbc5 {
            ram: true,
            battery: false,
            rumble: false,
        },
        0x2000,
    );
    cart.write_rom(0x2000, 0x80); // low 8 bits
    cart.write_rom(0x3000, 0x01); // high bit → bank 0x180
    assert_eq!(cart.read_rom(0x4000), 0x80, "wrapped lower byte stored");
    // Read at offset 0 within bank 0x180 — its first byte = 0x80 (truncated u8).
}

#[test]
fn mbc5_bank_zero_is_selectable_unlike_mbc1() {
    let mut cart = Cartridge::new(
        build_rom(8),
        CartType::Mbc5 {
            ram: true,
            battery: false,
            rumble: false,
        },
        0x2000,
    );
    cart.write_rom(0x2000, 0x00);
    cart.write_rom(0x3000, 0x00);
    // Bank 0 in the switchable window (no MBC1-style rewrite to 1).
    assert_eq!(cart.read_rom(0x4000), 0x00);
}

#[test]
fn mbc5_rom_bank_wraps_to_physical_rom_size() {
    let mut cart = Cartridge::new(
        build_rom(4),
        CartType::Mbc5 {
            ram: false,
            battery: false,
            rumble: false,
        },
        0,
    );
    cart.write_rom(0x2000, 0x05);
    assert_eq!(cart.read_rom(0x4000), 0x01);
}

#[test]
fn mbc5_ram_bank_uses_4_bit_select() {
    let mut cart = Cartridge::new(
        build_rom(2),
        CartType::Mbc5 {
            ram: true,
            battery: false,
            rumble: false,
        },
        0x20000, // 128 KiB → 16 banks
    );
    cart.write_rom(0x0000, 0x0A);
    cart.write_rom(0x4000, 0x0F);
    cart.write_ram(0xA000, 0xCD);
    cart.write_rom(0x4000, 0x00);
    assert_eq!(cart.read_ram(0xA000), 0xFF);
    cart.write_rom(0x4000, 0x0F);
    assert_eq!(cart.read_ram(0xA000), 0xCD);
}
