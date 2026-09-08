//! Switching changes the installed television profile and ejects media atomically.
use emu198x_shell::{
    FamilyRuntime, FirmwareImage, FirmwareSet, HeadlessSession, MachineCore, ResetKind,
};
use runtime_spectravideo_svi_328::{BIOS_FIRMWARE_ID, Model, Svi328Runtime};

#[test]
fn every_region_installs_its_framebuffer_and_budget_and_ejects_cartridges() {
    let rom = test_rom();
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(BIOS_FIRMWARE_ID, &rom));
    let mut session = HeadlessSession::new(Svi328Runtime::blank(Model::Svi328Ntsc), 1);
    for model in Model::ALL {
        assert_eq!(
            Svi328Runtime::model_from_id(model.variant_id()),
            Some(model)
        );
        session.swap_machine(model, &firmware).expect("switch");
        assert_eq!(
            session.machine().profile().profile_id.as_str(),
            model.profile_id()
        );
        assert_eq!(session.native_frame_ticks(), model.frame_ticks());
        assert!(!session.machine().cartridge_loaded());
        let expected_height = if model == Model::Svi328Pal { 288 } else { 240 };
        assert_eq!(
            session
                .machine()
                .machine()
                .expect("machine")
                .vdp()
                .framebuffer_height(),
            expected_height
        );
        session
            .machine_mut()
            .insert_cartridge(vec![0x5a; 8192])
            .expect("cartridge");
        assert!(session.machine().cartridge_loaded());
        session.machine_mut().reset(ResetKind::Hard);
        assert!(
            session.machine().cartridge_loaded(),
            "reset retains the cartridge"
        );
        let machine = session.machine_mut().machine_mut().expect("loaded");
        machine.run_frame();
        assert_eq!(machine.peek(0x8000), 0x5a);
        assert!(
            session
                .swap_machine(Model::Svi328Ntsc, &FirmwareSet::new())
                .is_err()
        );
        assert_eq!(session.machine().model(), model);
        assert!(
            session.machine().cartridge_loaded(),
            "failed switch retains media"
        );
        session.swap_machine(model, &firmware).expect("fresh boot");
        assert!(!session.machine().cartridge_loaded());
        session.run_frames(1).expect("one frame");
        assert_eq!(
            session.machine().machine().expect("machine").frame_count(),
            1
        );
    }
}

fn test_rom() -> Vec<u8> {
    // Enable the cartridge bank, sample PPI cassette presence into RAM, halt.
    let code = [
        0x3e, 0x0f, 0xd3, 0x88, 0x3e, 0x02, 0xd3, 0x8c, 0xdb, 0x98, 0x32, 0x00, 0xc0, 0x76,
    ];
    let mut rom = vec![0; 32768];
    rom[..code.len()].copy_from_slice(&code);
    rom
}

#[test]
fn switching_ejects_cassette_but_reset_and_a_failed_switch_keep_it() {
    use format198x_spectravideo_svi_cas::{CasImage, encode};
    let rom = test_rom();
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(BIOS_FIRMWARE_ID, &rom));
    let runtime = Svi328Runtime::from_firmware(Model::Svi328Ntsc, &firmware).expect("firmware");
    let mut session = HeadlessSession::new(runtime, Model::Svi328Ntsc.frame_ticks());
    let cassette = encode(&CasImage::new(vec![vec![0x80]]).expect("image")).expect("encode");
    session
        .machine_mut()
        .insert_cassette(&cassette)
        .expect("CAS image");
    session.machine_mut().reset(ResetKind::Hard);
    session.run_frames(1).expect("sample cassette");
    assert_eq!(
        session.machine().machine().expect("machine").peek(0xc000) & 0x40,
        0
    );
    assert!(
        session
            .swap_machine(Model::Svi328Pal, &FirmwareSet::new())
            .is_err()
    );
    session.machine_mut().reset(ResetKind::Hard);
    session.run_frames(1).expect("sample retained cassette");
    assert_eq!(
        session.machine().machine().expect("machine").peek(0xc000) & 0x40,
        0
    );
    session
        .swap_machine(Model::Svi328Pal, &firmware)
        .expect("switch");
    session.run_frames(1).expect("sample empty deck");
    assert_ne!(
        session.machine().machine().expect("machine").peek(0xc000) & 0x40,
        0
    );
}
