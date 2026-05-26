//! `AmigaA1200Session` — the family-MCP session that hosts an
//! `AmigaRuntimeKind::Aga(AmigaRuntime<AmigaA1200>)` plus the
//! debug-recorder surface the chip-level tools depend on.
//!
//! Migrated 2026-05-26 from a hand-rolled struct that held an
//! `AmigaA1200` directly. The shell's `HeadlessSession<M: MachineCore,
//! Q: SessionQueryProvider>` couldn't host A1200 before because A1200
//! didn't impl `AmigaMachine`; that landed in commit `3e33137` and
//! this commit moves the MCP server onto the family runtime.
//!
//! Per [`knowledge/decisions/amiga-machine-catalogue.md`], the
//! `AmigaLiveAccess` trait (the OCS/ECS/AGA-generic chip-level
//! accessors) is deferred to a follow-up commit. The 33 chip-level
//! MCP tools today still target the A1200 directly — they downcast
//! through [`AmigaA1200Session::machine_mut`], which panics if the
//! active kind variant isn't `Aga` (single-machine MCP guarantees
//! this for now). The downcast surfaces as the same `&mut AmigaA1200`
//! the previous session shape exposed; tool bodies migrate
//! mechanically (`s.machine.X` → `s.machine_mut().X`).
//!
//! [`knowledge/decisions/amiga-machine-catalogue.md`]: ../../../../knowledge/decisions/amiga-machine-catalogue.md

use std::path::PathBuf;

use emu198x_shell::{MachineError, VideoRecorder};
use machine_commodore_amiga_a1200::AmigaA1200;
use runtime_commodore_amiga::{AmigaLiveAccess, AmigaRuntimeKind, Model};

/// MCP server session — owns the family runtime kind and the
/// hand-rolled video-recorder state the chip-level tools use.
///
/// The `kind` field is always an `AmigaRuntimeKind::Aga(...)` while
/// the MCP is single-machine. When `set_machine` lands in Phase 3b,
/// the variant may swap to `Ocs` / `Ecs`, at which point the
/// chip-level tools that downcast through [`Self::machine_mut`]
/// will need their `AmigaLiveAccess`-trait replacements.
pub struct AmigaA1200Session {
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
}

impl AmigaA1200Session {
    /// Build a session from a ROM image already loaded into memory.
    /// Constructs the underlying `AmigaRuntime<AmigaA1200>` via
    /// `AmigaRuntimeKind::new(Model::A1200AgaPal, ...)` so the MCP
    /// session sees the same family-runtime surface a script-mode
    /// boot would.
    ///
    /// # Errors
    /// Returns `MachineError::InvalidFirmware` if the ROM size is
    /// wrong for a Kickstart 3.0/3.1 image (must be 512 KiB).
    pub fn new(rom_bytes: Vec<u8>, rom_path: PathBuf) -> Result<Self, MachineError> {
        let kind = AmigaRuntimeKind::new(Model::A1200AgaPal, rom_bytes)?;
        Ok(Self {
            kind,
            rom_path,
            recorder: None,
            last_recorded_tick: 0,
        })
    }

    /// Borrow the active A1200 chip stack. Panics if the kind
    /// variant isn't `Aga` — every MCP boot path constructs an
    /// `Aga` variant today, so this is an internal-invariant check.
    /// When OCS / ECS MCP support lands, callers gain a `match` on
    /// `&self.kind` and the downcast helpers stop being load-bearing.
    #[must_use]
    pub fn machine(&self) -> &AmigaA1200 {
        match &self.kind {
            AmigaRuntimeKind::Aga(rt) => rt.machine(),
            _ => panic!(
                "AmigaA1200Session::machine: active kind variant is not Aga \
                 (single-machine MCP invariant violated)"
            ),
        }
    }

    /// Mutable borrow of the active A1200 chip stack. Same downcast
    /// invariant as [`Self::machine`].
    #[must_use]
    pub fn machine_mut(&mut self) -> &mut AmigaA1200 {
        match &mut self.kind {
            AmigaRuntimeKind::Aga(rt) => rt.machine_mut(),
            _ => panic!(
                "AmigaA1200Session::machine_mut: active kind variant is not Aga \
                 (single-machine MCP invariant violated)"
            ),
        }
    }

    /// Chipset-agnostic chip-level access. Returns the active runtime
    /// kind under its [`AmigaLiveAccess`] impl — tool bodies that
    /// don't care which chipset is active call through this instead
    /// of the A1200 downcast. The AGA-specific tools (palette banks,
    /// AGA copper list dump) still go through [`Self::machine`] /
    /// [`Self::machine_mut`].
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
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use runtime_commodore_amiga::AmigaRuntimeKind;

    /// Session built from a blank Kickstart-sized ROM carries the
    /// Aga variant and exposes the A1200 chip stack through
    /// `machine()`/`machine_mut()`.
    #[test]
    fn session_new_dispatches_to_aga_variant() {
        // 512 KiB zero-filled ROM — passes validate_firmware_rom for
        // A1200 (KS 3.0/3.1 is 512 KiB).
        let rom = vec![0u8; 512 * 1024];
        let session = AmigaA1200Session::new(rom, PathBuf::from("/tmp/test.rom"))
            .expect("blank Kickstart-sized ROM should build");
        assert!(matches!(session.kind, AmigaRuntimeKind::Aga(_)));
        assert_eq!(session.last_recorded_tick, 0);
        assert!(session.recorder.is_none());
    }

    #[test]
    fn session_machine_accessors_return_a1200() {
        let rom = vec![0u8; 512 * 1024];
        let mut session = AmigaA1200Session::new(rom, PathBuf::from("/tmp/test.rom"))
            .expect("blank Kickstart-sized ROM should build");
        // PC starts at 0 in a freshly-built A1200; the read is just
        // checking the downcast doesn't panic.
        let _pc = session.machine().cpu().regs.pc;
        let _tick = session.machine_mut().tick_count();
    }
}
