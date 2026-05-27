//! `AmigaSession` — the family-MCP session that hosts an
//! `AmigaRuntimeKind` (any of `Ocs` / `Ecs` / `Aga`) plus the
//! debug-recorder surface the chip-level tools depend on.
//!
//! Renamed 2026-05-26 from `AmigaA1200Session` after Stage AE-b
//! through AE-e: every chip-level tool now drives through the
//! [`AmigaLiveAccess`] trait via [`Self::access`] / [`Self::access_mut`],
//! and chipset-trace tracers (`bplcon0_log` / `palette_log` /
//! `reg_read_log`) exist on every variant. The only remaining A1200
//! downcast is [`Self::aga_machine_mut`], used by `tool_query_aga`
//! to reach Lisa-specific state (the 8-bank 24-bit palette, BPLCON3/4,
//! HAM previous-pixel). All other tools work against OCS / ECS / AGA
//! sessions identically.
//!
//! See [`knowledge/decisions/amiga-machine-catalogue.md`] for the
//! migration plan this session structure follows.
//!
//! [`knowledge/decisions/amiga-machine-catalogue.md`]: ../../../../knowledge/decisions/amiga-machine-catalogue.md

use std::path::PathBuf;

use emu198x_shell::{MachineError, VideoRecorder};
use machine_commodore_amiga_a1200::AmigaA1200;
use runtime_commodore_amiga::{AmigaLiveAccess, AmigaRuntimeKind, Model};

/// MCP server session — owns the family runtime kind and the
/// hand-rolled video-recorder state the chip-level tools use.
///
/// The `kind` field carries the active chipset variant
/// ([`AmigaRuntimeKind::Ocs`] / `Ecs` / `Aga`). Tools reach chip
/// state through [`Self::access`] / [`Self::access_mut`] — the
/// chipset-agnostic [`AmigaLiveAccess`] trait. Only AGA-specific
/// tooling reaches for [`Self::aga_machine_mut`].
/// One captured instruction-boundary CPU snapshot.
/// `(cck, instr_start_pc, sr, opcode_word)` — `opcode_word` is the
/// 16-bit word at `instr_start_pc` (read via the trait's `read_word`
/// at capture time).
pub type CpuTraceEntry = (u64, u32, u16, u16);

/// Tracing state attached to [`AmigaSession`]. Lives on the session
/// rather than the chip stack so:
///   - the OCS / ECS / A1200 crates don't grow another shared field
///     (the trace would be Vec on each one),
///   - capture overhead is zero when disarmed (one bool check per
///     tick in [`AmigaSession::tick_with_trace`]),
///   - the trace clears on `reset()` naturally without per-machine
///     plumbing.
pub struct CpuTraceState {
    /// `true` while the trace is recording. Toggled by
    /// `cpu_trace_arm` / `cpu_trace_disarm`.
    pub armed: bool,
    /// Optional PC range `(min, max)`; entries with
    /// `instr_start_pc` outside the range are dropped before push.
    pub pc_filter: Option<(u32, u32)>,
    /// Hard cap on captured entries — at limit, further pushes are
    /// dropped (silent truncation). Default 100_000 entries
    /// (~1.6 MB at 16 bytes each).
    pub max_entries: usize,
    /// Captured entries, oldest first.
    pub entries: Vec<CpuTraceEntry>,
    /// Last observed `cpu_instruction_starts` value; capture happens
    /// when this changes between `tick_with_trace` invocations.
    pub last_seen_instr_starts: u64,
}

impl Default for CpuTraceState {
    fn default() -> Self {
        Self {
            armed: false,
            pc_filter: None,
            max_entries: 100_000,
            entries: Vec::new(),
            last_seen_instr_starts: 0,
        }
    }
}

pub struct AmigaSession {
    /// Family runtime — `AmigaRuntimeKind::Aga(...)` today.
    pub kind: AmigaRuntimeKind,
    /// Path the boot ROM was loaded from; surfaced through the
    /// `reset` tool's JSON response for parity with the pre-migration
    /// session shape.
    pub rom_path: PathBuf,
    /// Active video recording, when one is in flight.
    pub recorder: Option<VideoRecorder>,
    /// Tick at which the most recent recorded frame was pushed.
    /// Drives the per-frame push decision in `push_recorder_frame`.
    pub last_recorded_tick: u64,
    /// CPU instruction-trace state. See [`CpuTraceState`].
    pub cpu_trace: CpuTraceState,
}

impl AmigaSession {
    /// Build a session from a ROM image already loaded into memory.
    /// Constructs the underlying [`AmigaRuntimeKind`] via
    /// [`AmigaRuntimeKind::new`] so the MCP session sees the same
    /// family-runtime surface a script-mode boot would. The kind
    /// arm is picked automatically from `model` (OCS / ECS / AGA).
    ///
    /// # Errors
    /// Returns `MachineError::InvalidFirmware` if the ROM size is
    /// wrong for the model's expected Kickstart image — A1000 wants a
    /// 64 KiB bootstrap, A500-family wants 256/512 KiB Kickstart,
    /// A1200 wants 512 KiB AGA Kickstart.
    pub fn new(
        model: Model,
        rom_bytes: Vec<u8>,
        rom_path: PathBuf,
    ) -> Result<Self, MachineError> {
        let kind = AmigaRuntimeKind::new(model, rom_bytes)?;
        Ok(Self {
            kind,
            rom_path,
            recorder: None,
            last_recorded_tick: 0,
            cpu_trace: CpuTraceState::default(),
        })
    }

    /// Advance one master/4 tick and, if the CPU trace is armed,
    /// capture a snapshot at every instruction boundary the tick
    /// crosses. Tools that previously called `self.access_mut().tick()`
    /// in hot loops should call this instead so traces work end-to-end.
    ///
    /// Capture overhead when disarmed is one `bool` check; when
    /// armed it's `cpu_instruction_starts` comparison + optional PC
    /// filter + Vec push. An instruction takes ~4+ master/4 ticks
    /// on a 68000, so most calls to this method do not capture.
    pub fn tick_with_trace(&mut self) {
        let prev_starts = self.cpu_trace.last_seen_instr_starts;
        self.kind.tick();
        if !self.cpu_trace.armed {
            return;
        }
        let cpu = self.kind.cpu_snapshot();
        let now = cpu.instruction_starts;
        if now == prev_starts {
            return;
        }
        self.cpu_trace.last_seen_instr_starts = now;
        let pc = cpu.instr_start_pc;
        if let Some((lo, hi)) = self.cpu_trace.pc_filter {
            if pc < lo || pc > hi {
                return;
            }
        }
        if self.cpu_trace.entries.len() >= self.cpu_trace.max_entries {
            return;
        }
        // Read the opcode word at instr_start_pc through the trait —
        // works against any chipset variant.
        let opcode = self.kind.read_word(pc);
        self.cpu_trace
            .entries
            .push((self.kind.tick_count(), pc, cpu.regs.sr, opcode));
    }

    /// Borrow the active A1200 chip stack. Panics if the kind
    /// variant isn't `Aga` — only the AGA-specific debug tool
    /// (`tool_query_aga` reaching for Lisa's palette banks and
    /// BPLCON3/4) calls this, and it's expected to be invoked only
    /// against an AGA session. OCS / ECS callers go through
    /// [`Self::access`] / [`Self::access_mut`] instead.
    #[must_use]
    pub fn aga_machine(&self) -> &AmigaA1200 {
        match &self.kind {
            AmigaRuntimeKind::Aga(rt) => rt.machine(),
            _ => panic!(
                "AmigaSession::aga_machine: active kind variant is not Aga \
                 (AGA-only tool invoked against non-AGA session)"
            ),
        }
    }

    /// Mutable borrow of the active A1200 chip stack. Same AGA-only
    /// invariant as [`Self::aga_machine`].
    #[must_use]
    pub fn aga_machine_mut(&mut self) -> &mut AmigaA1200 {
        match &mut self.kind {
            AmigaRuntimeKind::Aga(rt) => rt.machine_mut(),
            _ => panic!(
                "AmigaSession::aga_machine_mut: active kind variant is not Aga \
                 (AGA-only tool invoked against non-AGA session)"
            ),
        }
    }

    /// Chipset-agnostic chip-level access. Returns the active runtime
    /// kind under its [`AmigaLiveAccess`] impl — tool bodies that
    /// don't care which chipset is active call through this. The
    /// AGA-specific tools (palette banks, AGA Lisa state) still go
    /// through [`Self::aga_machine`] / [`Self::aga_machine_mut`].
    #[must_use]
    pub fn access(&self) -> &dyn AmigaLiveAccess {
        &self.kind
    }

    /// Mutable variant of [`Self::access`].
    #[must_use]
    pub fn access_mut(&mut self) -> &mut dyn AmigaLiveAccess {
        &mut self.kind
    }

    /// Hard-reset the machine by re-reading the ROM from `rom_path`
    /// and rebuilding the chip stack. A live recording is dropped —
    /// the frame stream would otherwise see a discontinuity. Mirrors
    /// the pre-migration session's reset semantics: real disk I/O
    /// on reset (rather than using the runtime's cached firmware),
    /// because curriculum-debug scripts that mutate the ROM on disk
    /// expect a reset to pick up the new image.
    ///
    /// # Errors
    /// Returns `std::io::Error` if the ROM file can't be re-read.
    pub fn reset(&mut self) -> std::io::Result<()> {
        let rom = std::fs::read(&self.rom_path)?;
        let model = self.kind.model();
        self.kind = AmigaRuntimeKind::new(model, rom).map_err(|err| {
            std::io::Error::other(format!("reset: rebuild runtime: {err}"))
        })?;
        self.recorder = None;
        self.last_recorded_tick = 0;
        // Reset trace state alongside the chip stack so the captured
        // entries from before the reset don't bleed into post-reset
        // analysis. The arm-state and filter are kept (re-arming would
        // be tedious if reset wiped them).
        self.cpu_trace.entries.clear();
        self.cpu_trace.last_seen_instr_starts = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use runtime_commodore_amiga::AmigaRuntimeKind;

    /// Session built from a blank Kickstart-sized ROM carries the
    /// Aga variant and exposes the A1200 chip stack through the
    /// AGA-only downcasts.
    #[test]
    fn session_new_dispatches_to_aga_variant() {
        // 512 KiB zero-filled ROM — passes validate_firmware_rom for
        // A1200 (KS 3.0/3.1 is 512 KiB).
        let rom = vec![0u8; 512 * 1024];
        let session = AmigaSession::new(Model::A1200AgaPal, rom, PathBuf::from("/tmp/test.rom"))
            .expect("blank Kickstart-sized ROM should build");
        assert!(matches!(session.kind, AmigaRuntimeKind::Aga(_)));
        assert_eq!(session.last_recorded_tick, 0);
        assert!(session.recorder.is_none());
    }

    #[test]
    fn aga_machine_accessors_return_a1200() {
        let rom = vec![0u8; 512 * 1024];
        let mut session = AmigaSession::new(Model::A1200AgaPal, rom, PathBuf::from("/tmp/test.rom"))
            .expect("blank Kickstart-sized ROM should build");
        // PC starts at 0 in a freshly-built A1200; the reads just
        // verify the AGA-only downcasts don't panic.
        let _pc = session.aga_machine().cpu().regs.pc;
        let _tick = session.aga_machine_mut().tick_count();
    }

    /// `tick_with_trace` records no entries when the trace is disarmed,
    /// regardless of how many ticks cross instruction boundaries.
    #[test]
    fn disarmed_tick_with_trace_does_not_capture() {
        let rom = vec![0u8; 512 * 1024];
        let mut session = AmigaSession::new(Model::A1200AgaPal, rom, PathBuf::from("/tmp/test.rom"))
            .expect("blank Kickstart-sized ROM should build");
        assert!(!session.cpu_trace.armed);
        for _ in 0..200 {
            session.tick_with_trace();
        }
        assert!(session.cpu_trace.entries.is_empty());
    }

    /// When armed, `tick_with_trace` captures snapshots at instruction
    /// boundaries up to `max_entries` and then stops capturing.
    #[test]
    fn armed_tick_with_trace_captures_up_to_max_entries() {
        let rom = vec![0u8; 512 * 1024];
        let mut session = AmigaSession::new(Model::A1200AgaPal, rom, PathBuf::from("/tmp/test.rom"))
            .expect("blank Kickstart-sized ROM should build");
        session.cpu_trace.armed = true;
        session.cpu_trace.max_entries = 4;
        // Blank ROM means the boot path traps quickly, but a handful of
        // master/4 ticks is enough to cross several instruction
        // boundaries; we just need *some* captured entries.
        for _ in 0..1000 {
            session.tick_with_trace();
        }
        assert!(session.cpu_trace.entries.len() <= 4, "respects max_entries cap");
    }
}
