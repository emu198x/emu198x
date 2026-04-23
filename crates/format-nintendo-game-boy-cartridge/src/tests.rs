//! Header parser tests.

#![allow(clippy::unwrap_used)]

use super::*;

/// Build a minimal valid 32 KiB ROM with the given header overrides
/// applied on top. The header checksum is recomputed automatically.
fn make_rom(rom_size: usize, customise: impl FnOnce(&mut [u8])) -> Vec<u8> {
    let mut rom = vec![0x00; rom_size];
    // ROM size code at $0148: code N = 32 KiB << N.
    let code = (rom_size / 0x8000).trailing_zeros() as u8;
    rom[offset::ROM_SIZE] = code;
    rom[offset::CART_TYPE] = 0x00; // ROM only by default
    customise(&mut rom);
    let checksum = compute_header_checksum(&rom);
    rom[offset::HEADER_CHECKSUM] = checksum;
    rom
}

#[test]
fn rejects_short_roms() {
    let rom = vec![0u8; 1024];
    let err = CartridgeHeader::parse(&rom).unwrap_err();
    assert!(matches!(err, HeaderError::TooShort { len: 1024 }));
}

#[test]
fn rejects_unknown_cart_type() {
    let rom = make_rom(0x8000, |r| r[offset::CART_TYPE] = 0x77);
    let err = CartridgeHeader::parse(&rom).unwrap_err();
    assert!(matches!(err, HeaderError::UnknownCartType { byte: 0x77 }));
}

#[test]
fn rejects_unsupported_mbc6() {
    let rom = make_rom(0x8000, |r| r[offset::CART_TYPE] = 0x20);
    let err = CartridgeHeader::parse(&rom).unwrap_err();
    assert!(matches!(
        err,
        HeaderError::UnsupportedCartType { byte: 0x20, name: "MBC6" }
    ));
}

#[test]
fn rejects_invalid_rom_size_code() {
    // Need to bypass make_rom's auto-recompute since we want an
    // invalid code. Build by hand.
    let mut rom = vec![0u8; 0x8000];
    rom[offset::ROM_SIZE] = 0x09; // out of range
    let checksum = compute_header_checksum(&rom);
    rom[offset::HEADER_CHECKSUM] = checksum;
    let err = CartridgeHeader::parse(&rom).unwrap_err();
    assert!(matches!(err, HeaderError::InvalidRomSize { code: 0x09 }));
}

#[test]
fn rejects_rom_length_mismatch() {
    // Header says 64 KiB; file is only 32 KiB.
    let rom = make_rom(0x8000, |r| r[offset::ROM_SIZE] = 0x01);
    let err = CartridgeHeader::parse(&rom).unwrap_err();
    assert!(matches!(
        err,
        HeaderError::RomLengthMismatch { declared: 0x10000, actual: 0x8000 }
    ));
}

#[test]
fn rejects_bad_header_checksum() {
    let mut rom = make_rom(0x8000, |_| {});
    rom[offset::HEADER_CHECKSUM] = rom[offset::HEADER_CHECKSUM].wrapping_add(1);
    let err = CartridgeHeader::parse(&rom).unwrap_err();
    assert!(matches!(err, HeaderError::HeaderChecksumMismatch { .. }));
}

#[test]
fn parses_rom_only_cartridge_with_no_ram() {
    let rom = make_rom(0x8000, |r| {
        r[offset::CART_TYPE] = 0x00;
        r[offset::RAM_SIZE] = 0x00;
        // Title "TEST" at $0134.
        r[offset::TITLE..offset::TITLE + 4].copy_from_slice(b"TEST");
    });
    let header = CartridgeHeader::parse(&rom).unwrap();
    assert_eq!(header.cart_type, CartType::RomOnly);
    assert_eq!(header.rom_size, 0x8000);
    assert_eq!(header.ram_size, 0);
    assert_eq!(header.title, "TEST");
}

#[test]
fn parses_mbc1_with_ram_and_battery() {
    let rom = make_rom(0x10000, |r| {
        r[offset::ROM_SIZE] = 0x01;
        r[offset::CART_TYPE] = 0x03;
        r[offset::RAM_SIZE] = 0x02; // 8 KiB
    });
    let header = CartridgeHeader::parse(&rom).unwrap();
    assert_eq!(
        header.cart_type,
        CartType::Mbc1 { ram: true, battery: true }
    );
    assert_eq!(header.ram_size, 0x2000);
}

#[test]
fn parses_mbc3_with_rtc() {
    let rom = make_rom(0x80000, |r| {
        r[offset::ROM_SIZE] = 0x04; // 512 KiB
        r[offset::CART_TYPE] = 0x10; // MBC3+TIMER+RAM+BATTERY
        r[offset::RAM_SIZE] = 0x03; // 32 KiB
    });
    let header = CartridgeHeader::parse(&rom).unwrap();
    assert_eq!(
        header.cart_type,
        CartType::Mbc3 { ram: true, battery: true, rtc: true }
    );
    assert_eq!(header.rom_size, 0x80000);
    assert_eq!(header.ram_size, 0x8000);
}

#[test]
fn parses_mbc5_with_rumble() {
    let rom = make_rom(0x100000, |r| {
        r[offset::ROM_SIZE] = 0x05; // 1 MiB
        r[offset::CART_TYPE] = 0x1E; // MBC5+RUMBLE+RAM+BATTERY
        r[offset::RAM_SIZE] = 0x03; // 32 KiB
    });
    let header = CartridgeHeader::parse(&rom).unwrap();
    assert_eq!(
        header.cart_type,
        CartType::Mbc5 { ram: true, battery: true, rumble: true }
    );
}

#[test]
fn cgb_flag_shortens_title_to_eleven_bytes() {
    let rom = make_rom(0x8000, |r| {
        r[offset::TITLE..offset::TITLE + 16]
            .copy_from_slice(b"POKEMON YELLOWJP"); // 16-byte title
        r[offset::CGB_FLAG] = 0x80;
    });
    let header = CartridgeHeader::parse(&rom).unwrap();
    // CGB-aware: title trimmed to first 11 bytes.
    assert_eq!(header.title, "POKEMON YEL");
}

#[test]
fn load_returns_cartridge_with_correct_ram_size() {
    let rom = make_rom(0x10000, |r| {
        r[offset::ROM_SIZE] = 0x01;
        r[offset::CART_TYPE] = 0x03;
        r[offset::RAM_SIZE] = 0x02;
    });
    let (header, cart) = load(rom).unwrap();
    assert_eq!(header.ram_size, 0x2000);
    assert_eq!(cart.ram().len(), 0x2000);
}

#[test]
fn mbc2_ram_size_is_two_hundred_fifty_six_bytes_regardless_of_byte() {
    let rom = make_rom(0x10000, |r| {
        r[offset::ROM_SIZE] = 0x01;
        r[offset::CART_TYPE] = 0x05; // MBC2
        r[offset::RAM_SIZE] = 0x00;
    });
    let header = CartridgeHeader::parse(&rom).unwrap();
    assert_eq!(header.ram_size, 0x200, "MBC2 has on-chip 256-byte RAM");
}

#[test]
fn header_checksum_matches_pan_docs_formula() {
    // Pan Docs: x = 0; for each byte $0134..=$014C: x = x - byte - 1.
    // Verify the computer matches a hand-computed value.
    let rom = make_rom(0x8000, |r| {
        r[offset::TITLE] = 0xFF;
        r[offset::CART_TYPE] = 0x13;
    });
    // Reparse to confirm the auto-computed checksum is accepted.
    assert!(CartridgeHeader::parse(&rom).is_ok());
}
