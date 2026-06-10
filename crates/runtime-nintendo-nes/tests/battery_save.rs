//! #23: battery-backed PRG-RAM `.sav` round-trip through the runtime.
//!
//! A battery cartridge's PRG-RAM ($6000-$7FFF) must survive across
//! sessions: `cartridge_ram()` exports it to a `.sav`, and
//! `restore_cartridge_ram()` loads one back onto the bus. Carts without
//! the battery flag expose nothing to persist.

use emu198x_shell::{MachineCore, MediaImage, MediaKind, MediaSet};
use runtime_nintendo_nes::{Model, NesRuntime};

/// Minimal NROM iNES image with the battery flag (flags6 bit 1) set, so
/// it exposes battery-backed PRG-RAM at $6000-$7FFF.
fn battery_nrom_cart() -> Vec<u8> {
    let prg = vec![0xeau8; 16 * 1024];
    let chr = vec![0u8; 8 * 1024];
    let mut data = vec![0u8; 16 + prg.len() + chr.len()];
    data[0..4].copy_from_slice(b"NES\x1a");
    data[4] = 1; // 1 × 16 KiB PRG
    data[5] = 1; // 1 × 8 KiB CHR
    data[6] = 0x02; // flags6 bit 1 = battery-backed PRG-RAM
    data[16..16 + prg.len()].copy_from_slice(&prg);
    data[16 + prg.len()..].copy_from_slice(&chr);
    data
}

fn load(rom: &[u8]) -> NesRuntime {
    let mut rt = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, rom));
    rt.load_media(&media).expect("cartridge loads");
    rt
}

#[test]
fn battery_flag_gates_save_ram_exposure() {
    let rom = battery_nrom_cart();
    let rt = load(&rom);
    assert!(rt.has_battery_backed_ram(), "battery cart has save RAM");
    assert!(rt.cartridge_ram().is_some());

    // The same cart without the battery flag persists nothing.
    let mut no_batt = rom.clone();
    no_batt[6] = 0x00;
    let rt = load(&no_batt);
    assert!(!rt.has_battery_backed_ram());
    assert!(rt.cartridge_ram().is_none());
}

#[test]
fn save_ram_round_trips_through_a_sav_image() {
    let rom = battery_nrom_cart();

    // Session 1: write some save bytes through the CPU bus ($6000+).
    let mut rt = load(&rom);
    let nes = rt.machine_mut().expect("machine loaded");
    nes.mapper.cpu_write(0x6000, 0xA5);
    nes.mapper.cpu_write(0x6001, 0x5A);
    nes.mapper.cpu_write(0x7FFF, 0xC3);

    // Export the .sav image.
    let sav = rt.cartridge_ram().expect("battery RAM present").to_vec();
    assert_eq!(sav.len(), 8192, "NROM battery RAM is 8 KiB");
    assert_eq!((sav[0], sav[1], sav[8191]), (0xA5, 0x5A, 0xC3));

    // Session 2: a fresh cart starts blank; restoring the .sav brings the
    // saved bytes back onto the bus.
    let mut rt2 = load(&rom);
    assert_eq!(
        rt2.machine().expect("loaded").mapper.cpu_read(0x6000),
        0x00,
        "fresh"
    );
    rt2.restore_cartridge_ram(&sav).expect("restore");
    let m = &rt2.machine().expect("loaded").mapper;
    assert_eq!(m.cpu_read(0x6000), 0xA5);
    assert_eq!(m.cpu_read(0x6001), 0x5A);
    assert_eq!(m.cpu_read(0x7FFF), 0xC3);
}
