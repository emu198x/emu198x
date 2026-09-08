//! Synthetic images verify cold-boot state, cartridge mapping and regional pacing.
use emu198x_shell::{FamilyRuntime, FirmwareSet, HeadlessSession, MachineTime, ResetKind};
use runtime_coleco_colecovision::{CvRuntime, Model};

#[test]
fn catalogue_round_trips_and_declares_switching() {
    assert_eq!(CvRuntime::variant_ids(), Model::VARIANT_IDS);
    for model in Model::ALL {
        assert_eq!(Model::from_variant_id(model.variant_id()), Some(model));
        let profile = CvRuntime::profile_for(model);
        assert_eq!(profile.profile_id.as_str(), model.profile_id());
        assert!(
            profile
                .capabilities
                .contains(&emu198x_shell::known_capability("variant-switch"))
        );
    }
    assert_eq!(Model::from_variant_id("unknown"), None);
}

#[test]
fn regional_switches_keep_cartridge_but_reset_machine_and_session() {
    let mut firmware = FirmwareSet::new();
    let bios = vec![0; 8192];
    firmware.push(emu198x_shell::FirmwareImage::new(
        runtime_coleco_colecovision::BIOS_FIRMWARE_ID,
        &bios,
    ));
    let mut runtime =
        <CvRuntime as FamilyRuntime>::from_firmware(Model::CvNtsc, &firmware).expect("firmware");
    runtime.insert_cartridge(vec![0x5a; 8192]);
    let mut session = HeadlessSession::new(runtime, Model::CvNtsc.frame_ticks());
    for model in [Model::CvPal, Model::CvNtsc] {
        session.run_frames(1).expect("run");
        session
            .machine_mut()
            .machine_mut()
            .expect("machine")
            .poke(24576, 0x73);
        session.swap_machine(model, &firmware).expect("switch");
        assert_eq!(session.machine().model(), model);
        assert_eq!(session.native_frame_ticks(), model.frame_ticks());
        assert_eq!(session.time(), MachineTime::default());
        assert!(session.latest_frame().is_none());
        assert!(session.last_run_result().is_none());
        let machine = session.machine().machine().expect("retained cartridge");
        assert_eq!(machine.peek(32768), 0x5a);
        assert_ne!(machine.peek(24576), 0x73);
        assert_eq!(machine.frame_count(), 0);
        session.run_frames(1).expect("new region runs");
        assert_eq!(session.time().get(), model.frame_ticks());
        session.reset(ResetKind::Hard).expect("reset");
        assert_eq!(
            session
                .machine()
                .machine()
                .expect("reset cartridge")
                .peek(32768),
            0x5a
        );
    }
}

#[test]
fn invalid_target_firmware_preserves_running_state() {
    let mut runtime = CvRuntime::new(Model::CvNtsc, vec![0; 8192]).expect("BIOS");
    runtime.insert_cartridge(vec![0x5a; 8192]);
    let mut session = HeadlessSession::new(runtime, Model::CvNtsc.frame_ticks());
    session.run_frames(1).expect("run");
    let before = session.time();
    assert!(
        session
            .swap_machine(Model::CvPal, &FirmwareSet::new())
            .is_err()
    );
    assert_eq!(session.time(), before);
    assert_eq!(session.machine().model(), Model::CvNtsc);
    assert_eq!(session.native_frame_ticks(), Model::CvNtsc.frame_ticks());
    assert_eq!(
        session.machine().machine().expect("cartridge").peek(32768),
        0x5a
    );
}
