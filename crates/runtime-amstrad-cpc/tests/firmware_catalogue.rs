use emu198x_shell::{FamilyRuntime, FirmwareImage, FirmwareSet, MachineCore, MachineError};
use runtime_amstrad_cpc::{AMSDOS_FIRMWARE_ID, AmstradCpcRuntime, Model};

#[test]
fn the_catalogue_names_the_464_664_and_6128() {
    assert_eq!(
        AmstradCpcRuntime::variant_ids(),
        &["amstrad-cpc464", "amstrad-cpc664", "amstrad-cpc6128"]
    );
    assert_eq!(AmstradCpcRuntime::model_from_id("unknown"), None);
}

#[test]
fn each_model_builds_from_its_firmware() {
    let rom = vec![0; 32768];
    let amsdos = vec![0; 16384];
    for model in [Model::Cpc464, Model::Cpc664, Model::Cpc6128] {
        assert_eq!(
            AmstradCpcRuntime::model_from_id(model.profile_id()),
            Some(model)
        );
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new(model.firmware_id(), &rom));
        if model.has_disc() {
            firmware.push(FirmwareImage::new(AMSDOS_FIRMWARE_ID, &amsdos));
        }
        let runtime =
            <AmstradCpcRuntime as FamilyRuntime>::from_firmware(model, &firmware).expect("runtime");
        let machine = runtime.machine().expect("machine");
        assert_eq!(machine.model(), model.machine_model());
        assert_eq!(runtime.profile().profile_id.as_str(), model.profile_id());
        assert_eq!(runtime.native_frame_ticks(), model.frame_ticks());
    }
}

#[test]
fn a_disc_model_without_amsdos_is_refused() {
    let rom = vec![0; 32768];
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(Model::Cpc6128.firmware_id(), &rom));
    let result = <AmstradCpcRuntime as FamilyRuntime>::from_firmware(Model::Cpc6128, &firmware);
    assert!(matches!(
        result,
        Err(MachineError::MissingFirmware { .. } | MachineError::InvalidFirmware { .. })
    ));
}
