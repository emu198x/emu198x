//! Switching changes the installed television profile and ejects media atomically.
use emu198x_shell::{
    FamilyRuntime, FirmwareImage, FirmwareSet, HeadlessSession, MachineCore, ResetKind,
};
use runtime_sord_m5::{M5Runtime, Model, ROM_FIRMWARE_ID};

#[test]
fn every_region_installs_its_framebuffer_and_budget_and_ejects_cartridges() {
    let rom = vec![0; 8192];
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(ROM_FIRMWARE_ID, &rom));
    let mut session = HeadlessSession::new(M5Runtime::blank(Model::M5Ntsc), 1);
    for model in Model::ALL {
        assert_eq!(M5Runtime::model_from_id(model.variant_id()), Some(model));
        session.swap_machine(model, &firmware).expect("switch");
        assert_eq!(
            session.machine().profile().profile_id.as_str(),
            model.profile_id()
        );
        assert_eq!(session.native_frame_ticks(), model.frame_ticks());
        assert!(!session.machine().cartridge_loaded());
        let expected_height = if model == Model::M5Pal { 288 } else { 240 };
        assert_eq!(
            session
                .machine()
                .machine()
                .expect("machine")
                .vdp()
                .framebuffer_height(),
            expected_height
        );
        session.machine_mut().insert_cartridge(vec![0x5a; 8192]);
        assert!(session.machine().cartridge_loaded());
        session.machine_mut().reset(ResetKind::Hard);
        assert!(
            session.machine().cartridge_loaded(),
            "reset retains the cartridge"
        );
        let machine = session.machine_mut().machine_mut().expect("loaded");
        assert_eq!(machine.peek(0x2000), 0x5a);
        assert!(
            session
                .swap_machine(Model::M5Ntsc, &FirmwareSet::new())
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
