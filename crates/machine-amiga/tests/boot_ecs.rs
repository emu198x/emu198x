//! Boot screenshot tests for ECS (Enhanced Chip Set) Amiga models.

mod common;

use common::{BOOT_TICKS, BootExpect, boot_screenshot_test, boot_screenshot_test_expect, load_rom};
use machine_amiga::{AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};

/// A3000/A4000 need extra time: 68030 RomTag scan covers 4 MB via chip bus.
const A3000_BOOT_TICKS: u64 = 2_550_000_000; // ~90 seconds PAL

/// ECS insert-disk screen: hires, 3 planes (KS 2.x/3.x).
const EXPECT_INSERT_DISK_HIRES: BootExpect = BootExpect {
    dmacon_set: Some(0x0180),
    bplcon0: Some(0x8302), // 3 planes, hires (KS 2.04 uses $8303 on some variants)
    min_unique_colours: None,
    viewport_hash: None,
};

/// KS 3.1 ECS insert-disk: hires, 3 planes (may have ERSY bit).
const EXPECT_INSERT_DISK_KS31_ECS: BootExpect = BootExpect {
    dmacon_set: Some(0x0180),
    bplcon0: Some(0x8303),
    min_unique_colours: None,
    viewport_hash: None,
};

#[test]
#[ignore]
fn test_boot_kick204_a500plus() {
    let Some(rom) = load_rom("../../roms/kick204_37_175_a500plus.rom") else {
        return;
    };
    boot_screenshot_test_expect(
        AmigaConfig {
            model: AmigaModel::A500Plus,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        },
        "KS 2.04 A500+",
        "boot_kick204_a500plus",
        BOOT_TICKS,
        BootExpect {
            viewport_hash: Some(0x234FB7B73158ED52),
            ..EXPECT_INSERT_DISK_HIRES
        },
    );
}

#[test]
#[ignore]
fn test_boot_kick205_a600() {
    let Some(rom) = load_rom("../../roms/kick205_37_300_a600hd.rom") else {
        return;
    };
    boot_screenshot_test_expect(
        AmigaConfig {
            model: AmigaModel::A600,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        },
        "KS 2.05 A600",
        "boot_kick205_a600",
        BOOT_TICKS,
        BootExpect {
            viewport_hash: Some(0x63686AFA53E09FDD),
            ..EXPECT_INSERT_DISK_HIRES
        },
    );
}

#[test]
#[ignore]
fn test_boot_kick31_a500() {
    let Some(rom) = load_rom("../../roms/kick31_40_063_a500_a600_a2000.rom") else {
        return;
    };
    boot_screenshot_test_expect(
        AmigaConfig {
            model: AmigaModel::A500Plus,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        },
        "KS 3.1 A500",
        "boot_kick31_a500",
        BOOT_TICKS,
        BootExpect {
            viewport_hash: Some(0x9FC626634C8E1A25),
            ..EXPECT_INSERT_DISK_KS31_ECS
        },
    );
}

#[test]
#[ignore]
fn test_boot_kick31_a600() {
    let Some(rom) = load_rom("../../roms/kick31_40_063_a500_a600_a2000.rom") else {
        return;
    };
    boot_screenshot_test_expect(
        AmigaConfig {
            model: AmigaModel::A600,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        },
        "KS 3.1 A600",
        "boot_kick31_a600",
        BOOT_TICKS,
        BootExpect {
            viewport_hash: Some(0x9FC626634C8E1A25),
            ..EXPECT_INSERT_DISK_KS31_ECS
        },
    );
}

// A3000 variants: RAMSEY/Fat Gary at $DE0000, PMOVE stub, 32-bit
// address fallthrough, 68030 instruction cache, 2 MB fast RAM.

#[test]
#[ignore]
fn test_boot_kick13_a3000() {
    let Some(rom) = load_rom("../../roms/kick13_34_005_a3000.rom") else {
        return;
    };
    boot_screenshot_test(
        AmigaConfig {
            model: AmigaModel::A3000,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        },
        "KS 1.3 A3000",
        "boot_kick13_a3000",
        A3000_BOOT_TICKS,
    );
}

#[test]
#[ignore]
fn test_boot_kick202_a3000() {
    let Some(rom) = load_rom("../../roms/kick202_36_207_a3000.rom") else {
        return;
    };
    boot_screenshot_test_expect(
        AmigaConfig {
            model: AmigaModel::A3000,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        },
        "KS 2.02 A3000",
        "boot_kick202_a3000",
        A3000_BOOT_TICKS,
        BootExpect {
            dmacon_set: Some(0x0180),
            ..Default::default()
        },
    );
}

#[test]
#[ignore]
fn test_boot_kick31_a3000() {
    let Some(rom) = load_rom("../../roms/kick31_40_068_a3000.rom") else {
        return;
    };
    boot_screenshot_test_expect(
        AmigaConfig {
            model: AmigaModel::A3000,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        },
        "KS 3.1 A3000",
        "boot_kick31_a3000",
        A3000_BOOT_TICKS,
        BootExpect {
            dmacon_set: Some(0x0180),
            ..Default::default()
        },
    );
}
