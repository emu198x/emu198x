//! Header location, fields and the copier header.

use format_nintendo_snes_sfc::{
    Chipset, MapMode, SfcError, VideoStandard, compute_checksum, parse,
};

/// A ROM of `len` bytes with a well-formed header for `map` written in.
fn rom_with_header(len: usize, map: MapMode, mode: u8, chipset: u8, destination: u8) -> Vec<u8> {
    let mut rom = vec![0u8; len];
    let base = map.header_offset();
    rom[base..base + 21].copy_from_slice(b"EMU198X TEST CART    ");
    rom[base + 0x15] = mode;
    rom[base + 0x16] = chipset;
    rom[base + 0x17] = 0x08; // 256 KB
    rom[base + 0x18] = 0x03; // 8 KB RAM
    rom[base + 0x19] = destination;
    rom[base + 0x3C..base + 0x3E].copy_from_slice(&0x8000u16.to_le_bytes());
    rom[base + 0x2A..base + 0x2C].copy_from_slice(&0x8123u16.to_le_bytes());
    let checksum = compute_checksum(&rom);
    rom[base + 0x1C..base + 0x1E].copy_from_slice(&(!checksum).to_le_bytes());
    rom[base + 0x1E..base + 0x20].copy_from_slice(&checksum.to_le_bytes());
    rom
}

#[test]
fn a_lorom_header_is_found_and_read() {
    let rom = rom_with_header(0x40000, MapMode::LoRom, 0x30, 0x02, 0x01);
    let image = parse(&rom).expect("valid image");
    let h = &image.header;
    assert_eq!(h.map, MapMode::LoRom);
    assert_eq!(h.title, "EMU198X TEST CART");
    assert!(h.fast_rom);
    assert_eq!(h.chipset, Chipset::RomRamBattery);
    assert!(h.chipset.has_ram() && h.chipset.has_battery());
    assert_eq!(h.declared_rom_size, 256 * 1024);
    assert_eq!(h.ram_size, 8 * 1024);
    assert_eq!(h.video_standard(), VideoStandard::Ntsc);
    assert_eq!(h.vectors.reset, 0x8000);
    assert_eq!(h.vectors.native_nmi, 0x8123);
    assert!(h.checksum_pair_valid());
    assert!(!image.had_copier_header);
}

#[test]
fn a_hirom_header_wins_over_an_empty_lorom_slot() {
    let rom = rom_with_header(0x40000, MapMode::HiRom, 0x21, 0x00, 0x02);
    let image = parse(&rom).expect("valid image");
    assert_eq!(image.header.map, MapMode::HiRom);
    assert_eq!(image.header.video_standard(), VideoStandard::Pal);
    assert_eq!(image.header.chipset, Chipset::Rom);
    assert!(!image.header.chipset.has_ram());
}

#[test]
fn an_exhirom_header_is_found_past_four_megabytes() {
    let rom = rom_with_header(0x60_0000, MapMode::ExHiRom, 0x35, 0x00, 0x00);
    assert_eq!(parse(&rom).expect("valid").header.map, MapMode::ExHiRom);
}

#[test]
fn a_copier_header_is_skipped() {
    let rom = rom_with_header(0x40000, MapMode::LoRom, 0x20, 0x00, 0x00);
    let mut file = vec![0xAAu8; 512];
    file.extend_from_slice(&rom);
    let image = parse(&file).expect("valid");
    assert!(image.had_copier_header);
    assert_eq!(image.rom.len(), rom.len());
    assert_eq!(image.header.map, MapMode::LoRom);
}

#[test]
fn coprocessor_boards_are_named() {
    let rom = rom_with_header(0x40000, MapMode::LoRom, 0x20, 0x35, 0x00);
    let chipset = parse(&rom).expect("valid").header.chipset;
    assert_eq!(chipset.coprocessor_name(), Some("SA-1"));
    assert!(chipset.has_ram() && chipset.has_battery());
    assert_eq!(Chipset::Rom.coprocessor_name(), None);
    assert_eq!(Chipset::RomRam.coprocessor_name(), None);
    assert!(Chipset::RomRam.has_ram() && !Chipset::RomRam.has_battery());
}

#[test]
fn short_and_headerless_images_are_refused() {
    assert_eq!(parse(&[0; 100]), Err(SfcError::TooShort(100)));
    let blank = vec![0u8; 0x8000];
    assert_eq!(parse(&blank), Err(SfcError::NoHeader));
    assert!(!SfcError::NoHeader.to_string().is_empty());
    assert!(!SfcError::TooShort(1).to_string().is_empty());
}

#[test]
fn the_checksum_counts_a_non_power_of_two_rom_mirrored() {
    // 3 MB: the last 1 MB is counted twice.
    let mut rom = vec![0u8; 0x30_0000];
    rom[0] = 1;
    rom[0x20_0000] = 1;
    assert_eq!(compute_checksum(&rom), 3);
    assert_eq!(compute_checksum(&[]), 0);
}
