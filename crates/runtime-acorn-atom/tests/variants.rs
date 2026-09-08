//! Verify catalogue switching installs hardware, not just a new label.
use emu198x_shell::{FamilyRuntime, FirmwareImage, FirmwareSet, HeadlessSession, MachineCore};
use runtime_acorn_atom::{AtomRuntime, BIOS_FIRMWARE_ID, Model};

#[test]
fn every_preset_switches_with_its_real_memory_map_and_frame_budget() {
    let rom = vec![0; 24576];
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(BIOS_FIRMWARE_ID, &rom));
    let mut session = HeadlessSession::new(AtomRuntime::blank(Model::AtomBase), 1);
    for model in Model::ALL {
        assert_eq!(AtomRuntime::model_from_id(model.variant_id()), Some(model));
        session.swap_machine(model, &firmware).expect("switch");
        assert_eq!(
            session.machine().profile().profile_id.as_str(),
            model.profile_id()
        );
        assert_eq!(session.native_frame_ticks(), model.frame_ticks());
        let machine = session.machine_mut().machine_mut().expect("loaded");
        machine.poke(0x2800, 0x5a);
        assert_eq!(
            machine.peek(0x2800),
            if model == Model::AtomFull { 0x5a } else { 0xff }
        );
        let before = session.machine().model();
        assert!(
            session
                .swap_machine(Model::AtomBase, &FirmwareSet::new())
                .is_err()
        );
        assert_eq!(session.machine().model(), before);
    }
}

#[test]
fn switching_ejects_media_while_reset_and_failed_switches_keep_it() {
    use emu198x_shell::{MediaImage, MediaKind, MediaSet, ResetKind};
    let rom = vec![0; 24576];
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(BIOS_FIRMWARE_ID, &rom));
    let runtime = AtomRuntime::from_firmware(Model::AtomFull, &firmware).expect("firmware");
    let mut session = HeadlessSession::new(runtime, Model::AtomFull.frame_ticks());
    let mut tape = b"UEF File!\0\x0a\x00".to_vec();
    tape.extend_from_slice(&[0, 1, 1, 0, 0, 0, 0x41]);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-1", MediaKind::Tape, &tape));
    let pack = vec![0x5a; 4096];
    media.push(MediaImage::new("rom-pack-1", MediaKind::Cartridge, &pack));
    session.load_media(&media).expect("mount media");
    session.machine_mut().reset(ResetKind::Hard);
    assert!(session.machine().machine().expect("machine").tape_loaded());
    assert_eq!(
        session.machine().machine().expect("machine").peek(0xa000),
        0x5a
    );
    assert!(
        session
            .swap_machine(Model::AtomBase, &FirmwareSet::new())
            .is_err()
    );
    assert!(session.machine().machine().expect("machine").tape_loaded());
    assert!(
        session
            .machine()
            .machine()
            .expect("machine")
            .utility_rom_present()
    );
    session
        .swap_machine(Model::AtomBase, &firmware)
        .expect("switch");
    let machine = session.machine().machine().expect("machine");
    assert!(!machine.tape_loaded());
    assert!(!machine.utility_rom_present());
    session.run_frames(1).expect("one native frame");
    assert_eq!(
        session.machine().machine().expect("machine").frame_count(),
        1
    );
}
