//! `SpectrumMachine` impls + type aliases for each Spectrum-family variant.
//!
//! Every variant plugs into the generic `SpectrumRuntime<M>`. The
//! variant-specific query catalogue (boot banner, AY state, board
//! issue, kempston, SCLD high-res, …) is supplied here through
//! [`SpectrumMachine::variant_query_paths`] and
//! [`SpectrumMachine::resolve_variant_query`]; everything else (screen
//! text, keyboard, tape state, frame timing) is shared in
//! [`crate::queries`].

use common_sinclair_zx_spectrum::audio::{AudioControls, SpeakerChannel};
use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::snapshot::Snapshot;
use common_sinclair_zx_spectrum::tape::{TapeBlock, TapeSpan};
use common_sinclair_zx_spectrum::timing::{
    SCREEN_HEIGHT, SCREEN_WIDTH, SCREEN_WIDTH_HIRES, TIMING_48K, TIMING_128K, TIMING_PENTAGON,
    TIMING_PLUS2A, TIMING_SCORPION,
};
use common_sinclair_zx_spectrum_amstrad_class::{AmstradVariant, SpectrumAmstradClassCore};
use emu198x_shell::{QueryError, QueryResult};
use gi_ay_3_8912::Ay3_8912;
use machine_pentagon_128::Pentagon128;
use machine_scorpion_zs256::ScorpionZS256;
use machine_sinclair_zx_spectrum_16k::Spectrum16K;
use machine_sinclair_zx_spectrum_48k::{Spectrum48k, UlaRevision};
use machine_sinclair_zx_spectrum_128k::Spectrum128K;
use machine_sinclair_zx_spectrum_plus::SpectrumPlus;
use machine_sinclair_zx_spectrum_plus2::SpectrumPlus2;
use machine_sinclair_zx_spectrum_plus2a::SpectrumPlus2A;
use machine_sinclair_zx_spectrum_plus2b::SpectrumPlus2B;
use machine_sinclair_zx_spectrum_plus3::SpectrumPlus3;
use machine_timex_tc2048::TimexTC2048;
use machine_timex_ts2068::{TIMING_TS2068, TimexModel, TimexTS2068};
use peripheral_kempston_joystick::{KempstonButton, KempstonJoystick};
use serde_json::json;

/// Shared helper for [`SpectrumMachine::set_kempston_button`] overrides.
///
/// Translates the runtime-level button index (matching the bit position
/// on the Kempston read byte) into a [`KempstonButton`] enum value, then
/// flips the peripheral's `attached` flag and updates the bit. Returns
/// `false` for indices outside `0..=4` so callers can propagate the
/// failure signal back to the trait method's contract.
fn apply_kempston_button_index(joystick: &mut KempstonJoystick, button: u8, pressed: bool) -> bool {
    let Some(b) = KempstonButton::from_index(button) else {
        return false;
    };
    // First applied event makes the interface visible to software that
    // probes `$1F` for Kempston detection. Real hardware would have read
    // floating bus before the user touched the pad; we mirror that by
    // keeping `attached=false` until a recognised event arrives.
    joystick.attached = true;
    joystick.set_button(b, pressed);
    true
}

use crate::queries::{SpectrumBootStatus, boot_status_from_banners, screen_text_lines};
use crate::runtime::{SpectrumMachine, SpectrumRuntime};

/// ZX Spectrum 16K runtime.
pub type Spectrum16kRuntime = SpectrumRuntime<Spectrum16K>;

/// ZX Spectrum 48K runtime.
pub type Spectrum48kRuntime = SpectrumRuntime<Spectrum48k>;

/// ZX Spectrum+ runtime. The Spectrum+ is electrically identical to the
/// 48K — same Ferranti ULA, same 16 KiB ROM, same 48 KiB RAM — so the
/// underlying machine type is `SpectrumMachineCore<Spectrum48kMemory>`,
/// the same as the 48K's. Catalogue identity comes from `Model::SpectrumPlus`.
pub type SpectrumPlusRuntime = SpectrumRuntime<SpectrumPlus>;

/// ZX Spectrum 128K / +2 runtime.
pub type Spectrum128kRuntime = SpectrumRuntime<Spectrum128K>;

/// Sinclair-branded Amstrad-built grey +2 runtime.
pub type SpectrumPlus2Runtime = SpectrumRuntime<SpectrumPlus2>;

/// ZX Spectrum +2A runtime.
pub type SpectrumPlus2ARuntime = SpectrumRuntime<SpectrumPlus2A>;

/// ZX Spectrum +2B runtime.
pub type SpectrumPlus2BRuntime = SpectrumRuntime<SpectrumPlus2B>;

/// ZX Spectrum +3 runtime.
pub type SpectrumPlus3Runtime = SpectrumRuntime<SpectrumPlus3>;

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

// Confirmed 2026-05-01 by booting `~/.emu198x/roms/sinclair-zx-spectrum-128k/
// 128-{0,1}.rom` for 200 frames: row 23 reads
// `"© 1986 Sinclair Research Ltd"`. Reachable now because
// `glyph_byte` on `Spectrum128K` reads the standard table directly
// from ROM 1 (48 BASIC) regardless of paging — see
// `impl SpectrumMachine for Spectrum128K`.
//
// The grey +2 (Amstrad relabel of the 128K — `plus2-{0,1}.rom`)
// boots to the same Spectrum128K runtime but shows
// `"©1986, ©1982 Amstrad Consumer Electronics plc"` instead. Its
// rendering splits across rows 22-23, so the substring
// "Amstrad Consumer Electronics plc" is the most reliable single
// match. Both banners are accepted here so the same runtime
// detects "we're at the 128K menu" regardless of which ROM image
// is loaded.
const SPECTRUM_128K_BANNERS: &[&str] = &[
    "© 1986 Sinclair Research Ltd",
    "(C) 1986 Sinclair Research Ltd",
    // The Amstrad-built grey +2 ROMs occasionally end up in 128K
    // firmware paths; their banner wraps mid-phrase across rows 22-23
    // so we match on the row-22 substring rather than the full string.
    "Amstrad Consumer",
];

// The grey +2 boots to "©1986, ©1982 Amstrad Consumer" on row 22 and
// "Electronics plc" on row 23 — the banner wraps mid-phrase. The
// banner-substring matcher works line-by-line, so the substring has to
// fit within a single rendered row. "Amstrad Consumer" is distinctive
// enough to match the +2 splash without false positives elsewhere.
const SPECTRUM_PLUS2_BANNERS: &[&str] = &["Amstrad Consumer"];

// Confirmed 2026-05-01 by booting `~/.emu198x/roms/amstrad-zx-spectrum-plus3/
// plus3-{0,1,2,3}.rom` for 250 frames against each of Plus2A, Plus2B,
// and Plus3 models: row 22 reads `"©1982, 1986, 1987 Amstrad Plc."`
// across all three. Row 23 differs by model (+2A/+2B = "Drive M:
// available.", +3 = "Drives A:, B: and M: available.") but the
// row-22 banner is the same. Reachable now because `glyph_byte` on
// `SpectrumPlus` reads the standard table from ROM 3 (the 48 BASIC
// sub-ROM in the +3 layout) — see `impl SpectrumMachine for
// SpectrumPlus`.
const SPECTRUM_PLUS_BANNERS: &[&str] = &[
    "©1982, 1986, 1987 Amstrad Plc.",
    "(C)1982, 1986, 1987 Amstrad Plc.",
];

// Confirmed 2026-05-01 by booting `~/.emu198x/roms/pentagon-128/
// pentagon-{0,1}.rom` for 200 frames: row 23 reads
// `"© 1993 Sinclair Research Ltd"` — the Pentagon's revised banner
// dating the Russian-market reissue. Reachable via the same
// paging-aware `glyph_byte` override that 128K uses (Pentagon's
// ROM 1 carries the 48 BASIC glyph table).
const PENTAGON_128_BANNERS: &[&str] = &[
    "© 1993 Sinclair Research Ltd",
    "(C) 1993 Sinclair Research Ltd",
];

// BLOCKED 2026-05-01 (refined 2026-05-01 by `probe_scorpion_screen_ram`):
// the Scorpion's Service ROM (ROM 0 / ZSU monitor) boots silently
// to a monitor in upper RAM. After 2000 frames the screen RAM at
// $4000-$5AFF is **100% zero** but the CPU is alive (PC=$EB82,
// IFF1=true, IM=1, TR-DOS not paged) — interrupts are firing every
// frame, the monitor is just choosing not to paint anything until
// you interact. This is standard Soviet-Scorpion behaviour, NOT a
// TR-DOS implementation bug. The Beta-disk crate is fully
// implemented (442 lines, no `todo!`/`unimplemented!`); TR-DOS
// hasn't been paged in here because nothing has tried to read from
// `$3D00..$3DFF` yet.
//
// Banner detection here genuinely cannot use a screen-text scan.
// Three signal alternatives the next contributor could use:
//   (a) Insert a known idle disk image and wait for TR-DOS to page
//       in and paint its directory listing.
//   (b) Send a Caps Shift / key press at boot and wait for the
//       monitor's response screen.
//   (c) Detect the boot via PC range or I/O register state rather
//       than a screen scan (e.g. once PC enters the monitor's
//       command-loop region).
const SCORPION_ZS256_BANNERS: &[&str] = &[];

// Confirmed 2026-05-01 by booting `~/.emu198x/roms/timex-tc2048/tc2048.rom`
// for 200 frames and inspecting `screen.text.lines`: row 23 reads
// `"© 1982 Sinclair Research Ltd"` — same banner as the 48K because the
// TC2048 ships an enhanced 48K-compatible ROM (Timex of Portugal sold
// it as the Timex Computer 2048 for the European market). The decoder
// works directly on TC2048 because the ROM is a single 16K image — no
// paging, glyph table at $3D00 just like the 48K.
const TIMEX_TC2048_BANNERS: &[&str] = &[
    "(C) 1982 Sinclair Research Ltd",
    "© 1982 Sinclair Research Ltd",
    "1982 Sinclair Research Ltd",
];

// BLOCKED 2026-05-01 (refined 2026-05-01 by `probe_ts2068_screen_ram`):
// the TS2068 boot screen lives in **Timex 64-column high-resolution
// mode**, not the standard 32-column Sinclair layout. The probe shows
//   * standard pixel area $4000-$57FF: 3072/6144 nonzero (~50%)
//   * secondary pixel area $6000-$77FF: 3072/6144 nonzero (~50%)
//   * sample row 23: `39 00 39 00 39 00 39 00 …`
// The alternating `$39 $00` pattern is the SCLD (Timex hi-res
// controller) interleaving the two bitmap planes — even-indexed
// bytes from $4000-$57FF, odd-indexed from $6000-$77FF — to compose
// a 512×192 monochrome image. Our screen-text decoder reads only the
// 32-column standard layout, so it sees the even-indexed half of an
// interleaved hi-res bitmap and renders alternating `?` cells.
//
// Banner detection here can't use the standard $3D00 glyph table at
// all — there are no character cells to decode. The next contributor
// could either (a) add a Timex-mode-aware screen-text decoder that
// reconstructs the 512-pixel-wide bitmap and OCRs it, or (b) detect
// the boot via PC range / I/O register state (port $FF, the SCLD
// mode register) rather than a screen scan.
const TIMEX_TS2068_BANNERS: &[&str] = &[];

const COMMON_BOOT_PATHS: &[&str] = &["boot.detected", "boot.reason", "boot.row"];

/// AY-3-8912 query paths shared by every variant that owns an AY chip
/// (128K, +2A/+2B/+3, Pentagon, Scorpion, TS2068). The 48K and TC2048
/// have no AY and never expose these. Resolved by [`resolve_ay_path`].
const AY_QUERY_PATHS: &[&str] = &["spectrum.ay.selected_register", "spectrum.ay.registers"];

const SPECTRUM_48K_QUERY_PATHS: &[&str] = &[
    "boot.detected",
    "boot.reason",
    "boot.row",
    "spectrum.machine.issue",
];

// 16K shares the 48K ROM image (same banner) and ships in the same
// Issue 2 / Issue 3 boards, so it exposes exactly the same query
// surface. Kept as its own constant rather than aliased so any future
// 16K-only path lands in one obvious place.
const SPECTRUM_16K_QUERY_PATHS: &[&str] = &[
    "boot.detected",
    "boot.reason",
    "boot.row",
    "spectrum.machine.issue",
];

const SPECTRUM_16K_BANNERS: &[&str] = SPECTRUM_48K_BANNERS;

// Spectrum+ boots the same 48K ROM, so banner detection runs against
// the same set as the 48K and 16K. Its catalogue identity comes from
// `Model::SpectrumPlus`, not from a different banner.
const SPECTRUM_PROPER_PLUS_BANNERS: &[&str] = SPECTRUM_48K_BANNERS;
const SPECTRUM_PROPER_PLUS_QUERY_PATHS: &[&str] = SPECTRUM_16K_QUERY_PATHS;

const SPECTRUM_128K_QUERY_PATHS: &[&str] = &[
    "boot.detected",
    "boot.reason",
    "boot.row",
    "spectrum.ay.selected_register",
    "spectrum.ay.registers",
];

// +2 shares the 128K's chip set and so exposes the same query surface.
// Distinct constant so any future +2-only path lands in one place.
const SPECTRUM_PLUS2_QUERY_PATHS: &[&str] = &[
    "boot.detected",
    "boot.reason",
    "boot.row",
    "spectrum.ay.selected_register",
    "spectrum.ay.registers",
];

const SPECTRUM_PLUS_QUERY_PATHS: &[&str] = &[
    "boot.detected",
    "boot.reason",
    "boot.row",
    "spectrum.plus.disk_slot_supported",
    "spectrum.ay.selected_register",
    "spectrum.ay.registers",
];

const PENTAGON_128_QUERY_PATHS: &[&str] = &[
    "boot.detected",
    "boot.reason",
    "boot.row",
    "spectrum.kempston.state",
    "spectrum.ay.selected_register",
    "spectrum.ay.registers",
];

const SCORPION_ZS256_QUERY_PATHS: &[&str] = &[
    "boot.detected",
    "boot.reason",
    "boot.row",
    "spectrum.kempston.state",
    "spectrum.ay.selected_register",
    "spectrum.ay.registers",
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
    "spectrum.ay.selected_register",
    "spectrum.ay.registers",
];

fn boot_status_query<M: SpectrumMachine>(machine: &M, banners: &[&str]) -> SpectrumBootStatus {
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

/// Resolve `spectrum.ay.*` paths against an AY-3-8912 chip. Used by
/// every variant that owns an AY (128K, +2A/+2B/+3, Pentagon,
/// Scorpion, TS2068). Returns `Ok(None)` for any path outside the
/// `spectrum.ay.*` namespace so callers can chain into other
/// variant-specific resolvers.
fn resolve_ay_path(ay: &Ay3_8912, path: &str) -> Result<Option<QueryResult>, QueryError> {
    let value = match path {
        "spectrum.ay.selected_register" => json!(ay.selected_register()),
        "spectrum.ay.registers" => json!(ay.registers()),
        _ => return Ok(None),
    };
    Ok(Some(QueryResult {
        path: path.to_owned(),
        value,
    }))
}

/// Maps a [`UlaRevision`] to the stable string identifier returned by
/// the `spectrum.machine.issue` query path. The string values
/// (`"issue2"` / `"issue3"`) are preserved for backward compatibility
/// with the documented scripting API (see `docs/features/scripting.md`)
/// — the internal type rename to `UlaRevision::Ferranti5C` /
/// `UlaRevision::Ferranti6C` does not propagate to the JSON surface.
fn ula_revision_name(revision: UlaRevision) -> &'static str {
    match revision {
        UlaRevision::Ferranti5C => "issue2",
        UlaRevision::Ferranti6C => "issue3",
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
    fn audio_controls(&self) -> AudioControls {
        Spectrum48k::audio_controls(self)
    }
    fn set_audio_controls(&mut self, controls: AudioControls) {
        Spectrum48k::set_audio_controls(self, controls);
    }
    fn set_audio_channel_enabled(&mut self, channel: SpeakerChannel, enabled: bool) {
        Spectrum48k::set_audio_channel_enabled(self, channel, enabled);
    }
    fn set_audio_channel_gain(&mut self, channel: SpeakerChannel, gain: f32) {
        Spectrum48k::set_audio_channel_gain(self, channel, gain);
    }
    fn set_keyboard_rows(&mut self, rows: &[u8; 8]) {
        *self.keyboard_mut().rows_mut() = *rows;
    }
    fn set_kempston_button(&mut self, button: u8, pressed: bool) -> bool {
        apply_kempston_button_index(self.kempston_mut(), button, pressed)
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
    fn after_restore(&mut self) {
        self.restore_volatile_refs();
    }

    fn read_byte(&self, addr: u16) -> u8 {
        <Self as MemoryBus>::read(self, addr)
    }
    fn write_byte(&mut self, addr: u16, value: u8) {
        <Self as MemoryBus>::write(self, addr, value);
    }
    fn apply_snapshot(&mut self, snap: &Snapshot) {
        Spectrum48k::apply_snapshot(self, snap);
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

    fn resolve_variant_query(&self, path: &str) -> Result<Option<QueryResult>, QueryError> {
        if COMMON_BOOT_PATHS.contains(&path) {
            return resolve_boot_path(self, SPECTRUM_48K_BANNERS, path);
        }
        let value = match path {
            "spectrum.machine.issue" => json!(ula_revision_name(self.revision())),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }

    fn start_memory_write_watch(&mut self, addr: u16, len: u16) -> Result<(), &'static str> {
        Spectrum48k::start_memory_write_watch(self, addr, len);
        Ok(())
    }
    fn stop_memory_write_watch(&mut self) {
        Spectrum48k::stop_memory_write_watch(self);
    }
    fn memory_write_watch_records(
        &self,
    ) -> Option<&[common_sinclair_zx_spectrum::MemoryWriteRecord]> {
        Spectrum48k::memory_write_watch_records(self)
    }
    fn memory_write_watch_range(&self) -> Option<(u16, u16)> {
        Spectrum48k::memory_write_watch_range(self)
    }
    fn clear_memory_write_watch_records(&mut self) {
        Spectrum48k::clear_memory_write_watch_records(self);
    }
    fn z80_registers(&self) -> &zilog_z80::Registers {
        &self.z80().regs
    }
    fn z80_halted(&self) -> bool {
        self.z80().halt
    }
}

impl SpectrumMachine for Spectrum16K {
    const FRAME_WIDTH: u32 = SCREEN_WIDTH as u32;
    const FRAME_HEIGHT: u32 = SCREEN_HEIGHT as u32;

    fn frame_halfcycles(&self) -> u32 {
        TIMING_48K.halfcycles_per_frame
    }
    fn run_frame(&mut self) {
        Spectrum16K::run_frame(self);
    }
    fn framebuffer(&self) -> &[u8] {
        Spectrum16K::framebuffer(self)
    }
    fn audio_frame(&self) -> &[f32] {
        Spectrum16K::audio_frame(self)
    }
    fn audio_controls(&self) -> AudioControls {
        Spectrum16K::audio_controls(self)
    }
    fn set_audio_controls(&mut self, controls: AudioControls) {
        Spectrum16K::set_audio_controls(self, controls);
    }
    fn set_audio_channel_enabled(&mut self, channel: SpeakerChannel, enabled: bool) {
        Spectrum16K::set_audio_channel_enabled(self, channel, enabled);
    }
    fn set_audio_channel_gain(&mut self, channel: SpeakerChannel, gain: f32) {
        Spectrum16K::set_audio_channel_gain(self, channel, gain);
    }
    fn set_keyboard_rows(&mut self, rows: &[u8; 8]) {
        *self.keyboard_mut().rows_mut() = *rows;
    }
    fn set_kempston_button(&mut self, button: u8, pressed: bool) -> bool {
        apply_kempston_button_index(self.kempston_mut(), button, pressed)
    }
    fn load_tape_blocks(&mut self, blocks: Vec<TapeBlock>) {
        Spectrum16K::load_tape_blocks(self, blocks);
    }
    fn load_tape_stream(&mut self, stream: Vec<TapeSpan>) {
        Spectrum16K::load_tape_stream(self, stream);
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
    fn after_restore(&mut self) {
        self.restore_volatile_refs();
    }

    fn read_byte(&self, addr: u16) -> u8 {
        <Self as MemoryBus>::read(self, addr)
    }
    fn write_byte(&mut self, addr: u16, value: u8) {
        <Self as MemoryBus>::write(self, addr, value);
    }
    fn apply_snapshot(&mut self, snap: &Snapshot) {
        Spectrum16K::apply_snapshot(self, snap);
    }
    fn keyboard_rows(&self) -> &[u8; 8] {
        Spectrum16K::keyboard(self).rows()
    }
    fn tape_is_loaded(&self) -> bool {
        Spectrum16K::tape_is_loaded(self)
    }
    fn tape_is_playing(&self) -> bool {
        Spectrum16K::tape_is_playing(self)
    }
    fn half_cycle_in_frame(&self) -> u32 {
        Spectrum16K::hc(self)
    }
    fn tstate_in_frame(&self) -> u32 {
        Spectrum16K::tstate_in_frame(self)
    }

    fn variant_query_paths() -> &'static [&'static str] {
        SPECTRUM_16K_QUERY_PATHS
    }

    fn resolve_variant_query(&self, path: &str) -> Result<Option<QueryResult>, QueryError> {
        if COMMON_BOOT_PATHS.contains(&path) {
            return resolve_boot_path(self, SPECTRUM_16K_BANNERS, path);
        }
        let value = match path {
            "spectrum.machine.issue" => json!(ula_revision_name(self.revision())),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }

    fn start_memory_write_watch(&mut self, addr: u16, len: u16) -> Result<(), &'static str> {
        Spectrum16K::start_memory_write_watch(self, addr, len);
        Ok(())
    }
    fn stop_memory_write_watch(&mut self) {
        Spectrum16K::stop_memory_write_watch(self);
    }
    fn memory_write_watch_records(
        &self,
    ) -> Option<&[common_sinclair_zx_spectrum::MemoryWriteRecord]> {
        Spectrum16K::memory_write_watch_records(self)
    }
    fn memory_write_watch_range(&self) -> Option<(u16, u16)> {
        Spectrum16K::memory_write_watch_range(self)
    }
    fn clear_memory_write_watch_records(&mut self) {
        Spectrum16K::clear_memory_write_watch_records(self);
    }
    fn z80_registers(&self) -> &zilog_z80::Registers {
        &self.z80().regs
    }
    fn z80_halted(&self) -> bool {
        self.z80().halt
    }
}

// Spectrum+ shares the 48K's hardware and ROM. Its impl mirrors the 48K
// — same timing, same Ferranti ULA, same 48 BASIC ROM — but the
// phantom marker keeps it as a distinct Rust type so snapshots can't
// cross between the two and per-variant metadata can attach to the
// marker rather than the runtime.
impl SpectrumMachine for SpectrumPlus {
    const FRAME_WIDTH: u32 = SCREEN_WIDTH as u32;
    const FRAME_HEIGHT: u32 = SCREEN_HEIGHT as u32;

    fn frame_halfcycles(&self) -> u32 {
        TIMING_48K.halfcycles_per_frame
    }
    fn run_frame(&mut self) {
        SpectrumPlus::run_frame(self);
    }
    fn framebuffer(&self) -> &[u8] {
        SpectrumPlus::framebuffer(self)
    }
    fn audio_frame(&self) -> &[f32] {
        SpectrumPlus::audio_frame(self)
    }
    fn audio_controls(&self) -> AudioControls {
        SpectrumPlus::audio_controls(self)
    }
    fn set_audio_controls(&mut self, controls: AudioControls) {
        SpectrumPlus::set_audio_controls(self, controls);
    }
    fn set_audio_channel_enabled(&mut self, channel: SpeakerChannel, enabled: bool) {
        SpectrumPlus::set_audio_channel_enabled(self, channel, enabled);
    }
    fn set_audio_channel_gain(&mut self, channel: SpeakerChannel, gain: f32) {
        SpectrumPlus::set_audio_channel_gain(self, channel, gain);
    }
    fn set_keyboard_rows(&mut self, rows: &[u8; 8]) {
        *self.keyboard_mut().rows_mut() = *rows;
    }
    fn set_kempston_button(&mut self, button: u8, pressed: bool) -> bool {
        apply_kempston_button_index(self.kempston_mut(), button, pressed)
    }
    fn load_tape_blocks(&mut self, blocks: Vec<TapeBlock>) {
        SpectrumPlus::load_tape_blocks(self, blocks);
    }
    fn load_tape_stream(&mut self, stream: Vec<TapeSpan>) {
        SpectrumPlus::load_tape_stream(self, stream);
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
    fn after_restore(&mut self) {
        self.restore_volatile_refs();
    }

    fn read_byte(&self, addr: u16) -> u8 {
        <Self as MemoryBus>::read(self, addr)
    }
    fn write_byte(&mut self, addr: u16, value: u8) {
        <Self as MemoryBus>::write(self, addr, value);
    }
    fn apply_snapshot(&mut self, snap: &Snapshot) {
        SpectrumPlus::apply_snapshot(self, snap);
    }
    fn keyboard_rows(&self) -> &[u8; 8] {
        SpectrumPlus::keyboard(self).rows()
    }
    fn tape_is_loaded(&self) -> bool {
        SpectrumPlus::tape_is_loaded(self)
    }
    fn tape_is_playing(&self) -> bool {
        SpectrumPlus::tape_is_playing(self)
    }
    fn half_cycle_in_frame(&self) -> u32 {
        SpectrumPlus::hc(self)
    }
    fn tstate_in_frame(&self) -> u32 {
        SpectrumPlus::tstate_in_frame(self)
    }

    fn variant_query_paths() -> &'static [&'static str] {
        SPECTRUM_PROPER_PLUS_QUERY_PATHS
    }

    fn resolve_variant_query(&self, path: &str) -> Result<Option<QueryResult>, QueryError> {
        if COMMON_BOOT_PATHS.contains(&path) {
            return resolve_boot_path(self, SPECTRUM_PROPER_PLUS_BANNERS, path);
        }
        let value = match path {
            "spectrum.machine.issue" => json!(ula_revision_name(self.revision())),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }

    fn start_memory_write_watch(&mut self, addr: u16, len: u16) -> Result<(), &'static str> {
        SpectrumPlus::start_memory_write_watch(self, addr, len);
        Ok(())
    }
    fn stop_memory_write_watch(&mut self) {
        SpectrumPlus::stop_memory_write_watch(self);
    }
    fn memory_write_watch_records(
        &self,
    ) -> Option<&[common_sinclair_zx_spectrum::MemoryWriteRecord]> {
        SpectrumPlus::memory_write_watch_records(self)
    }
    fn memory_write_watch_range(&self) -> Option<(u16, u16)> {
        SpectrumPlus::memory_write_watch_range(self)
    }
    fn clear_memory_write_watch_records(&mut self) {
        SpectrumPlus::clear_memory_write_watch_records(self);
    }
    fn z80_registers(&self) -> &zilog_z80::Registers {
        &self.z80().regs
    }
    fn z80_halted(&self) -> bool {
        self.z80().halt
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
    fn audio_controls(&self) -> AudioControls {
        Spectrum128K::audio_controls(self)
    }
    fn set_audio_controls(&mut self, controls: AudioControls) {
        Spectrum128K::set_audio_controls(self, controls);
    }
    fn set_audio_channel_enabled(&mut self, channel: SpeakerChannel, enabled: bool) {
        Spectrum128K::set_audio_channel_enabled(self, channel, enabled);
    }
    fn set_audio_channel_gain(&mut self, channel: SpeakerChannel, gain: f32) {
        Spectrum128K::set_audio_channel_gain(self, channel, gain);
    }
    fn set_keyboard_rows(&mut self, rows: &[u8; 8]) {
        self.keyboard = *rows;
    }
    fn set_kempston_button(&mut self, button: u8, pressed: bool) -> bool {
        apply_kempston_button_index(self.kempston_mut(), button, pressed)
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
    fn after_restore(&mut self) {
        self.restore_volatile_refs();
    }

    fn apply_snapshot(&mut self, snap: &Snapshot) {
        Spectrum128K::apply_snapshot(self, snap);
    }

    fn read_byte(&self, addr: u16) -> u8 {
        self.memory.read(addr)
    }
    fn write_byte(&mut self, addr: u16, value: u8) {
        self.memory.write(addr, value);
    }
    /// 128K keeps the standard glyph table in ROM 1 (48 BASIC). After
    /// boot ROM 0 (the 128 BASIC editor) is mapped at $0000-$3FFF, so
    /// the default `read_byte($3D00 + offset)` would hit the editor.
    /// Read ROM 1 directly via the paging-aware accessor.
    fn glyph_byte(&self, offset: u16) -> u8 {
        self.memory.read_rom_byte(1, 0x3D00u16.wrapping_add(offset))
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
        SPECTRUM_128K_QUERY_PATHS
    }

    fn resolve_variant_query(&self, path: &str) -> Result<Option<QueryResult>, QueryError> {
        if COMMON_BOOT_PATHS.contains(&path) {
            return resolve_boot_path(self, SPECTRUM_128K_BANNERS, path);
        }
        if AY_QUERY_PATHS.contains(&path) {
            return resolve_ay_path(&self.ay, path);
        }
        Ok(None)
    }

    fn start_memory_write_watch(&mut self, addr: u16, len: u16) -> Result<(), &'static str> {
        Spectrum128K::start_memory_write_watch(self, addr, len);
        Ok(())
    }
    fn stop_memory_write_watch(&mut self) {
        Spectrum128K::stop_memory_write_watch(self);
    }
    fn memory_write_watch_records(
        &self,
    ) -> Option<&[common_sinclair_zx_spectrum::MemoryWriteRecord]> {
        Spectrum128K::memory_write_watch_records(self)
    }
    fn memory_write_watch_range(&self) -> Option<(u16, u16)> {
        Spectrum128K::memory_write_watch_range(self)
    }
    fn clear_memory_write_watch_records(&mut self) {
        Spectrum128K::clear_memory_write_watch_records(self);
    }
    fn z80_registers(&self) -> &zilog_z80::Registers {
        &self.z80.regs
    }
    fn z80_halted(&self) -> bool {
        self.z80.halt
    }
}

impl SpectrumMachine for SpectrumPlus2 {
    const FRAME_WIDTH: u32 = SCREEN_WIDTH as u32;
    const FRAME_HEIGHT: u32 = SCREEN_HEIGHT as u32;

    fn frame_halfcycles(&self) -> u32 {
        TIMING_128K.halfcycles_per_frame
    }
    fn run_frame(&mut self) {
        SpectrumPlus2::run_frame(self);
    }
    fn framebuffer(&self) -> &[u8] {
        &self.framebuffer
    }
    fn audio_frame(&self) -> &[f32] {
        &self.audio_frame
    }
    fn audio_controls(&self) -> AudioControls {
        SpectrumPlus2::audio_controls(self)
    }
    fn set_audio_controls(&mut self, controls: AudioControls) {
        SpectrumPlus2::set_audio_controls(self, controls);
    }
    fn set_audio_channel_enabled(&mut self, channel: SpeakerChannel, enabled: bool) {
        SpectrumPlus2::set_audio_channel_enabled(self, channel, enabled);
    }
    fn set_audio_channel_gain(&mut self, channel: SpeakerChannel, gain: f32) {
        SpectrumPlus2::set_audio_channel_gain(self, channel, gain);
    }
    fn set_keyboard_rows(&mut self, rows: &[u8; 8]) {
        self.keyboard = *rows;
    }
    fn set_kempston_button(&mut self, button: u8, pressed: bool) -> bool {
        apply_kempston_button_index(self.kempston_mut(), button, pressed)
    }
    fn load_tape_blocks(&mut self, blocks: Vec<TapeBlock>) {
        SpectrumPlus2::load_tape_blocks(self, blocks);
    }
    fn load_tape_stream(&mut self, stream: Vec<TapeSpan>) {
        SpectrumPlus2::load_tape_stream(self, stream);
    }
    fn tape_play(&mut self) {
        SpectrumPlus2::tape_play(self);
    }
    fn tape_stop(&mut self) {
        SpectrumPlus2::tape_stop(self);
    }
    fn reset_machine(&mut self) {
        SpectrumPlus2::reset(self);
    }
    fn after_restore(&mut self) {
        self.restore_volatile_refs();
    }

    fn apply_snapshot(&mut self, snap: &Snapshot) {
        SpectrumPlus2::apply_snapshot(self, snap);
    }

    fn read_byte(&self, addr: u16) -> u8 {
        self.memory.read(addr)
    }
    fn write_byte(&mut self, addr: u16, value: u8) {
        self.memory.write(addr, value);
    }
    /// +2 keeps the standard glyph table in ROM 1 (48 BASIC), same as
    /// the 128K. Read ROM 1 directly via the paging-aware accessor.
    fn glyph_byte(&self, offset: u16) -> u8 {
        self.memory.read_rom_byte(1, 0x3D00u16.wrapping_add(offset))
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
        SPECTRUM_PLUS2_QUERY_PATHS
    }

    fn resolve_variant_query(&self, path: &str) -> Result<Option<QueryResult>, QueryError> {
        if COMMON_BOOT_PATHS.contains(&path) {
            return resolve_boot_path(self, SPECTRUM_PLUS2_BANNERS, path);
        }
        if AY_QUERY_PATHS.contains(&path) {
            return resolve_ay_path(&self.ay, path);
        }
        Ok(None)
    }

    fn start_memory_write_watch(&mut self, addr: u16, len: u16) -> Result<(), &'static str> {
        SpectrumPlus2::start_memory_write_watch(self, addr, len);
        Ok(())
    }
    fn stop_memory_write_watch(&mut self) {
        SpectrumPlus2::stop_memory_write_watch(self);
    }
    fn memory_write_watch_records(
        &self,
    ) -> Option<&[common_sinclair_zx_spectrum::MemoryWriteRecord]> {
        SpectrumPlus2::memory_write_watch_records(self)
    }
    fn memory_write_watch_range(&self) -> Option<(u16, u16)> {
        SpectrumPlus2::memory_write_watch_range(self)
    }
    fn clear_memory_write_watch_records(&mut self) {
        SpectrumPlus2::clear_memory_write_watch_records(self);
    }
    fn z80_registers(&self) -> &zilog_z80::Registers {
        &self.z80.regs
    }
    fn z80_halted(&self) -> bool {
        self.z80.halt
    }
}

// Blanket impl across the Amstrad-class variants — +2A, +2B, +3 share
// every method. Variant-specific behaviour (disk slot acceptance,
// model id) comes from the marker trait's associated consts. This
// keeps `variants.rs` to one impl block instead of three near-identical
// copies.
impl<V: AmstradVariant> SpectrumMachine for SpectrumAmstradClassCore<V> {
    const FRAME_WIDTH: u32 = SCREEN_WIDTH as u32;
    const FRAME_HEIGHT: u32 = SCREEN_HEIGHT as u32;

    fn frame_halfcycles(&self) -> u32 {
        TIMING_PLUS2A.halfcycles_per_frame
    }
    fn run_frame(&mut self) {
        SpectrumAmstradClassCore::<V>::run_frame(self);
    }
    fn framebuffer(&self) -> &[u8] {
        &self.framebuffer
    }
    fn audio_frame(&self) -> &[f32] {
        &self.audio_frame
    }
    fn audio_controls(&self) -> AudioControls {
        SpectrumAmstradClassCore::<V>::audio_controls(self)
    }
    fn set_audio_controls(&mut self, controls: AudioControls) {
        SpectrumAmstradClassCore::<V>::set_audio_controls(self, controls);
    }
    fn set_audio_channel_enabled(&mut self, channel: SpeakerChannel, enabled: bool) {
        SpectrumAmstradClassCore::<V>::set_audio_channel_enabled(self, channel, enabled);
    }
    fn set_audio_channel_gain(&mut self, channel: SpeakerChannel, gain: f32) {
        SpectrumAmstradClassCore::<V>::set_audio_channel_gain(self, channel, gain);
    }
    fn set_keyboard_rows(&mut self, rows: &[u8; 8]) {
        self.keyboard = *rows;
    }
    fn load_tape_blocks(&mut self, blocks: Vec<TapeBlock>) {
        SpectrumAmstradClassCore::<V>::load_tape_blocks(self, blocks);
    }
    fn load_tape_stream(&mut self, stream: Vec<TapeSpan>) {
        SpectrumAmstradClassCore::<V>::load_tape_stream(self, stream);
    }
    fn tape_play(&mut self) {
        SpectrumAmstradClassCore::<V>::tape_play(self);
    }
    fn tape_stop(&mut self) {
        SpectrumAmstradClassCore::<V>::tape_stop(self);
    }
    fn reset_machine(&mut self) {
        SpectrumAmstradClassCore::<V>::reset(self);
    }
    fn after_restore(&mut self) {
        self.restore_volatile_refs();
    }

    fn apply_snapshot(&mut self, snap: &Snapshot) {
        SpectrumAmstradClassCore::<V>::apply_snapshot(self, snap);
    }

    fn supports_disk_slot(&self, slot: &str) -> bool {
        V::HAS_DISK_SLOT && slot == "disk-a"
    }

    fn load_disk_image(&mut self, slot: &str, bytes: &[u8]) -> Result<(), String> {
        if !self.supports_disk_slot(slot) {
            return Err(format!("unsupported disk slot `{slot}`"));
        }
        let image = format_amstrad_dsk::parse(bytes)?;
        // Insert via the FDC field directly — the +3-specific
        // `insert_disk` inherent only exists on Plus3Marker, but the
        // disk-slot guard above proves V::HAS_DISK_SLOT, so the FDC
        // is enabled and accepts the image.
        self.fdc.insert_disk(0, image);
        Ok(())
    }

    fn read_byte(&self, addr: u16) -> u8 {
        self.memory.read(addr)
    }
    fn write_byte(&mut self, addr: u16, value: u8) {
        self.memory.write(addr, value);
    }
    /// +2A/+2B/+3 keep the standard glyph table in ROM 3 (the 48
    /// BASIC sub-ROM). After boot ROM 0 (the +3 editor) is mapped at
    /// $0000-$3FFF, so the default glyph reader would miss. Reach
    /// ROM 3 directly via the paging-aware accessor.
    fn glyph_byte(&self, offset: u16) -> u8 {
        self.memory.read_rom_byte(3, 0x3D00u16.wrapping_add(offset))
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

    fn resolve_variant_query(&self, path: &str) -> Result<Option<QueryResult>, QueryError> {
        if COMMON_BOOT_PATHS.contains(&path) {
            return resolve_boot_path(self, SPECTRUM_PLUS_BANNERS, path);
        }
        if AY_QUERY_PATHS.contains(&path) {
            return resolve_ay_path(&self.ay, path);
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

    fn start_memory_write_watch(&mut self, addr: u16, len: u16) -> Result<(), &'static str> {
        SpectrumAmstradClassCore::<V>::start_memory_write_watch(self, addr, len);
        Ok(())
    }
    fn stop_memory_write_watch(&mut self) {
        SpectrumAmstradClassCore::<V>::stop_memory_write_watch(self);
    }
    fn memory_write_watch_records(
        &self,
    ) -> Option<&[common_sinclair_zx_spectrum::MemoryWriteRecord]> {
        SpectrumAmstradClassCore::<V>::memory_write_watch_records(self)
    }
    fn memory_write_watch_range(&self) -> Option<(u16, u16)> {
        SpectrumAmstradClassCore::<V>::memory_write_watch_range(self)
    }
    fn clear_memory_write_watch_records(&mut self) {
        SpectrumAmstradClassCore::<V>::clear_memory_write_watch_records(self);
    }
    fn z80_registers(&self) -> &zilog_z80::Registers {
        &self.z80.regs
    }
    fn z80_halted(&self) -> bool {
        self.z80.halt
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
    fn audio_controls(&self) -> AudioControls {
        Pentagon128::audio_controls(self)
    }
    fn set_audio_controls(&mut self, controls: AudioControls) {
        Pentagon128::set_audio_controls(self, controls);
    }
    fn set_audio_channel_enabled(&mut self, channel: SpeakerChannel, enabled: bool) {
        Pentagon128::set_audio_channel_enabled(self, channel, enabled);
    }
    fn set_audio_channel_gain(&mut self, channel: SpeakerChannel, gain: f32) {
        Pentagon128::set_audio_channel_gain(self, channel, gain);
    }
    fn set_keyboard_rows(&mut self, rows: &[u8; 8]) {
        self.keyboard = *rows;
    }
    fn set_kempston_button(&mut self, button: u8, pressed: bool) -> bool {
        apply_kempston_button_index(&mut self.kempston, button, pressed)
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
    fn after_restore(&mut self) {
        self.z80.rehydrate_walker_sequence();
    }

    fn apply_snapshot(&mut self, snap: &Snapshot) {
        Pentagon128::apply_snapshot(self, snap);
    }

    fn read_byte(&self, addr: u16) -> u8 {
        self.memory.read(addr)
    }
    fn write_byte(&mut self, addr: u16, value: u8) {
        self.memory.write(addr, value);
    }
    /// Pentagon is a 128K-derivative; its ROM 1 (48 BASIC) carries
    /// the standard glyph table. After boot ROM 0 is mapped at
    /// $0000-$3FFF, so we reach ROM 1 directly.
    fn glyph_byte(&self, offset: u16) -> u8 {
        self.memory.read_rom_byte(1, 0x3D00u16.wrapping_add(offset))
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

    fn z80_registers(&self) -> &zilog_z80::Registers {
        &self.z80.regs
    }
    fn z80_halted(&self) -> bool {
        self.z80.halt
    }

    fn variant_query_paths() -> &'static [&'static str] {
        PENTAGON_128_QUERY_PATHS
    }

    fn resolve_variant_query(&self, path: &str) -> Result<Option<QueryResult>, QueryError> {
        if COMMON_BOOT_PATHS.contains(&path) {
            return resolve_boot_path(self, PENTAGON_128_BANNERS, path);
        }
        if AY_QUERY_PATHS.contains(&path) {
            return resolve_ay_path(&self.ay, path);
        }
        let value = match path {
            "spectrum.kempston.state" => json!(self.kempston.state),
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
    fn audio_controls(&self) -> AudioControls {
        ScorpionZS256::audio_controls(self)
    }
    fn set_audio_controls(&mut self, controls: AudioControls) {
        ScorpionZS256::set_audio_controls(self, controls);
    }
    fn set_audio_channel_enabled(&mut self, channel: SpeakerChannel, enabled: bool) {
        ScorpionZS256::set_audio_channel_enabled(self, channel, enabled);
    }
    fn set_audio_channel_gain(&mut self, channel: SpeakerChannel, gain: f32) {
        ScorpionZS256::set_audio_channel_gain(self, channel, gain);
    }
    fn set_keyboard_rows(&mut self, rows: &[u8; 8]) {
        self.keyboard = *rows;
    }
    fn set_kempston_button(&mut self, button: u8, pressed: bool) -> bool {
        apply_kempston_button_index(&mut self.kempston, button, pressed)
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
    fn after_restore(&mut self) {
        self.z80.rehydrate_walker_sequence();
    }

    fn apply_snapshot(&mut self, snap: &Snapshot) {
        ScorpionZS256::apply_snapshot(self, snap);
    }

    fn read_byte(&self, addr: u16) -> u8 {
        self.memory.read(addr)
    }
    fn write_byte(&mut self, addr: u16, value: u8) {
        self.memory.write(addr, value);
    }
    /// Scorpion is 128K-derivative — ROM 1 (48 BASIC) holds the
    /// standard glyph table. ROMs 2 and 3 are TR-DOS / Service ROM.
    fn glyph_byte(&self, offset: u16) -> u8 {
        self.memory.read_rom_byte(1, 0x3D00u16.wrapping_add(offset))
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

    fn z80_registers(&self) -> &zilog_z80::Registers {
        &self.z80.regs
    }
    fn z80_halted(&self) -> bool {
        self.z80.halt
    }

    fn variant_query_paths() -> &'static [&'static str] {
        SCORPION_ZS256_QUERY_PATHS
    }

    fn resolve_variant_query(&self, path: &str) -> Result<Option<QueryResult>, QueryError> {
        if COMMON_BOOT_PATHS.contains(&path) {
            return resolve_boot_path(self, SCORPION_ZS256_BANNERS, path);
        }
        if AY_QUERY_PATHS.contains(&path) {
            return resolve_ay_path(&self.ay, path);
        }
        let value = match path {
            "spectrum.kempston.state" => json!(self.kempston.state),
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
    fn audio_controls(&self) -> AudioControls {
        TimexTC2048::audio_controls(self)
    }
    fn set_audio_controls(&mut self, controls: AudioControls) {
        TimexTC2048::set_audio_controls(self, controls);
    }
    fn set_audio_channel_enabled(&mut self, channel: SpeakerChannel, enabled: bool) {
        TimexTC2048::set_audio_channel_enabled(self, channel, enabled);
    }
    fn set_audio_channel_gain(&mut self, channel: SpeakerChannel, gain: f32) {
        TimexTC2048::set_audio_channel_gain(self, channel, gain);
    }
    fn set_keyboard_rows(&mut self, rows: &[u8; 8]) {
        self.keyboard = *rows;
    }
    fn set_kempston_button(&mut self, button: u8, pressed: bool) -> bool {
        apply_kempston_button_index(&mut self.kempston, button, pressed)
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
    fn after_restore(&mut self) {
        self.z80.rehydrate_walker_sequence();
    }

    fn apply_snapshot(&mut self, snap: &Snapshot) {
        TimexTC2048::apply_snapshot(self, snap);
    }

    fn read_byte(&self, addr: u16) -> u8 {
        self.memory.read(addr)
    }
    fn write_byte(&mut self, addr: u16, value: u8) {
        self.memory.write(addr, value);
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

    fn z80_registers(&self) -> &zilog_z80::Registers {
        &self.z80.regs
    }
    fn z80_halted(&self) -> bool {
        self.z80.halt
    }

    fn variant_query_paths() -> &'static [&'static str] {
        TIMEX_TC2048_QUERY_PATHS
    }

    fn resolve_variant_query(&self, path: &str) -> Result<Option<QueryResult>, QueryError> {
        if COMMON_BOOT_PATHS.contains(&path) {
            return resolve_boot_path(self, TIMEX_TC2048_BANNERS, path);
        }
        let value = match path {
            "spectrum.kempston.state" => json!(self.kempston.state),
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
    fn audio_controls(&self) -> AudioControls {
        TimexTS2068::audio_controls(self)
    }
    fn set_audio_controls(&mut self, controls: AudioControls) {
        TimexTS2068::set_audio_controls(self, controls);
    }
    fn set_audio_channel_enabled(&mut self, channel: SpeakerChannel, enabled: bool) {
        TimexTS2068::set_audio_channel_enabled(self, channel, enabled);
    }
    fn set_audio_channel_gain(&mut self, channel: SpeakerChannel, gain: f32) {
        TimexTS2068::set_audio_channel_gain(self, channel, gain);
    }
    fn set_keyboard_rows(&mut self, rows: &[u8; 8]) {
        self.keyboard = *rows;
    }
    fn set_kempston_button(&mut self, button: u8, pressed: bool) -> bool {
        apply_kempston_button_index(&mut self.kempston, button, pressed)
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
    fn after_restore(&mut self) {
        self.z80.rehydrate_walker_sequence();
    }

    fn apply_snapshot(&mut self, snap: &Snapshot) {
        TimexTS2068::apply_snapshot(self, snap);
    }

    fn read_byte(&self, addr: u16) -> u8 {
        self.memory.read(addr)
    }
    fn write_byte(&mut self, addr: u16, value: u8) {
        self.memory.write(addr, value);
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

    fn z80_registers(&self) -> &zilog_z80::Registers {
        &self.z80.regs
    }
    fn z80_halted(&self) -> bool {
        self.z80.halt
    }

    fn variant_query_paths() -> &'static [&'static str] {
        TIMEX_TS2068_QUERY_PATHS
    }

    fn resolve_variant_query(&self, path: &str) -> Result<Option<QueryResult>, QueryError> {
        if COMMON_BOOT_PATHS.contains(&path) {
            return resolve_boot_path(self, TIMEX_TS2068_BANNERS, path);
        }
        if AY_QUERY_PATHS.contains(&path) {
            return resolve_ay_path(&self.ay, path);
        }
        let value = match path {
            "spectrum.kempston.state" => json!(self.kempston.state),
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
