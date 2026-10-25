use emu198x_shell::{FamilyRuntime, FirmwareImage, FirmwareSet, MachineCore};
use runtime_acorn_electron::{
    ADFS_FIRMWARE_ID, BASIC_FIRMWARE_ID, ElectronRuntime, Model, OS_FIRMWARE_ID,
};

#[test]
fn catalogue_builds_both_profiles() {
    assert_eq!(
        ElectronRuntime::variant_ids(),
        &[
            Model::Electron.profile_id(),
            Model::ElectronPlus3.profile_id()
        ]
    );
    assert_eq!(ElectronRuntime::model_from_id("unknown"), None);
    let rom = vec![0; 16384];
    for model in Model::ALL {
        assert_eq!(
            ElectronRuntime::model_from_id(model.profile_id()),
            Some(model)
        );
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new(OS_FIRMWARE_ID, &rom));
        firmware.push(FirmwareImage::new(BASIC_FIRMWARE_ID, &rom));
        let runtime =
            <ElectronRuntime as FamilyRuntime>::from_firmware(model, &firmware).expect("runtime");
        let machine = runtime.machine().expect("machine");
        assert_eq!(machine.plus3().is_some(), model.has_plus3());
        assert_eq!(runtime.profile().profile_id.as_str(), model.profile_id());
        assert_eq!(runtime.native_frame_ticks(), model.frame_ticks());
    }
}

#[test]
fn the_plus3_fits_adfs_when_it_is_supplied() {
    let rom = vec![0; 16384];
    let adfs = vec![0x42; 16384];
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(OS_FIRMWARE_ID, &rom));
    firmware.push(FirmwareImage::new(BASIC_FIRMWARE_ID, &rom));
    firmware.push(FirmwareImage::new(ADFS_FIRMWARE_ID, &adfs));
    let runtime =
        <ElectronRuntime as FamilyRuntime>::from_firmware(Model::ElectronPlus3, &firmware)
            .expect("runtime");
    let plus3 = runtime.machine().and_then(|m| m.plus3()).expect("a Plus 3");
    assert!(plus3.has_adfs());
    assert_eq!(plus3.rom_byte(0x8000), 0xFF, "not paged in until ROM 4");
}
