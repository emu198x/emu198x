//! Switching installs the selected machine and clears its loaded cassette.
use emu198x_shell::{
    FamilyRuntime, FirmwareImage, FirmwareSet, HeadlessSession, MachineCore, MediaImage, MediaKind,
    MediaSet, ResetKind,
};
use runtime_oric_atmos::{BIOS_FIRMWARE_ID, Model, OricModel, OricRuntime};

#[test]
fn catalogue_switches_real_models_and_preserves_media_on_failure() {
    let rom = vec![0; 16384];
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(BIOS_FIRMWARE_ID, &rom));
    let mut session = HeadlessSession::new(OricRuntime::blank(Model::Atmos), 1);
    let tap = [
        0x16, 0x16, 0x16, 0x24, 0, 0, 0x80, 0, 0x40, 0, 0x40, 0, 0, b'T', 0, 0x60,
    ];
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-1", MediaKind::Tape, &tap));
    for model in Model::ALL {
        assert_eq!(OricRuntime::model_from_id(model.variant_id()), Some(model));
        assert_eq!(OricRuntime::model_from_id(model.profile_id()), Some(model));
        session.swap_machine(model, &firmware).expect("switch");
        assert_eq!(session.native_frame_ticks(), model.frame_ticks());
        assert_eq!(
            session.machine().profile().profile_id.as_str(),
            model.profile_id()
        );
        assert_eq!(
            session.machine().machine().expect("machine").model(),
            if model == Model::Atmos {
                OricModel::Atmos
            } else {
                OricModel::Oric1
            }
        );
        assert!(!session.machine().machine().expect("machine").tape_loaded());
        session.load_media(&media).expect("tape");
        session.machine_mut().reset(ResetKind::Hard);
        assert!(session.machine().machine().expect("machine").tape_loaded());
        assert!(
            session
                .swap_machine(Model::Atmos, &FirmwareSet::new())
                .is_err()
        );
        assert_eq!(session.machine().model(), model);
        assert!(session.machine().machine().expect("machine").tape_loaded());
    }
    session
        .swap_machine(Model::Oric1, &firmware)
        .expect("eject on final switch");
    assert!(!session.machine().machine().expect("machine").tape_loaded());
    session.run_frames(1).expect("one native frame");
    assert_eq!(
        session.machine().machine().expect("machine").frame_count(),
        1
    );
}
