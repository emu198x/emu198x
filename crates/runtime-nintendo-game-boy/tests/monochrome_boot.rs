//! Optional firmware must reach the real machine, not just its catalogue.
mod common;
use emu198x_shell::{FirmwareImage, FirmwareSet, MachineCore, MediaImage, MediaKind, MediaSet};
use runtime_nintendo_game_boy::{GameBoyRuntime, Model};

fn boot() -> Vec<u8> {
    let mut bytes = vec![0; 256];
    bytes[..3].copy_from_slice(&[0xC3, 0xFC, 0]);
    bytes[0xFC..].copy_from_slice(&[0x3E, 1, 0xE0, 0x50]);
    bytes
}

#[test]
fn optional_monochrome_firmware_is_executed_and_unmapped() {
    for (model, id) in [
        (Model::Dmg0, "dmg0-boot-rom"),
        (Model::Dmg, "dmg-boot-rom"),
        (Model::Mgb, "mgb-boot-rom"),
    ] {
        let firmware = boot();
        let mut set = FirmwareSet::new();
        set.push(FirmwareImage::new(id, &firmware));
        let mut runtime =
            GameBoyRuntime::from_firmware(model, &set).expect("optional monochrome firmware");
        let mut cartridge = common::loop_rom();
        cartridge[0] = 0x55;
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            "cartridge",
            MediaKind::Cartridge,
            &cartridge,
        ));
        runtime.load_media(&media).expect("synthetic cartridge");
        let machine = runtime.machine_mut().expect("loaded");
        assert_eq!(machine.cpu().pc, 0, "firmware must start at reset vector");
        assert_eq!(machine.peek(0), 0xC3);
        assert_eq!(machine.peek(0x100), 0x18);
        assert_eq!(machine.peek(0xFF50), 0xFE);
        for _ in 0..100 {
            machine.step_m_cycle();
            if machine.peek(0xFF50) == 0xFF {
                break;
            }
        }
        assert_eq!(machine.peek(0xFF50), 0xFF);
        assert_eq!(machine.peek(0), 0x55);
        machine.poke(0xFF50, 0);
        assert_eq!(machine.peek(0), 0x55, "cannot remap by writing zero");
    }
}

#[test]
fn boot_mapping_and_latch_survive_snapshot_and_cold_reset() {
    use emu198x_shell::ResetKind;
    for (model, id) in [
        (Model::Dmg0, "dmg0-boot-rom"),
        (Model::Dmg, "dmg-boot-rom"),
        (Model::Mgb, "mgb-boot-rom"),
    ] {
        let firmware = boot();
        let mut set = FirmwareSet::new();
        set.push(FirmwareImage::new(id, &firmware));
        let mut original = GameBoyRuntime::from_firmware(model, &set).expect("firmware");
        // Also preserve selected firmware when no cartridge is loaded yet.
        let empty = original.snapshot().expect("empty boot-equipped runtime");
        let mut runtime = GameBoyRuntime::blank(model);
        runtime.restore(&empty).expect("firmware-only state");
        let bytes = common::loop_rom();
        let mut media = MediaSet::new();
        media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &bytes));
        runtime.load_media(&media).expect("cartridge");
        let machine = runtime.machine_mut().expect("loaded");
        assert_eq!(machine.cpu().pc, 0);
        machine.poke(0xFF50, 2);
        assert_eq!(machine.peek(0xFF50), 0xFE);
        machine.step_instruction(); // JP $00FC, still mapped
        assert_eq!(machine.cpu().pc, 0xFC);
        let middle = runtime.snapshot().expect("mid-boot snapshot");
        let mut restored = GameBoyRuntime::blank(model);
        restored.restore(&middle).expect("restore mid-boot");
        assert_eq!(restored.snapshot().expect("same state"), middle);
        for target in [&mut runtime, &mut restored] {
            let machine = target.machine_mut().expect("loaded");
            machine.step_instruction();
            machine.step_instruction();
            assert_eq!(machine.cpu().pc, 0x100);
            assert_eq!(machine.peek(0xFF50), 0xFF);
            machine.poke(0xFF50, 0);
            assert_eq!(machine.peek(0xFF50), 0xFF);
        }
        assert_eq!(
            runtime.snapshot().expect("continued"),
            restored.snapshot().expect("replayed")
        );
        let after = runtime.snapshot().expect("unmapped state");
        original.restore(&after).expect("restore after unmap");
        original.reset(ResetKind::Hard);
        assert_eq!(original.machine().expect("reset machine").cpu().pc, 0);
        assert_eq!(
            original.machine().expect("reset machine").peek(0xFF50),
            0xFE
        );
    }
}

#[test]
fn wrong_firmware_sizes_are_errors_and_no_firmware_keeps_skipped_boot() {
    for (model, id) in [
        (Model::Dmg0, "dmg0-boot-rom"),
        (Model::Dmg, "dmg-boot-rom"),
        (Model::Mgb, "mgb-boot-rom"),
    ] {
        for size in [0, 1, 255, 257, 2304] {
            let bytes = vec![0; size];
            let mut set = FirmwareSet::new();
            set.push(FirmwareImage::new(id, &bytes));
            assert!(
                GameBoyRuntime::from_firmware(model, &set).is_err(),
                "size {size}"
            );
        }
        let mut runtime =
            GameBoyRuntime::from_firmware(model, &FirmwareSet::new()).expect("optional firmware");
        let bytes = common::loop_rom();
        let mut media = MediaSet::new();
        media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &bytes));
        runtime.load_media(&media).expect("cartridge");
        assert_eq!(runtime.machine().expect("loaded").cpu().pc, 0x100);
    }
}
