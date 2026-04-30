//! `SpectrumMachine` impls + type aliases for each Spectrum-family variant.
//!
//! Every variant plugs into the generic `SpectrumRuntime<M>`. The
//! variant-specific query catalogue (boot banner, AY state, board
//! issue, kempston, SCLD high-res, …) is supplied here through
//! [`SpectrumMachine::variant_query_paths`] and
//! [`SpectrumMachine::resolve_variant_query`]; everything else (screen
//! text, keyboard, tape state, frame timing) is shared in
//! [`crate::queries`].

use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::tape::{TapeBlock, TapeSpan};
use common_sinclair_zx_spectrum::timing::{
    SCREEN_HEIGHT, SCREEN_WIDTH, SCREEN_WIDTH_HIRES, TIMING_48K, TIMING_128K, TIMING_PENTAGON,
    TIMING_PLUS2A, TIMING_SCORPION,
};
use emu198x_shell::{QueryError, QueryResult};
use machine_pentagon_128::Pentagon128;
use machine_scorpion_zs256::ScorpionZS256;
use machine_sinclair_zx_spectrum_48k::{BoardIssue, Spectrum48k};
use machine_sinclair_zx_spectrum_128k::Spectrum128K;
use machine_sinclair_zx_spectrum_plus::{Model as PlusModel, SpectrumPlus};
use machine_timex_tc2048::TimexTC2048;
use machine_timex_ts2068::{TIMING_TS2068, TimexModel, TimexTS2068};
use serde_json::json;

use crate::queries::{SpectrumBootStatus, boot_status_from_banners, screen_text_lines};
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

// ─────────────────────────────────────────────────────────────────────
// Boot-banner constants
//
// Each variant supplies the screen-text banners its boot ROM paints.
// Where we can't confirm the banner from a real ROM we mark it TODO
// and fall back to "boot.detected = false". The 48K ROM is the only
// one whose banner strings have been verified against a booted
// machine in this fresh-start workspace.
// ─────────────────────────────────────────────────────────────────────

/// Banners painted by the 48K ROM ("(C) 1982 Sinclair Research Ltd").
const SPECTRUM_48K_BANNERS: &[&str] = &[
    "(C) 1982 Sinclair Research Ltd",
    "© 1982 Sinclair Research Ltd",
    "1982 Sinclair Research Ltd",
];

// TODO: confirm Spectrum 128K boot banner. The Sinclair 128K ROM is
// reported to display a "© 1986 Sinclair Research Ltd" banner under
// the 128 BASIC start menu, but the exact spacing has not been
// verified against the ROM in this workspace. Returning detected =
// false until a ROM-backed boot test fixes the banner.
const SPECTRUM_128K_BANNERS: &[&str] = &[];

// TODO: confirm SpectrumPlus (+2A / +2B / +3) boot banner. The Amstrad
// gate-array ROMs display "© 1986 Amstrad Consumer Electronics plc"
// under the menu but the exact on-screen rendering has not been
// captured in this workspace. Returning detected = false until
// confirmed.
const SPECTRUM_PLUS_BANNERS: &[&str] = &[];

// TODO: confirm Pentagon 128 boot banner. The Pentagon ROM is a
// modified 128K image — banner string is variant-specific to the
// Pentagon revision. Returning detected = false until confirmed.
const PENTAGON_128_BANNERS: &[&str] = &[];

// TODO: confirm Scorpion ZS-256 boot banner. The Scorpion ROM
// displays a Russian-language "Scorpion ZS-256" banner that has not
// been verified in this workspace. Returning detected = false until
// confirmed.
const SCORPION_ZS256_BANNERS: &[&str] = &[];

// TODO: confirm Timex TC2048 boot banner. The Timex ROM is a 48K
// derivative with a Portuguese-market splash. Banner not yet
// verified. Returning detected = false until confirmed.
const TIMEX_TC2048_BANNERS: &[&str] = &[];

// TODO: confirm Timex TC2068 / TS2068 boot banner. The Timex 2068
// ROM displays a "TIMEX SINCLAIR 2068" splash but the exact
// on-screen text has not been captured. Returning detected = false
// until confirmed.
const TIMEX_TS2068_BANNERS: &[&str] = &[];

const COMMON_BOOT_PATHS: &[&str] = &["boot.detected", "boot.reason", "boot.row"];

const SPECTRUM_48K_QUERY_PATHS: &[&str] = &[
    "boot.detected",
    "boot.reason",
    "boot.row",
    "spectrum.machine.issue",
];

const SPECTRUM_PLUS_QUERY_PATHS: &[&str] = &[
    "boot.detected",
    "boot.reason",
    "boot.row",
    "spectrum.plus.disk_slot_supported",
];

const PENTAGON_128_QUERY_PATHS: &[&str] = &[
    "boot.detected",
    "boot.reason",
    "boot.row",
    "spectrum.kempston.state",
];

const SCORPION_ZS256_QUERY_PATHS: &[&str] = &[
    "boot.detected",
    "boot.reason",
    "boot.row",
    "spectrum.kempston.state",
];

const TIMEX_TC2048_QUERY_PATHS: &[&str] = &[
    "boot.detected",
    "boot.reason",
    "boot.row",
    "spectrum.kempston.state",
];

const TIMEX_TS2068_QUERY_PATHS: &[&str] = &[
    "boot.detected",
    "boot.reason",
    "boot.row",
    "spectrum.kempston.state",
    "spectrum.timex.model",
];

fn boot_status_query<M: SpectrumMachine>(
    machine: &M,
    banners: &[&str],
) -> SpectrumBootStatus {
    if banners.is_empty() {
        return SpectrumBootStatus::not_detected();
    }
    let lines = screen_text_lines(machine);
    boot_status_from_banners(&lines, banners)
}

fn resolve_boot_path<M: SpectrumMachine>(
    machine: &M,
    banners: &[&str],
    path: &str,
) -> Result<Option<QueryResult>, QueryError> {
    let value = match path {
        "boot.detected" => json!(boot_status_query(machine, banners).detected),
        "boot.reason" => json!(boot_status_query(machine, banners).reason),
        "boot.row" => json!(boot_status_query(machine, banners).row),
        _ => return Ok(None),
    };
    Ok(Some(QueryResult {
        path: path.to_owned(),
        value,
    }))
}

fn board_issue_name(issue: BoardIssue) -> &'static str {
    match issue {
        BoardIssue::Issue2 => "issue2",
        BoardIssue::Issue3 => "issue3",
    }
}

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

    fn read_byte(&self, addr: u16) -> u8 {
        <Self as MemoryBus>::read(self, addr)
    }
    fn keyboard_rows(&self) -> &[u8; 8] {
        Spectrum48k::keyboard(self).rows()
    }
    fn tape_is_loaded(&self) -> bool {
        Spectrum48k::tape_is_loaded(self)
    }
    fn tape_is_playing(&self) -> bool {
        Spectrum48k::tape_is_playing(self)
    }
    fn half_cycle_in_frame(&self) -> u32 {
        Spectrum48k::hc(self)
    }
    fn tstate_in_frame(&self) -> u32 {
        Spectrum48k::tstate_in_frame(self)
    }

    fn variant_query_paths() -> &'static [&'static str] {
        SPECTRUM_48K_QUERY_PATHS
    }

    fn resolve_variant_query(
        &self,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        if COMMON_BOOT_PATHS.contains(&path) {
            return resolve_boot_path(self, SPECTRUM_48K_BANNERS, path);
        }
        let value = match path {
            "spectrum.machine.issue" => json!(board_issue_name(self.issue())),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
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

    fn read_byte(&self, addr: u16) -> u8 {
        self.memory.read(addr)
    }
    fn keyboard_rows(&self) -> &[u8; 8] {
        &self.keyboard
    }
    fn tape_is_loaded(&self) -> bool {
        self.tape.has_tape()
    }
    fn tape_is_playing(&self) -> bool {
        self.tape.is_playing()
    }
    fn half_cycle_in_frame(&self) -> u32 {
        <Self as SpectrumDriver>::hc(self)
    }
    fn tstate_in_frame(&self) -> u32 {
        <Self as SpectrumDriver>::hc(self) / TIMING_128K.cpu_divisor
    }

    fn variant_query_paths() -> &'static [&'static str] {
        COMMON_BOOT_PATHS
    }

    fn resolve_variant_query(
        &self,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        if COMMON_BOOT_PATHS.contains(&path) {
            return resolve_boot_path(self, SPECTRUM_128K_BANNERS, path);
        }
        Ok(None)
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

    fn read_byte(&self, addr: u16) -> u8 {
        self.memory.read(addr)
    }
    fn keyboard_rows(&self) -> &[u8; 8] {
        &self.keyboard
    }
    fn tape_is_loaded(&self) -> bool {
        self.tape.has_tape()
    }
    fn tape_is_playing(&self) -> bool {
        self.tape.is_playing()
    }
    fn half_cycle_in_frame(&self) -> u32 {
        <Self as SpectrumDriver>::hc(self)
    }
    fn tstate_in_frame(&self) -> u32 {
        <Self as SpectrumDriver>::hc(self) / TIMING_PLUS2A.cpu_divisor
    }

    fn variant_query_paths() -> &'static [&'static str] {
        SPECTRUM_PLUS_QUERY_PATHS
    }

    fn resolve_variant_query(
        &self,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        if COMMON_BOOT_PATHS.contains(&path) {
            return resolve_boot_path(self, SPECTRUM_PLUS_BANNERS, path);
        }
        let value = match path {
            "spectrum.plus.disk_slot_supported" => {
                json!(self.supports_disk_slot("disk-a"))
            }
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
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

    fn read_byte(&self, addr: u16) -> u8 {
        self.memory.read(addr)
    }
    fn keyboard_rows(&self) -> &[u8; 8] {
        &self.keyboard
    }
    fn tape_is_loaded(&self) -> bool {
        self.tape.has_tape()
    }
    fn tape_is_playing(&self) -> bool {
        self.tape.is_playing()
    }
    fn half_cycle_in_frame(&self) -> u32 {
        <Self as SpectrumDriver>::hc(self)
    }
    fn tstate_in_frame(&self) -> u32 {
        <Self as SpectrumDriver>::hc(self) / TIMING_PENTAGON.cpu_divisor
    }

    fn variant_query_paths() -> &'static [&'static str] {
        PENTAGON_128_QUERY_PATHS
    }

    fn resolve_variant_query(
        &self,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        if COMMON_BOOT_PATHS.contains(&path) {
            return resolve_boot_path(self, PENTAGON_128_BANNERS, path);
        }
        let value = match path {
            "spectrum.kempston.state" => json!(self.kempston),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
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

    fn read_byte(&self, addr: u16) -> u8 {
        self.memory.read(addr)
    }
    fn keyboard_rows(&self) -> &[u8; 8] {
        &self.keyboard
    }
    fn tape_is_loaded(&self) -> bool {
        self.tape.has_tape()
    }
    fn tape_is_playing(&self) -> bool {
        self.tape.is_playing()
    }
    fn half_cycle_in_frame(&self) -> u32 {
        <Self as SpectrumDriver>::hc(self)
    }
    fn tstate_in_frame(&self) -> u32 {
        <Self as SpectrumDriver>::hc(self) / TIMING_SCORPION.cpu_divisor
    }

    fn variant_query_paths() -> &'static [&'static str] {
        SCORPION_ZS256_QUERY_PATHS
    }

    fn resolve_variant_query(
        &self,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        if COMMON_BOOT_PATHS.contains(&path) {
            return resolve_boot_path(self, SCORPION_ZS256_BANNERS, path);
        }
        let value = match path {
            "spectrum.kempston.state" => json!(self.kempston),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
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

    fn read_byte(&self, addr: u16) -> u8 {
        self.memory.read(addr)
    }
    fn keyboard_rows(&self) -> &[u8; 8] {
        &self.keyboard
    }
    fn tape_is_loaded(&self) -> bool {
        self.tape.has_tape()
    }
    fn tape_is_playing(&self) -> bool {
        self.tape.is_playing()
    }
    fn half_cycle_in_frame(&self) -> u32 {
        <Self as SpectrumDriver>::hc(self)
    }
    fn tstate_in_frame(&self) -> u32 {
        <Self as SpectrumDriver>::hc(self) / TIMING_48K.cpu_divisor
    }

    fn variant_query_paths() -> &'static [&'static str] {
        TIMEX_TC2048_QUERY_PATHS
    }

    fn resolve_variant_query(
        &self,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        if COMMON_BOOT_PATHS.contains(&path) {
            return resolve_boot_path(self, TIMEX_TC2048_BANNERS, path);
        }
        let value = match path {
            "spectrum.kempston.state" => json!(self.kempston),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
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

    fn read_byte(&self, addr: u16) -> u8 {
        self.memory.read(addr)
    }
    fn keyboard_rows(&self) -> &[u8; 8] {
        &self.keyboard
    }
    fn tape_is_loaded(&self) -> bool {
        self.tape.has_tape()
    }
    fn tape_is_playing(&self) -> bool {
        self.tape.is_playing()
    }
    fn half_cycle_in_frame(&self) -> u32 {
        <Self as SpectrumDriver>::hc(self)
    }
    fn tstate_in_frame(&self) -> u32 {
        <Self as SpectrumDriver>::hc(self) / TIMING_48K.cpu_divisor
    }

    fn variant_query_paths() -> &'static [&'static str] {
        TIMEX_TS2068_QUERY_PATHS
    }

    fn resolve_variant_query(
        &self,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        if COMMON_BOOT_PATHS.contains(&path) {
            return resolve_boot_path(self, TIMEX_TS2068_BANNERS, path);
        }
        let value = match path {
            "spectrum.kempston.state" => json!(self.kempston),
            "spectrum.timex.model" => json!(match self.model {
                TimexModel::TC2068 => "tc2068",
                TimexModel::TS2068 => "ts2068",
            }),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}
