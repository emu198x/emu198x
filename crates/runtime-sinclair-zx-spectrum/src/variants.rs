//! `SpectrumMachine` impls + type aliases for each non-48K variant.
//!
//! The 48K machine has its own bespoke runtime (`Spectrum48kRuntime`)
//! because it carries the rich session query provider. Everything else
//! in the family plugs into the generic `SpectrumRuntime<M>`.

use common_sinclair_zx_spectrum::tape::{TapeBlock, TapeSpan};
use common_sinclair_zx_spectrum::timing::{
    SCREEN_HEIGHT, SCREEN_WIDTH, SCREEN_WIDTH_HIRES, TIMING_48K, TIMING_128K, TIMING_PENTAGON,
    TIMING_PLUS2A, TIMING_SCORPION,
};
use machine_pentagon_128::Pentagon128;
use machine_scorpion_zs256::ScorpionZS256;
use machine_sinclair_zx_spectrum_48k::Spectrum48k;
use machine_sinclair_zx_spectrum_128k::Spectrum128K;
use machine_sinclair_zx_spectrum_plus::{Model as PlusModel, SpectrumPlus};
use machine_timex_tc2048::TimexTC2048;
use machine_timex_ts2068::{TIMING_TS2068, TimexModel, TimexTS2068};

use crate::runtime::{SpectrumMachine, SpectrumRuntime};

/// ZX Spectrum 48K runtime.
pub type Spectrum48kRuntime = SpectrumRuntime<Spectrum48k>;

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

impl SpectrumMachine for Spectrum48k {
    const FRAME_WIDTH: u32 = SCREEN_WIDTH as u32;
    const FRAME_HEIGHT: u32 = SCREEN_HEIGHT as u32;

    fn frame_halfcycles(&self) -> u32 {
        TIMING_48K.halfcycles_per_frame
    }
    fn run_frame(&mut self) {
        Spectrum48k::run_frame(self);
    }
    fn framebuffer(&self) -> &[u8] {
        Spectrum48k::framebuffer(self)
    }
    fn audio_frame(&self) -> &[f32] {
        Spectrum48k::audio_frame(self)
    }
    fn set_keyboard_rows(&mut self, rows: &[u8; 8]) {
        *self.keyboard_mut().rows_mut() = *rows;
    }
    fn load_tape_blocks(&mut self, blocks: Vec<TapeBlock>) {
        Spectrum48k::load_tape_blocks(self, blocks);
    }
    fn load_tape_stream(&mut self, stream: Vec<TapeSpan>) {
        Spectrum48k::load_tape_stream(self, stream);
    }
    fn tape_play(&mut self) {
        self.play_tape();
    }
    fn tape_stop(&mut self) {
        self.stop_tape();
    }
    fn reset_machine(&mut self) {
        self.reset();
    }
}

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

    fn supports_disk_slot(&self, slot: &str) -> bool {
        matches!(self.model, PlusModel::Plus3) && slot == "disk-a"
    }

    fn load_disk_image(&mut self, slot: &str, bytes: &[u8]) -> Result<(), String> {
        if !self.supports_disk_slot(slot) {
            return Err(format!("unsupported disk slot `{slot}`"));
        }
        let image = format_amstrad_dsk::parse(bytes)?;
        self.insert_disk(image);
        Ok(())
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
}
