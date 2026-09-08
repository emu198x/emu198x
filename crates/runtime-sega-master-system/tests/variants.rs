//! Catalogue switches cold-boot the hardware while retaining cartridge SRAM.
use emu198x_shell::{FirmwareSet, HeadlessSession, MachineCore, MachineTime, ResetKind};
use runtime_sega_master_system::{Model, SmsRuntime};

#[test]
fn every_model_round_trips_and_switches_to_its_actual_hardware() {
    let mut runtime = SmsRuntime::new(Model::SmsNtsc, vec![0x5a; 32768]);
    runtime
        .restore_cartridge_save_image(&vec![0x42; 32768])
        .expect("SRAM");
    let mut session = HeadlessSession::new(runtime, Model::SmsNtsc.frame_ticks());
    for model in Model::ALL {
        assert_eq!(Model::from_variant_id(model.variant_id()), Some(model));
        assert_eq!(Model::from_variant_id(model.profile_id()), Some(model));
        session.run_frames(1).expect("run");
        session
            .machine_mut()
            .machine_mut()
            .expect("machine")
            .poke(0xc123, 0x73);
        session
            .swap_machine(model, &FirmwareSet::new())
            .expect("switch");
        assert_eq!(session.machine().model(), model);
        assert_eq!(session.native_frame_ticks(), model.frame_ticks());
        assert_eq!(session.time(), MachineTime::default());
        let machine = session.machine().machine().expect("cartridge");
        assert_eq!(machine.variant(), model.variant());
        assert_eq!(machine.peek(0), 0x5a);
        assert_ne!(machine.peek(0xc123), 0x73);
        assert_eq!(machine.cartridge_ram()[0x123], 0x42);
        assert!(!machine.cartridge_ram_dirty());
        assert!(session.machine().cartridge_save_image().is_none());
        assert!(session.latest_frame().is_none());
        session.reset(ResetKind::Hard).expect("reset");
        assert_eq!(
            session
                .machine()
                .machine()
                .expect("machine")
                .cartridge_ram()[0x123],
            0x42
        );
    }
    assert_eq!(Model::from_variant_id("game-gear"), None);
}

#[test]
fn dirty_sram_survives_replacement_reset_and_snapshot() {
    let mut runtime = SmsRuntime::new(Model::SmsNtsc, vec![0; 32768]);
    let machine = runtime.machine_mut().expect("machine");
    machine.poke(0xfffc, 0x08);
    machine.poke(0x8123, 0x5a);
    let mut runtime = emu198x_shell::build_replacement(
        &runtime,
        Model::SmsPal,
        &emu198x_shell::FirmwareOverrides::none(),
    )
    .expect("replacement");
    assert_eq!(
        runtime.cartridge_save_image().expect("dirty SRAM")[0x123],
        0x5a
    );
    runtime.reset(ResetKind::Hard);
    let snapshot = runtime.snapshot().expect("snapshot");
    let mut restored = SmsRuntime::blank(Model::SmsPal);
    restored.restore(&snapshot).expect("restore");
    assert_eq!(
        restored.cartridge_save_image().expect("dirty SRAM")[0x123],
        0x5a
    );
    assert_eq!(restored.snapshot().expect("snapshot"), snapshot);
}
