//! Boot screenshot tests for AGA (Advanced Graphics Architecture) Amiga models.

mod common;

use common::{boot_screenshot_test, load_rom, BOOT_TICKS};
use machine_amiga::{AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};

#[test]
#[ignore]
fn test_boot_kick30_a1200() {
    let Some(rom) = load_rom("../../roms/kick30_39_106_a1200.rom") else {
        return;
    };
    boot_screenshot_test(
        AmigaConfig {
            model: AmigaModel::A1200,
            chipset: AmigaChipset::Aga,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 0,
        },
        "KS 3.0 A1200",
        "boot_kick30_a1200",
        BOOT_TICKS,
    );
}

#[test]
#[ignore]
fn test_boot_kick31_a1200() {
    let Some(rom) = load_rom("../../roms/kick31_40_068_a1200.rom") else {
        return;
    };
    boot_screenshot_test(
        AmigaConfig {
            model: AmigaModel::A1200,
            chipset: AmigaChipset::Aga,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 0,
        },
        "KS 3.1 A1200",
        "boot_kick31_a1200",
        BOOT_TICKS,
    );
}

#[test]
#[ignore]
fn test_boot_kick30_a4000() {
    let Some(rom) = load_rom("../../roms/kick30_39_106_a4000.rom") else {
        return;
    };
    boot_screenshot_test(
        AmigaConfig {
            model: AmigaModel::A4000,
            chipset: AmigaChipset::Aga,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 0,
        },
        "KS 3.0 A4000",
        "boot_kick30_a4000",
        BOOT_TICKS,
    );
}

#[test]
#[ignore]
fn test_boot_kick31_a4000() {
    let Some(rom) = load_rom("../../roms/kick31_40_068_a4000.rom") else {
        return;
    };
    boot_screenshot_test(
        AmigaConfig {
            model: AmigaModel::A4000,
            chipset: AmigaChipset::Aga,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 0,
        },
        "KS 3.1 A4000",
        "boot_kick31_a4000",
        BOOT_TICKS,
    );
}
