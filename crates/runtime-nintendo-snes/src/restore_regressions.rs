//! Cache/state regressions found before the regional extraction.
use crate::test_cartridge as common;
use crate::{Model, SnesRuntime};
use emu198x_shell::{FamilyRuntime, FirmwareImage, FirmwareSet, MachineCore, ResetKind};
fn firmware() -> [u8; 64] {
    let mut v = [0; 64];
    v[..2].copy_from_slice(&[0x2f, 0xfe]);
    v[62..].copy_from_slice(&[0xc0, 0xff]);
    v
}
fn runtime() -> SnesRuntime {
    let ipl = firmware();
    let mut set = FirmwareSet::new();
    set.push(FirmwareImage::new("snes-ipl", &ipl));
    let mut r = SnesRuntime::from_firmware(Model::SnesNtsc, &set).expect("IPL");
    let mut rom = common::headed_cartridge(1);
    rom[0x7fd8] = 1;
    rom[0x7fd6] = 2;
    let sum = format_nintendo_snes_sfc::compute_checksum(&rom);
    rom[0x7fdc..0x7fde].copy_from_slice(&(!sum).to_le_bytes());
    rom[0x7fde..0x7fe0].copy_from_slice(&sum.to_le_bytes());
    r.insert_cartridge(rom).expect("cartridge");
    r.machine_mut().expect("machine").poke(0x700000, 0xa5);
    assert_eq!(r.machine().expect("machine").peek(0x700000), 0xa5);
    r
}
#[test]
fn restored_ipl_survives_hard_reset() {
    let live = runtime();
    let mut r = SnesRuntime::blank(Model::SnesNtsc);
    r.restore(&live.snapshot().expect("save")).expect("restore");
    r.reset(ResetKind::Hard);
    assert_eq!(r.machine().expect("machine").apu.ipl, firmware());
    assert!(r.has_original_ipl());
}
#[test]
fn sram_survives_hard_reset() {
    let mut r = runtime();
    r.reset(ResetKind::Hard);
    assert_eq!(r.machine().expect("machine").peek(0x700000), 0xa5);
}
#[test]
fn empty_restore_clears_the_cartridge_cache() {
    let empty = SnesRuntime::blank(Model::SnesNtsc)
        .snapshot()
        .expect("empty");
    let mut r = runtime();
    r.restore(&empty).expect("restore empty");
    r.reset(ResetKind::Hard);
    assert!(r.machine().is_none(), "old cartridge was resurrected");
}

#[test]
fn supplying_the_builtin_program_does_not_claim_original_firmware() {
    let rom = common_nintendo_snes::ipl::CLEAN_ROOM;
    let mut set = FirmwareSet::new();
    set.push(FirmwareImage::new("snes-ipl", &rom));
    let mut r = SnesRuntime::from_firmware(Model::SnesNtsc, &set).expect("firmware");
    assert!(!r.has_original_ipl());
    r.insert_cartridge(common::cartridge()).expect("cart");
    let saved = r.snapshot().expect("save");
    let mut restored = SnesRuntime::blank(Model::SnesNtsc);
    restored.restore(&saved).expect("restore");
    assert!(!restored.has_original_ipl());
}

#[test]
fn a_blank_save_retains_firmware_for_a_later_cartridge() {
    let ipl = firmware();
    let mut set = FirmwareSet::new();
    set.push(FirmwareImage::new("snes-ipl", &ipl));
    let live = SnesRuntime::from_firmware(Model::SnesNtsc, &set).expect("firmware");
    assert!(live.machine().is_none());
    let mut restored = SnesRuntime::blank(Model::SnesNtsc);
    restored
        .restore(&live.snapshot().expect("save"))
        .expect("restore");
    restored
        .insert_cartridge(common::cartridge())
        .expect("cartridge");
    assert_eq!(restored.machine().expect("machine").apu.ipl, ipl);
}

#[test]
fn switching_region_keeps_the_loaded_cartridges_sram() {
    let r = runtime();
    let replacement = r
        .replacement(Model::SnesPal, &FirmwareSet::new())
        .expect("switch");
    assert_eq!(replacement.machine().expect("machine").peek(0x700000), 0xa5);
}
