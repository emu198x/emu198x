//! Family construction and session switching preserve each board's configuration.

use emu198x_shell::{FamilyRuntime, FirmwareImage, FirmwareSet, HeadlessSession, MachineCore};
use runtime_sinclair_zx80::{Model, ROM_FIRMWARE_ID, Zx80Runtime};

#[test]
fn switching_from_a_ram_pack_to_usa_changes_ram_and_session_pacing() {
    let rom = vec![0; 4096];
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(ROM_FIRMWARE_ID, &rom));
    let runtime = Zx80Runtime::from_firmware(Model::Zx80RamPack, &firmware).expect("runtime");
    let pal_ticks = runtime.native_frame_ticks();
    let mut session = HeadlessSession::new(runtime, pal_ticks);
    assert_eq!(session.machine().ram_bytes(), 16384);
    session
        .swap_machine(Model::Zx80Usa, &firmware)
        .expect("swap");
    assert_eq!(session.machine().ram_bytes(), 1024);
    assert_eq!(
        session
            .machine()
            .machine()
            .expect("loaded")
            .television_standard(),
        machine_sinclair_zx80::TelevisionStandard::SixtyHz
    );
    assert_eq!(
        session
            .machine()
            .machine()
            .expect("loaded")
            .framebuffer_height(),
        240
    );
    assert_eq!(
        session.machine().profile().profile_id.as_str(),
        "sinclair-zx80-usa"
    );
    assert!(session.native_frame_ticks() < pal_ticks);
    assert_eq!(
        session.native_frame_ticks(),
        session.machine().native_frame_ticks()
    );

    let empty = FirmwareSet::new();
    assert!(session.swap_machine(Model::Zx80, &empty).is_err());
    assert_eq!(session.machine().model(), Model::Zx80Usa);
    assert_eq!(session.machine().ram_bytes(), 1024);
}

#[test]
fn every_catalogue_id_round_trips_and_has_the_profiles_firmware() {
    for id in Zx80Runtime::variant_ids() {
        let model = Zx80Runtime::model_from_id(id).expect("catalogued model");
        assert_eq!(Zx80Runtime::variant_id(model), *id);
        let profile = Zx80Runtime::profile_for(model);
        let sources = Zx80Runtime::firmware_sources(model);
        assert_eq!(sources.len(), profile.firmware.len());
        assert_eq!(sources[0].id, profile.firmware[0].id.as_ref());
    }
    assert_eq!(Zx80Runtime::model_from_id("unknown"), None);
}
