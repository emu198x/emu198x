use emu198x_shell::{FamilyRuntime, FirmwareImage, FirmwareSet, MachineCore, ResetKind};
use runtime_msx::{BIOS_FIRMWARE_ID, MapperType, Model, MsxRegion, MsxRuntime};

#[test]
fn regional_replacement_retains_restored_cartridges_and_cold_boots() {
    let bios = vec![0x76; 32768];
    let cart1 = vec![0x5a; 65536];
    let cart2 = vec![0x42; 65536];
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(BIOS_FIRMWARE_ID, &bios));
    for source_model in Model::ALL {
        let mut source = MsxRuntime::from_firmware(source_model, &firmware).expect("firmware");
        source.insert_cartridge1(cart1.clone(), MapperType::KonamiScc);
        source.insert_cartridge2(cart2.clone(), MapperType::Ascii16);
        source.machine_mut().expect("machine").cpu_mut().regs.pc = 1234;
        let snapshot = source.snapshot().expect("snapshot");
        let mut restored = MsxRuntime::new(source_model, vec![0; 32768]).expect("runtime");
        restored.insert_cartridge1(vec![0; 8192], MapperType::Plain);
        restored.restore(&snapshot).expect("restore");
        assert_eq!(restored.snapshot().expect("snapshot"), snapshot);
        assert_eq!(restored.cart1_bytes(), Some(cart1.as_slice()));
        assert_eq!(restored.cart2_bytes(), Some(cart2.as_slice()));
        restored.reset(ResetKind::Hard);
        assert_eq!(restored.machine().expect("machine").bios_rom(), bios);
        for model in Model::ALL {
            let replacement = restored.replacement(model, &firmware).expect("replacement");
            assert_eq!(replacement.model(), model);
            assert_eq!(
                replacement.profile().profile_id.as_str(),
                model.profile_id()
            );
            assert_eq!(replacement.native_frame_ticks(), model.frame_ticks());
            assert_eq!(Model::from_variant_id(model.variant_id()), Some(model));
            let machine = replacement.machine().expect("machine");
            assert_eq!(
                machine.region(),
                if model == Model::Msx1Pal {
                    MsxRegion::Pal
                } else {
                    MsxRegion::Ntsc
                }
            );
            assert_eq!(machine.cpu().regs.pc, 0);
            assert_eq!(
                machine.cartridge(1),
                Some((cart1.as_slice(), MapperType::KonamiScc))
            );
            assert_eq!(
                machine.cartridge(2),
                Some((cart2.as_slice(), MapperType::Ascii16))
            );
        }
        let blank = MsxRuntime::blank(source_model)
            .snapshot()
            .expect("blank snapshot");
        restored.restore(&blank).expect("restore blank");
        restored.reset(ResetKind::Hard);
        assert!(restored.machine().is_none());
        assert!(restored.cart1_bytes().is_none());
        assert!(restored.cart2_bytes().is_none());
    }
}
