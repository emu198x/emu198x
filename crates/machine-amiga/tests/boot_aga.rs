//! Boot screenshot tests for AGA (Advanced Graphics Architecture) Amiga models.
//!
//! AGA machines need longer boot time than OCS/ECS because the KS 3.x serial
//! diagnostic loop runs its full ~200ms timeout per call (no serial device
//! attached → RBF stays 0). On the 68EC020, each call takes ~6s due to slow
//! CIA E-clock access in the polling loop, and there are multiple calls.

mod common;

use common::{boot_screenshot_test, load_rom, BOOT_TICKS};
use machine_amiga::{AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};

/// AGA boot needs longer than OCS/ECS. The A1200 ROM has an extended
/// serial diagnostic calibrated for the 68EC020 pipeline speed. Our
/// 68000 microcode timing makes each timeout loop take ~27s instead of
/// ~200ms, and the warm-reset cycle can't converge until we implement
/// proper 68020 instruction timing. For now these tests capture
/// screenshots and register state but don't assert boot completion.
const AGA_BOOT_TICKS: u64 = BOOT_TICKS; // same as OCS/ECS for now

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
        AGA_BOOT_TICKS,
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
        AGA_BOOT_TICKS,
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
        AGA_BOOT_TICKS,
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
        AGA_BOOT_TICKS,
    );
}
