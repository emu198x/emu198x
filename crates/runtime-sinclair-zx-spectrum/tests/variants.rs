//! Integration tests for the generic `SpectrumRuntime<M>` driving the
//! non-48K family variants. These exercise frame/audio output, the
//! disk-slot opt-in (Plus3 vs +2A), snapshot round-trips for the two
//! representative variants (128K and Scorpion ZS-256), and keyboard
//! plumbing end-to-end through `run_until`.
//!
//! Lifted verbatim from the inline `#[cfg(test)] mod tests` block in
//! `src/variants.rs` — every test exercises the public API, so the
//! move is a pure relocation.

use common_sinclair_zx_spectrum::timing::{
    SCREEN_HEIGHT, SCREEN_WIDTH, SCREEN_WIDTH_HIRES, TIMING_48K, TIMING_128K, TIMING_PENTAGON,
    TIMING_PLUS2A, TIMING_SCORPION,
};
use emu198x_shell::{
    AudioPacket, AudioSink, ControlCommand, FramePacket, FrameSink, HostIo, InputEvent,
    MachineCore, MachineError, MachineTime, MediaImage, MediaKind, MediaSet, MediaTransportAction,
    MediaTransportCommand, NullTraceSink, PixelFormat, ResetKind, SessionQueryProvider,
    known_capability,
};
use machine_pentagon_128::Pentagon128;
use machine_scorpion_zs256::ScorpionZS256;
use machine_sinclair_zx_spectrum_48k::Spectrum48k;
use machine_sinclair_zx_spectrum_128k::Spectrum128K;
use machine_sinclair_zx_spectrum_plus::{Model as PlusModel, SpectrumPlus};
use machine_timex_tc2048::TimexTC2048;
use machine_timex_ts2068::{TIMING_TS2068, TimexModel, TimexTS2068};
use runtime_sinclair_zx_spectrum::{
    Model, Pentagon128Runtime, ScorpionZS256Runtime, Spectrum48kRuntime, Spectrum128kRuntime,
    SpectrumMachine, SpectrumPlusRuntime, SpectrumRuntime, SpectrumSessionQueryProvider,
    TimexTC2048Runtime, TimexTS2068Runtime,
};

#[derive(Default)]
struct RecordingFrameSink {
    frames: usize,
    last_dimensions: Option<(u32, u32)>,
    last_format: Option<PixelFormat>,
}

impl FrameSink for RecordingFrameSink {
    fn push_frame(&mut self, frame: FramePacket<'_>) -> Result<(), MachineError> {
        self.frames += 1;
        self.last_dimensions = Some((frame.width, frame.height));
        self.last_format = Some(frame.format);
        Ok(())
    }
}

#[derive(Default)]
struct RecordingAudioSink {
    packets: usize,
}

impl AudioSink for RecordingAudioSink {
    fn push_audio(&mut self, _packet: AudioPacket<'_>) -> Result<(), MachineError> {
        self.packets += 1;
        Ok(())
    }
}

fn run_single_frame<M: SpectrumMachine>(
    mut runtime: SpectrumRuntime<M>,
    frame_halfcycles: u32,
) -> (usize, usize, Option<(u32, u32)>) {
    let mut frame_sink = RecordingFrameSink::default();
    let mut audio_sink = RecordingAudioSink::default();
    let mut trace_sink = NullTraceSink;
    let mut host = HostIo {
        input_events: &[],
        frame_sink: &mut frame_sink,
        audio_sink: &mut audio_sink,
        trace_sink: &mut trace_sink,
    };
    runtime
        .run_until(MachineTime::new(u64::from(frame_halfcycles)), &mut host)
        .expect("single frame should run");
    (
        frame_sink.frames,
        audio_sink.packets,
        frame_sink.last_dimensions,
    )
}

fn run_single_frame_by_ref<M: SpectrumMachine>(
    runtime: &mut SpectrumRuntime<M>,
    frame_halfcycles: u32,
) -> (usize, usize, Option<(u32, u32)>) {
    let mut frame_sink = RecordingFrameSink::default();
    let mut audio_sink = RecordingAudioSink::default();
    let mut trace_sink = NullTraceSink;
    let mut host = HostIo {
        input_events: &[],
        frame_sink: &mut frame_sink,
        audio_sink: &mut audio_sink,
        trace_sink: &mut trace_sink,
    };
    runtime
        .run_until(MachineTime::new(u64::from(frame_halfcycles)), &mut host)
        .expect("single frame should run");
    (
        frame_sink.frames,
        audio_sink.packets,
        frame_sink.last_dimensions,
    )
}

#[test]
fn spectrum_128k_runtime_emits_frame_and_audio() {
    let runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
    let (frames, audio, dims) = run_single_frame(runtime, TIMING_128K.halfcycles_per_frame);
    assert_eq!(frames, 1);
    assert_eq!(audio, 1);
    assert_eq!(dims, Some((SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32)));
}

#[test]
fn pentagon_128_runtime_emits_frame_at_pentagon_dimensions() {
    let runtime = Pentagon128Runtime::new(Model::Pentagon128, Pentagon128::new());
    let (frames, _, dims) = run_single_frame(runtime, TIMING_PENTAGON.halfcycles_per_frame);
    assert_eq!(frames, 1);
    assert_eq!(dims, Some((SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32)));
}

#[test]
fn timex_tc2048_runtime_emits_hires_dimensions() {
    let runtime = TimexTC2048Runtime::new(Model::TimexTC2048, TimexTC2048::new());
    let (frames, _, dims) = run_single_frame(runtime, TIMING_48K.halfcycles_per_frame);
    assert_eq!(frames, 1);
    assert_eq!(
        dims,
        Some((SCREEN_WIDTH_HIRES as u32, SCREEN_HEIGHT as u32))
    );
}

#[test]
fn timex_ts2068_runtime_uses_ntsc_frame_length_for_ts() {
    let runtime = TimexTS2068Runtime::new(Model::TimexTS2068, TimexTS2068::new(TimexModel::TS2068));
    let (frames, _, _) = run_single_frame(runtime, TIMING_TS2068.halfcycles_per_frame);
    assert_eq!(frames, 1);
}

#[test]
fn spectrum_128k_runtime_round_trips_through_snapshot() {
    let mut runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
    run_single_frame_by_ref(&mut runtime, TIMING_128K.halfcycles_per_frame);
    let bytes = runtime.snapshot().expect("snapshot should encode");

    let mut restored = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
    restored
        .restore(&bytes)
        .expect("snapshot should restore into a fresh runtime");

    let round_trip = restored
        .snapshot()
        .expect("restored snapshot should encode");
    assert_eq!(round_trip, bytes);
}

#[test]
fn spectrum_plus3_accepts_disk_slot_via_machine_core() {
    let mut runtime =
        SpectrumPlusRuntime::new(Model::SpectrumPlus3, SpectrumPlus::new(PlusModel::Plus3));
    let mut media = MediaSet::new();
    let dsk = minimal_standard_dsk();
    media.push(MediaImage::new("disk-a", MediaKind::Disk, &dsk));

    runtime
        .load_media(&media)
        .expect("Plus3 should accept DSK media into disk-a");
}

#[test]
fn spectrum_plus2a_rejects_disk_slot() {
    let mut runtime =
        SpectrumPlusRuntime::new(Model::SpectrumPlus2A, SpectrumPlus::new(PlusModel::Plus2A));
    let mut media = MediaSet::new();
    let dsk = minimal_standard_dsk();
    media.push(MediaImage::new("disk-a", MediaKind::Disk, &dsk));

    let err = runtime
        .load_media(&media)
        .expect_err("+2A has no drive and must refuse disk slots");
    assert!(matches!(err, MachineError::UnknownMediaSlot { .. }));
}

fn minimal_standard_dsk() -> Vec<u8> {
    // Standard DSK: 256-byte disk header + one 256-byte Track-Info
    // block with zero sectors. 512 bytes in total.
    let mut dsk = vec![0u8; 512];
    dsk[..8].copy_from_slice(b"MV - CPC");
    dsk[0x30] = 1; // 1 track per side
    dsk[0x31] = 1; // single-sided
    // Track size in bytes (little-endian u16): 256 bytes.
    dsk[0x32] = 0x00;
    dsk[0x33] = 0x01;
    // Track-Info block at offset 256.
    dsk[256..256 + 10].copy_from_slice(b"Track-Info");
    // track_n = 2 (512-byte sectors — unused here), sector_count = 0.
    dsk[256 + 0x14] = 2;
    dsk[256 + 0x15] = 0;
    dsk
}

#[test]
fn scorpion_runtime_round_trips_through_snapshot() {
    // Scorpion has 16 RAM banks + 4 ROMs — the largest of the family
    // at ~320 KB inline. Verifies the heap-backed bank storage holds.
    let mut runtime = ScorpionZS256Runtime::new(Model::ScorpionZS256, ScorpionZS256::new());
    run_single_frame_by_ref(&mut runtime, TIMING_SCORPION.halfcycles_per_frame);
    let bytes = runtime.snapshot().expect("snapshot should encode");

    let mut restored = ScorpionZS256Runtime::new(Model::ScorpionZS256, ScorpionZS256::new());
    restored
        .restore(&bytes)
        .expect("snapshot should restore into a fresh Scorpion runtime");

    let round_trip = restored
        .snapshot()
        .expect("restored snapshot should encode");
    assert_eq!(round_trip, bytes);
}

#[test]
fn keyboard_input_updates_machine_matrix() {
    let mut runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
    let inputs = [InputEvent::Key {
        name: "space".into(),
        pressed: true,
    }];
    let mut frame_sink = RecordingFrameSink::default();
    let mut audio_sink = RecordingAudioSink::default();
    let mut trace_sink = NullTraceSink;
    let mut host = HostIo {
        input_events: &inputs,
        frame_sink: &mut frame_sink,
        audio_sink: &mut audio_sink,
        trace_sink: &mut trace_sink,
    };

    runtime
        .run_until(
            MachineTime::new(u64::from(TIMING_128K.halfcycles_per_frame)),
            &mut host,
        )
        .expect("frame should run with key held");

    // Space is at row 7, bit 0 — active-low.
    assert_eq!(runtime.machine().keyboard[7] & 0x01, 0x00);
}

// ─────────────────────────────────────────────────────────────────────
// Per-variant smoke matrix: construct + frame + reset + tape-load +
// snapshot round-trip via the generic SpectrumRuntime<M>. These exist
// to exercise every `impl SpectrumMachine` block in `src/variants.rs`
// — most variants only had a frame test before Cov-5b, so 37 of 72
// trait-method instantiations sat uncovered.
// ─────────────────────────────────────────────────────────────────────

fn run_one_frame_with_inputs<M: SpectrumMachine>(
    runtime: &mut SpectrumRuntime<M>,
    frame_halfcycles: u32,
    inputs: &[InputEvent],
) {
    let mut frame_sink = RecordingFrameSink::default();
    let mut audio_sink = RecordingAudioSink::default();
    let mut trace_sink = NullTraceSink;
    let mut host = HostIo {
        input_events: inputs,
        frame_sink: &mut frame_sink,
        audio_sink: &mut audio_sink,
        trace_sink: &mut trace_sink,
    };
    runtime
        .run_until(MachineTime::new(u64::from(frame_halfcycles)), &mut host)
        .expect("frame should run");
}

fn minimal_tap() -> Vec<u8> {
    // 19-byte standard-speed header block: length-prefixed (0x0013),
    // flag byte 0x00, 17 bytes of payload (zeroed), checksum 0x00.
    let mut tap = vec![0x13, 0x00];
    tap.push(0x00);
    tap.extend_from_slice(&[0; 17]);
    tap.push(0x00);
    tap
}

fn minimal_tzx() -> Vec<u8> {
    // ZXTape! 0x1A, version 1.20, no blocks. Round-trips through
    // `tzx_to_stream` to an empty span list; exercises the TZX
    // dispatch arm in `runtime::load_tape_bytes`.
    let mut tzx = b"ZXTape!\x1a".to_vec();
    tzx.push(1);
    tzx.push(20);
    tzx
}

fn load_tape_into_runtime<M: SpectrumMachine>(runtime: &mut SpectrumRuntime<M>, bytes: &[u8]) {
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-1", MediaKind::Tape, bytes));
    runtime.load_media(&media).expect("tape media should load");
}

fn snapshot_round_trip_is_fixed_point<M, F>(runtime: &mut SpectrumRuntime<M>, mut fresh_fn: F)
where
    M: SpectrumMachine,
    F: FnMut() -> SpectrumRuntime<M>,
{
    let bytes = runtime.snapshot().expect("snapshot should encode");
    let mut restored = fresh_fn();
    restored
        .restore(&bytes)
        .expect("snapshot should restore into a fresh runtime");
    let round_trip = restored
        .snapshot()
        .expect("restored snapshot should encode");
    assert_eq!(round_trip, bytes);
}

/// Run `f` on a dedicated thread with an 8 MiB stack. Some
/// Spectrum-family machines (Timex TC2048 / TS2068) carry 64 KiB of
/// memory inline as `[u8; …]`; constructing two of them plus snapshot
/// bytes on the default test thread stack overflows.
fn run_with_large_stack<F>(f: F)
where
    F: FnOnce() + Send + 'static,
{
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(f)
        .expect("worker thread should spawn")
        .join()
        .expect("worker thread should not panic");
}

#[test]
fn spectrum_128k_runtime_loads_tap_and_drives_transport() {
    let mut runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
    load_tape_into_runtime(&mut runtime, &minimal_tap());

    runtime
        .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
            "tape-1",
            MediaTransportAction::Start,
        )))
        .expect("128K tape transport start should succeed");
    runtime
        .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
            "tape-1",
            MediaTransportAction::Stop,
        )))
        .expect("128K tape transport stop should succeed");
}

#[test]
fn spectrum_128k_runtime_loads_tzx_via_runtime() {
    let mut runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
    load_tape_into_runtime(&mut runtime, &minimal_tzx());
}

#[test]
fn spectrum_128k_runtime_resets_via_machine_core() {
    let mut runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
    run_one_frame_with_inputs(
        &mut runtime,
        TIMING_128K.halfcycles_per_frame,
        &[InputEvent::Key {
            name: "space".into(),
            pressed: true,
        }],
    );
    assert!(runtime.time().get() > 0);
    runtime.reset(ResetKind::Hard);
    assert_eq!(runtime.time(), MachineTime::default());
    // Reset should clear the cached keyboard matrix back to the
    // "all keys released" baseline (active-low: 0xFF).
    assert_eq!(runtime.machine().keyboard, [0xFF; 8]);
}

#[test]
fn spectrum_plus2a_runtime_runs_frame_loads_tape_and_resets() {
    let mut runtime =
        SpectrumPlusRuntime::new(Model::SpectrumPlus2A, SpectrumPlus::new(PlusModel::Plus2A));
    run_one_frame_with_inputs(&mut runtime, TIMING_PLUS2A.halfcycles_per_frame, &[]);
    load_tape_into_runtime(&mut runtime, &minimal_tap());
    runtime
        .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
            "tape-1",
            MediaTransportAction::Start,
        )))
        .expect("+2A tape transport start should succeed");
    runtime
        .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
            "tape-1",
            MediaTransportAction::Stop,
        )))
        .expect("+2A tape transport stop should succeed");
    runtime.reset(ResetKind::Hard);
    assert_eq!(runtime.time(), MachineTime::default());
}

#[test]
fn spectrum_plus2a_runtime_round_trips_through_snapshot() {
    let mut runtime =
        SpectrumPlusRuntime::new(Model::SpectrumPlus2A, SpectrumPlus::new(PlusModel::Plus2A));
    run_one_frame_with_inputs(&mut runtime, TIMING_PLUS2A.halfcycles_per_frame, &[]);
    snapshot_round_trip_is_fixed_point(&mut runtime, || {
        SpectrumPlusRuntime::new(Model::SpectrumPlus2A, SpectrumPlus::new(PlusModel::Plus2A))
    });
}

#[test]
fn spectrum_plus2b_runtime_runs_frame_and_resets() {
    let mut runtime =
        SpectrumPlusRuntime::new(Model::SpectrumPlus2B, SpectrumPlus::new(PlusModel::Plus2B));
    run_one_frame_with_inputs(&mut runtime, TIMING_PLUS2A.halfcycles_per_frame, &[]);
    load_tape_into_runtime(&mut runtime, &minimal_tap());
    runtime.reset(ResetKind::Hard);
    assert_eq!(runtime.time(), MachineTime::default());
}

#[test]
fn spectrum_plus3_runtime_runs_frame_loads_tape_and_resets() {
    let mut runtime =
        SpectrumPlusRuntime::new(Model::SpectrumPlus3, SpectrumPlus::new(PlusModel::Plus3));
    run_one_frame_with_inputs(&mut runtime, TIMING_PLUS2A.halfcycles_per_frame, &[]);
    load_tape_into_runtime(&mut runtime, &minimal_tap());
    runtime
        .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
            "tape-1",
            MediaTransportAction::Start,
        )))
        .expect("+3 tape transport start should succeed");
    runtime.reset(ResetKind::Hard);
    assert_eq!(runtime.time(), MachineTime::default());
}

#[test]
fn spectrum_plus3_runtime_round_trips_through_snapshot() {
    let mut runtime =
        SpectrumPlusRuntime::new(Model::SpectrumPlus3, SpectrumPlus::new(PlusModel::Plus3));
    run_one_frame_with_inputs(&mut runtime, TIMING_PLUS2A.halfcycles_per_frame, &[]);
    snapshot_round_trip_is_fixed_point(&mut runtime, || {
        SpectrumPlusRuntime::new(Model::SpectrumPlus3, SpectrumPlus::new(PlusModel::Plus3))
    });
}

#[test]
fn spectrum_plus3_runtime_rejects_malformed_disk_image() {
    let mut runtime =
        SpectrumPlusRuntime::new(Model::SpectrumPlus3, SpectrumPlus::new(PlusModel::Plus3));
    let mut media = MediaSet::new();
    let bogus = vec![0u8; 32];
    media.push(MediaImage::new("disk-a", MediaKind::Disk, &bogus));

    let err = runtime
        .load_media(&media)
        .expect_err("Plus3 must reject malformed DSK bytes");
    assert!(matches!(err, MachineError::InvalidMedia { .. }));
}

#[test]
fn pentagon_128_runtime_loads_tape_resets_and_round_trips() {
    let mut runtime = Pentagon128Runtime::new(Model::Pentagon128, Pentagon128::new());
    run_one_frame_with_inputs(&mut runtime, TIMING_PENTAGON.halfcycles_per_frame, &[]);
    load_tape_into_runtime(&mut runtime, &minimal_tap());
    runtime
        .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
            "tape-1",
            MediaTransportAction::Start,
        )))
        .expect("Pentagon tape transport start should succeed");
    runtime
        .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
            "tape-1",
            MediaTransportAction::Stop,
        )))
        .expect("Pentagon tape transport stop should succeed");
    runtime.reset(ResetKind::Hard);
    assert_eq!(runtime.time(), MachineTime::default());

    let mut runtime = Pentagon128Runtime::new(Model::Pentagon128, Pentagon128::new());
    run_one_frame_with_inputs(&mut runtime, TIMING_PENTAGON.halfcycles_per_frame, &[]);
    snapshot_round_trip_is_fixed_point(&mut runtime, || {
        Pentagon128Runtime::new(Model::Pentagon128, Pentagon128::new())
    });
}

#[test]
fn pentagon_128_runtime_rejects_disk_slot_via_default_supports_disk_slot() {
    // Pentagon does not override `supports_disk_slot`; the default
    // returns `false`, which sends every disk slot down the
    // `UnknownMediaSlot` path in `MachineCore::load_media`.
    let mut runtime = Pentagon128Runtime::new(Model::Pentagon128, Pentagon128::new());
    let mut media = MediaSet::new();
    media.push(MediaImage::new("disk-a", MediaKind::Disk, &[0u8; 16]));

    let err = runtime
        .load_media(&media)
        .expect_err("Pentagon has no drive — disk slots must be unknown");
    assert!(matches!(err, MachineError::UnknownMediaSlot { .. }));
}

#[test]
fn scorpion_runtime_loads_tape_drives_transport_and_resets() {
    let mut runtime = ScorpionZS256Runtime::new(Model::ScorpionZS256, ScorpionZS256::new());
    run_one_frame_with_inputs(&mut runtime, TIMING_SCORPION.halfcycles_per_frame, &[]);
    load_tape_into_runtime(&mut runtime, &minimal_tap());
    runtime
        .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
            "tape-1",
            MediaTransportAction::Start,
        )))
        .expect("Scorpion tape transport start should succeed");
    runtime
        .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
            "tape-1",
            MediaTransportAction::Stop,
        )))
        .expect("Scorpion tape transport stop should succeed");
    runtime.reset(ResetKind::Hard);
    assert_eq!(runtime.time(), MachineTime::default());
}

#[test]
fn timex_tc2048_runtime_loads_tape_resets_and_round_trips() {
    run_with_large_stack(|| {
        let mut runtime = TimexTC2048Runtime::new(Model::TimexTC2048, TimexTC2048::new());
        run_one_frame_with_inputs(&mut runtime, TIMING_48K.halfcycles_per_frame, &[]);
        load_tape_into_runtime(&mut runtime, &minimal_tap());
        runtime
            .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
                "tape-1",
                MediaTransportAction::Start,
            )))
            .expect("TC2048 tape transport start should succeed");
        runtime
            .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
                "tape-1",
                MediaTransportAction::Stop,
            )))
            .expect("TC2048 tape transport stop should succeed");
        runtime.reset(ResetKind::Hard);
        assert_eq!(runtime.time(), MachineTime::default());

        let mut runtime = TimexTC2048Runtime::new(Model::TimexTC2048, TimexTC2048::new());
        run_one_frame_with_inputs(&mut runtime, TIMING_48K.halfcycles_per_frame, &[]);
        snapshot_round_trip_is_fixed_point(&mut runtime, || {
            TimexTC2048Runtime::new(Model::TimexTC2048, TimexTC2048::new())
        });
    });
}

#[test]
fn timex_tc2068_runtime_runs_pal_frame_length_and_round_trips() {
    run_with_large_stack(|| {
        let mut runtime =
            TimexTS2068Runtime::new(Model::TimexTC2068, TimexTS2068::new(TimexModel::TC2068));
        run_one_frame_with_inputs(&mut runtime, TIMING_48K.halfcycles_per_frame, &[]);
        load_tape_into_runtime(&mut runtime, &minimal_tap());
        runtime
            .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
                "tape-1",
                MediaTransportAction::Start,
            )))
            .expect("TC2068 tape transport start should succeed");
        runtime.reset(ResetKind::Hard);
        assert_eq!(runtime.time(), MachineTime::default());

        let mut runtime =
            TimexTS2068Runtime::new(Model::TimexTC2068, TimexTS2068::new(TimexModel::TC2068));
        run_one_frame_with_inputs(&mut runtime, TIMING_48K.halfcycles_per_frame, &[]);
        snapshot_round_trip_is_fixed_point(&mut runtime, || {
            TimexTS2068Runtime::new(Model::TimexTC2068, TimexTS2068::new(TimexModel::TC2068))
        });
    });
}

#[test]
fn timex_ts2068_runtime_loads_tape_resets_and_round_trips() {
    run_with_large_stack(|| {
        let mut runtime =
            TimexTS2068Runtime::new(Model::TimexTS2068, TimexTS2068::new(TimexModel::TS2068));
        run_one_frame_with_inputs(&mut runtime, TIMING_TS2068.halfcycles_per_frame, &[]);
        load_tape_into_runtime(&mut runtime, &minimal_tap());
        runtime
            .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
                "tape-1",
                MediaTransportAction::Start,
            )))
            .expect("TS2068 tape transport start should succeed");
        runtime
            .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
                "tape-1",
                MediaTransportAction::Stop,
            )))
            .expect("TS2068 tape transport stop should succeed");
        runtime.reset(ResetKind::Hard);
        assert_eq!(runtime.time(), MachineTime::default());

        let mut runtime =
            TimexTS2068Runtime::new(Model::TimexTS2068, TimexTS2068::new(TimexModel::TS2068));
        run_one_frame_with_inputs(&mut runtime, TIMING_TS2068.halfcycles_per_frame, &[]);
        snapshot_round_trip_is_fixed_point(&mut runtime, || {
            TimexTS2068Runtime::new(Model::TimexTS2068, TimexTS2068::new(TimexModel::TS2068))
        });
    });
}

// Cross-variant snapshot mismatch: a snapshot captured against the
// 128K profile should refuse to restore into a Pentagon runtime, even
// though both wrap the same Z80 / AY infrastructure. This drives the
// `decode` profile-id check on a non-48K pair so the generic envelope
// is exercised end-to-end.
#[test]
fn snapshot_refuses_to_restore_across_variants() {
    let mut source = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
    run_one_frame_with_inputs(&mut source, TIMING_128K.halfcycles_per_frame, &[]);
    let bytes = source.snapshot().expect("source snapshot should encode");

    let mut target = Pentagon128Runtime::new(Model::Pentagon128, Pentagon128::new());
    let err = target
        .restore(&bytes)
        .expect_err("128K snapshot must not restore into Pentagon runtime");
    assert!(matches!(err, MachineError::InvalidSnapshot { .. }));
}

#[test]
fn variant_runtime_rejects_unknown_tape_slot() {
    let mut runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
    let mut media = MediaSet::new();
    let tap = minimal_tap();
    media.push(MediaImage::new("tape-99", MediaKind::Tape, &tap));

    let err = runtime
        .load_media(&media)
        .expect_err("only `tape-1` is recognised");
    assert!(matches!(err, MachineError::UnknownMediaSlot { ref slot } if slot == "tape-99"));
}

#[test]
fn variant_runtime_rejects_unsupported_media_kind() {
    let mut runtime = Pentagon128Runtime::new(Model::Pentagon128, Pentagon128::new());
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cart-1", MediaKind::Cartridge, &[0u8; 8]));

    let err = runtime
        .load_media(&media)
        .expect_err("Spectrum runtimes do not accept cartridge-kind media");
    assert!(matches!(err, MachineError::UnsupportedMediaKind { .. }));
}

#[test]
fn variant_runtime_rejects_unknown_transport_slot() {
    let mut runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
    let err = runtime
        .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
            "tape-9",
            MediaTransportAction::Start,
        )))
        .expect_err("transport command on unknown slot must fail");
    assert!(matches!(err, MachineError::UnknownMediaSlot { .. }));
}

#[test]
fn each_variant_accepts_tzx_via_runtime() {
    // Drives the `load_tape_stream` arm of each `SpectrumMachine`
    // impl. Tape-only variants share the path; the SpectrumPlus impl
    // covers all three Plus models since they reuse the same machine
    // type.
    let mut runtime128 = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
    load_tape_into_runtime(&mut runtime128, &minimal_tzx());

    let mut plus2a =
        SpectrumPlusRuntime::new(Model::SpectrumPlus2A, SpectrumPlus::new(PlusModel::Plus2A));
    load_tape_into_runtime(&mut plus2a, &minimal_tzx());

    let mut pentagon = Pentagon128Runtime::new(Model::Pentagon128, Pentagon128::new());
    load_tape_into_runtime(&mut pentagon, &minimal_tzx());

    let mut scorpion = ScorpionZS256Runtime::new(Model::ScorpionZS256, ScorpionZS256::new());
    load_tape_into_runtime(&mut scorpion, &minimal_tzx());

    run_with_large_stack(|| {
        let mut tc2048 = TimexTC2048Runtime::new(Model::TimexTC2048, TimexTC2048::new());
        load_tape_into_runtime(&mut tc2048, &minimal_tzx());

        let mut ts2068 =
            TimexTS2068Runtime::new(Model::TimexTS2068, TimexTS2068::new(TimexModel::TS2068));
        load_tape_into_runtime(&mut ts2068, &minimal_tzx());
    });
}

#[test]
fn variant_runtime_capabilities_reflects_profile() {
    let runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
    let caps = runtime.capabilities();
    // The 128K profile bundles both AY audio and banked memory; both
    // come from the shared `ay_capabilities()` constructor.
    assert!(caps.contains(&known_capability("ay-audio")));
    assert!(caps.contains(&known_capability("banked-memory")));
}

// ─────────────────────────────────────────────────────────────────────
// Per-variant query smoke matrix
//
// Every variant now plugs into the generic
// `SpectrumSessionQueryProvider`. These tests assert that each one
// resolves the shared screen / keyboard / tape / timing paths plus
// its own variant-specific path catalogue. Boot-banner detection
// itself is covered by the 48K integration tests; for variants whose
// banners are still TODO, we just verify the path is wired up and
// returns a sensible default (`detected = false`).
// ─────────────────────────────────────────────────────────────────────

fn assert_shared_query_paths_resolve<M: SpectrumMachine>(runtime: &SpectrumRuntime<M>) {
    let provider = SpectrumSessionQueryProvider;

    let cols = provider
        .query(runtime, "screen.text.cols")
        .expect("screen.text.cols should resolve")
        .expect("provider must own screen.text.cols");
    let rows = provider
        .query(runtime, "screen.text.rows")
        .expect("screen.text.rows should resolve")
        .expect("provider must own screen.text.rows");
    let kbd = provider
        .query(runtime, "spectrum.keyboard.rows")
        .expect("keyboard rows should resolve")
        .expect("provider must own spectrum.keyboard.rows");
    let tape_loaded = provider
        .query(runtime, "spectrum.tape.loaded")
        .expect("tape loaded should resolve")
        .expect("provider must own spectrum.tape.loaded");
    let tape_playing = provider
        .query(runtime, "spectrum.tape.playing")
        .expect("tape playing should resolve")
        .expect("provider must own spectrum.tape.playing");
    let hc = provider
        .query(runtime, "spectrum.machine.half_cycle_in_frame")
        .expect("half-cycle should resolve")
        .expect("provider must own half_cycle_in_frame");
    let tstate = provider
        .query(runtime, "spectrum.machine.tstate_in_frame")
        .expect("tstate should resolve")
        .expect("provider must own tstate_in_frame");

    assert_eq!(cols.value, serde_json::json!(32));
    assert_eq!(rows.value, serde_json::json!(24));
    assert!(kbd.value.is_array());
    assert_eq!(
        kbd.value.as_array().expect("rows must be JSON array").len(),
        8
    );
    assert!(tape_loaded.value.is_boolean());
    assert!(tape_playing.value.is_boolean());
    assert!(hc.value.is_u64());
    assert!(tstate.value.is_u64());

    let unknown = provider
        .query(runtime, "this.path.does.not.exist")
        .expect("unknown path query should not error");
    assert!(unknown.is_none(), "unknown paths must surface as Ok(None)");
}

fn assert_boot_paths_wired<M: SpectrumMachine>(runtime: &SpectrumRuntime<M>) {
    let provider = SpectrumSessionQueryProvider;

    let detected = provider
        .query(runtime, "boot.detected")
        .expect("boot.detected should resolve")
        .expect("provider must own boot.detected");
    let reason = provider
        .query(runtime, "boot.reason")
        .expect("boot.reason should resolve")
        .expect("provider must own boot.reason");
    let row = provider
        .query(runtime, "boot.row")
        .expect("boot.row should resolve")
        .expect("provider must own boot.row");

    assert!(detected.value.is_boolean());
    assert!(reason.value.is_string());
    // `row` is `null` when no banner was found, otherwise a u32. Both
    // are valid — we just want the path to resolve cleanly.
    assert!(row.value.is_null() || row.value.is_u64());
}

#[test]
fn spectrum_128k_runtime_exposes_shared_and_variant_query_paths() {
    let runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
    assert_shared_query_paths_resolve(&runtime);
    assert_boot_paths_wired(&runtime);

    let provider = SpectrumSessionQueryProvider;
    let paths = provider.query_paths(&runtime, Some("screen.text."));
    assert_eq!(
        paths,
        vec![
            "screen.text.cols".to_owned(),
            "screen.text.lines".to_owned(),
            "screen.text.rows".to_owned(),
        ]
    );
}

#[test]
fn spectrum_plus_runtime_resolves_disk_slot_query() {
    let runtime3 =
        SpectrumPlusRuntime::new(Model::SpectrumPlus3, SpectrumPlus::new(PlusModel::Plus3));
    assert_shared_query_paths_resolve(&runtime3);
    assert_boot_paths_wired(&runtime3);

    let provider = SpectrumSessionQueryProvider;
    let plus3_disk = provider
        .query(&runtime3, "spectrum.plus.disk_slot_supported")
        .expect("disk slot query should resolve")
        .expect("provider must own disk_slot_supported");
    assert_eq!(plus3_disk.value, serde_json::json!(true));

    let runtime2a =
        SpectrumPlusRuntime::new(Model::SpectrumPlus2A, SpectrumPlus::new(PlusModel::Plus2A));
    let plus2a_disk = provider
        .query(&runtime2a, "spectrum.plus.disk_slot_supported")
        .expect("disk slot query should resolve")
        .expect("provider must own disk_slot_supported");
    assert_eq!(plus2a_disk.value, serde_json::json!(false));
}

#[test]
fn pentagon_128_runtime_exposes_kempston_state_query() {
    let runtime = Pentagon128Runtime::new(Model::Pentagon128, Pentagon128::new());
    assert_shared_query_paths_resolve(&runtime);
    assert_boot_paths_wired(&runtime);

    let provider = SpectrumSessionQueryProvider;
    let kempston = provider
        .query(&runtime, "spectrum.kempston.state")
        .expect("kempston query should resolve")
        .expect("provider must own kempston.state");
    // Default kempston state is 0 (no buttons pressed).
    assert_eq!(kempston.value, serde_json::json!(0));
}

#[test]
fn scorpion_runtime_exposes_kempston_state_query() {
    let runtime = ScorpionZS256Runtime::new(Model::ScorpionZS256, ScorpionZS256::new());
    assert_shared_query_paths_resolve(&runtime);
    assert_boot_paths_wired(&runtime);

    let provider = SpectrumSessionQueryProvider;
    let kempston = provider
        .query(&runtime, "spectrum.kempston.state")
        .expect("kempston query should resolve")
        .expect("provider must own kempston.state");
    assert_eq!(kempston.value, serde_json::json!(0));
}

#[test]
fn timex_tc2048_runtime_exposes_kempston_state_query() {
    run_with_large_stack(|| {
        let runtime = TimexTC2048Runtime::new(Model::TimexTC2048, TimexTC2048::new());
        assert_shared_query_paths_resolve(&runtime);
        assert_boot_paths_wired(&runtime);

        let provider = SpectrumSessionQueryProvider;
        let kempston = provider
            .query(&runtime, "spectrum.kempston.state")
            .expect("kempston query should resolve")
            .expect("provider must own kempston.state");
        assert_eq!(kempston.value, serde_json::json!(0));
    });
}

#[test]
fn timex_ts2068_runtime_exposes_model_query_alongside_kempston() {
    run_with_large_stack(|| {
        let runtime =
            TimexTS2068Runtime::new(Model::TimexTS2068, TimexTS2068::new(TimexModel::TS2068));
        assert_shared_query_paths_resolve(&runtime);
        assert_boot_paths_wired(&runtime);

        let provider = SpectrumSessionQueryProvider;
        let model = provider
            .query(&runtime, "spectrum.timex.model")
            .expect("timex model query should resolve")
            .expect("provider must own timex.model");
        assert_eq!(model.value, serde_json::json!("ts2068"));

        let runtime_tc =
            TimexTS2068Runtime::new(Model::TimexTC2068, TimexTS2068::new(TimexModel::TC2068));
        let model_tc = provider
            .query(&runtime_tc, "spectrum.timex.model")
            .expect("timex model query should resolve")
            .expect("provider must own timex.model");
        assert_eq!(model_tc.value, serde_json::json!("tc2068"));
    });
}

/// Exercise the `spectrum.ay.*` query surface on every AY-equipped
/// variant. The protocol is identical across the family — write a
/// distinct register pattern through the chip-crate API, then read
/// the selected-register pointer and the full 16-byte register file
/// back via the runtime query provider. Reuses
/// `runtime.machine_mut().ay` because each machine exposes its AY
/// chip publicly.
fn assert_ay_query_round_trip<M>(runtime: &mut SpectrumRuntime<M>)
where
    M: SpectrumMachine + HasAy,
{
    // Drive the AY directly through the chip-crate API.
    let ay = HasAy::ay_mut(runtime.machine_mut());
    ay.select_register(0);
    ay.write_data(0xAB); // fine tone A
    ay.select_register(1);
    ay.write_data(0xFF); // coarse tone A — clipped to 0x0F
    ay.select_register(7);
    ay.write_data(0x3E); // mixer
    ay.select_register(13);
    ay.write_data(0x09); // envelope shape

    let provider = SpectrumSessionQueryProvider;

    let selected = provider
        .query(runtime, "spectrum.ay.selected_register")
        .expect("AY register query should resolve")
        .expect("provider must own ay.selected_register");
    assert_eq!(selected.value, serde_json::json!(13));

    let regs = provider
        .query(runtime, "spectrum.ay.registers")
        .expect("AY register-file query should resolve")
        .expect("provider must own ay.registers");
    let arr = regs.value.as_array().expect("registers value is an array");
    assert_eq!(arr.len(), 16);
    assert_eq!(arr[0], serde_json::json!(0xAB));
    assert_eq!(arr[1], serde_json::json!(0x0F));
    assert_eq!(arr[7], serde_json::json!(0x3E));
    assert_eq!(arr[13], serde_json::json!(0x09));
    assert_eq!(arr[2], serde_json::json!(0x00));
}

/// Bridge trait so a single generic test covers every AY-equipped
/// variant without each machine needing identical inherent helpers.
trait HasAy {
    fn ay_mut(&mut self) -> &mut gi_ay_3_8912::Ay3_8912;
}

impl HasAy for Spectrum128K {
    fn ay_mut(&mut self) -> &mut gi_ay_3_8912::Ay3_8912 {
        &mut self.ay
    }
}

impl HasAy for SpectrumPlus {
    fn ay_mut(&mut self) -> &mut gi_ay_3_8912::Ay3_8912 {
        &mut self.ay
    }
}

impl HasAy for Pentagon128 {
    fn ay_mut(&mut self) -> &mut gi_ay_3_8912::Ay3_8912 {
        &mut self.ay
    }
}

impl HasAy for ScorpionZS256 {
    fn ay_mut(&mut self) -> &mut gi_ay_3_8912::Ay3_8912 {
        &mut self.ay
    }
}

impl HasAy for TimexTS2068 {
    fn ay_mut(&mut self) -> &mut gi_ay_3_8912::Ay3_8912 {
        &mut self.ay
    }
}

#[test]
fn spectrum_128k_runtime_exposes_ay_register_queries() {
    let mut runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
    assert_ay_query_round_trip(&mut runtime);
}

#[test]
fn spectrum_plus2a_runtime_exposes_ay_register_queries() {
    let mut runtime =
        SpectrumPlusRuntime::new(Model::SpectrumPlus2A, SpectrumPlus::new(PlusModel::Plus2A));
    assert_ay_query_round_trip(&mut runtime);
}

#[test]
fn pentagon_128_runtime_exposes_ay_register_queries() {
    let mut runtime = Pentagon128Runtime::new(Model::Pentagon128, Pentagon128::new());
    assert_ay_query_round_trip(&mut runtime);
}

#[test]
fn scorpion_runtime_exposes_ay_register_queries() {
    let mut runtime = ScorpionZS256Runtime::new(Model::ScorpionZS256, ScorpionZS256::new());
    assert_ay_query_round_trip(&mut runtime);
}

#[test]
fn timex_ts2068_runtime_exposes_ay_register_queries() {
    run_with_large_stack(|| {
        let mut runtime =
            TimexTS2068Runtime::new(Model::TimexTS2068, TimexTS2068::new(TimexModel::TS2068));
        assert_ay_query_round_trip(&mut runtime);
    });
}

/// Variants without an AY chip (48K and TC2048) MUST NOT advertise
/// the `spectrum.ay.*` paths and MUST return Ok(None) when asked.
#[test]
fn non_ay_variants_do_not_advertise_ay_paths() {
    let provider = SpectrumSessionQueryProvider;

    let runtime48 = Spectrum48kRuntime::new(Model::Spectrum48KPal, Spectrum48k::new());
    let paths_48 = provider.query_paths(&runtime48, Some("spectrum.ay."));
    assert!(
        paths_48.is_empty(),
        "48K should not advertise spectrum.ay.* paths, got {paths_48:?}"
    );
    assert!(
        provider
            .query(&runtime48, "spectrum.ay.selected_register")
            .expect("query call must not error")
            .is_none(),
        "48K must not own spectrum.ay.selected_register"
    );

    run_with_large_stack(move || {
        let runtime_tc = TimexTC2048Runtime::new(Model::TimexTC2048, TimexTC2048::new());
        let paths_tc = provider.query_paths(&runtime_tc, Some("spectrum.ay."));
        assert!(
            paths_tc.is_empty(),
            "TC2048 should not advertise spectrum.ay.* paths, got {paths_tc:?}"
        );
        assert!(
            provider
                .query(&runtime_tc, "spectrum.ay.registers")
                .expect("query call must not error")
                .is_none(),
            "TC2048 must not own spectrum.ay.registers"
        );
    });
}

#[test]
fn variant_query_paths_include_boot_paths() {
    // Every variant exposes boot.detected / boot.reason / boot.row,
    // even those still TODO-stubbed against banner detection.
    let pentagon = Pentagon128Runtime::new(Model::Pentagon128, Pentagon128::new());
    let provider = SpectrumSessionQueryProvider;
    let paths = provider.query_paths(&pentagon, Some("boot."));
    assert_eq!(
        paths,
        vec![
            "boot.detected".to_owned(),
            "boot.reason".to_owned(),
            "boot.row".to_owned(),
        ]
    );
}

#[test]
fn scorpion_boot_status_returns_not_detected_until_banner_confirmed() {
    // Scorpion ROM boots into TR-DOS with no disk inserted — screen
    // stays blank, no banner to detect. Provider returns
    // `detected = false` cleanly.
    let runtime = ScorpionZS256Runtime::new(Model::ScorpionZS256, ScorpionZS256::new());
    let provider = SpectrumSessionQueryProvider;
    let detected = provider
        .query(&runtime, "boot.detected")
        .expect("boot.detected should resolve")
        .expect("provider must own boot.detected");
    let reason = provider
        .query(&runtime, "boot.reason")
        .expect("boot.reason should resolve")
        .expect("provider must own boot.reason");
    assert_eq!(detected.value, serde_json::json!(false));
    assert_eq!(
        reason.value,
        serde_json::json!("copyright banner not visible")
    );
}

// ---- ROM-backed banner regression + diagnostic ----
//
// The TC2048 banner is confirmed and asserted as a regression. The
// other six variants are diagnostic-only — running
// `probe_all_variant_banners` with `--ignored --nocapture` prints
// each variant's boot screen so future contributors can investigate
// what's blocking banner detection for the paged-ROM variants
// (128K family) and graphic-splash variants (Pentagon, Scorpion,
// TS2068). See `BLOCKED 2026-05-01` notes in `src/variants.rs` for
// the per-variant rationale.

fn rom_dir(suffix: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(suffix))
}

fn print_screen(label: &str, lines: &[String]) {
    eprintln!("\n=== {label} ===");
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_end();
        if !trimmed.is_empty() {
            eprintln!("  row {i:2}: {line:?}");
        }
    }
}

fn run_frames<M: SpectrumMachine>(rt: &mut SpectrumRuntime<M>, n: u32) {
    use emu198x_shell::NullAudioSink;
    let mut frame = RecordingFrameSink::default();
    let mut audio = NullAudioSink;
    let mut trace = NullTraceSink;
    let hc_per_frame = u64::from(rt.machine().frame_halfcycles());
    let target_hc = hc_per_frame * u64::from(n);
    rt.run_until(
        MachineTime::new(target_hc),
        &mut HostIo {
            input_events: &[],
            frame_sink: &mut frame,
            audio_sink: &mut audio,
            trace_sink: &mut trace,
        },
    )
    .expect("variant runtime should run");
}

fn screen_lines<M: SpectrumMachine>(rt: &SpectrumRuntime<M>) -> Vec<String> {
    let provider = SpectrumSessionQueryProvider;
    let result = provider
        .query(rt, "screen.text.lines")
        .expect("screen.text.lines should not error")
        .expect("screen.text.lines should resolve");
    let arr = result.value.as_array().expect("array");
    arr.iter()
        .map(|v| v.as_str().expect("string").to_owned())
        .collect()
}

/// Regression for the TC2048 banner confirmed in
/// `src/variants.rs::TIMEX_TC2048_BANNERS`. Boots the local ROM and
/// asserts the boot status matches.
#[test]
#[ignore = "requires local Timex TC2048 ROM at ~/.emu198x/roms/timex-tc2048/tc2048.rom"]
fn tc2048_boot_banner_is_detected_with_real_rom() {
    let dir = rom_dir(".emu198x/roms/timex-tc2048").expect("HOME set");
    let path = dir.join("tc2048.rom");
    if !path.exists() {
        eprintln!("TC2048 ROM not at {} — skipping", path.display());
        return;
    }
    let rom = std::fs::read(&path).expect("read TC2048 ROM");
    let mut m = TimexTC2048::new();
    m.memory.load_rom_data(&rom);
    let mut rt = TimexTC2048Runtime::new(Model::TimexTC2048, m);
    run_frames(&mut rt, 200);

    let provider = SpectrumSessionQueryProvider;
    let detected = provider
        .query(&rt, "boot.detected")
        .expect("boot.detected resolves")
        .expect("provider owns boot.detected");
    let reason = provider
        .query(&rt, "boot.reason")
        .expect("boot.reason resolves")
        .expect("provider owns boot.reason");
    assert_eq!(detected.value, serde_json::json!(true));
    let reason_str = reason.value.as_str().expect("reason is string");
    assert!(
        reason_str.contains("copyright banner") && reason_str.contains("row"),
        "TC2048 boot.reason should announce a copyright-banner row hit; got {reason_str:?}",
    );

    // The decoded screen should literally contain the verified
    // Sinclair banner string so the regression catches a reader-side
    // change too.
    let lines = screen_lines(&rt);
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Sinclair Research Ltd")),
        "TC2048 screen.text.lines should contain 'Sinclair Research Ltd'; got {lines:?}",
    );
}

/// Regression for the 128K banner confirmed in `src/variants.rs::
/// SPECTRUM_128K_BANNERS`. Boots the local ROMs and asserts the
/// boot status matches the verified Sinclair 1986 banner.
#[test]
#[ignore = "requires local Spectrum 128K ROMs at ~/.emu198x/roms/sinclair-zx-spectrum-128k/128-{0,1}.rom"]
fn spectrum_128k_boot_banner_is_detected_with_real_rom() {
    let dir = rom_dir(".emu198x/roms/sinclair-zx-spectrum-128k").expect("HOME set");
    let rom0 = std::fs::read(dir.join("128-0.rom"));
    let rom1 = std::fs::read(dir.join("128-1.rom"));
    let (Ok(rom0), Ok(rom1)) = (rom0, rom1) else {
        eprintln!("128K ROMs missing — skipping");
        return;
    };
    let mut m = Spectrum128K::new();
    m.memory.load_roms(&rom0, &rom1);
    let mut rt = Spectrum128kRuntime::new(Model::Spectrum128KPal, m);
    run_frames(&mut rt, 200);

    let provider = SpectrumSessionQueryProvider;
    let detected = provider
        .query(&rt, "boot.detected")
        .expect("boot.detected resolves")
        .expect("provider owns boot.detected");
    assert_eq!(detected.value, serde_json::json!(true));
    let lines = screen_lines(&rt);
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Sinclair Research Ltd")),
        "128K screen.text.lines should contain 'Sinclair Research Ltd'; got {lines:?}",
    );
}

/// Regression for the +3 banner confirmed in `src/variants.rs::
/// SPECTRUM_PLUS_BANNERS`. Boots the +3 (Plus3 model) from the local
/// 4-ROM set and asserts the Amstrad 1982/1986/1987 banner is
/// detected. The Plus2A and Plus2B share the same row-22 banner —
/// covered by the same constant.
#[test]
#[ignore = "requires local +3 ROMs at ~/.emu198x/roms/amstrad-zx-spectrum-plus3/plus3-{0..3}.rom"]
fn spectrum_plus3_boot_banner_is_detected_with_real_rom() {
    let dir = rom_dir(".emu198x/roms/amstrad-zx-spectrum-plus3").expect("HOME set");
    let r0 = std::fs::read(dir.join("plus3-0.rom"));
    let r1 = std::fs::read(dir.join("plus3-1.rom"));
    let r2 = std::fs::read(dir.join("plus3-2.rom"));
    let r3 = std::fs::read(dir.join("plus3-3.rom"));
    let (Ok(r0), Ok(r1), Ok(r2), Ok(r3)) = (r0, r1, r2, r3) else {
        eprintln!("+3 ROMs missing — skipping");
        return;
    };
    let mut m = SpectrumPlus::new(PlusModel::Plus3);
    m.memory.load_roms(&r0, &r1, &r2, &r3);
    let mut rt = SpectrumPlusRuntime::new(Model::SpectrumPlus3, m);
    run_frames(&mut rt, 250);

    let provider = SpectrumSessionQueryProvider;
    let detected = provider
        .query(&rt, "boot.detected")
        .expect("boot.detected resolves")
        .expect("provider owns boot.detected");
    assert_eq!(detected.value, serde_json::json!(true));
    let lines = screen_lines(&rt);
    assert!(
        lines.iter().any(|line| line.contains("Amstrad Plc")),
        "+3 screen.text.lines should contain 'Amstrad Plc'; got {lines:?}",
    );
}

/// Regression for the Pentagon 128 banner confirmed in
/// `src/variants.rs::PENTAGON_128_BANNERS`. Boots the local Pentagon
/// ROMs and asserts the 1993 Sinclair banner is detected.
#[test]
#[ignore = "requires local Pentagon ROMs at ~/.emu198x/roms/pentagon-128/pentagon-{0,1}.rom"]
fn pentagon_128_boot_banner_is_detected_with_real_rom() {
    let dir = rom_dir(".emu198x/roms/pentagon-128").expect("HOME set");
    let r0 = std::fs::read(dir.join("pentagon-0.rom"));
    let r1 = std::fs::read(dir.join("pentagon-1.rom"));
    let (Ok(r0), Ok(r1)) = (r0, r1) else {
        eprintln!("Pentagon ROMs missing — skipping");
        return;
    };
    let mut m = Pentagon128::new();
    m.memory.load_roms(&r0, &r1);
    let mut rt = Pentagon128Runtime::new(Model::Pentagon128, m);
    run_frames(&mut rt, 200);

    let provider = SpectrumSessionQueryProvider;
    let detected = provider
        .query(&rt, "boot.detected")
        .expect("boot.detected resolves")
        .expect("provider owns boot.detected");
    assert_eq!(detected.value, serde_json::json!(true));
    let lines = screen_lines(&rt);
    assert!(
        lines
            .iter()
            .any(|line| line.contains("1993 Sinclair Research Ltd")),
        "Pentagon screen.text.lines should contain '1993 Sinclair Research Ltd'; got {lines:?}",
    );
}

/// Probe the raw screen RAM after Scorpion boot. The screen-text
/// decoder shows a uniformly empty screen, but the raw bitmap bytes
/// at $4000-$57FF will tell us whether the Service ROM is painting
/// anything at all. If non-zero bytes show up, the issue is decoder
/// scope (we're missing a screen mode or screen address). If
/// everything is zero, the issue is upstream — Service ROM never
/// writes to screen.
#[test]
#[ignore = "diagnostic — Scorpion screen-RAM dump (pixel bitmap + attributes)"]
fn probe_scorpion_screen_ram() {
    let dir = rom_dir(".emu198x/roms/scorpion-zs256").expect("HOME set");
    let r0 = std::fs::read(dir.join("scorpion-0.rom"));
    let r1 = std::fs::read(dir.join("scorpion-1.rom"));
    let r2 = std::fs::read(dir.join("scorpion-2.rom"));
    let r3 = std::fs::read(dir.join("scorpion-3.rom"));
    let (Ok(r0), Ok(r1), Ok(r2), Ok(r3)) = (r0, r1, r2, r3) else {
        eprintln!("Scorpion ROMs missing — skipping");
        return;
    };
    let mut m = ScorpionZS256::new();
    m.memory.load_roms(&r0, &r1, &r2, &r3);
    let mut rt = ScorpionZS256Runtime::new(Model::ScorpionZS256, m);

    let mut total_frames = 0u32;
    for &target_total in &[50u32, 200, 500, 1000, 2000] {
        let delta = target_total - total_frames;
        run_frames(&mut rt, delta);
        total_frames = target_total;
        let nonzero_pixels = (0x4000u16..0x5800)
            .filter(|&a| rt.machine().read_byte(a) != 0)
            .count();
        let nonzero_attrs = (0x5800u16..0x5B00)
            .filter(|&a| rt.machine().read_byte(a) != 0)
            .count();
        eprintln!(
            "Scorpion @ {target_total} frames: pixel-nonzero={nonzero_pixels} / 6144,  attr-nonzero={nonzero_attrs} / 768"
        );
    }

    // Sample some bytes from a few specific addresses that text
    // would touch (top of screen, middle, near the bottom).
    eprintln!("\nSample bytes at common text positions (visible at $4000-$5AFF):");
    for addr in [
        0x4000u16, 0x4020, 0x4400, 0x4800, 0x5000, 0x5800, 0x5820, 0x5A00,
    ] {
        eprintln!("  ${addr:04X} = ${:02X}", rt.machine().read_byte(addr));
    }

    // Where is the CPU actually stuck?
    let regs = &rt.machine().z80.regs;
    eprintln!("\nCPU state after 2000 frames:");
    eprintln!("  PC=${:04X}  IFF1={}  IM={}", regs.pc, regs.iff1, regs.im);
    eprintln!("  TR-DOS paged: {}", rt.machine().beta.trdos_paged);
}

/// Probe the raw screen RAM after TS2068 boot. Standard Spectrum
/// screen is $4000-$57FF (6144 bytes pixels) + $5800-$5AFF (768
/// attributes). Timex high-res mode uses $4000-$57FF *and*
/// $6000-$77FF as two interleaved bitmap planes (8192 pixels each).
/// Dump raw byte counts in each region to see which layout is in
/// play.
#[test]
#[ignore = "diagnostic — TS2068 screen-RAM dump (standard + high-res addresses)"]
fn probe_ts2068_screen_ram() {
    let dir = rom_dir(".emu198x/roms/timex-ts2068").expect("HOME set");
    let main_path = dir.join("ts2068.rom");
    let exrom_path = dir.join("exrom.rom");
    if !main_path.exists() || !exrom_path.exists() {
        eprintln!("TS2068 ROMs missing — skipping");
        return;
    }
    let mut m = TimexTS2068::new(TimexModel::TS2068);
    m.memory.load_rom(&main_path).expect("ts2068 main ROM");
    m.memory.load_exrom(&exrom_path).expect("ts2068 exrom");
    let mut rt = TimexTS2068Runtime::new(Model::TimexTS2068, m);
    run_frames(&mut rt, 200);

    let standard_pixels = (0x4000u16..0x5800)
        .filter(|&a| rt.machine().read_byte(a) != 0)
        .count();
    let standard_attrs = (0x5800u16..0x5B00)
        .filter(|&a| rt.machine().read_byte(a) != 0)
        .count();
    let hires_secondary_pixels = (0x6000u16..0x7800)
        .filter(|&a| rt.machine().read_byte(a) != 0)
        .count();
    let hires_secondary_attrs = (0x7800u16..0x7B00)
        .filter(|&a| rt.machine().read_byte(a) != 0)
        .count();
    eprintln!("TS2068 @ 200 frames:");
    eprintln!("  standard $4000-$57FF pixels nonzero: {standard_pixels} / 6144");
    eprintln!("  standard $5800-$5AFF attrs  nonzero: {standard_attrs} / 768");
    eprintln!("  hires    $6000-$77FF pixels nonzero: {hires_secondary_pixels} / 6144");
    eprintln!("  hires    $7800-$7AFF attrs  nonzero: {hires_secondary_attrs} / 768");

    eprintln!("\nFirst few bytes of standard screen RAM:");
    for addr in [0x4000u16, 0x4020, 0x4040, 0x4400, 0x4800, 0x5000] {
        let b = rt.machine().read_byte(addr);
        eprintln!("  ${addr:04X} = ${b:02X} ({b:08b})");
    }

    eprintln!("\nLast row of pixel data (row 23 = $50E0..=$50FF):");
    for col in 0..32 {
        let b = rt.machine().read_byte(0x50E0 + col);
        eprint!("{b:02X} ");
    }
    eprintln!();
}

#[test]
#[ignore = "diagnostic — boots six variants from ~/.emu198x/roms and prints banners"]
fn probe_all_variant_banners() {
    if let Some(dir) = rom_dir(".emu198x/roms/sinclair-zx-spectrum-128k") {
        let r0 = std::fs::read(dir.join("128-0.rom"));
        let r1 = std::fs::read(dir.join("128-1.rom"));
        if let (Ok(rom0), Ok(rom1)) = (r0, r1) {
            let mut m = Spectrum128K::new();
            m.memory.load_roms(&rom0, &rom1);
            let mut rt = Spectrum128kRuntime::new(Model::Spectrum128KPal, m);
            run_frames(&mut rt, 200);
            print_screen("Spectrum 128K (200 frames)", &screen_lines(&rt));
        } else {
            eprintln!("128K ROMs missing");
        }
    }

    if let Some(dir) = rom_dir(".emu198x/roms/amstrad-zx-spectrum-plus2") {
        let r0 = std::fs::read(dir.join("plus2-0.rom"));
        let r1 = std::fs::read(dir.join("plus2-1.rom"));
        if let (Ok(rom0), Ok(rom1)) = (r0, r1) {
            // The grey +2 used 128K-style 2 ROMs. Boot it through the
            // Spectrum128K machine.
            let mut m = Spectrum128K::new();
            m.memory.load_roms(&rom0, &rom1);
            let mut rt = Spectrum128kRuntime::new(Model::Spectrum128KPal, m);
            run_frames(&mut rt, 200);
            print_screen(
                "Spectrum +2 grey (via 128K machine, 200 frames)",
                &screen_lines(&rt),
            );
        } else {
            eprintln!("+2 ROMs missing");
        }
    }

    if let Some(dir) = rom_dir(".emu198x/roms/amstrad-zx-spectrum-plus3") {
        let r0 = std::fs::read(dir.join("plus3-0.rom"));
        let r1 = std::fs::read(dir.join("plus3-1.rom"));
        let r2 = std::fs::read(dir.join("plus3-2.rom"));
        let r3 = std::fs::read(dir.join("plus3-3.rom"));
        if let (Ok(rom0), Ok(rom1), Ok(rom2), Ok(rom3)) = (r0, r1, r2, r3) {
            for model in [PlusModel::Plus2A, PlusModel::Plus2B, PlusModel::Plus3] {
                let mut m = SpectrumPlus::new(model);
                m.memory.load_roms(&rom0, &rom1, &rom2, &rom3);
                let runtime_model = match model {
                    PlusModel::Plus2A => Model::SpectrumPlus2A,
                    PlusModel::Plus2B => Model::SpectrumPlus2B,
                    PlusModel::Plus3 => Model::SpectrumPlus3,
                };
                let mut rt = SpectrumPlusRuntime::new(runtime_model, m);
                run_frames(&mut rt, 250);
                print_screen(
                    &format!("Spectrum {model:?} (250 frames)"),
                    &screen_lines(&rt),
                );
            }
        } else {
            eprintln!("+3 ROMs missing");
        }
    }

    if let Some(dir) = rom_dir(".emu198x/roms/pentagon-128") {
        let r0 = std::fs::read(dir.join("pentagon-0.rom"));
        let r1 = std::fs::read(dir.join("pentagon-1.rom"));
        if let (Ok(rom0), Ok(rom1)) = (r0, r1) {
            let mut m = Pentagon128::new();
            m.memory.load_roms(&rom0, &rom1);
            let mut rt = Pentagon128Runtime::new(Model::Pentagon128, m);
            run_frames(&mut rt, 200);
            print_screen("Pentagon 128 (200 frames)", &screen_lines(&rt));
        } else {
            eprintln!("Pentagon ROMs missing");
        }
    }

    if let Some(dir) = rom_dir(".emu198x/roms/scorpion-zs256") {
        let r0 = std::fs::read(dir.join("scorpion-0.rom"));
        let r1 = std::fs::read(dir.join("scorpion-1.rom"));
        let r2 = std::fs::read(dir.join("scorpion-2.rom"));
        let r3 = std::fs::read(dir.join("scorpion-3.rom"));
        if let (Ok(rom0), Ok(rom1), Ok(rom2), Ok(rom3)) = (r0, r1, r2, r3) {
            let mut m = ScorpionZS256::new();
            m.memory.load_roms(&rom0, &rom1, &rom2, &rom3);
            let mut rt = ScorpionZS256Runtime::new(Model::ScorpionZS256, m);
            run_frames(&mut rt, 500);
            print_screen("Scorpion ZS-256 (500 frames)", &screen_lines(&rt));
        } else {
            eprintln!("Scorpion ROMs missing");
        }
    }

    if let Some(dir) = rom_dir(".emu198x/roms/timex-tc2048") {
        if let Ok(rom) = std::fs::read(dir.join("tc2048.rom")) {
            let mut m = TimexTC2048::new();
            m.memory.load_rom_data(&rom);
            let mut rt = TimexTC2048Runtime::new(Model::TimexTC2048, m);
            run_frames(&mut rt, 200);
            print_screen("Timex TC2048 (200 frames)", &screen_lines(&rt));
        } else {
            eprintln!("TC2048 ROM missing");
        }
    }

    if let Some(dir) = rom_dir(".emu198x/roms/timex-ts2068") {
        let main_path = dir.join("ts2068.rom");
        let exrom_path = dir.join("exrom.rom");
        if main_path.exists() && exrom_path.exists() {
            let mut m = TimexTS2068::new(TimexModel::TS2068);
            m.memory.load_rom(&main_path).expect("ts2068 main ROM");
            m.memory.load_exrom(&exrom_path).expect("ts2068 exrom");
            let mut rt = TimexTS2068Runtime::new(Model::TimexTS2068, m);
            run_frames(&mut rt, 200);
            print_screen("Timex TS2068 (200 frames)", &screen_lines(&rt));
        } else {
            eprintln!("TS2068 ROMs missing");
        }
    }
}
