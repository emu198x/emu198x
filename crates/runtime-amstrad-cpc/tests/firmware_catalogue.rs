use emu198x_shell::{FamilyRuntime, FirmwareImage, FirmwareSet, MachineCore};
use runtime_amstrad_cpc::{AmstradCpcRuntime, Model, ROM_FIRMWARE_ID};

#[test]
fn single_model_catalogue_builds_the_existing_profile() {
    let model = Model::Cpc464;
    assert_eq!(AmstradCpcRuntime::variant_ids(), &[model.profile_id()]);
    assert_eq!(
        AmstradCpcRuntime::model_from_id(model.profile_id()),
        Some(model)
    );
    assert_eq!(AmstradCpcRuntime::model_from_id("unknown"), None);
    let rom = vec![0; 32768];
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(ROM_FIRMWARE_ID, &rom));
    let runtime =
        <AmstradCpcRuntime as FamilyRuntime>::from_firmware(model, &firmware).expect("runtime");
    assert!(runtime.machine().is_some());
    assert_eq!(runtime.profile().profile_id.as_str(), model.profile_id());
    assert_eq!(runtime.native_frame_ticks(), model.frame_ticks());
}
