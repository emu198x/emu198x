//! Boot screenshot tests for AGA (Advanced Graphics Architecture) Amiga models.
//!
//! A1200 tests assert full insert-disk screen (STRAP display with sprites).
//! A4000 tests capture screenshots but don't assert boot completion — the
//! 68040 model needs additional work before boot reaches the STRAP display.

mod common;

use common::{BootExpect, BOOT_TICKS, boot_screenshot_test_expect, load_rom};
use machine_amiga::{AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};

const AGA_BOOT_TICKS: u64 = BOOT_TICKS;

#[test]
#[ignore]
fn test_boot_kick30_a1200() {
    let Some(rom) = load_rom("../../roms/kick30_39_106_a1200.rom") else {
        return;
    };
    boot_screenshot_test_expect(
        AmigaConfig {
            model: AmigaModel::A1200,
            chipset: AmigaChipset::Aga,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        },
        "KS 3.0 A1200",
        "boot_kick30_a1200",
        AGA_BOOT_TICKS,
        BootExpect {
            dmacon_set: Some(0x03C0), // bitplane + copper + blitter + sprite DMA
            bplcon0: Some(0x8303),    // HIRES, COLOR, GAUD, ERSY, LACE off
            ..Default::default()
        },
    );
}

#[test]
#[ignore]
fn test_boot_kick31_a1200() {
    let Some(rom) = load_rom("../../roms/kick31_40_068_a1200.rom") else {
        return;
    };
    boot_screenshot_test_expect(
        AmigaConfig {
            model: AmigaModel::A1200,
            chipset: AmigaChipset::Aga,
            region: AmigaRegion::Pal,
            kickstart: rom,
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        },
        "KS 3.1 A1200",
        "boot_kick31_a1200",
        AGA_BOOT_TICKS,
        BootExpect {
            dmacon_set: Some(0x03C0),
            bplcon0: Some(0x8303),
            viewport_hash: Some(0x90B8E2E9ABEE586B),
            ..Default::default()
        },
    );
}

// Future: A4000 needs 68040 timing, Ramsey, Buster, SCSI.
// Boot doesn't reach STRAP display — lower priority than A1200.
//
// #[test]
// #[ignore]
// fn test_boot_kick30_a4000() { .. }
// fn test_boot_kick31_a4000() { .. }
