//! The runtime end to end on a synthetic cartridge: frames, sound, input,
//! queries, snapshots, model switches and the IPL firmware slot.

mod common;

use emu198x_shell::{
    FirmwareImage, FirmwareSet, HeadlessSession, HostIo, InputEvent, MachineCore, MachineError,
    MachineTime, MediaImage, MediaKind, MediaSet, NullAudioSink, NullFrameSink, NullTraceSink,
    ResetKind, SessionQueryProvider,
};
use runtime_nintendo_snes::{Model, SnesRuntime, SnesSessionQueryProvider, blank, with_cartridge};

fn run_to(runtime: &mut SnesRuntime, target: u64, input: &[InputEvent]) {
    runtime
        .run_until(
            MachineTime::new(target),
            &mut HostIo {
                input_events: input,
                frame_sink: &mut NullFrameSink,
                audio_sink: &mut NullAudioSink,
                trace_sink: &mut NullTraceSink,
            },
        )
        .expect("runs");
}

#[test]
fn frames_are_512_wide_and_a_frame_of_master_clocks() {
    for model in Model::ALL {
        let runtime = with_cartridge(model, common::cartridge());
        let mut session = HeadlessSession::new(runtime, model.frame_ticks());
        session.run_frames(3).expect("run");
        let machine = session.machine().machine().expect("machine");
        assert_eq!(session.time().get(), machine.mclk());
        let frame = session.latest_frame().expect("a frame");
        assert_eq!((frame.width, frame.height), (512, 240));
        assert!(machine.wram()[common::FRAME_COUNT] >= 2, "NMI ran");
    }
}

#[test]
fn a_pad_press_reaches_the_program_through_auto_read() {
    let mut runtime = with_cartridge(Model::SnesNtsc, common::cartridge());
    let press = [InputEvent::Button {
        port: 1,
        name: "start".to_owned().into(),
        pressed: true,
    }];
    run_to(&mut runtime, 4 * Model::SnesNtsc.frame_ticks(), &press);
    let w = runtime.machine().expect("machine").wram();
    let joy = u16::from(w[common::JOY1]) | u16::from(w[common::JOY1 + 1]) << 8;
    assert_eq!(joy, 0x1000, "Start is bit 12");
}

#[test]
fn queries_resolve_with_a_cartridge_and_need_one() {
    let mut runtime = with_cartridge(Model::SnesNtsc, common::cartridge());
    run_to(&mut runtime, Model::SnesNtsc.frame_ticks(), &[]);
    let provider = SnesSessionQueryProvider;
    for path in provider.query_paths(&runtime, None) {
        provider
            .query(&runtime, &path)
            .expect("no error")
            .unwrap_or_else(|| panic!("{path} resolves"));
    }
    let value =
        |rt: &SnesRuntime, path: &str| provider.query(rt, path).expect("ok").expect("some").value;
    assert_eq!(value(&runtime, "machine.variant"), "snes-ntsc");
    assert_eq!(value(&runtime, "apu.original_ipl"), false);
    assert_eq!(value(&runtime, "cartridge.map"), "LoRom");
    assert_eq!(
        provider.query_paths(&runtime, Some("apu")),
        [
            "apu",
            "apu.ipl_enabled",
            "apu.original_ipl",
            "apu.pc",
            "apu.ports"
        ]
    );
    let blank = blank(Model::SnesNtsc);
    assert_eq!(value(&blank, "cartridge.loaded"), false);
    assert!(provider.query(&blank, "cpu.pc").is_err());
    assert!(
        provider
            .query(&blank, "no.such.path")
            .expect("ok")
            .is_none()
    );
}

#[test]
fn a_snapshot_restores_and_runs_in_lockstep() {
    let mut runtime = with_cartridge(Model::SnesNtsc, common::cartridge());
    // An odd time, so the snapshot lands mid-cycle.
    run_to(&mut runtime, 2 * Model::SnesNtsc.frame_ticks() + 1234, &[]);
    let snap = runtime.snapshot().expect("snapshot");
    let mut restored = blank(Model::SnesNtsc);
    restored.restore(&snap).expect("restore");
    assert_eq!(restored.time(), runtime.time());
    assert_eq!(restored.snapshot().expect("re-snapshot"), snap);
    let target = 6 * Model::SnesNtsc.frame_ticks();
    run_to(&mut runtime, target, &[]);
    run_to(&mut restored, target, &[]);
    assert_eq!(
        restored.snapshot().expect("snapshot"),
        runtime.snapshot().expect("snapshot")
    );
    restored.reset(ResetKind::Hard);
    assert!(restored.machine().is_some(), "the cartridge came with it");
}

#[test]
fn model_switches_cold_boot_with_the_same_cartridge() {
    let runtime = SnesRuntime::new(Model::SnesNtsc, common::cartridge());
    let mut session = HeadlessSession::new(runtime, Model::SnesNtsc.frame_ticks());
    for model in Model::ALL {
        session.run_frames(2).expect("run");
        session
            .swap_machine(model, &FirmwareSet::new())
            .expect("switch");
        assert_eq!(session.machine().model(), model);
        assert_eq!(session.native_frame_ticks(), model.frame_ticks());
        assert_eq!(session.time(), MachineTime::default());
        let machine = session.machine().machine().expect("cartridge");
        assert_eq!(machine.region(), model.machine_region());
        assert_eq!(machine.wram()[common::LOOP_COUNT], 0, "a cold boot");
    }
}

#[test]
fn a_soft_reset_keeps_ram() {
    let mut runtime = with_cartridge(Model::SnesNtsc, common::cartridge());
    run_to(&mut runtime, 2 * Model::SnesNtsc.frame_ticks(), &[]);
    runtime
        .machine_mut()
        .expect("machine")
        .poke(0x7E_2000, 0x5A);
    runtime.reset(ResetKind::Soft);
    assert_eq!(runtime.machine().expect("machine").peek(0x7E_2000), 0x5A);
    runtime.reset(ResetKind::Hard);
    assert_eq!(runtime.machine().expect("machine").peek(0x7E_2000), 0);
}

#[test]
fn the_ipl_rom_is_taken_when_it_is_64_bytes() {
    use emu198x_shell::FamilyRuntime;
    let ipl = [0u8; 64];
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new("snes-ipl", &ipl));
    let runtime = SnesRuntime::from_firmware(Model::SnesNtsc, &firmware).expect("accepted");
    assert!(runtime.has_original_ipl());

    let short = [0u8; 32];
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new("snes-ipl", &short));
    assert!(matches!(
        SnesRuntime::from_firmware(Model::SnesNtsc, &firmware),
        Err(MachineError::InvalidFirmware { .. })
    ));
    let none = SnesRuntime::from_firmware(Model::SnesNtsc, &FirmwareSet::new()).expect("ok");
    assert!(!none.has_original_ipl());
}

#[test]
fn media_goes_into_the_cartridge_slot_only() {
    let mut runtime = blank(Model::SnesPal);
    let rom = common::headed_cartridge(2);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("loads");
    assert!(runtime.machine().is_some());

    let mut wrong = MediaSet::new();
    wrong.push(MediaImage::new("cartridge-2", MediaKind::Cartridge, &rom));
    assert!(runtime.load_media(&wrong).is_err());
    let mut tape = MediaSet::new();
    tape.push(MediaImage::new("tape-1", MediaKind::Tape, &rom));
    assert!(runtime.load_media(&tape).is_err());

    // A SuperFX board is refused.
    let mut superfx = rom.clone();
    superfx[0x7FD6] = 0x13;
    let checksum = format_nintendo_snes_sfc::compute_checksum(&superfx);
    superfx[0x7FDC..0x7FDE].copy_from_slice(&(!checksum).to_le_bytes());
    superfx[0x7FDE..0x7FE0].copy_from_slice(&checksum.to_le_bytes());
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        "cartridge-1",
        MediaKind::Cartridge,
        &superfx,
    ));
    assert!(matches!(
        runtime.load_media(&media),
        Err(MachineError::InvalidMedia { .. })
    ));
}

#[test]
fn the_header_names_the_model_a_cartridge_wants() {
    use runtime_nintendo_snes::cartridge::destination;
    assert_eq!(destination(&common::headed_cartridge(2)), Some((2, true)));
    let (code, pal) = destination(&common::headed_cartridge(0)).expect("header");
    assert_eq!(Model::for_destination(code, pal), Model::SuperFamicom);
}

/// Native events, rather than the session's pacing hint, determine both count
/// and elapsed time. The independent machine path advances the same fields.
#[test]
fn exact_native_frames_match_machine_boundaries_for_every_profile() {
    for model in Model::ALL {
        for pacing in [1, model.frame_ticks(), model.frame_ticks() * 3] {
            for count in [0, 1, 2, 5] {
                for traced in [false, true] {
                    let mut expected = with_cartridge(model, common::cartridge());
                    for _ in 0..count {
                        expected.machine_mut().expect("machine").run_frame();
                    }
                    let mut session =
                        HeadlessSession::new(with_cartridge(model, common::cartridge()), pacing);
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
