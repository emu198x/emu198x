use emu198x_shell::{FamilyRuntime, FirmwareSet, MachineCore, ResetKind};
use runtime_atari_2600::{Atari2600Runtime, Model};

#[test]
fn restored_cartridge_survives_reset_and_regional_replacement() {
    let rom = vec![0x5a; 4096];
    for model in Model::ALL {
        let source = Atari2600Runtime::new(model, rom.clone()).expect("source");
        let snapshot = source.snapshot().expect("snapshot");
        let mut restored = Atari2600Runtime::new(model, vec![0x42; 4096]).expect("other cartridge");
        restored.restore(&snapshot).expect("restore");
        assert_eq!(restored.snapshot().expect("snapshot"), snapshot);
        restored.machine_mut().expect("machine").poke(128, 0xa5);
        restored.reset(ResetKind::Hard);
        assert_eq!(restored.snapshot().expect("reset snapshot"), snapshot);
        for target in Model::ALL {
            let replacement = restored
                .replacement(target, &FirmwareSet::new())
                .expect("replacement");
            let fresh = Atari2600Runtime::new(target, rom.clone()).expect("fresh");
            assert_eq!(
                replacement.snapshot().expect("replacement snapshot"),
                fresh.snapshot().expect("fresh snapshot")
            );
            assert_eq!(replacement.machine().expect("machine").peek(4096), 0x5a);
            assert_eq!(replacement.native_frame_ticks(), target.frame_ticks());
            assert_eq!(Model::from_variant_id(target.variant_id()), Some(target));
        }
        let blank = Atari2600Runtime::blank(model).snapshot().expect("blank");
        restored.restore(&blank).expect("restore blank");
        restored.reset(ResetKind::Hard);
        assert!(!restored.cartridge_loaded());
    }
}

#[test]
fn invalid_cartridge_does_not_poison_the_next_reset_or_switch() {
    let mut runtime = Atari2600Runtime::new(Model::default(), vec![0x5a; 4096]).expect("runtime");
    let before = runtime.snapshot().expect("snapshot");
    assert!(runtime.insert_cartridge(vec![0; 5000]).is_err());
    runtime.reset(ResetKind::Hard);
    assert_eq!(runtime.snapshot().expect("snapshot"), before);
    assert!(
        runtime
            .replacement(Model::Vcs2600Pal, &FirmwareSet::new())
            .is_ok()
    );
}

#[test]
fn banked_and_supercharger_images_cold_boot_like_fresh_cartridges() {
    for size in [8192, 10240, 12288, 16384, 32768, 65536, 8448] {
        let image = vec![0x5a; size];
        let mut source =
            Atari2600Runtime::new(Model::Vcs2600Ntsc, image.clone()).expect("cartridge");
        source.machine_mut().expect("machine").poke(0x1ff8, 0);
        let snapshot = source.snapshot().expect("snapshot");
        let mut restored = Atari2600Runtime::blank(Model::Vcs2600Ntsc);
        restored.restore(&snapshot).expect("restore");
        for model in Model::ALL {
            let replacement = restored
                .replacement(model, &FirmwareSet::new())
                .expect("switch");
            let fresh = Atari2600Runtime::new(model, image.clone()).expect("fresh");
            assert_eq!(
                replacement.snapshot().expect("snapshot"),
                fresh.snapshot().expect("snapshot"),
                "{size} bytes, {model:?}"
            );
        }
    }
}
