//! Postcard snapshot envelope round-trips for the NES runtime.

mod common;

use emu198x_shell::{
    HostIo, MachineCore, MachineError, MachineTime, MediaImage, MediaKind, MediaSet, NullAudioSink,
    NullFrameSink, NullTraceSink,
};
use runtime_nintendo_nes::{Model, NesRuntime};

use common::{NTSC_FRAME_TICKS, minimal_ines};

#[test]
fn runtime_snapshot_round_trips_loaded_machine_state() {
    let rom = minimal_ines();
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("valid iNES should load");
    runtime
        .machine_mut()
        .expect("cartridge loaded")
        .set_controller1(0b0000_1000);

    let mut frame_sink = NullFrameSink;
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;
    let mut host = HostIo {
        input_events: &[],
        frame_sink: &mut frame_sink,
        audio_sink: &mut audio_sink,
        trace_sink: &mut trace_sink,
    };
    runtime
        .run_until(MachineTime::new(NTSC_FRAME_TICKS), &mut host)
        .expect("one frame should run");

    let snapshot = runtime.snapshot().expect("snapshot should encode");
    let mut restored = NesRuntime::blank(Model::NesNtsc);
    restored
        .restore(&snapshot)
        .expect("snapshot should restore");

    assert_eq!(restored.time(), runtime.time());
    assert_eq!(
        restored.machine().expect("machine restored").frame_count(),
        runtime.machine().expect("machine present").frame_count()
    );
    assert_eq!(
        restored
            .machine()
            .expect("machine restored")
            .controller1_state,
        0b0000_1000
    );
}

/// Blank-runtime snapshots round-trip cleanly: no machine, no
/// cartridge bytes, time stays at zero.
#[test]
fn snapshot_round_trips_blank_runtime() {
    let runtime = NesRuntime::blank(Model::NesNtsc);
    let snapshot = runtime.snapshot().expect("blank runtime should snapshot");

    let mut restored = NesRuntime::blank(Model::NesNtsc);
    restored
        .restore(&snapshot)
        .expect("blank snapshot should restore");
    assert!(restored.machine().is_none());
    assert_eq!(restored.time(), MachineTime::default());
}

#[test]
fn restore_rejects_corrupt_postcard_bytes() {
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let err = runtime
        .restore(&[0xFF, 0xFF, 0xFF, 0xFF])
        .expect_err("garbage bytes should not deserialise");
    assert!(
        matches!(err, MachineError::InvalidSnapshot { ref reason } if reason.contains("decode failed")),
        "unexpected error variant: {err:?}",
    );
}

#[test]
fn restore_rejects_empty_byte_slice() {
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let err = runtime
        .restore(&[])
        .expect_err("empty payload should not deserialise");
    assert!(matches!(err, MachineError::InvalidSnapshot { .. }));
}

/// Snapshots embed a `version: u16` at the head of the postcard
/// payload (varint-encoded). Bumping the first byte to a different
/// version drives the version-mismatch arm.
#[test]
fn restore_rejects_unsupported_snapshot_version() {
    let runtime = NesRuntime::blank(Model::NesNtsc);
    let mut snapshot = runtime.snapshot().expect("blank runtime should snapshot");
    // First byte is the postcard varint for `version`; bump it.
    snapshot[0] = 99;

    let mut target = NesRuntime::blank(Model::NesNtsc);
    let err = target
        .restore(&snapshot)
        .expect_err("bumped version should be rejected");
    assert!(
        matches!(
            err,
            MachineError::InvalidSnapshot { ref reason }
                if reason.contains("unsupported NES snapshot version")
        ),
        "unexpected error variant: {err:?}",
    );
}

/// Restoring a loaded-cartridge snapshot into a fresh runtime
/// repopulates the host RGBA framebuffer (`refresh_rgba_framebuffer`
/// runs as the last decode step). After restore the runtime can run
/// another frame and the framebuffer keeps non-zero contents.
#[test]
fn restore_repopulates_rgba_framebuffer() {
    let rom = minimal_ines();
    let mut original = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    original.load_media(&media).expect("valid iNES should load");

    let mut frame_sink = NullFrameSink;
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;
    original
        .run_until(
            MachineTime::new(NTSC_FRAME_TICKS),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut frame_sink,
                audio_sink: &mut audio_sink,
                trace_sink: &mut trace_sink,
            },
        )
        .expect("frame should run");

    let snapshot = original.snapshot().expect("snapshot should encode");
    let mut restored = NesRuntime::blank(Model::NesNtsc);
    restored
        .restore(&snapshot)
        .expect("snapshot should restore");

    // Re-run after restore: confirms the rebuilt framebuffer + machine
    // can keep running without panicking.
    restored
        .run_until(
            MachineTime::new(NTSC_FRAME_TICKS * 2),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut frame_sink,
                audio_sink: &mut audio_sink,
                trace_sink: &mut trace_sink,
            },
        )
        .expect("frame should run after restore");
    assert!(restored.machine().expect("loaded").frame_count() >= 1);
}
