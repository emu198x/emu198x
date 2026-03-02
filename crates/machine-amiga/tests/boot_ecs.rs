//! Boot screenshot tests for ECS (Enhanced Chip Set) Amiga models.

mod common;

use common::{boot_screenshot_test, load_rom, BOOT_TICKS};
use machine_amiga::{AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};

#[test]
#[ignore]
fn test_boot_kick204_a500plus() {
    let Some(rom) = load_rom("../../roms/kick204_37_175_a500plus.rom") else {
        return;
    };
    boot_screenshot_test(
        AmigaConfig {
            model: AmigaModel::A500Plus,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 0,
        },
        "KS 2.04 A500+",
        "boot_kick204_a500plus",
        BOOT_TICKS,
    );
}

#[test]
#[ignore]
fn test_boot_kick205_a600() {
    let Some(rom) = load_rom("../../roms/kick205_37_300_a600hd.rom") else {
        return;
    };
    boot_screenshot_test(
        AmigaConfig {
            model: AmigaModel::A600,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 0,
        },
        "KS 2.05 A600",
        "boot_kick205_a600",
        BOOT_TICKS,
    );
}

#[test]
#[ignore]
fn test_boot_kick31_a500() {
    let Some(rom) = load_rom("../../roms/kick31_40_063_a500_a600_a2000.rom") else {
        return;
    };
    boot_screenshot_test(
        AmigaConfig {
            model: AmigaModel::A500Plus,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 0,
        },
        "KS 3.1 A500",
        "boot_kick31_a500",
        BOOT_TICKS,
    );
}

#[test]
#[ignore]
fn test_boot_kick31_a600() {
    let Some(rom) = load_rom("../../roms/kick31_40_063_a500_a600_a2000.rom") else {
        return;
    };
    boot_screenshot_test(
        AmigaConfig {
            model: AmigaModel::A600,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 0,
        },
        "KS 3.1 A600",
        "boot_kick31_a600",
        BOOT_TICKS,
    );
}

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
        },
        "KS 1.3 A3000",
        "boot_kick13_a3000",
        BOOT_TICKS,
    );
}

#[test]
#[ignore]
fn test_boot_kick202_a3000() {
    let Some(rom) = load_rom("../../roms/kick202_36_207_a3000.rom") else {
        return;
    };
    boot_screenshot_test(
        AmigaConfig {
            model: AmigaModel::A3000,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 0,
        },
        "KS 2.02 A3000",
        "boot_kick202_a3000",
        BOOT_TICKS,
    );
}

#[test]
#[ignore]
fn test_boot_kick31_a3000() {
    let Some(rom) = load_rom("../../roms/kick31_40_068_a3000.rom") else {
        return;
    };
    boot_screenshot_test(
        AmigaConfig {
            model: AmigaModel::A3000,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 0,
        },
        "KS 3.1 A3000",
        "boot_kick31_a3000",
        BOOT_TICKS,
    );
}
