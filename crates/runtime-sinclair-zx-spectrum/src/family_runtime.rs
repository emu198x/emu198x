//! `SpectrumRuntimeKind` — runtime-time dispatch over every Spectrum
//! family variant.
//!
//! Mirrors the `AmigaRuntimeKind` pattern in `runtime-commodore-amiga`:
//! a one-of enum that wraps a concrete `SpectrumRuntime<M>` per variant
//! and implements [`emu198x_shell::MachineCore`] by forwarding to the
//! inner case. Used by family-level MCP sessions that need to swap the
//! active variant at runtime through the `set_machine` script step.
//!
//! ## When to use this vs the concrete runtimes
//!
//! - Use the concrete `Spectrum48kRuntime` / `Spectrum128kRuntime` /
//!   `…` type aliases when the binary is single-machine (script-mode
//!   verifier targeting one variant, the UI binary's eager boot path).
//! - Use [`SpectrumRuntimeKind`] when the binary needs to host any
//!   variant chosen at runtime — today the MCP server, eventually any
//!   harness that drives the `set_machine` step.
//!
//! ## Variant coverage
//!
//! All 13 Spectrum-family variants — the SOLID 8 (16K, 48K, +, 128K, +2,
//! +2A, +2B, +3) plus the five exotics (Pentagon 128, Scorpion ZS-256,
//! Timex TC2048 / TC2068 / TS2068). The exotics share runtime types
//! (TC2068 and TS2068 both wrap `TimexTS2068`) and dispatch arms but
//! each gets its own catalogue identity through the inner runtime's
//! profile.

use emu198x_shell::{
    ControlCommand, FamilyRuntime, FirmwareSet, HostIo, MachineCore, MachineError, MachineProfile,
    MachineTime, MediaSet, QueryError, QueryResult, ResetKind, RunResult, SessionQueryProvider,
};

use crate::queries::SpectrumSessionQueryProvider;
use crate::runtime::{SpectrumMachine, SpectrumRuntime};
use crate::variants::{
    Pentagon128Runtime, ScorpionZS256Runtime, Spectrum16kRuntime, Spectrum48kRuntime,
    Spectrum128kRuntime, SpectrumPlus2ARuntime, SpectrumPlus2BRuntime, SpectrumPlus2Runtime,
    SpectrumPlus3Runtime, SpectrumPlusRuntime, TimexTC2048Runtime, TimexTS2068Runtime,
};
use emu198x_shell::display::Display;

/// Narrow Spectrum-machine surface that family-level helpers
/// (`autoload_basic_tape`, `load_basic_program`) need.
///
/// The helpers used to bind directly to `HeadlessSession<SpectrumRuntime<M>, …>`
/// and reach into `.machine().machine()`. That works for single-machine
/// binaries (script mode, UI) but not for family-MCP where the inner type
/// is [`SpectrumRuntimeKind`]. This trait abstracts over both shapes so
/// one implementation of the helpers covers every binary.
///
/// Methods are grouped into a single trait (rather than split by
/// concern) so a future `impl SpectrumLiveAccess for SpectrumRuntimeKind`
/// stays a single match block per method — easy to scan and easy to
/// keep aligned with the trait surface.
pub trait SpectrumLiveAccess {
    /// `true` while a tape image is loaded in the default tape slot.
    fn tape_is_loaded(&self) -> bool;
    /// Direct chip-RAM byte write. Used by the BASIC loader to install
    /// a tokenised program into the visible address space without
    /// re-running the tape decoder.
    fn write_byte(&mut self, addr: u16, val: u8);
    /// Direct chip-RAM byte read. Used by basic-loader tests to
    /// verify the installed program; not on any production path.
    fn read_byte(&self, addr: u16) -> u8;
    /// `true` while tape transport is active. Used by autoload tests
    /// to confirm the helper started playback before returning.
    fn tape_is_playing(&self) -> bool;
    /// Begin recording CPU writes in the half-open range
    /// `[addr, addr + len)`. Variants that don't implement the tracer
    /// return `Err`. See
    /// [`crate::runtime::SpectrumMachine::start_memory_write_watch`].
    ///
    /// # Errors
    /// Returns the reason string from the inner machine when the
    /// variant doesn't support the tracer.
    fn start_memory_write_watch(&mut self, addr: u16, len: u16) -> Result<(), &'static str>;
    /// Stop the current write watch and drop captured records.
    fn stop_memory_write_watch(&mut self);
    /// Captured CPU writes since the last `start_memory_write_watch`.
    /// `None` means either no watch is configured or the variant
    /// doesn't support the tracer.
    fn memory_write_watch_records(
        &self,
    ) -> Option<&[common_sinclair_zx_spectrum::MemoryWriteRecord]>;
    /// Current watch range as `(addr, len)`, or `None` when no watch
    /// is configured.
    fn memory_write_watch_range(&self) -> Option<(u16, u16)>;
    /// Drop captured write records without removing the watch range.
    fn clear_memory_write_watch_records(&mut self);
    /// Z80 register file. Every Spectrum-family variant carries a Z80
    /// so this is always available.
    fn z80_registers(&self) -> &zilog_z80::Registers;
    /// Whether the Z80 is currently halted.
    fn z80_halted(&self) -> bool;
    /// `true` when the Z80 is at an instruction boundary.
    fn z80_instruction_complete(&self) -> bool;
    /// Run cycles until `n` instructions complete. Returns the total
    /// half-cycles consumed.
    fn step_instructions(&mut self, n: u32) -> u32;
    /// Run cycles until PC reaches `target` or `max_halfcycles` is
    /// exhausted. Returns `(reached, halfcycles, instructions)`.
    fn run_until_pc(&mut self, target: u16, max_halfcycles: u32) -> (bool, u32, u32);
    /// Bus-level Z80 I/O port read.
    fn port_read(&mut self, port: u16) -> u8;
    /// Bus-level Z80 I/O port write.
    fn port_write(&mut self, port: u16, value: u8);
    /// Begin tracing every `OUT ($BFFD), data` write. Variants
    /// without an AY return `Err`.
    ///
    /// # Errors
    ///
    /// Returns the reason string from the inner machine when the
    /// variant doesn't carry an AY-3-8912.
    fn start_ay_write_watch(&mut self) -> Result<(), &'static str>;
    /// Stop the AY tracer.
    fn stop_ay_write_watch(&mut self);
    /// Captured AY writes since the last `start_ay_write_watch`.
    fn ay_write_watch_records(&self) -> Option<&[common_sinclair_zx_spectrum::AyWriteRecord]>;
    /// Drop captured AY records without removing the watch.
    fn clear_ay_write_watch_records(&mut self);
    /// Apply a parsed portable snapshot (`.sna` / `.z80`) to the live
    /// machine. The family-MCP path uses this to share the GUI / script
    /// portable-snapshot loader without enumerating variants at the
    /// call site.
    fn apply_snapshot(&mut self, snap: &common_sinclair_zx_spectrum::snapshot::Snapshot);
}

impl<M: SpectrumMachine> SpectrumLiveAccess for SpectrumRuntime<M> {
    fn tape_is_loaded(&self) -> bool {
        self.machine().tape_is_loaded()
    }

    fn write_byte(&mut self, addr: u16, val: u8) {
        self.machine_mut().write_byte(addr, val);
    }

    fn read_byte(&self, addr: u16) -> u8 {
        self.machine().read_byte(addr)
    }

    fn tape_is_playing(&self) -> bool {
        self.machine().tape_is_playing()
    }

    fn start_memory_write_watch(&mut self, addr: u16, len: u16) -> Result<(), &'static str> {
        self.machine_mut().start_memory_write_watch(addr, len)
    }

    fn stop_memory_write_watch(&mut self) {
        self.machine_mut().stop_memory_write_watch();
    }

    fn memory_write_watch_records(
        &self,
    ) -> Option<&[common_sinclair_zx_spectrum::MemoryWriteRecord]> {
        self.machine().memory_write_watch_records()
    }

    fn memory_write_watch_range(&self) -> Option<(u16, u16)> {
        self.machine().memory_write_watch_range()
    }

    fn clear_memory_write_watch_records(&mut self) {
        self.machine_mut().clear_memory_write_watch_records();
    }

    fn z80_registers(&self) -> &zilog_z80::Registers {
        self.machine().z80_registers()
    }

    fn z80_halted(&self) -> bool {
        self.machine().z80_halted()
    }

    fn z80_instruction_complete(&self) -> bool {
        self.machine().z80_instruction_complete()
    }

    fn step_instructions(&mut self, n: u32) -> u32 {
        self.machine_mut().step_instructions(n)
    }

    fn run_until_pc(&mut self, target: u16, max_halfcycles: u32) -> (bool, u32, u32) {
        self.machine_mut().run_until_pc(target, max_halfcycles)
    }

    fn port_read(&mut self, port: u16) -> u8 {
        self.machine_mut().port_read(port)
    }

    fn port_write(&mut self, port: u16, value: u8) {
        self.machine_mut().port_write(port, value);
    }

    fn start_ay_write_watch(&mut self) -> Result<(), &'static str> {
        self.machine_mut().start_ay_write_watch()
    }

    fn stop_ay_write_watch(&mut self) {
        self.machine_mut().stop_ay_write_watch();
    }

    fn ay_write_watch_records(&self) -> Option<&[common_sinclair_zx_spectrum::AyWriteRecord]> {
        self.machine().ay_write_watch_records()
    }

    fn clear_ay_write_watch_records(&mut self) {
        self.machine_mut().clear_ay_write_watch_records();
    }

    fn apply_snapshot(&mut self, snap: &common_sinclair_zx_spectrum::snapshot::Snapshot) {
        SpectrumMachine::apply_snapshot(self.machine_mut(), snap);
    }
}

/// Runtime-time dispatch over every Spectrum-family variant.
///
/// Constructed by the host (typically the MCP server) — pass a fresh
/// concrete runtime in. Re-construct (don't mutate the variant in
/// place) to swap machines mid-session; the host clears session-side
/// state separately.
pub enum SpectrumRuntimeKind {
    /// ZX Spectrum 16K.
    Spectrum16K(Spectrum16kRuntime),
    /// ZX Spectrum 48K.
    Spectrum48K(Spectrum48kRuntime),
    /// ZX Spectrum+ (electrically identical to 48K; identity is in the
    /// profile).
    SpectrumPlus(SpectrumPlusRuntime),
    /// ZX Spectrum 128K.
    Spectrum128K(Spectrum128kRuntime),
    /// Sinclair-branded Amstrad-built grey +2.
    SpectrumPlus2(SpectrumPlus2Runtime),
    /// ZX Spectrum +2A.
    SpectrumPlus2A(SpectrumPlus2ARuntime),
    /// ZX Spectrum +2B.
    SpectrumPlus2B(SpectrumPlus2BRuntime),
    /// ZX Spectrum +3.
    SpectrumPlus3(SpectrumPlus3Runtime),
    /// Pentagon 128 — Russian clone with no contention and Beta disk.
    Pentagon128(Pentagon128Runtime),
    /// Scorpion ZS-256 — Russian extended Spectrum with 256 KB RAM.
    ScorpionZS256(ScorpionZS256Runtime),
    /// Timex TC2048 — Portuguese 48K-compatible with SCLD hi-res.
    TimexTC2048(TimexTC2048Runtime),
    /// Timex TC2068 (PAL).
    TimexTC2068(TimexTS2068Runtime),
    /// Timex TS2068 (NTSC).
    TimexTS2068(TimexTS2068Runtime),
}

impl SpectrumRuntimeKind {
    /// Master half-cycles per frame for the active variant. Different
    /// Spectrum classes run at different master clocks: the 48K family
    /// at 14 MHz / 69888 hc/frame, the 128K family at 14.16 MHz / 70908,
    /// the +2A/+2B/+3 family at 17.7 MHz / 70908, the Pentagon at
    /// 14.336 MHz / 71680, the Scorpion at 14 MHz / 71680, the TC2048
    /// at the 48K rate, and the TC2068/TS2068 at their SCLD rate.
    /// Framebuffer pixels emitted per second for the active variant.
    ///
    /// Two pixels per T-state across the whole family, so this is the CPU
    /// clock doubled — and the CPU clock is the master crystal over the
    /// variant's divisor, which is where the classes part company. The 48K
    /// runs 14 MHz ÷ 4 for 7.00 MHz; the 128K runs four times the PAL colour
    /// subcarrier ÷ 5 for 7.09 MHz. Close enough to look like rounding, far
    /// enough that the picture is a different shape.
    ///
    /// Taken from the same timing tables as [`Self::frame_halfcycles`] rather
    /// than restated, so a corrected crystal reaches both.
    #[must_use]
    pub fn pixel_clock_hz(&self) -> f64 {
        use common_sinclair_zx_spectrum::timing::{
            TIMING_48K, TIMING_128K, TIMING_PENTAGON, TIMING_PLUS2A, TIMING_SCORPION,
        };
        use machine_timex_ts2068::TIMING_TS2068;
        let timing = match self {
            Self::Spectrum16K(_) | Self::Spectrum48K(_) | Self::SpectrumPlus(_) => &TIMING_48K,
            Self::Spectrum128K(_) | Self::SpectrumPlus2(_) => &TIMING_128K,
            Self::SpectrumPlus2A(_) | Self::SpectrumPlus2B(_) | Self::SpectrumPlus3(_) => {
                &TIMING_PLUS2A
            }
            Self::Pentagon128(_) => &TIMING_PENTAGON,
            Self::ScorpionZS256(_) => &TIMING_SCORPION,
            Self::TimexTC2048(_) | Self::TimexTC2068(_) => &TIMING_48K,
            Self::TimexTS2068(_) => &TIMING_TS2068,
        };
        timing.master_hz as f64 / f64::from(timing.cpu_divisor) * 2.0
    }

    #[must_use]
    pub fn frame_halfcycles(&self) -> u32 {
        use common_sinclair_zx_spectrum::timing::{
            TIMING_48K, TIMING_128K, TIMING_PENTAGON, TIMING_PLUS2A, TIMING_SCORPION,
        };
        use machine_timex_ts2068::TIMING_TS2068;
        match self {
            Self::Spectrum16K(_) | Self::Spectrum48K(_) | Self::SpectrumPlus(_) => {
                TIMING_48K.halfcycles_per_frame
            }
            Self::Spectrum128K(_) | Self::SpectrumPlus2(_) => TIMING_128K.halfcycles_per_frame,
            Self::SpectrumPlus2A(_) | Self::SpectrumPlus2B(_) | Self::SpectrumPlus3(_) => {
                TIMING_PLUS2A.halfcycles_per_frame
            }
            Self::Pentagon128(_) => TIMING_PENTAGON.halfcycles_per_frame,
            Self::ScorpionZS256(_) => TIMING_SCORPION.halfcycles_per_frame,
            Self::TimexTC2048(_) | Self::TimexTC2068(_) => TIMING_48K.halfcycles_per_frame,
            Self::TimexTS2068(_) => TIMING_TS2068.halfcycles_per_frame,
        }
    }

    /// Returns a mutable reference to the inner 48K runtime when this
    /// kind is `Spectrum48K`, otherwise `None`. Used by 48K-only
    /// helpers (`autoload_basic_tape`, `load_basic_program`) on the
    /// family-MCP path so they keep working when the active variant
    /// is 48K and gracefully error otherwise.
    pub fn as_48k_mut(&mut self) -> Option<&mut Spectrum48kRuntime> {
        if let Self::Spectrum48K(rt) = self {
            Some(rt)
        } else {
            None
        }
    }
}

impl FamilyRuntime for SpectrumRuntimeKind {
    type Model = crate::Model;

    /// Build the requested Spectrum variant — SOLID 8 + the five exotics
    /// (Pentagon, Scorpion, Timex TC2048 / TC2068 / TS2068) — by routing
    /// to its concrete `SpectrumRuntime<M>::from_firmware` constructor.
    fn from_firmware(
        model: crate::Model,
        firmware: &FirmwareSet<'_>,
    ) -> Result<Self, MachineError> {
        Ok(match model {
            crate::Model::Spectrum16KPal => {
                Self::Spectrum16K(Spectrum16kRuntime::from_firmware(firmware)?)
            }
            crate::Model::Spectrum48KPal => {
                Self::Spectrum48K(Spectrum48kRuntime::from_firmware(firmware)?)
            }
            crate::Model::SpectrumPlus => {
                Self::SpectrumPlus(SpectrumPlusRuntime::from_firmware(firmware)?)
            }
            crate::Model::Spectrum128KPal => {
                Self::Spectrum128K(Spectrum128kRuntime::from_firmware(firmware)?)
            }
            crate::Model::SpectrumPlus2 => {
                Self::SpectrumPlus2(SpectrumPlus2Runtime::from_firmware(firmware)?)
            }
            crate::Model::SpectrumPlus2A => {
                Self::SpectrumPlus2A(SpectrumPlus2ARuntime::from_firmware(firmware)?)
            }
            crate::Model::SpectrumPlus2B => {
                Self::SpectrumPlus2B(SpectrumPlus2BRuntime::from_firmware(firmware)?)
            }
            crate::Model::SpectrumPlus3 => {
                Self::SpectrumPlus3(SpectrumPlus3Runtime::from_firmware(firmware)?)
            }
            crate::Model::Pentagon128 => {
                Self::Pentagon128(Pentagon128Runtime::from_firmware(firmware)?)
            }
            crate::Model::ScorpionZS256 => {
                Self::ScorpionZS256(ScorpionZS256Runtime::from_firmware(firmware)?)
            }
            crate::Model::TimexTC2048 => {
                Self::TimexTC2048(TimexTC2048Runtime::from_firmware(firmware)?)
            }
            crate::Model::TimexTC2068 => Self::TimexTC2068(TimexTS2068Runtime::from_firmware(
                crate::Model::TimexTC2068,
                firmware,
            )?),
            crate::Model::TimexTS2068 => Self::TimexTS2068(TimexTS2068Runtime::from_firmware(
                crate::Model::TimexTS2068,
                firmware,
            )?),
        })
    }

    fn native_frame_ticks(&self) -> u64 {
        u64::from(self.frame_halfcycles())
    }
}

/// Forwards one method call to the active variant across all 13
/// `SpectrumRuntimeKind` cases, so each `MachineCore` /
/// `SpectrumLiveAccess` / `SessionQueryProvider` body fits on one line.
///
/// **Deliberately a macro — leave it.** This was reviewed against two
/// alternatives and kept on purpose (June 2026, #456):
///
/// - *Explicit 13-arm `match` per forwarder* (the Amiga's style, which
///   reads fine at its 3 variants): here it expands to ~440 lines where
///   12 of every 13 lines are byte-identical — reinstating exactly the
///   duplication this collapses — and makes every future variant a
///   34-site edit instead of one arm. The Spectrum clone space
///   (Pentagon, Scorpion, Didaktik, Timex, Eastern-bloc clones) is the
///   fleet's largest, so "the variant set is closed" is a hope, not a
///   guarantee; one arm keeps that bet free.
/// - *`Box<dyn>` to erase the enum*: rejected in
///   `knowledge/decisions/runtime-internal-shape.md` — it would route
///   the hot per-tick `run_until` through a vtable and the query surface
///   (`variant_query_paths`, a static method) isn't object-safe.
///
/// This is the most benign macro class: private to one file, no
/// recursion, no token munching, and it preserves **static dispatch**.
/// Do not "simplify" it into hand-forwarding or `Box<dyn>`.
macro_rules! match_kind {
    ($self:expr, |$rt:ident| $body:expr) => {
        match $self {
            SpectrumRuntimeKind::Spectrum16K($rt) => $body,
            SpectrumRuntimeKind::Spectrum48K($rt) => $body,
            SpectrumRuntimeKind::SpectrumPlus($rt) => $body,
            SpectrumRuntimeKind::Spectrum128K($rt) => $body,
            SpectrumRuntimeKind::SpectrumPlus2($rt) => $body,
            SpectrumRuntimeKind::SpectrumPlus2A($rt) => $body,
            SpectrumRuntimeKind::SpectrumPlus2B($rt) => $body,
            SpectrumRuntimeKind::SpectrumPlus3($rt) => $body,
            SpectrumRuntimeKind::Pentagon128($rt) => $body,
            SpectrumRuntimeKind::ScorpionZS256($rt) => $body,
            SpectrumRuntimeKind::TimexTC2048($rt) => $body,
            SpectrumRuntimeKind::TimexTC2068($rt) => $body,
            SpectrumRuntimeKind::TimexTS2068($rt) => $body,
        }
    };
}

impl SpectrumRuntimeKind {
    /// Flushes any captured tape `SAVE` on the active variant to `.tap` bytes,
    /// or `None` if nothing was recorded (or the variant does not yet capture).
    #[must_use]
    pub fn flush_tape_image(&self) -> Option<Vec<u8>> {
        match_kind!(self, |rt| rt.flush_tape_image())
    }

    /// Discards any captured tape `SAVE` signal on the active variant.
    pub fn clear_tape_recording(&mut self) {
        match_kind!(self, |rt| rt.clear_tape_recording());
    }
}

impl MachineCore for SpectrumRuntimeKind {
    fn profile(&self) -> &MachineProfile {
        match_kind!(self, |rt| rt.profile())
    }

    fn time(&self) -> MachineTime {
        match_kind!(self, |rt| rt.time())
    }

    fn reset(&mut self, kind: ResetKind) {
        match_kind!(self, |rt| rt.reset(kind))
    }

    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        match_kind!(self, |rt| rt.load_media(media))
    }

    fn eject_media(&mut self, slot: &str) -> Result<(), MachineError> {
        match_kind!(self, |rt| rt.eject_media(slot))
    }

    fn run_until(
        &mut self,
        target: MachineTime,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        match_kind!(self, |rt| rt.run_until(target, host))
    }

    fn snapshot(&self) -> Result<Vec<u8>, MachineError> {
        match_kind!(self, |rt| rt.snapshot())
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        match_kind!(self, |rt| rt.restore(bytes))
    }

    fn command(&mut self, command: &ControlCommand) -> Result<(), MachineError> {
        match_kind!(self, |rt| rt.command(command))
    }

    /// This is the case the runtime argument exists for: switching variant
    /// changes the answer while the framebuffer keeps its dimensions.
    fn display(&self) -> Option<Display> {
        let region = self.profile().region;
        Some(Display::Television {
            region,
            pixel_clock_hz: self.pixel_clock_hz(),
            lines_per_tv_height: emu198x_shell::display::active_lines(region)?,
        })
    }

    fn capabilities(&self) -> emu198x_shell::CapabilitySet {
        match_kind!(self, |rt| rt.capabilities())
    }

    // The Spectrum joins the shared debug tier via `impl DebugPrimitives`
    // (see `debug.rs`), which the shell's blanket impl turns into
    // `DebugTarget`. Always present: the family enum is always backed by a
    // live Z80 machine.
    fn debug_target(&self) -> Option<&dyn emu198x_shell::DebugTarget> {
        Some(self)
    }
    fn debug_target_mut(&mut self) -> Option<&mut dyn emu198x_shell::DebugTarget> {
        Some(self)
    }

    // The Spectrum joins the shared watch tier via `impl WatchTarget` below,
    // which the generic `watch_memory_*` / `watch_ay_*` arms drive. Memory
    // watch is always available; AY watch is advertised family-wide but
    // `start_ay_watch` errors on the AY-less variants (16K/48K/+/TC2048).
    fn watch_target(&self) -> Option<&dyn emu198x_shell::WatchTarget> {
        Some(self)
    }
    fn watch_target_mut(&mut self) -> Option<&mut dyn emu198x_shell::WatchTarget> {
        Some(self)
    }

    fn keyboard_target(&self) -> Option<&dyn emu198x_shell::KeyboardTarget> {
        Some(self)
    }
}

impl emu198x_shell::KeyboardTarget for SpectrumRuntimeKind {
    fn key_name_is_valid(&self, name: &str) -> bool {
        common_sinclair_zx_spectrum::keyboard::SpectrumKey::from_name(name).is_some()
    }

    fn key_names_hint(&self) -> &'static str {
        "A-Z, 0-9, Space, Enter, CapsShift, SymbolShift; compound names \
         Edit, CapsLock, TrueVideo, InvVideo, Up/Down/Left/Right, Graphics, \
         Delete, Break, ExtendMode"
    }

    fn keys_for_char(&self, ch: char) -> Option<Vec<String>> {
        // Uppercase needs CapsShift; the default charset draws letters in
        // upper case, so a lowercase source char presses the bare keycap.
        let (key_name, needs_caps_shift) = match ch {
            'a'..='z' => (ch.to_ascii_uppercase().to_string(), false),
            'A'..='Z' => (ch.to_string(), true),
            '0'..='9' => (ch.to_string(), false),
            ' ' => ("Space".to_owned(), false),
            '\n' => ("Enter".to_owned(), false),
            _ => return None,
        };
        // Skip anything the layout doesn't actually name.
        common_sinclair_zx_spectrum::keyboard::SpectrumKey::from_name(&key_name)?;
        Some(if needs_caps_shift {
            vec!["CapsShift".to_owned(), key_name]
        } else {
            vec![key_name]
        })
    }

    fn key_timing(&self) -> emu198x_shell::KeyTiming {
        emu198x_shell::KeyTiming {
            default_hold_frames: 3,
            max_hold_frames: 600,
            press_settle_frames: 1,
            inter_key_settle_frames: 1,
            repeat_settle_frames: 3,
            default_type_settle_frames: 10,
        }
    }

    fn expand_named_key(&self, name: &str) -> Option<Vec<String>> {
        // The Spectrum's 40-key keyboard has no dedicated cursor/Edit/video
        // keys: those legends printed above the number row are CapsShift
        // chords. Expose them as friendly single names so `press_key("Edit")`
        // stands in for `press_keys ["CapsShift", "1"]`. Mapping per the
        // Spectrum keyboard faceplate (CapsShift + 1-0 = Edit / Caps Lock /
        // True Video / Inv Video / ← ↓ ↑ → / Graphics / Delete).
        let caps = |second: &str| Some(vec!["CapsShift".to_owned(), second.to_owned()]);
        // Normalise: drop whitespace/underscores, lower-case.
        let key: String = name
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '_')
            .flat_map(char::to_lowercase)
            .collect();
        match key.as_str() {
            "edit" => caps("1"),
            "capslock" => caps("2"),
            "truevideo" => caps("3"),
            "invvideo" | "inversevideo" => caps("4"),
            "left" | "arrowleft" | "cursorleft" => caps("5"),
            "down" | "arrowdown" | "cursordown" => caps("6"),
            "up" | "arrowup" | "cursorup" => caps("7"),
            "right" | "arrowright" | "cursorright" => caps("8"),
            "graph" | "graphics" => caps("9"),
            "delete" | "del" | "backspace" => caps("0"),
            "break" => caps("Space"),
            "extend" | "extendmode" => caps("SymbolShift"),
            _ => None,
        }
    }
}

impl emu198x_shell::WatchTarget for SpectrumRuntimeKind {
    fn supports_memory_watch(&self) -> bool {
        true
    }

    fn start_memory_watch(
        &mut self,
        addr: u32,
        len: u32,
    ) -> Result<u32, emu198x_shell::WatchError> {
        let start = u16::try_from(addr).map_err(|_| {
            emu198x_shell::WatchError::Invalid(format!(
                "address ${addr:08X} is outside the Z80 0000-FFFF address space"
            ))
        })?;
        let len_u16 = u16::try_from(len).map_err(|_| {
            emu198x_shell::WatchError::Invalid(format!(
                "`len` {len} exceeds the Z80 64 KiB address space"
            ))
        })?;
        self.start_memory_write_watch(start, len_u16)
            .map_err(|err| emu198x_shell::WatchError::Invalid(err.to_owned()))?;
        Ok(common_sinclair_zx_spectrum::DEFAULT_WATCH_CAP as u32)
    }

    fn clear_memory_watch(&mut self) -> (bool, u32) {
        let captured = self
            .memory_write_watch_records()
            .map_or(0, |r| r.len() as u32);
        let had_watch = self.memory_write_watch_records().is_some();
        self.stop_memory_write_watch();
        (had_watch, captured)
    }

    fn memory_watch_range(&self) -> Option<(u32, u32)> {
        self.memory_write_watch_range()
            .map(|(lo, len)| (u32::from(lo), u32::from(len)))
    }

    fn memory_watch_records(&self) -> Option<Vec<emu198x_shell::WatchMemoryRecord>> {
        self.memory_write_watch_records().map(|records| {
            records
                .iter()
                .map(|r| emu198x_shell::WatchMemoryRecord {
                    pc: u32::from(r.pc),
                    addr: u32::from(r.addr),
                    value: u32::from(r.value),
                    cck: None,
                    size_bytes: 1,
                    source: None,
                })
                .collect()
        })
    }

    fn supports_ay_watch(&self) -> bool {
        true
    }

    fn start_ay_watch(&mut self) -> Result<u32, emu198x_shell::WatchError> {
        self.start_ay_write_watch()
            .map_err(|err| emu198x_shell::WatchError::Invalid(err.to_owned()))?;
        Ok(common_sinclair_zx_spectrum::DEFAULT_AY_WATCH_CAP as u32)
    }

    fn clear_ay_watch(&mut self) -> (bool, u32) {
        let captured = self.ay_write_watch_records().map_or(0, |r| r.len() as u32);
        let had_watch = self.ay_write_watch_records().is_some();
        self.stop_ay_write_watch();
        (had_watch, captured)
    }

    fn ay_watch_records(&self) -> Option<Vec<emu198x_shell::WatchAyRecord>> {
        self.ay_write_watch_records().map(|records| {
            records
                .iter()
                .map(|r| emu198x_shell::WatchAyRecord {
                    pc: u32::from(r.pc),
                    register: r.register,
                    value: r.value,
                })
                .collect()
        })
    }
}

impl SpectrumLiveAccess for SpectrumRuntimeKind {
    fn tape_is_loaded(&self) -> bool {
        match_kind!(self, |rt| rt.tape_is_loaded())
    }

    fn write_byte(&mut self, addr: u16, val: u8) {
        match_kind!(self, |rt| rt.write_byte(addr, val))
    }

    fn read_byte(&self, addr: u16) -> u8 {
        match_kind!(self, |rt| rt.read_byte(addr))
    }

    fn tape_is_playing(&self) -> bool {
        match_kind!(self, |rt| rt.tape_is_playing())
    }

    fn start_memory_write_watch(&mut self, addr: u16, len: u16) -> Result<(), &'static str> {
        match_kind!(self, |rt| rt.start_memory_write_watch(addr, len))
    }

    fn stop_memory_write_watch(&mut self) {
        match_kind!(self, |rt| rt.stop_memory_write_watch())
    }

    fn memory_write_watch_records(
        &self,
    ) -> Option<&[common_sinclair_zx_spectrum::MemoryWriteRecord]> {
        match_kind!(self, |rt| rt.memory_write_watch_records())
    }

    fn memory_write_watch_range(&self) -> Option<(u16, u16)> {
        match_kind!(self, |rt| rt.memory_write_watch_range())
    }

    fn clear_memory_write_watch_records(&mut self) {
        match_kind!(self, |rt| rt.clear_memory_write_watch_records())
    }

    fn z80_registers(&self) -> &zilog_z80::Registers {
        match_kind!(self, |rt| rt.z80_registers())
    }

    fn z80_halted(&self) -> bool {
        match_kind!(self, |rt| rt.z80_halted())
    }

    fn z80_instruction_complete(&self) -> bool {
        match_kind!(self, |rt| rt.z80_instruction_complete())
    }

    fn step_instructions(&mut self, n: u32) -> u32 {
        match_kind!(self, |rt| rt.step_instructions(n))
    }

    fn run_until_pc(&mut self, target: u16, max_halfcycles: u32) -> (bool, u32, u32) {
        match_kind!(self, |rt| rt.run_until_pc(target, max_halfcycles))
    }

    fn port_read(&mut self, port: u16) -> u8 {
        match_kind!(self, |rt| rt.port_read(port))
    }

    fn port_write(&mut self, port: u16, value: u8) {
        match_kind!(self, |rt| rt.port_write(port, value))
    }

    fn start_ay_write_watch(&mut self) -> Result<(), &'static str> {
        match_kind!(self, |rt| rt.start_ay_write_watch())
    }

    fn stop_ay_write_watch(&mut self) {
        match_kind!(self, |rt| rt.stop_ay_write_watch())
    }

    fn ay_write_watch_records(&self) -> Option<&[common_sinclair_zx_spectrum::AyWriteRecord]> {
        match_kind!(self, |rt| rt.ay_write_watch_records())
    }

    fn clear_ay_write_watch_records(&mut self) {
        match_kind!(self, |rt| rt.clear_ay_write_watch_records())
    }

    fn apply_snapshot(&mut self, snap: &common_sinclair_zx_spectrum::snapshot::Snapshot) {
        match_kind!(self, |rt| rt.apply_snapshot(snap))
    }
}

impl SessionQueryProvider<SpectrumRuntimeKind> for SpectrumSessionQueryProvider {
    fn query_paths(&self, runtime: &SpectrumRuntimeKind, prefix: Option<&str>) -> Vec<String> {
        match_kind!(runtime, |rt| self.query_paths(rt, prefix))
    }

    fn query(
        &self,
        runtime: &SpectrumRuntimeKind,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        match_kind!(runtime, |rt| self.query(rt, path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::{ResetKind, SessionQueryProvider};

    #[test]
    fn machine_core_dispatches_to_inner_variants() {
        let mut kind = SpectrumRuntimeKind::Spectrum48K(Spectrum48kRuntime::blank());
        // Sanity: profile id starts with the family slug.
        assert!(kind.profile().profile_id.as_str().contains("48k"));
        // Reset is a no-op-ish call but must not panic across the
        // dispatch boundary.
        kind.reset(ResetKind::Hard);
    }

    #[test]
    fn query_provider_dispatches_to_inner_variants() {
        let kind = SpectrumRuntimeKind::Spectrum48K(Spectrum48kRuntime::blank());
        let provider = SpectrumSessionQueryProvider;
        let paths = provider.query_paths(&kind, Some("tape."));
        assert!(
            paths.iter().any(|p| p.starts_with("tape.")),
            "expected at least one spectrum.tape.* path; got {paths:?}"
        );
    }

    #[test]
    fn step_advances_pc_through_runtime_kind() {
        let mut kind = SpectrumRuntimeKind::Spectrum48K(Spectrum48kRuntime::blank());
        // Blank machine: PC=0, RAM/ROM uninitialised → opcodes are 0xFF
        // (RST $38) which pushes PC and jumps to $0038. Single-step
        // should leave PC != 0.
        let pc_before = kind.z80_registers().pc;
        let halfcycles = kind.step_instructions(1);
        let pc_after = kind.z80_registers().pc;
        assert_ne!(pc_after, pc_before, "step should advance PC");
        assert!(halfcycles > 0, "step should consume cycles");
    }

    #[test]
    fn ay_write_watch_dispatches_through_runtime_kind() {
        // 48K has no AY — start should error.
        let mut k48 = SpectrumRuntimeKind::Spectrum48K(Spectrum48kRuntime::blank());
        assert!(k48.start_ay_write_watch().is_err());
        assert!(k48.ay_write_watch_records().is_none());

        // 128K has an AY — start should succeed, range should be present.
        let mut k128 = SpectrumRuntimeKind::Spectrum128K(Spectrum128kRuntime::blank());
        k128.start_ay_write_watch()
            .expect("128K supports the AY watch");
        assert!(k128.ay_write_watch_records().is_some());
        assert_eq!(
            k128.ay_write_watch_records()
                .expect("128K AY watch active")
                .len(),
            0
        );

        k128.stop_ay_write_watch();
        assert!(k128.ay_write_watch_records().is_none());
    }

    #[test]
    fn port_round_trips_through_runtime_kind() {
        // Port $FE on a 48K writes the border colour in bits 0-2.
        // After port_write(0xFE, 5), spectrum.border.colour should be 5.
        // Using a port_read on $FE returns the keyboard scan, not the
        // border, so we don't round-trip the value through port_read —
        // we only verify both methods reach the inner machine without
        // panic and don't return a fixed sentinel.
        let mut kind = SpectrumRuntimeKind::Spectrum48K(Spectrum48kRuntime::blank());
        let before = kind.port_read(0x00FE);
        kind.port_write(0x00FE, 5);
        let after = kind.port_read(0x00FE);
        // Both reads should produce defined values (not panic). On a
        // blank machine with no keys pressed the high bits are stable
        // and the EAR bit is set/clear deterministically.
        assert_eq!(before, after, "no key press → keyboard scan unchanged");
    }

    #[test]
    fn run_until_pc_returns_within_budget() {
        let mut kind = SpectrumRuntimeKind::Spectrum48K(Spectrum48kRuntime::blank());
        // Target an unlikely PC; budget is tiny so we expect timeout.
        let (reached, _hc, _instr) = kind.run_until_pc(0xCAFE, 1000);
        assert!(!reached, "0xCAFE should not be reached in a 1000-hc budget");
    }

    #[test]
    fn z80_registers_dispatch_through_runtime_kind() {
        let kind = SpectrumRuntimeKind::Spectrum48K(Spectrum48kRuntime::blank());
        // Fresh Z80 has the well-known boot reset state: PC=0, SP=0xFFFF, AF=0xFFFF.
        let regs = kind.z80_registers();
        assert_eq!(regs.pc, 0x0000);
        assert_eq!(regs.sp, 0xFFFF);
        assert_eq!(regs.af, 0xFFFF);
        assert!(!kind.z80_halted());
    }

    #[test]
    fn named_compound_keys_expand_to_capsshift_chords() {
        use emu198x_shell::KeyboardTarget;
        let kind = SpectrumRuntimeKind::Spectrum48K(Spectrum48kRuntime::blank());

        let caps = |s: &str| Some(vec!["CapsShift".to_owned(), s.to_owned()]);
        // The number-row legends, per the Spectrum faceplate.
        assert_eq!(kind.expand_named_key("Edit"), caps("1"));
        assert_eq!(kind.expand_named_key("True Video"), caps("3"));
        assert_eq!(kind.expand_named_key("inv video"), caps("4"));
        assert_eq!(kind.expand_named_key("graphics"), caps("9"));
        assert_eq!(kind.expand_named_key("delete"), caps("0"));
        // Cursor keys (CapsShift + 5-8) and the two non-digit chords.
        assert_eq!(kind.expand_named_key("up"), caps("7"));
        assert_eq!(kind.expand_named_key("ArrowRight"), caps("8"));
        assert_eq!(kind.expand_named_key("Break"), caps("Space"));
        assert_eq!(kind.expand_named_key("Extend Mode"), caps("SymbolShift"));
        // A plain key is not a compound name — it stays a single keystroke.
        assert_eq!(kind.expand_named_key("A"), None);
        assert_eq!(kind.expand_named_key("CapsShift"), None);
    }

    #[test]
    fn memory_write_watch_dispatches_through_runtime_kind() {
        let mut kind = SpectrumRuntimeKind::Spectrum48K(Spectrum48kRuntime::blank());
        assert!(kind.memory_write_watch_range().is_none());
        assert!(kind.memory_write_watch_records().is_none());

        kind.start_memory_write_watch(0x4000, 0x300)
            .expect("48K supports the write watch");
        assert_eq!(kind.memory_write_watch_range(), Some((0x4000, 0x300)));
        assert!(kind.memory_write_watch_records().is_some());

        kind.stop_memory_write_watch();
        assert!(kind.memory_write_watch_range().is_none());
        assert!(kind.memory_write_watch_records().is_none());
    }
}
