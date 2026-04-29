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
    TIMING_SCORPION,
};
use emu198x_shell::{
    AudioPacket, AudioSink, FramePacket, FrameSink, HostIo, InputEvent, MachineCore, MachineError,
    MachineTime, MediaImage, MediaKind, MediaSet, NullTraceSink, PixelFormat,
};
use machine_pentagon_128::Pentagon128;
use machine_scorpion_zs256::ScorpionZS256;
use machine_sinclair_zx_spectrum_128k::Spectrum128K;
use machine_sinclair_zx_spectrum_plus::{Model as PlusModel, SpectrumPlus};
use machine_timex_tc2048::TimexTC2048;
use machine_timex_ts2068::{TIMING_TS2068, TimexModel, TimexTS2068};
use runtime_sinclair_zx_spectrum::{
    Model, Pentagon128Runtime, ScorpionZS256Runtime, Spectrum128kRuntime, SpectrumMachine,
    SpectrumPlusRuntime, SpectrumRuntime, TimexTC2048Runtime, TimexTS2068Runtime,
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
    let runtime =
        TimexTS2068Runtime::new(Model::TimexTS2068, TimexTS2068::new(TimexModel::TS2068));
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
