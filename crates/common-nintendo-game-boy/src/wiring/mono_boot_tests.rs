//! Local original firmware only; committed cartridge bytes are project-authored.
use super::*;

#[test]
#[ignore = "FIXTURE: needs original DMG0/DMG/MGB boot ROMs via EMU198X_GB_BOOT_ROM_DIR"]
fn original_firmware_checks_logo_checksum_and_hands_off_with_hardware_registers() {
    let directory = std::path::PathBuf::from(
        std::env::var("EMU198X_GB_BOOT_ROM_DIR").expect("firmware directory"),
    );
    // The project demo's skipped-boot header intentionally leaves its logo blank.
    // Build a valid boot input in memory from the known DMG ROM's logo data table;
    // neither the loaded cartridge nor the firmware file is modified.
    let logo_reference =
        std::fs::read(directory.join("dmg_boot.bin")).expect("DMG reference logo table");
    assert_eq!(logo_reference.len(), 256);
    let cartridge = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/synthetic-cartridges/nintendo-game-boy-logo.gb"),
    )
    .expect("original project cartridge");
    for (model, id, file, expected) in [
        (
            BootProfile::Dmg0,
            "dmg0-boot-rom",
            "dmg0_boot.bin",
            [0x01, 0x00, 0xFF, 0x13, 0x00, 0xC1, 0x84, 0x03],
        ),
        (
            BootProfile::DmgAbc,
            "dmg-boot-rom",
            "dmg_boot.bin",
            [0x01, 0xB0, 0x00, 0x13, 0x00, 0xD8, 0x01, 0x4D],
        ),
        (
            BootProfile::Mgb,
            "mgb-boot-rom",
            "mgb_boot.bin",
            [0xFF, 0xB0, 0x00, 0x13, 0x00, 0xD8, 0x01, 0x4D],
        ),
    ] {
        let firmware = std::fs::read(directory.join(file)).expect("local original boot ROM");
        assert_eq!(firmware.len(), 256);
        for failure in [None, Some(0x104usize), Some(0x14D)] {
            let mut bytes = cartridge.clone();
            bytes[0x104..0x134].copy_from_slice(&logo_reference[0xA8..0xD8]);
            if let Some(offset) = failure {
                bytes[offset] ^= 1;
            }
            let cart = Cartridge::new(bytes, CartType::RomOnly { battery: false }, 0);
            let mut machine =
                GameBoyState::try_new_with_boot_rom(cart, model, Some(firmware.clone()))
                    .expect("original boot firmware");
            assert_eq!(machine.cpu().pc, 0);
            let mut unmapped = None;
            for cycle in 0..8_000_000u32 {
                machine.step_m_cycle();
                if machine.peek(0xFF50) == 0xFF {
                    unmapped = Some(cycle + 1);
                    break;
                }
            }
            if failure.is_some() {
                assert!(unmapped.is_none(), "{} accepted bad header {failure:?}", id);
                assert_eq!(machine.peek(0xFF50), 0xFE);
            } else {
                let cycles = unmapped.unwrap_or_else(|| {
                    panic!(
                        "{} boot did not hand off: PC={:04x} LCDC={:02x} LY={} DIV={:04x}",
                        id,
                        machine.cpu().pc,
                        machine.peek(0xFF40),
                        machine.peek(0xFF44),
                        machine.peek(0xFF04)
                    )
                });
                let cpu = machine.cpu();
                assert_eq!(cpu.pc, 0x100);
                assert_eq!(cpu.sp, 0xFFFE);
                assert_eq!(
                    [cpu.a, cpu.f, cpu.b, cpu.c, cpu.d, cpu.e, cpu.h, cpu.l],
                    expected,
                    "{} registers",
                    id
                );
                assert!(
                    machine.framebuffer().iter().any(|&pixel| pixel != 0),
                    "boot logo must render"
                );
                println!(
                    "{} handoff: {cycles} m-cycles; DIV={:04x}; registers agree with whichboot hardware values",
                    id, machine.timer.counter
                );
            }
        }
    }
}
