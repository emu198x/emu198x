//! Family construction and session switching preserve each board's configuration.

use emu198x_shell::{FamilyRuntime, FirmwareImage, FirmwareSet, HeadlessSession, MachineCore};
use runtime_sinclair_zx81::{Model, ROM_FIRMWARE_ID, Zx81Runtime};

#[test]
fn switching_from_a_ram_pack_to_ts1000_changes_ram_and_session_pacing() {
    let rom = vec![0; 8192];
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(ROM_FIRMWARE_ID, &rom));
    let runtime = Zx81Runtime::from_firmware(Model::Zx81Ram16k, &firmware).expect("runtime");
    let pal_ticks = runtime.native_frame_ticks();
    let mut session = HeadlessSession::new(runtime, pal_ticks);
    assert_eq!(session.machine().ram_bytes(), 16384);
    session
        .swap_machine(Model::Ts1000, &firmware)
        .expect("swap");
    assert_eq!(session.machine().ram_bytes(), 2048);
    assert_eq!(
        session.machine().profile().profile_id.as_str(),
        "timex-ts1000"
    );
    assert!(session.native_frame_ticks() < pal_ticks);
    assert_eq!(
        session.native_frame_ticks(),
        session.machine().native_frame_ticks()
    );

    let empty = FirmwareSet::new();
    assert!(session.swap_machine(Model::Zx81, &empty).is_err());
    assert_eq!(session.machine().model(), Model::Ts1000);
    assert_eq!(session.machine().ram_bytes(), 2048);
}

#[test]
fn every_catalogue_id_round_trips_and_has_the_profiles_firmware() {
    for id in Zx81Runtime::variant_ids() {
        let model = Zx81Runtime::model_from_id(id).expect("catalogued model");
        assert_eq!(Zx81Runtime::variant_id(model), *id);
        let profile = Zx81Runtime::profile_for(model);
        let sources = Zx81Runtime::firmware_sources(model);
        assert_eq!(sources.len(), profile.firmware.len());
        assert_eq!(sources[0].id, profile.firmware[0].id.as_ref());
    }
    assert_eq!(Zx81Runtime::model_from_id("unknown"), None);
}
