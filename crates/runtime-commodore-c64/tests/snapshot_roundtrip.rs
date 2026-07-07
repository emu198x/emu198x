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
            restored.machine().framebuffer(),
            expected_machine.framebuffer()
        );
    }
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
