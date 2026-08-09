//! Snapshot/restore round-trip coverage for the C64 runtime.

mod common;

use emu198x_shell::{HostIo, MachineCore, MachineError, MachineTime, NullAudioSink, NullTraceSink};
use runtime_commodore_c64::{C64Runtime, Model};

use common::{FrameCollector, blank_firmware, blank_firmware_with_drive};

#[test]
fn snapshot_round_trip_preserves_mid_cycle_runtime_state() {
    let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("blank C64 firmware should construct a runtime");
    let mut frame_sink = FrameCollector::default();
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;
    let target = MachineTime::new(3);

    runtime
        .run_until(
            target,
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut frame_sink,
                audio_sink: &mut audio_sink,
                trace_sink: &mut trace_sink,
            },
        )
        .expect("blank C64 runtime should run a few cycles");

    let snapshot = runtime
        .snapshot()
        .expect("blank C64 runtime should snapshot");
    let mut expected_machine = runtime.machine().clone();
    let expected_time = runtime.time();

    let mut restored = C64Runtime::blank(Model::C64PalBreadbin);
    restored
        .restore(&snapshot)
        .expect("snapshot restore should succeed");

    assert_eq!(restored.time(), expected_time);
    assert_eq!(restored.machine().cpu().regs, expected_machine.cpu().regs);
    assert_eq!(restored.machine().cpu().addr, expected_machine.cpu().addr);
    assert_eq!(restored.machine().cpu().rw, expected_machine.cpu().rw);
    assert_eq!(restored.machine().cpu().sync, expected_machine.cpu().sync);
    assert_eq!(
        restored.machine().raster_line(),
        expected_machine.raster_line()
    );
    assert_eq!(
        restored.machine().cycle_in_line(),
        expected_machine.cycle_in_line()
    );
    assert_eq!(
        restored.machine().framebuffer(),
        expected_machine.framebuffer()
    );

    for _ in 0..8 {
        let expected_frame_complete = expected_machine.tick();
        let restored_frame_complete = restored.machine_mut().tick();
        assert_eq!(restored_frame_complete, expected_frame_complete);
        assert_eq!(restored.machine().cpu().regs, expected_machine.cpu().regs);
        assert_eq!(restored.machine().cpu().addr, expected_machine.cpu().addr);
        assert_eq!(restored.machine().cpu().rw, expected_machine.cpu().rw);
        assert_eq!(restored.machine().cpu().sync, expected_machine.cpu().sync);
        assert_eq!(
            restored.machine().cpu().total_cycles,
            expected_machine.cpu().total_cycles
        );
        assert_eq!(
            restored.machine().raster_line(),
            expected_machine.raster_line()
        );
        assert_eq!(
            restored.machine().cycle_in_line(),
            expected_machine.cycle_in_line()
        );
        assert_eq!(restored.machine().vic().irq, expected_machine.vic().irq);
        assert_eq!(
            restored.machine().vic().ba_low,
            expected_machine.vic().ba_low
        );
        assert_eq!(
            restored.machine().vic().ba_low_cycles(),
            expected_machine.vic().ba_low_cycles()
        );
        assert_eq!(
            restored.machine().vic().aec_is_low(),
            expected_machine.vic().aec_is_low()
        );
        assert_eq!(
            restored.machine().vic().badline_ba_is_low(),
            expected_machine.vic().badline_ba_is_low()
        );
        assert_eq!(
            restored.machine().vic().sprite_ba_is_low(),
            expected_machine.vic().sprite_ba_is_low()
        );
        assert_eq!(
            restored.machine().vic().c_access_is_active(),
            expected_machine.vic().c_access_is_active()
        );
        assert_eq!(
            restored.machine().vic().pending_d011_write_cycle(),
            expected_machine.vic().pending_d011_write_cycle()
        );
        assert_eq!(
            restored.machine().vic().late_badline_fetches_remaining(),
            expected_machine.vic().late_badline_fetches_remaining()
        );
        assert_eq!(
            restored.machine().framebuffer(),
            expected_machine.framebuffer()
        );
    }
}

#[test]
fn snapshot_round_trip_preserves_ba_to_aec_handover() {
    let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("blank C64 firmware should construct a runtime");
    let machine = runtime.machine_mut();
    machine.cpu_write(0xD011, 0x11); // DEN on; line $30 is not initially bad
    while machine.raster_line() != 0x30 || machine.cycle_in_line() != 16 {
        machine.tick();
    }

    // Force a badline after the ordinary cycles 12-14 BA lead has passed.
    // Processing cycle 16 performs the first invalid c-access and leaves the
    // global handover counter one cycle old.
    machine.cpu_write(0xD011, 0x10);
    machine.tick();
    assert!(machine.vic().ba_low);
    assert!(!machine.vic().aec_is_low());
    assert_eq!(machine.vic().ba_low_cycles(), 1);

    let snapshot = runtime
        .snapshot()
        .expect("mid-handover C64 runtime should snapshot");
    let mut expected = runtime.machine().clone();
    let mut restored = C64Runtime::blank(Model::C64PalBreadbin);
    restored
        .restore(&snapshot)
        .expect("mid-handover snapshot should restore");

    for _ in 0..4 {
        assert_eq!(restored.machine_mut().tick(), expected.tick());
        assert_eq!(
            restored.machine().vic().ba_low_cycles(),
            expected.vic().ba_low_cycles()
        );
        assert_eq!(
            restored.machine().vic().aec_is_low(),
            expected.vic().aec_is_low()
        );
        assert_eq!(
            restored.machine().vic().late_badline_fetches_remaining(),
            expected.vic().late_badline_fetches_remaining()
        );
        assert_eq!(restored.machine().framebuffer(), expected.framebuffer());
    }
}

#[test]
fn snapshot_round_trip_preserves_exhausted_late_badline_window() {
    let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("blank C64 firmware should construct a runtime");
    let machine = runtime.machine_mut();
    machine.cpu_write(0xD011, 0x11); // DEN on; line $30 is not initially bad
    while machine.raster_line() != 0x30 || machine.cycle_in_line() != 53 {
        machine.tick();
    }

    machine.cpu_write(0xD011, 0x10);
    assert_eq!(machine.vic().pending_d011_write_cycle(), Some(53));
    let pending_snapshot = runtime
        .snapshot()
        .expect("pending late-badline write should snapshot");
    let mut restored = C64Runtime::blank(Model::C64PalBreadbin);
    restored
        .restore(&pending_snapshot)
        .expect("pending late-badline write should restore");
    assert_eq!(
        restored.machine().vic().pending_d011_write_cycle(),
        Some(53)
    );

    restored.machine_mut().tick();
    assert!(restored.machine().vic().c_access_is_active());
    assert_eq!(
        restored.machine().vic().late_badline_fetches_remaining(),
        Some(0)
    );

    let exhausted_snapshot = restored
        .snapshot()
        .expect("exhausted late-badline window should snapshot");
    let mut expected = restored.machine().clone();
    let mut exhausted = C64Runtime::blank(Model::C64PalBreadbin);
    exhausted
        .restore(&exhausted_snapshot)
        .expect("exhausted late-badline window should restore");
    assert_eq!(
        exhausted.machine().vic().late_badline_fetches_remaining(),
        Some(0)
    );
    assert!(exhausted.machine().vic().badline_ba_is_low());
    assert!(exhausted.machine().vic().c_access_is_active());

    assert_eq!(exhausted.machine_mut().tick(), expected.tick());
    assert!(!exhausted.machine().vic().badline_ba_is_low());
    assert!(!exhausted.machine().vic().c_access_is_active());
    assert_eq!(
        exhausted.machine().vic().late_badline_fetches_remaining(),
        Some(0)
    );
}

#[test]
fn snapshot_round_trip_preserves_live_sprite_pipeline() {
    let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("blank C64 firmware should construct a runtime");

    // Put one solid sprite through the live MC/MCBASE fetch chain and the
    // draw-stage shift register. The snapshot point is deliberately inside
    // the sprite rather than at a frame boundary: resetting either pipeline
    // to its default state on restore drops the remaining pixels.
    let machine = runtime.machine_mut();
    machine.cpu_write(0xD011, 0x1B); // DEN + 25-row display
    machine.cpu_write(0xD018, 0x14); // screen matrix $0400
    machine.cpu_write(0xD015, 0x01); // sprite 0 enabled
    machine.cpu_write(0xD000, 172); // X -> framebuffer x=196
    machine.cpu_write(0xD001, 100); // first rendered row is 101 in this harness
    machine.cpu_write(0xD027, 0x01); // white
    machine.cpu_write(0x07F8, 0x80); // sprite pointer -> $2000
    for offset in 0..63u16 {
        machine.cpu_write(0x2000 + offset, 0xFF);
    }
    while machine.raster_line() != 101 || machine.cycle_in_line() != 35 {
        machine.tick();
    }

    let snapshot = runtime
        .snapshot()
        .expect("active-sprite C64 runtime should snapshot");
    let mut expected = runtime.machine().clone();
    let mut restored = C64Runtime::blank(Model::C64PalBreadbin);
    restored
        .restore(&snapshot)
        .expect("active-sprite snapshot should restore");

    for _ in 0..8 {
        assert_eq!(restored.machine_mut().tick(), expected.tick());
    }
    let row_start = 101 * mos_vic_ii::FB_WIDTH as usize;
    let sprite_span = row_start + 190..row_start + 230;
    assert_eq!(
        &restored.machine().framebuffer()[sprite_span.clone()],
        &expected.framebuffer()[sprite_span],
        "restored sprite shift/fetch state must draw the same remaining pixels"
    );
}

#[test]
fn snapshot_round_trip_preserves_queued_sid_samples() {
    let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("blank C64 firmware should construct a runtime");

    // Runtime audio is drained only when a frame completes. Snapshot in the
    // middle of the frame, after the deterministic SID decimator has queued
    // output that belongs to the eventual frame packet.
    for _ in 0..1_000 {
        runtime.machine_mut().tick();
    }
    assert!(
        runtime.machine().sid().buffer_len() > 0,
        "test setup must have queued SID output"
    );

    let snapshot = runtime
        .snapshot()
        .expect("mid-frame audio C64 runtime should snapshot");
    let mut expected = runtime.machine().clone();
    let mut restored = C64Runtime::blank(Model::C64PalBreadbin);
    restored
        .restore(&snapshot)
        .expect("mid-frame audio snapshot should restore");

    assert_eq!(
        restored.machine_mut().take_audio_buffer(),
        expected.take_audio_buffer(),
        "restored runtime must retain samples already generated this frame"
    );
    assert_eq!(
        restored.machine_mut().take_audio_channel_buffers(),
        expected.take_audio_channel_buffers(),
        "restored runtime must retain per-voice samples generated this frame"
    );
}

#[test]
fn runtime_drains_sid_voice_buffers_at_frame_boundary() {
    use common_commodore_c64::TIMING_PAL_BREADBIN;

    let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("blank C64 firmware should construct a runtime");
    let mut frame_sink = FrameCollector::default();
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;

    runtime
        .run_until(
            MachineTime::new(u64::from(TIMING_PAL_BREADBIN.cycles_per_frame)),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut frame_sink,
                audio_sink: &mut audio_sink,
                trace_sink: &mut trace_sink,
            },
        )
        .expect("blank C64 runtime should complete one frame");

    let channels = runtime.machine_mut().take_audio_channel_buffers();
    assert!(
        channels.iter().all(Vec::is_empty),
        "runtime must not retain diagnostic voice samples after frame output"
    );
}

#[test]
fn snapshot_round_trip_preserves_attached_drive_state() {
    let mut runtime =
        C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware_with_drive())
            .expect("blank C64 firmware with drive should construct a runtime");
    let mut frame_sink = FrameCollector::default();
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;

    runtime
        .run_until(
            MachineTime::new(64),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut frame_sink,
                audio_sink: &mut audio_sink,
                trace_sink: &mut trace_sink,
            },
        )
        .expect("runtime with an attached drive should run");

    let expected_cycles = runtime
        .drive8()
        .expect("drive should be attached before snapshot")
        .cycles();
    let expected_pc = runtime
        .drive8()
        .expect("drive should be attached before snapshot")
        .cpu()
        .regs
        .pc;

    let snapshot = runtime.snapshot().expect("runtime should snapshot");
    let mut restored = C64Runtime::blank(Model::C64PalBreadbin);
    restored
        .restore(&snapshot)
        .expect("snapshot restore should succeed");

    let drive = restored
        .drive8()
        .expect("drive should restore from snapshot");
    assert_eq!(drive.cycles(), expected_cycles);
    assert_eq!(drive.cpu().regs.pc, expected_pc);
}

#[test]
fn snapshot_round_trip_preserves_ram_expansion_and_mouse_bookkeeping() {
    // Regression: the runtime-level "stays plugged in across a reset"
    // bookkeeping (REU/GeoRAM sizes, the 1351 mouse port, the cartridge image)
    // was not in the snapshot envelope, so a restored snapshot dropped it on
    // the next reset. A REU in the expansion port and a mouse in a control port
    // are independent, so both can be attached at once.
    let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("blank C64 firmware should construct a runtime");
    runtime.set_reu(Some(512));
    runtime.set_mouse_1351(Some(1));
    assert_eq!(runtime.reu_kb(), Some(512));
    assert_eq!(runtime.mouse_1351_port(), Some(1));

    let snapshot = runtime.snapshot().expect("runtime should snapshot");
    let mut restored = C64Runtime::blank(Model::C64PalBreadbin);
    restored
        .restore(&snapshot)
        .expect("snapshot restore should succeed");

    assert_eq!(
        restored.reu_kb(),
        Some(512),
        "REU size must survive restore so a reset re-attaches it"
    );
    assert_eq!(
        restored.mouse_1351_port(),
        Some(1),
        "1351 mouse port must survive restore"
    );
}

#[test]
fn restore_rejects_corrupt_postcard_bytes() {
    let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("blank C64 firmware should construct a runtime");
    let err = runtime
        .restore(&[0xFF, 0xFF, 0xFF, 0xFF])
        .expect_err("garbage bytes should not deserialise");
    assert!(
        matches!(err, MachineError::InvalidSnapshot { ref reason } if reason.contains("decode failed")),
        "unexpected error variant: {err:?}",
    );
}

#[test]
fn restore_rejects_old_schema_before_decoding_its_payload() {
    let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("blank C64 firmware should construct a runtime");
    let err = runtime
        .restore(&[5])
        .expect_err("version 5 snapshot should be rejected before payload decode");
    assert!(
        matches!(err, MachineError::InvalidSnapshot { ref reason }
            if reason == "unsupported snapshot version 5; expected 6"),
        "unexpected error variant: {err:?}",
    );
}

/// Profile mismatch is the safety belt that prevents an NTSC
/// snapshot from being restored into a PAL runtime (or vice-versa).
/// Build a runtime in one model, snapshot it, then try to restore
/// into the other model.
#[test]
fn restore_rejects_snapshot_from_different_profile() {
    let mut pal = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("PAL runtime should build from blank firmware");
    let mut frame_sink = FrameCollector::default();
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;

    pal.run_until(
        MachineTime::new(8),
        &mut HostIo {
            input_events: &[],
            frame_sink: &mut frame_sink,
            audio_sink: &mut audio_sink,
            trace_sink: &mut trace_sink,
        },
    )
    .expect("PAL runtime should run a few cycles");

    let snapshot = pal.snapshot().expect("PAL runtime should snapshot");
    let mut ntsc = C64Runtime::blank(Model::C64NtscBreadbin);
    let err = ntsc
        .restore(&snapshot)
        .expect_err("NTSC runtime should refuse a PAL-profile snapshot");
    assert!(
        matches!(err, MachineError::InvalidSnapshot { ref reason } if reason.contains("profile")),
        "unexpected error variant: {err:?}",
    );
}
