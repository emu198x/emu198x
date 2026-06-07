//! Integration tests covering the iNES / NES 2.0 parser plus
//! per-mapper behaviour exercised through the public API.
//!
//! Two MMC1 tests that poke private fields (`shift_count`, `control`,
//! etc.) live inline in `src/mappers/mmc1.rs`; everything else is
//! here.

mod common;

use common::{expect_err, make_ines, make_nes2};
use format_nintendo_nes_ines::{
    Action53, AxRom, BxRom, Camerica, CnRom, ColorDreams, Mapper, Mirroring, Mmc1, Mmc3, Mmc5,
    Nina001, Sunsoft4, UxRom, Vrc2a, parse_ines,
};

// ─── NROM ──────────────────────────────────────────────────────────

#[test]
fn parse_valid_nrom_16k() {
    let data = make_ines(1, 1, 0x00);
    let parsed = parse_ines(&data).expect("parse failed");
    assert_eq!(parsed.header.mapper_number, 0);
    assert_eq!(parsed.mapper.mirroring(), Mirroring::Horizontal);
    // PRG at $8000 is the first byte of the ROM.
    assert_eq!(parsed.mapper.cpu_read(0x8000), 0x00);
    // 16 KiB cart: $C000 mirrors $8000.
    assert_eq!(parsed.mapper.cpu_read(0xC000), 0x00);
}

#[test]
fn parse_valid_nrom_32k() {
    let data = make_ines(2, 1, 0x01);
    let parsed = parse_ines(&data).expect("parse failed");
    assert_eq!(parsed.mapper.mirroring(), Mirroring::Vertical);
    assert_eq!(parsed.mapper.cpu_read(0x8000), 0x00);
    // 32 KiB cart: $C000 starts the second 16 KiB. Offset
    // 0x4000 mod 256 == 0.
    assert_eq!(parsed.mapper.cpu_read(0xC000), 0x00);
}

#[test]
fn nrom_16k_mirrors_high_half() {
    // 16 KiB cart, distinct PRG bytes: confirm $C001 mirrors
    // $8001 (both return 0x01 from the offset-fill pattern).
    let data = make_ines(1, 1, 0x00);
    let mapper = parse_ines(&data).expect("parse failed").mapper;
    assert_eq!(mapper.cpu_read(0x8001), 0x01);
    assert_eq!(mapper.cpu_read(0xC001), 0x01);
}

#[test]
fn nrom_cpu_write_prg_ram_roundtrip() {
    let data = make_ines(1, 1, 0x00);
    let mut mapper = parse_ines(&data).expect("parse failed").mapper;
    mapper.cpu_write(0x6123, 0x42);
    assert_eq!(mapper.cpu_read(0x6123), 0x42);
}

#[test]
fn nrom_prg_rom_not_writable() {
    let data = make_ines(1, 1, 0x00);
    let mut mapper = parse_ines(&data).expect("parse failed").mapper;
    let before = mapper.cpu_read(0x8000);
    mapper.cpu_write(0x8000, 0xFF);
    assert_eq!(mapper.cpu_read(0x8000), before);
}

#[test]
fn nrom_chr_ram_roundtrip() {
    let data = make_ines(1, 0, 0x00); // CHR RAM (chr_banks == 0)
    let mut mapper = parse_ines(&data).expect("parse failed").mapper;
    assert_eq!(mapper.chr_read(0x0000), 0);
    mapper.chr_write(0x0000, 0xAB);
    assert_eq!(mapper.chr_read(0x0000), 0xAB);
}

#[test]
fn nrom_chr_rom_not_writable() {
    let data = make_ines(1, 1, 0x00);
    let mut mapper = parse_ines(&data).expect("parse failed").mapper;
    let before = mapper.chr_read(0x0000);
    mapper.chr_write(0x0000, 0xFF);
    assert_eq!(mapper.chr_read(0x0000), before);
}

#[test]
fn nrom_default_irq_not_pending() {
    let data = make_ines(1, 1, 0x00);
    let mapper = parse_ines(&data).expect("parse failed").mapper;
    assert!(!mapper.irq_pending());
}

// ─── MMC1 ──────────────────────────────────────────────────────────

fn make_mmc1(prg_banks: u8, chr_banks: u8) -> Mmc1 {
    let prg_size = usize::from(prg_banks) * 16384;
    let chr_size = usize::from(chr_banks) * 8192;
    let mut prg_rom = vec![0u8; prg_size];
    for bank in 0..usize::from(prg_banks) {
        for byte in &mut prg_rom[bank * 16384..(bank + 1) * 16384] {
            *byte = bank as u8;
        }
    }
    let chr_data = if chr_size > 0 {
        let mut chr = vec![0u8; chr_size];
        for page in 0..chr_size / 4096 {
            for byte in &mut chr[page * 4096..(page + 1) * 4096] {
                *byte = page as u8;
            }
        }
        chr
    } else {
        Vec::new()
    };
    Mmc1::new(prg_rom, chr_data)
}

fn mmc1_write_5(mapper: &mut Mmc1, addr: u16, value: u8) {
    for bit in 0..5 {
        mapper.cpu_write(addr, (value >> bit) & 1);
    }
}

#[test]
fn parse_valid_mmc1() {
    let data = make_ines(8, 2, 0x10);
    let parsed = parse_ines(&data).expect("parse failed");

    assert_eq!(parsed.header.mapper_number, 1);
    assert_eq!(parsed.mapper.mirroring(), Mirroring::SingleScreenLower);
}

#[test]
fn mmc1_prg_mode_3_switches_low_and_fixes_last_bank() {
    let mut mapper = make_mmc1(8, 0);

    mmc1_write_5(&mut mapper, 0xE000, 2);

    assert_eq!(mapper.cpu_read(0x8000), 2);
    assert_eq!(mapper.cpu_read(0xC000), 7);
}

#[test]
fn mmc1_prg_mode_2_fixes_first_and_switches_high_bank() {
    let mut mapper = make_mmc1(8, 0);

    mmc1_write_5(&mut mapper, 0x8000, 0b01000);
    mmc1_write_5(&mut mapper, 0xE000, 5);

    assert_eq!(mapper.cpu_read(0x8000), 0);
    assert_eq!(mapper.cpu_read(0xC000), 5);
}

#[test]
fn mmc1_prg_32k_mode_ignores_low_bank_bit() {
    let mut mapper = make_mmc1(8, 0);

    mmc1_write_5(&mut mapper, 0x8000, 0b00000);
    mmc1_write_5(&mut mapper, 0xE000, 3);

    assert_eq!(mapper.cpu_read(0x8000), 2);
    assert_eq!(mapper.cpu_read(0xC000), 3);
}

#[test]
fn mmc1_chr_4k_mode_selects_two_pages() {
    let mut mapper = make_mmc1(2, 2);

    mmc1_write_5(&mut mapper, 0x8000, 0b11100);
    mmc1_write_5(&mut mapper, 0xA000, 1);
    mmc1_write_5(&mut mapper, 0xC000, 3);

    assert_eq!(mapper.chr_read(0x0000), 1);
    assert_eq!(mapper.chr_read(0x1000), 3);
}

#[test]
fn mmc1_chr_8k_mode_ignores_low_bank_bit() {
    let mut mapper = make_mmc1(2, 2);

    mmc1_write_5(&mut mapper, 0x8000, 0b01100);
    mmc1_write_5(&mut mapper, 0xA000, 3);

    assert_eq!(mapper.chr_read(0x0000), 2);
    assert_eq!(mapper.chr_read(0x1000), 3);
}

#[test]
fn mmc1_chr_ram_writes_through_selected_bank() {
    let mut mapper = make_mmc1(2, 0);

    mmc1_write_5(&mut mapper, 0x8000, 0b11100);
    mmc1_write_5(&mut mapper, 0xA000, 1);
    mapper.chr_write(0x0004, 0xA5);

    assert_eq!(mapper.chr_read(0x0004), 0xA5);
}

#[test]
fn mmc1_prg_ram_roundtrip() {
    let mut mapper = make_mmc1(2, 0);

    mapper.cpu_write(0x6000, 0x42);
    mapper.cpu_write(0x7FFF, 0xAB);

    assert_eq!(mapper.cpu_read(0x6000), 0x42);
    assert_eq!(mapper.cpu_read(0x7FFF), 0xAB);
}

#[test]
fn mmc1_mirroring_is_dynamic() {
    let mut mapper = make_mmc1(2, 0);

    assert_eq!(mapper.mirroring(), Mirroring::SingleScreenLower);
    mmc1_write_5(&mut mapper, 0x8000, 0b01110);
    assert_eq!(mapper.mirroring(), Mirroring::Vertical);
    mmc1_write_5(&mut mapper, 0x8000, 0b01111);
    assert_eq!(mapper.mirroring(), Mirroring::Horizontal);
    mmc1_write_5(&mut mapper, 0x8000, 0b01101);
    assert_eq!(mapper.mirroring(), Mirroring::SingleScreenUpper);
}

// ─── UxROM ─────────────────────────────────────────────────────────

#[test]
fn parse_valid_uxrom() {
    let data = make_ines(8, 0, 0x20);
    let parsed = parse_ines(&data).expect("parse failed");

    assert_eq!(parsed.header.mapper_number, 2);
    assert_eq!(parsed.mapper.mirroring(), Mirroring::Horizontal);
}

#[test]
fn uxrom_switches_low_prg_bank_and_fixes_high_bank() {
    let mut prg = vec![0u8; 8 * 16384];
    for bank in 0..8usize {
        for byte in &mut prg[bank * 16384..(bank + 1) * 16384] {
            *byte = bank as u8;
        }
        prg[bank * 16384] = 0xFF; // bus-conflict-safe write target
    }
    let mut mapper = UxRom::new(prg, Vec::new(), Mirroring::Vertical);

    assert_eq!(mapper.cpu_read(0x8001), 0);
    assert_eq!(mapper.cpu_read(0xC001), 7);

    mapper.cpu_write(0x8000, 3);

    assert_eq!(mapper.cpu_read(0x8001), 3);
    assert_eq!(mapper.cpu_read(0xC001), 7);
}

#[test]
fn uxrom_chr_ram_roundtrip() {
    let mut mapper = UxRom::new(vec![0u8; 16384], Vec::new(), Mirroring::Horizontal);

    mapper.chr_write(0x1000, 0xAB);

    assert_eq!(mapper.chr_read(0x1000), 0xAB);
}

// ─── CNROM ─────────────────────────────────────────────────────────

#[test]
fn parse_valid_cnrom() {
    let data = make_ines(2, 4, 0x30);
    let parsed = parse_ines(&data).expect("parse failed");

    assert_eq!(parsed.header.mapper_number, 3);
    assert_eq!(parsed.mapper.mirroring(), Mirroring::Horizontal);
}

#[test]
fn cnrom_prg_is_unbanked_and_16k_mirrors_high_half() {
    let mut prg = vec![0u8; 16384];
    prg[0] = 0xCC;
    prg[1] = 0xDD;
    let mapper = CnRom::new(prg, vec![0u8; 8192], Mirroring::Horizontal);

    assert_eq!(mapper.cpu_read(0x8000), 0xCC);
    assert_eq!(mapper.cpu_read(0xC000), 0xCC);
    assert_eq!(mapper.cpu_read(0x8001), 0xDD);
    assert_eq!(mapper.cpu_read(0xC001), 0xDD);
}

#[test]
fn cnrom_prg_is_unbanked_32k() {
    let mut prg = vec![0u8; 32768];
    prg[0] = 0xAA;
    prg[0x4000] = 0xBB;
    let mapper = CnRom::new(prg, vec![0u8; 8192], Mirroring::Vertical);

    assert_eq!(mapper.cpu_read(0x8000), 0xAA);
    assert_eq!(mapper.cpu_read(0xC000), 0xBB);
}

#[test]
fn cnrom_switches_8k_chr_banks() {
    let mut chr = vec![0u8; 4 * 8192];
    for bank in 0..4usize {
        for byte in &mut chr[bank * 8192..(bank + 1) * 8192] {
            *byte = bank as u8;
        }
    }
    let mut mapper = CnRom::new(vec![0xFFu8; 32768], chr, Mirroring::Vertical);

    assert_eq!(mapper.chr_read(0x0000), 0);

    mapper.cpu_write(0x8000, 2);
    assert_eq!(mapper.chr_read(0x0000), 2);

    mapper.cpu_write(0xFFFF, 3);
    assert_eq!(mapper.chr_read(0x1FFF), 3);
}

#[test]
fn cnrom_chr_bank_write_obeys_bus_conflict() {
    let mut chr = vec![0u8; 4 * 8192];
    for bank in 0..4usize {
        for byte in &mut chr[bank * 8192..(bank + 1) * 8192] {
            *byte = bank as u8;
        }
    }
    let mut prg = vec![0xFFu8; 32768];
    prg[0] = 0x01;
    let mut mapper = CnRom::new(prg, chr, Mirroring::Vertical);

    mapper.cpu_write(0x8000, 3);

    assert_eq!(mapper.chr_read(0x0000), 1);
}

#[test]
fn cnrom_chr_rom_not_writable() {
    let mut mapper = CnRom::new(vec![0xFFu8; 32768], vec![0x44u8; 8192], Mirroring::Vertical);

    mapper.chr_write(0x0000, 0xAB);

    assert_eq!(mapper.chr_read(0x0000), 0x44);
}

// ─── MMC3 ──────────────────────────────────────────────────────────

fn make_mmc3(prg_8k_banks: usize, chr_1k_pages: usize) -> Mmc3 {
    let mut prg_rom = vec![0u8; prg_8k_banks * 8192];
    for bank in 0..prg_8k_banks {
        for byte in &mut prg_rom[bank * 8192..(bank + 1) * 8192] {
            *byte = bank as u8;
        }
    }

    let chr_data = if chr_1k_pages == 0 {
        Vec::new()
    } else {
        let mut chr = vec![0u8; chr_1k_pages * 1024];
        for page in 0..chr_1k_pages {
            for byte in &mut chr[page * 1024..(page + 1) * 1024] {
                *byte = page as u8;
            }
        }
        chr
    };

    Mmc3::new(prg_rom, chr_data)
}

#[test]
fn parse_valid_mmc3() {
    let data = make_ines(4, 4, 0x40);
    let parsed = parse_ines(&data).expect("parse failed");

    assert_eq!(parsed.header.mapper_number, 4);
    assert_eq!(parsed.mapper.mirroring(), Mirroring::Vertical);
}

#[test]
fn mmc3_prg_mode_0_maps_r6_r7_second_last_last() {
    let mut mapper = make_mmc3(32, 8);

    mapper.cpu_write(0x8000, 6);
    mapper.cpu_write(0x8001, 5);
    mapper.cpu_write(0x8000, 7);
    mapper.cpu_write(0x8001, 10);

    assert_eq!(mapper.cpu_read(0x8000), 5);
    assert_eq!(mapper.cpu_read(0xA000), 10);
    assert_eq!(mapper.cpu_read(0xC000), 30);
    assert_eq!(mapper.cpu_read(0xE000), 31);
}

#[test]
fn mmc3_prg_mode_1_swaps_r6_with_second_last() {
    let mut mapper = make_mmc3(32, 8);

    mapper.cpu_write(0x8000, 0x46);
    mapper.cpu_write(0x8001, 5);
    mapper.cpu_write(0x8000, 0x47);
    mapper.cpu_write(0x8001, 10);

    assert_eq!(mapper.cpu_read(0x8000), 30);
    assert_eq!(mapper.cpu_read(0xA000), 10);
    assert_eq!(mapper.cpu_read(0xC000), 5);
    assert_eq!(mapper.cpu_read(0xE000), 31);
}

#[test]
fn mmc3_chr_mode_0_maps_two_2k_then_four_1k_banks() {
    let mut mapper = make_mmc3(4, 256);

    mapper.cpu_write(0x8000, 0);
    mapper.cpu_write(0x8001, 4);
    mapper.cpu_write(0x8000, 1);
    mapper.cpu_write(0x8001, 8);
    mapper.cpu_write(0x8000, 2);
    mapper.cpu_write(0x8001, 20);
    mapper.cpu_write(0x8000, 3);
    mapper.cpu_write(0x8001, 21);
    mapper.cpu_write(0x8000, 4);
    mapper.cpu_write(0x8001, 22);
    mapper.cpu_write(0x8000, 5);
    mapper.cpu_write(0x8001, 23);

    assert_eq!(mapper.chr_read(0x0000), 4);
    assert_eq!(mapper.chr_read(0x0400), 5);
    assert_eq!(mapper.chr_read(0x0800), 8);
    assert_eq!(mapper.chr_read(0x0C00), 9);
    assert_eq!(mapper.chr_read(0x1000), 20);
    assert_eq!(mapper.chr_read(0x1400), 21);
    assert_eq!(mapper.chr_read(0x1800), 22);
    assert_eq!(mapper.chr_read(0x1C00), 23);
}

#[test]
fn mmc3_chr_mode_1_inverts_chr_windows() {
    let mut mapper = make_mmc3(4, 256);

    mapper.cpu_write(0x8000, 0x80);
    mapper.cpu_write(0x8001, 4);
    mapper.cpu_write(0x8000, 0x81);
    mapper.cpu_write(0x8001, 8);
    mapper.cpu_write(0x8000, 0x82);
    mapper.cpu_write(0x8001, 20);
    mapper.cpu_write(0x8000, 0x83);
    mapper.cpu_write(0x8001, 21);
    mapper.cpu_write(0x8000, 0x84);
    mapper.cpu_write(0x8001, 22);
    mapper.cpu_write(0x8000, 0x85);
    mapper.cpu_write(0x8001, 23);

    assert_eq!(mapper.chr_read(0x0000), 20);
    assert_eq!(mapper.chr_read(0x0400), 21);
    assert_eq!(mapper.chr_read(0x0800), 22);
    assert_eq!(mapper.chr_read(0x0C00), 23);
    assert_eq!(mapper.chr_read(0x1000), 4);
    assert_eq!(mapper.chr_read(0x1400), 5);
    assert_eq!(mapper.chr_read(0x1800), 8);
    assert_eq!(mapper.chr_read(0x1C00), 9);
}

#[test]
fn mmc3_prg_ram_respects_enable_and_write_protect() {
    let mut mapper = make_mmc3(4, 8);

    mapper.cpu_write(0x6000, 0x42);
    assert_eq!(mapper.cpu_read(0x6000), 0x42);

    mapper.cpu_write(0xA001, 0xC0);
    mapper.cpu_write(0x6000, 0x99);
    assert_eq!(mapper.cpu_read(0x6000), 0x42);

    mapper.cpu_write(0xA001, 0x00);
    assert_eq!(mapper.cpu_read(0x6000), 0x00);
    mapper.cpu_write(0x6000, 0xAB);
    assert_eq!(mapper.cpu_read(0x6000), 0x00);

    mapper.cpu_write(0xA001, 0x80);
    mapper.cpu_write(0x6000, 0xAB);
    assert_eq!(mapper.cpu_read(0x6000), 0xAB);
}

#[test]
fn mmc3_mirroring_is_dynamic() {
    let mut mapper = make_mmc3(4, 8);

    assert_eq!(mapper.mirroring(), Mirroring::Vertical);
    mapper.cpu_write(0xA000, 1);
    assert_eq!(mapper.mirroring(), Mirroring::Horizontal);
    mapper.cpu_write(0xA000, 0);
    assert_eq!(mapper.mirroring(), Mirroring::Vertical);
}

#[test]
fn mmc3_chr_ram_writes_through_selected_bank() {
    let mut mapper = make_mmc3(4, 0);

    mapper.cpu_write(0x8000, 0);
    mapper.cpu_write(0x8001, 4);
    mapper.chr_write(0x0002, 0x5A);

    assert_eq!(mapper.chr_read(0x0002), 0x5A);
}

/// Simulate one debounced A12 rising edge: A12 falls, stays low well
/// past the mapper's filter window, then rises. `clock` is the PPU
/// master-clock counter (4 units per dot) threaded across calls.
fn mmc3_a12_edge(mapper: &mut Mmc3, clock: &mut u64) {
    *clock += 1;
    mapper.notify_a12_rendering(false, *clock);
    *clock += 64; // comfortably above A12_FILTER_CYCLES (40)
    mapper.notify_a12_rendering(true, *clock);
}

#[test]
fn mmc3_irq_counter_clocks_on_debounced_a12_edges() {
    let mut mapper = make_mmc3(4, 8);
    let mut clock = 0u64;

    mapper.cpu_write(0xC000, 3);
    mapper.cpu_write(0xC001, 0);
    mapper.cpu_write(0xE001, 0);

    mmc3_a12_edge(&mut mapper, &mut clock);
    assert!(!mapper.irq_pending());
    mmc3_a12_edge(&mut mapper, &mut clock);
    assert!(!mapper.irq_pending());
    mmc3_a12_edge(&mut mapper, &mut clock);
    assert!(!mapper.irq_pending());
    mmc3_a12_edge(&mut mapper, &mut clock);
    assert!(mapper.irq_pending());
}

#[test]
fn mmc3_irq_disable_acknowledges_pending_irq() {
    let mut mapper = make_mmc3(4, 8);
    let mut clock = 0u64;

    mapper.cpu_write(0xC000, 0);
    mapper.cpu_write(0xC001, 0);
    mapper.cpu_write(0xE001, 0);
    mmc3_a12_edge(&mut mapper, &mut clock);
    assert!(mapper.irq_pending());

    mapper.cpu_write(0xE000, 0);

    assert!(!mapper.irq_pending());
}

// ─── MMC5 ──────────────────────────────────────────────────────────

fn make_mmc5(prg_8k_banks: usize, chr_1k_pages: usize) -> Mmc5 {
    let mut prg_rom = vec![0u8; prg_8k_banks * 8192];
    for bank in 0..prg_8k_banks {
        for byte in &mut prg_rom[bank * 8192..(bank + 1) * 8192] {
            *byte = bank as u8;
        }
    }
    let mut chr = vec![0u8; chr_1k_pages * 1024];
    for bank in 0..chr_1k_pages {
        for byte in &mut chr[bank * 1024..(bank + 1) * 1024] {
            *byte = bank as u8;
        }
    }
    Mmc5::new(prg_rom, chr)
}

#[test]
fn parse_valid_mmc5() {
    let data = make_ines(8, 8, 0x50);
    let parsed = parse_ines(&data).expect("mapper 5 should parse");

    assert_eq!(parsed.header.mapper_number, 5);
}

#[test]
fn mmc5_prg_mode_3_maps_four_8k_windows() {
    let mut mapper = make_mmc5(16, 8);

    mapper.cpu_write(0x5100, 3);
    mapper.cpu_write(0x5114, 0x82);
    mapper.cpu_write(0x5115, 0x84);
    mapper.cpu_write(0x5116, 0x86);
    mapper.cpu_write(0x5117, 0x88);

    assert_eq!(mapper.cpu_read(0x8000), 2);
    assert_eq!(mapper.cpu_read(0xA000), 4);
    assert_eq!(mapper.cpu_read(0xC000), 6);
    assert_eq!(mapper.cpu_read(0xE000), 8);
}

#[test]
fn mmc5_prg_mode_0_maps_one_32k_rom_window() {
    let mut mapper = make_mmc5(16, 8);

    mapper.cpu_write(0x5100, 0);
    mapper.cpu_write(0x5117, 0x84);

    assert_eq!(mapper.cpu_read(0x8000), 4);
    assert_eq!(mapper.cpu_read(0xA000), 5);
    assert_eq!(mapper.cpu_read(0xC000), 6);
    assert_eq!(mapper.cpu_read(0xE000), 7);
}

#[test]
fn mmc5_prg_ram_requires_protection_sequence() {
    let mut mapper = Mmc5::new(vec![0u8; 4 * 8192], Vec::new());

    mapper.cpu_write(0x6000, 0x11);
    assert_eq!(mapper.cpu_read(0x6000), 0);

    mapper.cpu_write(0x5102, 0x02);
    mapper.cpu_write(0x5103, 0x01);
    mapper.cpu_write(0x6000, 0x22);

    assert_eq!(mapper.cpu_read(0x6000), 0x22);
}

#[test]
fn mmc5_chr_mode_3_maps_1k_banks() {
    let mut mapper = make_mmc5(4, 32);

    mapper.cpu_write(0x5101, 3);
    mapper.cpu_write(0x5120, 3);
    mapper.cpu_write(0x5121, 5);

    assert_eq!(mapper.chr_read(0x0000), 3);
    assert_eq!(mapper.chr_read(0x0400), 5);
}

#[test]
fn mmc5_nametables_can_map_exram_and_fill_mode() {
    let mut mapper = Mmc5::new(vec![0u8; 4 * 8192], Vec::new());

    mapper.cpu_write(0x5105, 0b11_10_01_00);
    mapper.cpu_write(0x5106, 0x44);
    mapper.cpu_write(0x5107, 0x02);

    assert!(mapper.nametable_write(0x2000, 0x11));
    assert!(mapper.nametable_write(0x2400, 0x22));
    assert!(mapper.nametable_write(0x2800, 0x33));

    assert_eq!(mapper.nametable_read(0x2000), Some(0x11));
    assert_eq!(mapper.nametable_read(0x2400), Some(0x22));
    assert_eq!(mapper.nametable_read(0x2800), Some(0x33));
    assert_eq!(mapper.nametable_read(0x2C00), Some(0x44));
    assert_eq!(mapper.nametable_read(0x2FC0), Some(0xAA));
}

#[test]
fn mmc5_multiplier_reports_product() {
    let mut mapper = Mmc5::new(vec![0u8; 4 * 8192], Vec::new());

    mapper.cpu_write(0x5205, 13);
    mapper.cpu_write(0x5206, 17);

    assert_eq!(mapper.cpu_read(0x5205), 221);
    assert_eq!(mapper.cpu_read(0x5206), 0);
}

#[test]
fn mmc5_scanline_irq_detects_three_matching_nametable_reads_then_next_read() {
    let mut mapper = Mmc5::new(vec![0u8; 4 * 8192], Vec::new());
    mapper.cpu_write(0x5203, 1);
    mapper.cpu_write(0x5204, 0x80);

    for _ in 0..3 {
        mapper.notify_ppu_read(0x2000, true);
    }
    mapper.notify_ppu_read(0x23C0, true);
    assert_eq!(mapper.cpu_read(0x5204) & 0x40, 0x40);
    assert!(!mapper.irq_pending());

    for _ in 0..3 {
        mapper.notify_ppu_read(0x2008, true);
    }
    mapper.notify_ppu_read(0x23C0, true);

    assert!(mapper.irq_pending());
    assert_eq!(mapper.cpu_read_side_effect(0x5204) & 0x80, 0x80);
    assert!(!mapper.irq_pending());
}

#[test]
fn mmc5_cpu_tick_clears_in_frame_after_ppu_read_gap() {
    let mut mapper = Mmc5::new(vec![0u8; 4 * 8192], Vec::new());

    for _ in 0..3 {
        mapper.notify_ppu_read(0x2000, true);
    }
    mapper.notify_ppu_read(0x23C0, true);
    assert_eq!(mapper.cpu_read(0x5204) & 0x40, 0x40);

    mapper.cpu_tick();
    mapper.cpu_tick();
    mapper.cpu_tick();

    assert_eq!(mapper.cpu_read(0x5204) & 0x40, 0x00);
}

#[test]
fn mmc5_pcm_write_mode_outputs_audio_and_irq_on_zero() {
    let mut mapper = Mmc5::new(vec![0u8; 4 * 8192], Vec::new());

    mapper.cpu_write(0x5010, 0x80);
    mapper.cpu_write(0x5011, 0x40);
    assert!(mapper.expansion_audio_sample() > 0.0);
    assert!(!mapper.irq_pending());

    mapper.cpu_write(0x5011, 0x00);
    assert!(mapper.irq_pending());
    assert_eq!(mapper.cpu_read_side_effect(0x5010) & 0x80, 0x80);
    assert!(!mapper.irq_pending());
}

#[test]
fn mmc5_pulse_expansion_audio_produces_samples_when_enabled() {
    let mut mapper = Mmc5::new(vec![0u8; 4 * 8192], Vec::new());

    mapper.cpu_write(0x5010, 0x00);
    mapper.cpu_write(0x5011, 0x01);
    let pcm_baseline = mapper.expansion_audio_sample();
    mapper.cpu_write(0x5015, 0x01);
    mapper.cpu_write(0x5000, 0b0101_1111);
    mapper.cpu_write(0x5002, 8);
    mapper.cpu_write(0x5003, 0x08);

    let mut max = 0.0f32;
    for _ in 0..128 {
        mapper.cpu_tick();
        max = max.max(mapper.expansion_audio_sample());
    }

    assert!(
        max > pcm_baseline,
        "expected pulse contribution above baseline {pcm_baseline}, got {max}"
    );
    assert_eq!(mapper.cpu_read(0x5015) & 1, 1);
}

// ─── AxROM ─────────────────────────────────────────────────────────

#[test]
fn parse_valid_axrom() {
    let data = make_ines(2, 0, 0x70);
    let parsed = parse_ines(&data).expect("parse failed");

    assert_eq!(parsed.header.mapper_number, 7);
    assert_eq!(parsed.mapper.mirroring(), Mirroring::SingleScreenLower);
}

#[test]
fn axrom_switches_32k_prg_bank() {
    let mut prg = vec![0u8; 8 * 32768];
    for bank in 0..8usize {
        for byte in &mut prg[bank * 32768..(bank + 1) * 32768] {
            *byte = bank as u8;
        }
        prg[bank * 32768] = 0xFF;
    }
    let mut mapper = AxRom::new(prg);

    assert_eq!(mapper.cpu_read(0x8001), 0);
    assert_eq!(mapper.cpu_read(0xC001), 0);

    mapper.cpu_write(0x8000, 3);

    assert_eq!(mapper.cpu_read(0x8001), 3);
    assert_eq!(mapper.cpu_read(0xC001), 3);
}

#[test]
fn axrom_bank_write_obeys_bus_conflict() {
    let mut prg = vec![0xFFu8; 8 * 32768];
    for bank in 0..8usize {
        for byte in &mut prg[bank * 32768..(bank + 1) * 32768] {
            *byte = bank as u8;
        }
    }
    prg[0] = 0x01;
    let mut mapper = AxRom::new(prg);

    mapper.cpu_write(0x8000, 3);

    assert_eq!(mapper.cpu_read(0x8001), 1);
}

#[test]
fn axrom_selects_single_screen_mirroring() {
    let mut mapper = AxRom::new(vec![0xFFu8; 32768]);

    assert_eq!(mapper.mirroring(), Mirroring::SingleScreenLower);
    mapper.cpu_write(0x8000, 0x10);
    assert_eq!(mapper.mirroring(), Mirroring::SingleScreenUpper);
    mapper.cpu_write(0x8000, 0x02);
    assert_eq!(mapper.mirroring(), Mirroring::SingleScreenLower);
}

#[test]
fn axrom_chr_ram_roundtrip() {
    let mut mapper = AxRom::new(vec![0u8; 32768]);

    mapper.chr_write(0x0000, 0xAB);
    mapper.chr_write(0x1FFF, 0xCD);

    assert_eq!(mapper.chr_read(0x0000), 0xAB);
    assert_eq!(mapper.chr_read(0x1FFF), 0xCD);
}

// ─── BxROM / NINA-001 (Mapper 34) ──────────────────────────────────

#[test]
fn parse_valid_bxrom() {
    let data = make_ines(4, 0, 0x20 | 0x01);
    let mut data = data;
    data[7] = 0x20; // mapper 34 high nibble
    let parsed = parse_ines(&data).expect("parse failed");

    assert_eq!(parsed.header.mapper_number, 34);
    assert_eq!(parsed.mapper.mirroring(), Mirroring::Vertical);
}

#[test]
fn bxrom_switches_32k_prg_bank() {
    let mut prg = vec![0u8; 4 * 32768];
    for bank in 0..4usize {
        for byte in &mut prg[bank * 32768..(bank + 1) * 32768] {
            *byte = bank as u8;
        }
        prg[bank * 32768] = 0xFF;
    }
    let mut mapper = BxRom::new(prg, Mirroring::Horizontal);

    assert_eq!(mapper.cpu_read(0x8001), 0);

    mapper.cpu_write(0x8000, 2);

    assert_eq!(mapper.cpu_read(0x8001), 2);
    assert_eq!(mapper.cpu_read(0xC001), 2);
}

#[test]
fn bxrom_bank_write_obeys_bus_conflict() {
    let mut prg = vec![0xFFu8; 4 * 32768];
    for bank in 0..4usize {
        for byte in &mut prg[bank * 32768..(bank + 1) * 32768] {
            *byte = bank as u8;
        }
    }
    prg[0] = 0x01;
    let mut mapper = BxRom::new(prg, Mirroring::Horizontal);

    mapper.cpu_write(0x8000, 3);

    assert_eq!(mapper.cpu_read(0x8001), 1);
}

#[test]
fn bxrom_chr_ram_roundtrip() {
    let mut mapper = BxRom::new(vec![0u8; 32768], Mirroring::Horizontal);

    mapper.chr_write(0x0000, 0xAB);
    mapper.chr_write(0x1FFF, 0xCD);

    assert_eq!(mapper.chr_read(0x0000), 0xAB);
    assert_eq!(mapper.chr_read(0x1FFF), 0xCD);
}

#[test]
fn parse_mapper_34_with_chr_rom_selects_nina001() {
    let mut data = make_ines(4, 8, 0x20 | 0x01);
    data[7] = 0x20;
    let mut mapper = parse_ines(&data).expect("parse failed").mapper;

    assert_eq!(mapper.chr_read(0x0000), 0x80);

    mapper.cpu_write(0x7FFE, 2);

    assert_eq!(mapper.chr_read(0x0000), 0x80);
}

#[test]
fn nina001_switches_32k_prg_bank_via_7ffd() {
    let mut prg = vec![0u8; 4 * 32768];
    for bank in 0..4usize {
        for byte in &mut prg[bank * 32768..(bank + 1) * 32768] {
            *byte = bank as u8;
        }
    }
    let mut mapper = Nina001::new(prg, vec![0u8; 8192], Mirroring::Horizontal);

    assert_eq!(mapper.cpu_read(0x8000), 0);

    mapper.cpu_write(0x7FFD, 2);

    assert_eq!(mapper.cpu_read(0x8000), 2);
}

#[test]
fn nina001_switches_two_4k_chr_banks() {
    let mut chr = vec![0u8; 16 * 4096];
    for bank in 0..16usize {
        for byte in &mut chr[bank * 4096..(bank + 1) * 4096] {
            *byte = bank as u8;
        }
    }
    let mut mapper = Nina001::new(vec![0u8; 32768], chr, Mirroring::Horizontal);

    assert_eq!(mapper.chr_read(0x0000), 0);
    assert_eq!(mapper.chr_read(0x1000), 1);

    mapper.cpu_write(0x7FFE, 3);
    mapper.cpu_write(0x7FFF, 7);

    assert_eq!(mapper.chr_read(0x0000), 3);
    assert_eq!(mapper.chr_read(0x1000), 7);
}

#[test]
fn nina001_register_writes_are_prg_ram_writes_too() {
    let mut mapper = Nina001::new(vec![0u8; 32768], vec![0u8; 8192], Mirroring::Horizontal);

    mapper.cpu_write(0x7FFD, 0xA5);
    mapper.cpu_write(0x7FFE, 0x5A);
    mapper.cpu_write(0x7FFF, 0xC3);

    assert_eq!(mapper.cpu_read(0x7FFD), 0xA5);
    assert_eq!(mapper.cpu_read(0x7FFE), 0x5A);
    assert_eq!(mapper.cpu_read(0x7FFF), 0xC3);
}

#[test]
fn nina001_chr_rom_not_writable() {
    let mut mapper = Nina001::new(vec![0u8; 32768], vec![0x44u8; 8192], Mirroring::Horizontal);

    mapper.chr_write(0x0000, 0xAB);

    assert_eq!(mapper.chr_read(0x0000), 0x44);
}

// ─── Camerica (Mapper 71) ──────────────────────────────────────────

#[test]
fn parse_valid_camerica() {
    let mut data = make_ines(4, 0, 0x70 | 0x01);
    data[7] = 0x40;
    let parsed = parse_ines(&data).expect("parse failed");

    assert_eq!(parsed.header.mapper_number, 71);
    assert_eq!(parsed.mapper.mirroring(), Mirroring::Vertical);
}

#[test]
fn camerica_switches_low_16k_prg_bank_and_fixes_last() {
    let mut prg = vec![0u8; 4 * 16384];
    for bank in 0..4usize {
        for byte in &mut prg[bank * 16384..(bank + 1) * 16384] {
            *byte = bank as u8;
        }
    }
    let mut mapper = Camerica::new(prg, Mirroring::Vertical);

    assert_eq!(mapper.cpu_read(0x8000), 0);
    assert_eq!(mapper.cpu_read(0xC000), 3);

    mapper.cpu_write(0xC000, 2);

    assert_eq!(mapper.cpu_read(0x8000), 2);
    assert_eq!(mapper.cpu_read(0xC000), 3);
}

#[test]
fn camerica_mirroring_control_selects_single_screen() {
    let mut mapper = Camerica::new(vec![0u8; 32768], Mirroring::Vertical);

    assert_eq!(mapper.mirroring(), Mirroring::Vertical);
    mapper.cpu_write(0x9000, 0x10);
    assert_eq!(mapper.mirroring(), Mirroring::SingleScreenUpper);
    mapper.cpu_write(0x9000, 0x00);
    assert_eq!(mapper.mirroring(), Mirroring::SingleScreenLower);
}

#[test]
fn camerica_chr_ram_roundtrip() {
    let mut mapper = Camerica::new(vec![0u8; 32768], Mirroring::Horizontal);

    mapper.chr_write(0x0000, 0xAB);
    mapper.chr_write(0x1FFF, 0xCD);

    assert_eq!(mapper.chr_read(0x0000), 0xAB);
    assert_eq!(mapper.chr_read(0x1FFF), 0xCD);
}

// ─── Header / parser ───────────────────────────────────────────────

#[test]
fn parse_rejects_short_file() {
    let data = vec![0u8; 8];
    assert!(parse_ines(&data).is_err());
}

#[test]
fn parse_rejects_bad_magic() {
    let data = vec![0u8; 32];
    assert!(parse_ines(&data).is_err());
}

#[test]
fn parse_rejects_unsupported_mapper() {
    // Put mapper number 15 in the high nibble of flags6.
    let mut data = make_ines(1, 1, 0xF0);
    // Ensure flags7 high nibble is zero.
    data[7] = 0;
    let err = expect_err(parse_ines(&data), "mapper 15 should be rejected");
    assert!(err.contains("Unsupported mapper: 15"), "got: {err}");
}

#[test]
fn parse_rejects_truncated_prg() {
    // Header claims 2 PRG banks but the file only carries one.
    let mut data = make_ines(1, 1, 0x00);
    data[4] = 2;
    let err = expect_err(parse_ines(&data), "truncated file should be rejected");
    assert!(err.contains("too short"), "got: {err}");
}

#[test]
fn parse_battery_flag() {
    let data = make_ines(1, 1, 0x02); // battery bit set
    let parsed = parse_ines(&data).expect("parse failed");
    assert!(parsed.has_battery);
    assert!(parsed.header.has_battery);
}

#[test]
fn parse_four_screen_mirroring() {
    let data = make_ines(1, 1, 0x08);
    let parsed = parse_ines(&data).expect("parse failed");
    assert_eq!(parsed.mapper.mirroring(), Mirroring::FourScreen);
}

// ─── Color Dreams (Mapper 11) ──────────────────────────────────────

#[test]
fn parse_valid_color_dreams_mapper_11() {
    let data = make_ines(4, 8, 0xB0);
    let parsed = parse_ines(&data).expect("mapper 11 should parse");

    assert_eq!(parsed.header.mapper_number, 11);
    assert_eq!(parsed.mapper.mirroring(), Mirroring::Horizontal);
}

#[test]
fn color_dreams_switches_32k_prg_and_8k_chr() {
    let mut prg = vec![0u8; 4 * 32768];
    for bank in 0..4usize {
        prg[bank * 32768] = 0xFF;
        prg[bank * 32768 + 1] = bank as u8;
    }
    let mut chr = vec![0u8; 4 * 8192];
    for bank in 0..4usize {
        chr[bank * 8192] = (0xA0 + bank) as u8;
    }
    let mut mapper = ColorDreams::new(prg, chr, Mirroring::Vertical);

    mapper.cpu_write(0x8000, 0x21);

    assert_eq!(mapper.cpu_read(0x8001), 1);
    assert_eq!(mapper.chr_read(0x0000), 0xA2);
}

#[test]
fn color_dreams_bank_write_obeys_bus_conflict() {
    let mut prg = vec![0xFFu8; 4 * 32768];
    prg[0] = 0x11;
    prg[32768 + 1] = 1;
    let mut chr = vec![0u8; 2 * 8192];
    chr[8192] = 0x55;
    let mut mapper = ColorDreams::new(prg, chr, Mirroring::Horizontal);

    mapper.cpu_write(0x8000, 0x33);

    assert_eq!(mapper.cpu_read(0x8001), 1);
    assert_eq!(mapper.chr_read(0x0000), 0x55);
}

// ─── VRC2a (Mapper 22) ─────────────────────────────────────────────

#[test]
fn parse_valid_vrc2a_mapper_22() {
    let mut data = make_ines(8, 8, 0x60);
    data[7] = 0x10;
    let parsed = parse_ines(&data).expect("mapper 22 should parse");

    assert_eq!(parsed.header.mapper_number, 22);
    assert_eq!(parsed.mapper.mirroring(), Mirroring::Vertical);
}

#[test]
fn vrc2a_switches_two_prg_banks_and_fixes_last_16k() {
    let mut prg = vec![0u8; 8 * 8192];
    for bank in 0..8usize {
        for byte in &mut prg[bank * 8192..(bank + 1) * 8192] {
            *byte = bank as u8;
        }
    }
    let mut mapper = Vrc2a::new(prg, vec![0u8; 8192]);

    mapper.cpu_write(0x8000, 2);
    mapper.cpu_write(0xA000, 4);

    assert_eq!(mapper.cpu_read(0x8000), 2);
    assert_eq!(mapper.cpu_read(0xA000), 4);
    assert_eq!(mapper.cpu_read(0xC000), 6);
    assert_eq!(mapper.cpu_read(0xE000), 7);
}

#[test]
fn vrc2a_switches_1k_chr_banks_with_a1_a0_register_order() {
    let prg = vec![0u8; 4 * 8192];
    let mut chr = vec![0u8; 64 * 1024];
    for bank in 0..64usize {
        chr[bank * 1024] = bank as u8;
    }
    let mut mapper = Vrc2a::new(prg, chr);

    mapper.cpu_write(0xB000, 0x06);
    mapper.cpu_write(0xB002, 0x00);
    mapper.cpu_write(0xB001, 0x0A);
    mapper.cpu_write(0xB003, 0x00);

    assert_eq!(mapper.chr_read(0x0000), 3);
    assert_eq!(mapper.chr_read(0x0400), 5);
}

#[test]
fn vrc2a_mirroring_and_latch_are_writable() {
    let mut mapper = Vrc2a::new(vec![0u8; 4 * 8192], Vec::new());

    assert_eq!(mapper.mirroring(), Mirroring::Vertical);
    mapper.cpu_write(0x9000, 0xFF);
    assert_eq!(mapper.mirroring(), Mirroring::Horizontal);

    mapper.cpu_write(0x6000, 1);
    assert_eq!(mapper.cpu_read(0x6100), 0x61);
    mapper.cpu_write(0x6000, 0);
    assert_eq!(mapper.cpu_read(0x6100), 0x60);
}

// ─── Action 53 (Mapper 28) ─────────────────────────────────────────

#[test]
fn parse_valid_action53_mapper_28() {
    let mut data = make_ines(8, 0, 0xC0);
    data[7] = 0x10;
    let parsed = parse_ines(&data).expect("mapper 28 should parse");

    assert_eq!(parsed.header.mapper_number, 28);
}

#[test]
fn action53_switches_32k_prg_banks() {
    let mut prg = vec![0u8; 8 * 16384];
    for bank in 0..8usize {
        for byte in &mut prg[bank * 16384..(bank + 1) * 16384] {
            *byte = bank as u8;
        }
    }
    let mut mapper = Action53::new(prg, Vec::new());

    mapper.cpu_write(0x5000, 0x80);
    mapper.cpu_write(0x8000, 0x10);
    mapper.cpu_write(0x5000, 0x81);
    mapper.cpu_write(0x8000, 0x02);
    mapper.cpu_write(0x5000, 0x01);
    mapper.cpu_write(0x8000, 0x01);

    assert_eq!(mapper.cpu_read(0x8000), 6);
    assert_eq!(mapper.cpu_read(0xC000), 7);
}

#[test]
fn action53_supports_unrom_fixed_high_mode_and_mirroring() {
    let mut prg = vec![0u8; 8 * 16384];
    for bank in 0..8usize {
        for byte in &mut prg[bank * 16384..(bank + 1) * 16384] {
            *byte = bank as u8;
        }
    }
    let mut mapper = Action53::new(prg, Vec::new());

    mapper.cpu_write(0x5000, 0x80);
    mapper.cpu_write(0x8000, 0x1F);
    assert_eq!(mapper.mirroring(), Mirroring::Horizontal);
    mapper.cpu_write(0x5000, 0x81);
    mapper.cpu_write(0x8000, 0x02);
    mapper.cpu_write(0x5000, 0x01);
    mapper.cpu_write(0x8000, 0x02);

    assert_eq!(mapper.cpu_read(0x8000), 2);
    assert_eq!(mapper.cpu_read(0xC000), 5);
}

#[test]
fn action53_switches_chr_ram_banks() {
    let prg = vec![0u8; 4 * 16384];
    let mut mapper = Action53::new(prg, Vec::new());

    mapper.chr_write(0x0000, 0x11);
    mapper.cpu_write(0x5000, 0x00);
    mapper.cpu_write(0x8000, 0x01);
    mapper.chr_write(0x0000, 0x22);

    assert_eq!(mapper.chr_read(0x0000), 0x22);
    mapper.cpu_write(0x8000, 0x00);
    assert_eq!(mapper.chr_read(0x0000), 0x11);
}

// ─── Sunsoft-4 (Mapper 68) ─────────────────────────────────────────

#[test]
fn parse_valid_sunsoft4_mapper_68() {
    let mut data = make_ines(8, 32, 0x40);
    data[7] = 0x40;
    let parsed = parse_ines(&data).expect("mapper 68 should parse");

    assert_eq!(parsed.header.mapper_number, 68);
    assert_eq!(parsed.mapper.mirroring(), Mirroring::Horizontal);
}

#[test]
fn sunsoft4_switches_prg_and_chr_banks() {
    let mut prg = vec![0u8; 8 * 16384];
    for bank in 0..8usize {
        prg[bank * 16384] = bank as u8;
    }
    let mut chr = vec![0u8; 8 * 2048];
    for bank in 0..8usize {
        chr[bank * 2048] = (0x80 + bank) as u8;
    }
    let mut mapper = Sunsoft4::new(prg, chr, Mirroring::Vertical);

    mapper.cpu_write(0xF000, 3);
    mapper.cpu_write(0x8000, 4);
    mapper.cpu_write(0x9000, 5);

    assert_eq!(mapper.cpu_read(0x8000), 3);
    assert_eq!(mapper.cpu_read(0xC000), 7);
    assert_eq!(mapper.chr_read(0x0000), 0x84);
    assert_eq!(mapper.chr_read(0x0800), 0x85);
}

#[test]
fn sunsoft4_can_source_nametables_from_chr_rom() {
    let prg = vec![0u8; 2 * 16384];
    let mut chr = vec![0u8; 132 * 1024];
    chr[0x80 * 1024] = 0x41;
    chr[0x81 * 1024] = 0x42;
    let mut mapper = Sunsoft4::new(prg, chr, Mirroring::Horizontal);

    assert_eq!(mapper.nametable_read(0x2000), None);
    mapper.cpu_write(0xC000, 0x00);
    mapper.cpu_write(0xD000, 0x01);
    mapper.cpu_write(0xE000, 0x11);

    assert_eq!(mapper.nametable_read(0x2000), Some(0x41));
    assert_eq!(mapper.nametable_read(0x2800), Some(0x42));
    assert!(mapper.nametable_write(0x2000, 0x99));
}

// ─── NES 2.0 header ────────────────────────────────────────────────

#[test]
fn nes2_detected_and_parsed() {
    let data = make_nes2(2, 1, 0);
    let parsed = parse_ines(&data).expect("NES 2.0 parse failed");
    assert_eq!(parsed.header.mapper_number, 0);
    assert_eq!(parsed.mapper.cpu_read(0x8000), 0x00);
}

#[test]
fn nes2_mapper_number_12bit() {
    // Mapper 256 is beyond the 8-bit range, so a correct NES
    // 2.0 parser sees it as mapper 256 and this port rejects
    // it as unsupported. This confirms the 12-bit extraction
    // is wired in.
    let data = make_nes2(1, 1, 256);
    let err = expect_err(parse_ines(&data), "mapper 256 should be rejected");
    assert!(err.contains("256"), "got: {err}");
}

#[test]
fn ines1_still_works_after_nes2_support() {
    // An iNES 1.0 file must keep parsing correctly even though
    // the parser now has an NES 2.0 branch.
    let data = make_ines(2, 1, 0x01);
    let parsed = parse_ines(&data).expect("iNES 1.0 parse failed");
    assert_eq!(parsed.mapper.mirroring(), Mirroring::Vertical);
}
