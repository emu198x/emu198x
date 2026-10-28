//! Catalogue switches cold-boot the console on the new model with the same
//! cartridge, and each model reports its own region to the game.

mod common;

use emu198x_shell::{FirmwareSet, HeadlessSession, MachineCore, MachineTime, ResetKind};
use runtime_sega_mega_drive::{MegaDriveRuntime, Model, cartridge};

#[test]
fn every_model_round_trips_and_switches_to_its_actual_hardware() {
    let runtime = MegaDriveRuntime::new(Model::Genesis, common::cartridge());
    let mut session = HeadlessSession::new(runtime, Model::Genesis.frame_ticks());
    for model in Model::ALL {
        assert_eq!(Model::from_variant_id(model.variant_id()), Some(model));
        assert_eq!(Model::from_variant_id(model.profile_id()), Some(model));
        session.run_frames(2).expect("run");
        session
            .machine_mut()
            .machine_mut()
            .expect("machine")
            .poke(0xFF_1000, 0x73);
        session
            .swap_machine(model, &FirmwareSet::new())
            .expect("switch");
        assert_eq!(session.machine().model(), model);
        assert_eq!(session.native_frame_ticks(), model.frame_ticks());
        assert_eq!(session.time(), MachineTime::default());
        let machine = session.machine().machine().expect("cartridge");
        assert_eq!(machine.region, model.machine_region());
        assert_eq!(machine.rom(), common::cartridge());
        assert_ne!(machine.peek(0xFF_1000), 0x73, "a cold boot");
        assert!(session.latest_frame().is_none());
        session.reset(ResetKind::Hard).expect("reset");
        assert_eq!(
            session.machine().machine().expect("machine").rom(),
            common::cartridge()
        );
    }
    assert_eq!(Model::from_variant_id("sms-ntsc"), None);
}

/// A run's frames take a frame's master clocks: 262 or 313 lines of 3420.
#[test]
fn a_frame_is_a_frame_of_master_clocks() {
    for model in Model::ALL {
        let runtime = MegaDriveRuntime::new(model, common::cartridge());
        let mut session = HeadlessSession::new(runtime, model.frame_ticks());
        session.run_frames(2).expect("run");
        let machine = session.machine().machine().expect("machine");
        assert_eq!(session.time().get(), machine.mclk());
        let frame = session.latest_frame().expect("a frame");
        assert_eq!(
            (frame.width, frame.height),
            (machine.frame_width(), machine.frame_height())
        );
    }
}

/// A soft reset is the reset button: work RAM survives it, the 68000
/// starts over.
#[test]
fn a_soft_reset_keeps_ram_and_a_hard_one_clears_it() {
    let mut runtime = MegaDriveRuntime::new(Model::Genesis, common::cartridge());
    let machine = runtime.machine_mut().expect("machine");
    machine.run_frame();
    assert_ne!(machine.cpu.regs.pc, 0x200, "the program has moved on");
    machine.poke(0xFF_2000, 0x5A);
    runtime.reset(ResetKind::Soft);
    let machine = runtime.machine().expect("machine");
    assert_eq!(machine.peek(0xFF_2000), 0x5A);
    assert_eq!(machine.cpu.regs.pc, 0x200, "back at the vector");
    runtime.reset(ResetKind::Hard);
    assert_eq!(runtime.machine().expect("machine").peek(0xFF_2000), 0);
}

/// An `.smd` image goes in de-interleaved.
#[test]
fn an_smd_cartridge_is_de_interleaved_on_insert() {
    let rom = {
        let mut rom = common::cartridge();
        rom.resize(0x4000, 0);
        rom
    };
    let mut smd = vec![0; 512];
    smd[8] = 0xAA;
    smd[9] = 0xBB;
    smd.extend(rom.iter().skip(1).step_by(2));
    smd.extend(rom.iter().step_by(2));
    assert!(cartridge::is_smd(&smd));
    let runtime = MegaDriveRuntime::new(Model::Genesis, smd);
    assert_eq!(runtime.machine().expect("machine").rom(), rom);
}

/// Native events, rather than the session's pacing hint, determine both count
/// and elapsed time. The independent machine path advances the same fields.
#[test]
fn exact_native_frames_match_machine_boundaries_for_every_profile() {
    for model in Model::ALL {
        for pacing in [1, model.frame_ticks(), model.frame_ticks() * 3] {
            for count in [0, 1, 2, 5] {
                for traced in [false, true] {
                    let mut expected = MegaDriveRuntime::new(model, common::cartridge());
                    for _ in 0..count {
                        expected.machine_mut().expect("machine").run_frame();
                    }
                    let mut session = HeadlessSession::new(
                        MegaDriveRuntime::new(model, common::cartridge()),
                        pacing,
                    );
                    let mut trace = emu198x_shell::NullTraceSink;
                    if traced {
                        session
                            .run_frames_with_trace_sink(count, &mut trace)
                            .expect("frames");
                    } else {
                        session.run_frames(count).expect("frames");
                    }
                    assert_eq!(
                        session.frames_emitted(),
                        u64::from(count),
                        "{model:?}, traced={traced}"
                    );
                    assert_eq!(
                        session.time().get(),
                        expected.machine().expect("reference").mclk()
                    );
                    assert_eq!(
                        session.time().get(),
                        session.machine().machine().expect("machine").mclk()
                    );
                    // Repeated calls start at the actual prior boundary.
                    for _ in 0..3 {
                        expected.machine_mut().expect("machine").run_frame();
                    }
                    session.run_frames(3).expect("successive frames");
                    assert_eq!(session.frames_emitted(), u64::from(count + 3));
                    assert_eq!(
                        session.time().get(),
                        expected.machine().expect("reference").mclk()
                    );
                }
            }
        }
    }
}
