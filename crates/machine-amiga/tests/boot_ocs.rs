//! Boot screenshot tests for OCS (Original Chip Set) Amiga models.
//!
//! A500 and A2000 tests include 512KB slow RAM ($C00000) matching
//! the standard trapdoor expansion. Without it, KS 1.2+ exec init
//! crashes because it uses expansion space for the initial stack.

mod common;

use common::{BOOT_TICKS, BootExpect, boot_screenshot_test, boot_screenshot_test_expect, load_rom};
use machine_amiga::{AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};

/// Insert-disk screen: DMA active (bitplane + copper at minimum),
/// 2-plane lowres display.
const EXPECT_INSERT_DISK_LORES: BootExpect = BootExpect {
    dmacon_set: Some(0x0180), // bitplane DMA + copper DMA
    bplcon0: Some(0x2302),    // 2 planes, lowres, colour
    min_unique_colours: None,
    viewport_hash: None, // varies by ROM version — set per-test below
};

/// Insert-disk screen (hires variant used by KS 3.1 OCS).
const EXPECT_INSERT_DISK_HIRES: BootExpect = BootExpect {
    dmacon_set: Some(0x0180),
    bplcon0: Some(0x8302), // 3 planes, hires, colour
    min_unique_colours: None,
    viewport_hash: None,
};

#[test]
#[ignore]
fn test_boot_kick10_a1000() {
    let Some(rom) = load_rom("../../roms/kick10.rom") else {
        return;
    };
    boot_screenshot_test(
        AmigaConfig {
            model: AmigaModel::A1000,
            chipset: AmigaChipset::Ocs,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        },
        "KS 1.0 A1000",
        "boot_kick10_a1000",
        BOOT_TICKS,
    );
}

#[test]
#[ignore]
fn test_boot_kick12_a1000() {
    let Some(rom) = load_rom("../../roms/kick12_33_166_a1000.rom") else {
        return;
    };
    // A1000 has no trapdoor, but a front-panel memory expansion was
    // standard for any machine running KS 1.2. 512KB at $C00000.
    boot_screenshot_test_expect(
        AmigaConfig {
            model: AmigaModel::A1000,
            chipset: AmigaChipset::Ocs,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 512 * 1024,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        },
        "KS 1.2 A1000",
        "boot_kick12_a1000",
        BOOT_TICKS,
        EXPECT_INSERT_DISK_LORES,
    );
}

#[test]
#[ignore]
fn test_boot_kick12_a500() {
    let Some(rom) = load_rom("../../roms/kick12_33_180_a500_a1000_a2000.rom") else {
        return;
    };
    boot_screenshot_test_expect(
        AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ocs,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 512 * 1024,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        },
        "KS 1.2 A500",
        "boot_kick12_a500",
        BOOT_TICKS,
        BootExpect {
            viewport_hash: Some(0x10618BA02631C1E8),
            ..EXPECT_INSERT_DISK_LORES
        },
    );
}

#[test]
#[ignore]
fn test_boot_kick12_a2000() {
    let Some(rom) = load_rom("../../roms/kick12_33_180_a500_a1000_a2000.rom") else {
        return;
    };
    boot_screenshot_test_expect(
        AmigaConfig {
            model: AmigaModel::A2000,
            chipset: AmigaChipset::Ocs,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 512 * 1024,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        },
        "KS 1.2 A2000",
        "boot_kick12_a2000",
        BOOT_TICKS,
        BootExpect {
            viewport_hash: Some(0x10618BA02631C1E8),
            ..EXPECT_INSERT_DISK_LORES
        },
    );
}

#[test]
#[ignore]
fn test_boot_kick13_a500() {
    let Some(rom) = load_rom("../../roms/kick13.rom") else {
        return;
    };
    boot_screenshot_test_expect(
        AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ocs,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 512 * 1024,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        },
        "KS 1.3 A500",
        "boot_kick13_a500",
        BOOT_TICKS,
        BootExpect {
            viewport_hash: Some(0xEDEADF62396F56A7),
            ..EXPECT_INSERT_DISK_LORES
        },
    );
}

#[test]
#[ignore]
fn test_boot_kick13_a2000() {
    let Some(rom) = load_rom("../../roms/kick13_34_005_a500_a1000_a2000_cdtv.rom") else {
        return;
    };
    boot_screenshot_test_expect(
        AmigaConfig {
            model: AmigaModel::A2000,
            chipset: AmigaChipset::Ocs,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 512 * 1024,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        },
        "KS 1.3 A2000",
        "boot_kick13_a2000",
        BOOT_TICKS,
        BootExpect {
            viewport_hash: Some(0xEDEADF62396F56A7),
            ..EXPECT_INSERT_DISK_LORES
        },
    );
}

#[test]
#[ignore]
fn test_boot_kick31_a2000() {
    let Some(rom) = load_rom("../../roms/kick31_40_063_a500_a600_a2000.rom") else {
        return;
    };
    boot_screenshot_test_expect(
        AmigaConfig {
            model: AmigaModel::A2000,
            chipset: AmigaChipset::Ocs,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 512 * 1024,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        },
        "KS 3.1 A2000",
        "boot_kick31_a2000",
        BOOT_TICKS,
        BootExpect {
            viewport_hash: Some(0xB65A79875C823176),
            ..EXPECT_INSERT_DISK_HIRES
        },
    );
}
