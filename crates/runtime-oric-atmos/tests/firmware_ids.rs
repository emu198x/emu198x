//! Firmware identity is selected by the ROM profile, never shared implicitly.
use emu198x_shell::{FirmwareImage, FirmwareSet, MachineCore, MachineError};
use runtime_oric_atmos::{BIOS_FIRMWARE_ID, Model, ORIC1_FIRMWARE_ID, OricRuntime};
#[test]
fn each_profile_requires_its_own_firmware_and_names_size_errors() {
    for (model, id, other) in [
        (Model::Oric1, ORIC1_FIRMWARE_ID, BIOS_FIRMWARE_ID),
        (Model::Atmos, BIOS_FIRMWARE_ID, ORIC1_FIRMWARE_ID),
    ] {
        let empty = OricRuntime::from_firmware(model, &FirmwareSet::new())
            .err()
            .expect("missing required ROM");
        assert!(matches!(empty,MachineError::MissingFirmware{id:actual} if actual==id));
        let custom = vec![0x57; 16384];
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new(other, &custom));
        assert!(
            matches!(OricRuntime::from_firmware(model,&firmware),Err(MachineError::UnknownFirmware{id:actual}) if actual==other)
        );
        let mut selected = FirmwareSet::new();
        selected.push(FirmwareImage::new(id, &custom));
        let mut runtime = OricRuntime::from_firmware(model, &selected)
            .expect("explicit custom firmware remains valid");
        assert_eq!(runtime.machine().expect("live").peek(0xc000), 0x57);
        let before = runtime.snapshot().expect("before invalid image");
        assert!(
            matches!(runtime.set_bios(vec![0;7]),Err(MachineError::InvalidFirmware{id:actual,..}) if actual==id)
        );
        assert_eq!(runtime.snapshot().expect("unchanged"), before);
    }
}
