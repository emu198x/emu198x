use emu198x_shell::{FamilyRuntime, FirmwareImage, FirmwareSet, MachineCore, ResetKind};
use runtime_commodore_pet::{
    BASIC_FIRMWARE_ID, CHAR_FIRMWARE_ID, EDITOR_FIRMWARE_ID, KERNAL_FIRMWARE_ID, Model, PetRuntime,
};

#[test]
fn catalogue_installs_all_firmware_and_snapshot_reset_retains_it() {
    let images = [
        (KERNAL_FIRMWARE_ID, vec![0x4c; 4096]),
        (BASIC_FIRMWARE_ID, vec![0x42; 8192]),
        (EDITOR_FIRMWARE_ID, vec![0x45; 2048]),
        (CHAR_FIRMWARE_ID, vec![0x3c; 4096]),
    ];
    let mut firmware = FirmwareSet::new();
    for (id, bytes) in &images {
        firmware.push(FirmwareImage::new(*id, bytes));
    }
    for model in Model::ALL {
        let runtime = PetRuntime::from_firmware(model, &firmware).expect("runtime");
        let snap = runtime.snapshot().expect("snapshot");
        let mut restored = PetRuntime::blank(model);
        restored.restore(&snap).expect("restore");
        restored.reset(ResetKind::Hard);
        assert_eq!(restored.snapshot().expect("snapshot"), snap);
        assert_eq!(restored.machine().expect("machine").peek(0xe000), 0x45);
        assert_eq!(restored.native_frame_ticks(), 20_000);
        assert_eq!(Model::from_variant_id(model.variant_id()), Some(model));
        for target in Model::ALL {
            let replacement = restored
                .replacement(target, &firmware)
                .expect("replacement");
            assert_eq!(replacement.model(), target);
            assert_eq!(
                replacement.profile().profile_id.as_str(),
                target.profile_id()
            );
        }
    }
}
