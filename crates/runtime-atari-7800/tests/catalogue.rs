use emu198x_shell::{FamilyRuntime, FirmwareSet, MachineCore, ResetKind};
use runtime_atari_7800::{Atari7800Runtime, Model};

#[test]
fn restored_cartridge_survives_reset_and_regional_replacement() {
    let rom = vec![0x5a; 16384];
    for model in Model::ALL {
        let source = Atari7800Runtime::new(model, rom.clone()).expect("source");
        let snapshot = source.snapshot().expect("snapshot");
        let mut restored =
            Atari7800Runtime::new(model, vec![0x42; 16384]).expect("other cartridge");
        restored.restore(&snapshot).expect("restore");
        assert_eq!(restored.snapshot().expect("snapshot"), snapshot);
        restored.machine_mut().expect("machine").poke(6144, 0xa5);
        restored.reset(ResetKind::Hard);
        assert_eq!(restored.snapshot().expect("reset snapshot"), snapshot);
        for target in Model::ALL {
            let replacement = restored
                .replacement(target, &FirmwareSet::new())
                .expect("replacement");
            let fresh = Atari7800Runtime::new(target, rom.clone()).expect("fresh");
            assert_eq!(
                replacement.snapshot().expect("replacement snapshot"),
                fresh.snapshot().expect("fresh snapshot")
            );
            assert_eq!(replacement.machine().expect("machine").peek(49152), 0x5a);
            assert_eq!(replacement.native_frame_ticks(), target.frame_ticks());
            assert_eq!(Model::from_variant_id(target.variant_id()), Some(target));
        }
        let blank = Atari7800Runtime::blank(model).snapshot().expect("blank");
        restored.restore(&blank).expect("restore blank");
        restored.reset(ResetKind::Hard);
        assert!(!restored.cartridge_loaded());
    }
}

#[test]
fn invalid_cartridge_does_not_poison_the_next_reset_or_switch() {
    let mut runtime = Atari7800Runtime::new(Model::default(), vec![0x5a; 16384]).expect("runtime");
    let before = runtime.snapshot().expect("snapshot");
    assert!(runtime.insert_cartridge(vec![0; 200_000]).is_err());
    runtime.reset(ResetKind::Hard);
    assert_eq!(runtime.snapshot().expect("snapshot"), before);
    assert!(
        runtime
            .replacement(Model::A7800Pal, &FirmwareSet::new())
            .is_ok()
    );
}

#[test]
fn a78_mapper_ram_and_pokey_configuration_survive_restoration_and_switching() {
    let mut image = vec![0; 128];
    image[0] = 4;
    image[1..10].copy_from_slice(b"ATARI7800");
    image[49..53].copy_from_slice(&131_072_u32.to_be_bytes());
    image[64] = 1; // SuperGame
    image[65] = 1; // 16 KiB cartridge RAM
    image[67] = 1; // POKEY at $0440
    image.extend(vec![0x5a; 131_072]);
    let mut source = Atari7800Runtime::new(Model::A7800Ntsc, image.clone()).expect("A78");
    source.machine_mut().expect("machine").poke(0x4000, 0x42);
    assert_eq!(source.machine().expect("machine").peek(0x4000), 0x42);
    let snapshot = source.snapshot().expect("snapshot");
    let mut restored = Atari7800Runtime::blank(Model::A7800Ntsc);
    restored.restore(&snapshot).expect("restore");
    assert_eq!(restored.machine().expect("machine").peek(0x4000), 0x42);
    for model in Model::ALL {
        let replacement = restored
            .replacement(model, &FirmwareSet::new())
            .expect("switch");
        let fresh = Atari7800Runtime::new(model, image.clone()).expect("fresh A78");
        // The complete snapshot includes mapper, cartridge RAM and the POKEY chip.
        assert_eq!(
            replacement.snapshot().expect("snapshot"),
            fresh.snapshot().expect("snapshot")
        );
        assert_eq!(replacement.machine().expect("machine").peek(0x4000), 0);
    }
}
