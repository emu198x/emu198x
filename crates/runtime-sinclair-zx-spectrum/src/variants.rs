//! `SpectrumMachine` impls + type aliases for each non-48K variant.
//!
//! The 48K machine has its own bespoke runtime (`Spectrum48kRuntime`)
//! because it carries the rich session query provider. Everything else
//! in the family plugs into the generic `SpectrumRuntime<M>`.

use common_sinclair_zx_spectrum::tape::{TapeBlock, TapeSpan};
use common_sinclair_zx_spectrum::timing::{
    SCREEN_HEIGHT, SCREEN_WIDTH, SCREEN_WIDTH_HIRES, TIMING_128K, TIMING_48K, TIMING_PENTAGON,
    TIMING_PLUS2A, TIMING_SCORPION,
};
use machine_pentagon_128::Pentagon128;
use machine_scorpion_zs256::ScorpionZS256;
use machine_sinclair_zx_spectrum_128k::Spectrum128K;
use machine_sinclair_zx_spectrum_plus::SpectrumPlus;
use machine_timex_tc2048::TimexTC2048;
use machine_timex_ts2068::{TIMING_TS2068, TimexModel, TimexTS2068};

use crate::spectrum_runtime::{SpectrumMachine, SpectrumRuntime};

/// ZX Spectrum 128K / +2 runtime.
pub type Spectrum128kRuntime = SpectrumRuntime<Spectrum128K>;

/// ZX Spectrum +2A / +2B / +3 runtime.
pub type SpectrumPlusRuntime = SpectrumRuntime<SpectrumPlus>;

/// Pentagon 128 runtime.
pub type Pentagon128Runtime = SpectrumRuntime<Pentagon128>;

/// Scorpion ZS-256 runtime.
pub type ScorpionZS256Runtime = SpectrumRuntime<ScorpionZS256>;

/// Timex TC2048 runtime.
pub type TimexTC2048Runtime = SpectrumRuntime<TimexTC2048>;

/// Timex TC2068 / TS2068 runtime.
pub type TimexTS2068Runtime = SpectrumRuntime<TimexTS2068>;

impl SpectrumMachine for Spectrum128K {
    const FRAME_WIDTH: u32 = SCREEN_WIDTH as u32;
    const FRAME_HEIGHT: u32 = SCREEN_HEIGHT as u32;

    fn frame_halfcycles(&self) -> u32 {
        TIMING_128K.halfcycles_per_frame
    }
    fn run_frame(&mut self) {
        Spectrum128K::run_frame(self);
    }
    fn framebuffer(&self) -> &[u8] {
        &self.framebuffer
    }
    fn audio_frame(&self) -> &[f32] {
        &self.audio_frame
    }
    fn set_keyboard_rows(&mut self, rows: &[u8; 8]) {
        self.keyboard = *rows;
    }
    fn load_tape_blocks(&mut self, blocks: Vec<TapeBlock>) {
        Spectrum128K::load_tape_blocks(self, blocks);
    }
    fn load_tape_stream(&mut self, stream: Vec<TapeSpan>) {
        Spectrum128K::load_tape_stream(self, stream);
    }
    fn tape_play(&mut self) {
        Spectrum128K::tape_play(self);
    }
    fn tape_stop(&mut self) {
        Spectrum128K::tape_stop(self);
    }
    fn reset_machine(&mut self) {
        Spectrum128K::reset(self);
    }
}

impl SpectrumMachine for SpectrumPlus {
    const FRAME_WIDTH: u32 = SCREEN_WIDTH as u32;
    const FRAME_HEIGHT: u32 = SCREEN_HEIGHT as u32;

    fn frame_halfcycles(&self) -> u32 {
        TIMING_PLUS2A.halfcycles_per_frame
    }
    fn run_frame(&mut self) {
        SpectrumPlus::run_frame(self);
    }
    fn framebuffer(&self) -> &[u8] {
        &self.framebuffer
    }
    fn audio_frame(&self) -> &[f32] {
        &self.audio_frame
    }
    fn set_keyboard_rows(&mut self, rows: &[u8; 8]) {
        self.keyboard = *rows;
    }
    fn load_tape_blocks(&mut self, blocks: Vec<TapeBlock>) {
        SpectrumPlus::load_tape_blocks(self, blocks);
    }
    fn load_tape_stream(&mut self, stream: Vec<TapeSpan>) {
        SpectrumPlus::load_tape_stream(self, stream);
    }
    fn tape_play(&mut self) {
        SpectrumPlus::tape_play(self);
    }
    fn tape_stop(&mut self) {
        SpectrumPlus::tape_stop(self);
    }
    fn reset_machine(&mut self) {
        SpectrumPlus::reset(self);
    }
}

impl SpectrumMachine for Pentagon128 {
    const FRAME_WIDTH: u32 = SCREEN_WIDTH as u32;
    const FRAME_HEIGHT: u32 = SCREEN_HEIGHT as u32;

    fn frame_halfcycles(&self) -> u32 {
        TIMING_PENTAGON.halfcycles_per_frame
    }
    fn run_frame(&mut self) {
        Pentagon128::run_frame(self);
    }
    fn framebuffer(&self) -> &[u8] {
        &self.framebuffer
    }
    fn audio_frame(&self) -> &[f32] {
        &self.audio_frame
    }
    fn set_keyboard_rows(&mut self, rows: &[u8; 8]) {
        self.keyboard = *rows;
    }
    fn load_tape_blocks(&mut self, blocks: Vec<TapeBlock>) {
        Pentagon128::load_tape_blocks(self, blocks);
    }
    fn load_tape_stream(&mut self, stream: Vec<TapeSpan>) {
        Pentagon128::load_tape_stream(self, stream);
    }
    fn tape_play(&mut self) {
        Pentagon128::tape_play(self);
    }
    fn tape_stop(&mut self) {
        Pentagon128::tape_stop(self);
    }
    fn reset_machine(&mut self) {
        Pentagon128::reset(self);
    }
}

impl SpectrumMachine for ScorpionZS256 {
    const FRAME_WIDTH: u32 = SCREEN_WIDTH as u32;
    const FRAME_HEIGHT: u32 = SCREEN_HEIGHT as u32;

    fn frame_halfcycles(&self) -> u32 {
        TIMING_SCORPION.halfcycles_per_frame
    }
    fn run_frame(&mut self) {
        ScorpionZS256::run_frame(self);
    }
    fn framebuffer(&self) -> &[u8] {
        &self.framebuffer
    }
    fn audio_frame(&self) -> &[f32] {
        &self.audio_frame
    }
    fn set_keyboard_rows(&mut self, rows: &[u8; 8]) {
        self.keyboard = *rows;
    }
    fn load_tape_blocks(&mut self, blocks: Vec<TapeBlock>) {
        ScorpionZS256::load_tape_blocks(self, blocks);
    }
    fn load_tape_stream(&mut self, stream: Vec<TapeSpan>) {
        ScorpionZS256::load_tape_stream(self, stream);
    }
    fn tape_play(&mut self) {
        ScorpionZS256::tape_play(self);
    }
    fn tape_stop(&mut self) {
        ScorpionZS256::tape_stop(self);
    }
    fn reset_machine(&mut self) {
        ScorpionZS256::reset(self);
    }
}

impl SpectrumMachine for TimexTC2048 {
    const FRAME_WIDTH: u32 = SCREEN_WIDTH_HIRES as u32;
    const FRAME_HEIGHT: u32 = SCREEN_HEIGHT as u32;

    fn frame_halfcycles(&self) -> u32 {
        TIMING_48K.halfcycles_per_frame
    }
    fn run_frame(&mut self) {
        TimexTC2048::run_frame(self);
    }
    fn framebuffer(&self) -> &[u8] {
        &self.framebuffer
    }
    fn audio_frame(&self) -> &[f32] {
        &self.audio_frame
    }
    fn set_keyboard_rows(&mut self, rows: &[u8; 8]) {
        self.keyboard = *rows;
    }
    fn load_tape_blocks(&mut self, blocks: Vec<TapeBlock>) {
        TimexTC2048::load_tape_blocks(self, blocks);
    }
    fn load_tape_stream(&mut self, stream: Vec<TapeSpan>) {
        TimexTC2048::load_tape_stream(self, stream);
    }
    fn tape_play(&mut self) {
        TimexTC2048::tape_play(self);
    }
    fn tape_stop(&mut self) {
        TimexTC2048::tape_stop(self);
    }
    fn reset_machine(&mut self) {
        TimexTC2048::reset(self);
    }
}

impl SpectrumMachine for TimexTS2068 {
    const FRAME_WIDTH: u32 = SCREEN_WIDTH_HIRES as u32;
    const FRAME_HEIGHT: u32 = SCREEN_HEIGHT as u32;

    fn frame_halfcycles(&self) -> u32 {
        match self.model {
            TimexModel::TC2068 => TIMING_48K.halfcycles_per_frame,
            TimexModel::TS2068 => TIMING_TS2068.halfcycles_per_frame,
        }
    }
    fn run_frame(&mut self) {
        TimexTS2068::run_frame(self);
    }
    fn framebuffer(&self) -> &[u8] {
        &self.framebuffer
    }
    fn audio_frame(&self) -> &[f32] {
        &self.audio_frame
    }
    fn set_keyboard_rows(&mut self, rows: &[u8; 8]) {
        self.keyboard = *rows;
    }
    fn load_tape_blocks(&mut self, blocks: Vec<TapeBlock>) {
        TimexTS2068::load_tape_blocks(self, blocks);
    }
    fn load_tape_stream(&mut self, stream: Vec<TapeSpan>) {
        TimexTS2068::load_tape_stream(self, stream);
    }
    fn tape_play(&mut self) {
        TimexTS2068::tape_play(self);
    }
    fn tape_stop(&mut self) {
        TimexTS2068::tape_stop(self);
    }
    fn reset_machine(&mut self) {
        TimexTS2068::reset(self);
    }
    fn post_deserialize(&mut self) {
        self.restore_timing();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::{
        AudioPacket, AudioSink, FramePacket, FrameSink, HostIo, InputEvent, MachineCore,
        MachineError, MachineTime, NullTraceSink, PixelFormat,
    };

    use crate::Model;

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
        (frame_sink.frames, audio_sink.packets, frame_sink.last_dimensions)
    }

    #[test]
    fn spectrum_128k_runtime_emits_frame_and_audio() {
        let runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
        let (frames, audio, dims) =
            run_single_frame(runtime, TIMING_128K.halfcycles_per_frame);
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
        let mut runtime =
            Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
        run_single_frame_by_ref(&mut runtime, TIMING_128K.halfcycles_per_frame);
        let bytes = runtime.snapshot().expect("snapshot should encode");

        let mut restored =
            Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
        restored
            .restore(&bytes)
            .expect("snapshot should restore into a fresh runtime");

        let round_trip = restored.snapshot().expect("restored snapshot should encode");
        assert_eq!(round_trip, bytes);
    }

    #[test]
    fn scorpion_runtime_round_trips_through_snapshot() {
        // Scorpion has 16 RAM banks + 4 ROMs — the largest of the family
        // at ~320 KB inline. Verifies the heap-backed bank storage holds.
        let mut runtime =
            ScorpionZS256Runtime::new(Model::ScorpionZS256, ScorpionZS256::new());
        run_single_frame_by_ref(&mut runtime, TIMING_SCORPION.halfcycles_per_frame);
        let bytes = runtime.snapshot().expect("snapshot should encode");

        let mut restored =
            ScorpionZS256Runtime::new(Model::ScorpionZS256, ScorpionZS256::new());
        restored
            .restore(&bytes)
            .expect("snapshot should restore into a fresh Scorpion runtime");

        let round_trip = restored.snapshot().expect("restored snapshot should encode");
        assert_eq!(round_trip, bytes);
    }

    #[test]
    fn keyboard_input_updates_machine_matrix() {
        let mut runtime =
            Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
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
}
