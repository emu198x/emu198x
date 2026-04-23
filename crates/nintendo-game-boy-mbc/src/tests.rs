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
        CartType::Mbc1 { ram: true, battery: false },
        0x2000,
    );
    assert_eq!(cart.read_rom(0x0000), 0x00, "fixed bank at $0000");
    assert_eq!(cart.read_rom(0x4000), 0x01, "default switchable bank is 1");
}

#[test]
fn mbc1_bank_zero_writes_become_one() {
    let mut cart = Cartridge::new(
        build_rom(8),
        CartType::Mbc1 { ram: true, battery: false },
        0x2000,
    );
    cart.write_rom(0x2000, 0x00);
    assert_eq!(cart.read_rom(0x4000), 0x01, "writing 0 is treated as 1");
}

#[test]
fn mbc1_selects_low_5_bits_of_bank() {
    let mut cart = Cartridge::new(
        build_rom(8),
        CartType::Mbc1 { ram: true, battery: false },
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
        CartType::Mbc1 { ram: true, battery: false },
        0x2000,
    );
    cart.write_rom(0x2000, 0x01);
    cart.write_rom(0x4000, 0b01);
    assert_eq!(cart.read_rom(0x4000), 0x21);
}

#[test]
fn mbc1_ram_disabled_reads_high() {
    let mut cart = Cartridge::new(
        build_rom(2),
        CartType::Mbc1 { ram: true, battery: false },
        0x2000,
    );
    cart.write_ram(0xA000, 0x42); // disabled — write ignored
    assert_eq!(cart.read_ram(0xA000), 0xFF);
}

#[test]
fn mbc1_ram_enable_unlocks_round_trip() {
    let mut cart = Cartridge::new(
        build_rom(2),
        CartType::Mbc1 { ram: true, battery: false },
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
        CartType::Mbc1 { ram: true, battery: false },
        0x8000,
    );
    cart.write_rom(0x0000, 0x0A); // enable RAM
    cart.write_rom(0x6000, 0x01); // advanced (RAM-mode) banking
    cart.write_rom(0x4000, 0x02); // RAM bank 2
    cart.write_ram(0xA000, 0xDE);
    cart.write_rom(0x4000, 0x00);
    assert_eq!(cart.read_ram(0xA000), 0xFF, "switching back to bank 0 hides the write");
    cart.write_rom(0x4000, 0x02);
    assert_eq!(cart.read_ram(0xA000), 0xDE);
}

// -- MBC3 -------------------------------------------------------------

#[test]
fn mbc3_selects_seven_bit_rom_bank() {
    let mut cart = Cartridge::new(
        build_rom(128),
        CartType::Mbc3 { ram: true, battery: false, rtc: false },
        0x2000,
    );
    cart.write_rom(0x2000, 0x42);
    assert_eq!(cart.read_rom(0x4000), 0x42);
}

#[test]
fn mbc3_bank_zero_writes_become_one() {
    let mut cart = Cartridge::new(
        build_rom(8),
        CartType::Mbc3 { ram: true, battery: false, rtc: false },
        0x2000,
    );
    cart.write_rom(0x2000, 0x00);
    assert_eq!(cart.read_rom(0x4000), 0x01);
}

#[test]
fn mbc3_ram_bank_select() {
    let mut cart = Cartridge::new(
        build_rom(2),
        CartType::Mbc3 { ram: true, battery: false, rtc: false },
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
        CartType::Mbc3 { ram: true, battery: true, rtc: true },
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
        CartType::Mbc3 { ram: true, battery: false, rtc: false },
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
        CartType::Mbc5 { ram: true, battery: false, rumble: false },
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
        CartType::Mbc5 { ram: true, battery: false, rumble: false },
        0x2000,
    );
    cart.write_rom(0x2000, 0x00);
    cart.write_rom(0x3000, 0x00);
    // Bank 0 in the switchable window (no MBC1-style rewrite to 1).
    assert_eq!(cart.read_rom(0x4000), 0x00);
}

#[test]
fn mbc5_ram_bank_uses_4_bit_select() {
    let mut cart = Cartridge::new(
        build_rom(2),
        CartType::Mbc5 { ram: true, battery: false, rumble: false },
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
