//! Synthetic images verify cold-boot state, cartridge mapping and regional pacing.
use emu198x_shell::{FamilyRuntime, FirmwareSet, HeadlessSession, MachineTime, ResetKind};
use runtime_sega_sg_1000::{Model, Sg1000Runtime};

#[test]
fn catalogue_round_trips_and_declares_switching() {
    assert_eq!(Sg1000Runtime::variant_ids(), Model::VARIANT_IDS);
    for model in Model::ALL {
        assert_eq!(Model::from_variant_id(model.variant_id()), Some(model));
        let profile = Sg1000Runtime::profile_for(model);
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
    let firmware = FirmwareSet::new();
    let mut runtime = <Sg1000Runtime as FamilyRuntime>::from_firmware(Model::Sg1000Ntsc, &firmware)
        .expect("firmware");
    runtime.insert_cartridge(vec![0x5a; 8192]);
    let mut session = HeadlessSession::new(runtime, Model::Sg1000Ntsc.frame_ticks());
    for model in [Model::Sg1000Pal, Model::Sg1000Ntsc] {
        session.run_frames(1).expect("run");
        session
            .machine_mut()
            .machine_mut()
            .expect("machine")
            .poke(49152, 0x73);
        session.swap_machine(model, &firmware).expect("switch");
        assert_eq!(session.machine().model(), model);
        assert_eq!(session.native_frame_ticks(), model.frame_ticks());
        assert_eq!(session.time(), MachineTime::default());
        assert!(session.latest_frame().is_none());
        assert!(session.last_run_result().is_none());
        let machine = session.machine().machine().expect("retained cartridge");
        assert_eq!(machine.peek(0), 0x5a);
        assert_ne!(machine.peek(49152), 0x73);
        assert_eq!(machine.frame_count(), 0);
        session.run_frames(1).expect("new region runs");
        assert_eq!(session.time().get(), model.frame_ticks());
        session.reset(ResetKind::Hard).expect("reset");
        assert_eq!(
            session
                .machine()
                .machine()
                .expect("reset cartridge")
                .peek(0),
            0x5a
        );
    }
}
