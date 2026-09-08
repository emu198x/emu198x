use emu198x_shell::{FamilyRuntime, FirmwareImage, FirmwareSet, MachineCore};
use runtime_mattel_aquarius::{AquariusRuntime, BIOS_FIRMWARE_ID, CHAR_FIRMWARE_ID, Model};

#[test]
fn single_model_catalogue_builds_the_existing_profile() {
    let model = Model::Aquarius;
    assert_eq!(AquariusRuntime::variant_ids(), &[model.profile_id()]);
    assert_eq!(
        AquariusRuntime::model_from_id(model.profile_id()),
        Some(model)
    );
    assert_eq!(AquariusRuntime::model_from_id("unknown"), None);
    let rom = vec![0; 8192];
    let chars = vec![0xff; 2048];
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(BIOS_FIRMWARE_ID, &rom));
    firmware.push(FirmwareImage::new(CHAR_FIRMWARE_ID, &chars));
    let runtime =
        <AquariusRuntime as FamilyRuntime>::from_firmware(model, &firmware).expect("runtime");
    assert!(runtime.machine().is_some());
    assert_eq!(runtime.profile().profile_id.as_str(), model.profile_id());
    assert_eq!(runtime.native_frame_ticks(), model.frame_ticks());
}
