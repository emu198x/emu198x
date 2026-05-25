//! `AmigaA1200Session` — the context every MCP tool dispatches against.
//!
//! Holds a live `AmigaA1200` machine plus the boot ROM path it was
//! loaded from (so a `reset` tool can re-load the same image). This
//! is deliberately *not* a `HeadlessSession`: that abstraction is
//! tied to `MachineCore`, which the A1200 doesn't impl directly, and
//! the Stage Q debugging surface wants direct access to chip-level
//! state (CPU regs, copper list, CIA timers, BPLCON0) that the
//! generic shell session doesn't surface.
//!
//! Tools in `tools.rs` borrow `&mut AmigaA1200Session` and reach into
//! `session.machine` for register / memory / tick access.

use std::path::PathBuf;

use emu198x_shell::VideoRecorder;
use machine_commodore_amiga_a1200::AmigaA1200;

/// Live A1200 + the ROM path it was loaded from.
pub struct AmigaA1200Session {
    /// The running machine.
    pub machine: AmigaA1200,
    /// Path the boot ROM was loaded from; used by `reset` to recover
    /// the same state without a separate `load_rom` round-trip.
    pub rom_path: PathBuf,
    /// Active video recording, when one is in flight.
    pub recorder: Option<VideoRecorder>,
    /// Tick at which the current frame began; the run loop pushes a
    /// frame to `recorder` whenever the tick counter crosses a frame
    /// boundary and a recording is active.
    pub last_recorded_tick: u64,
}

impl AmigaA1200Session {
    /// Build a session from a ROM image already loaded into memory.
    /// Caller supplies the ROM bytes (so this can stay sync / file-IO-free).
    #[must_use]
    pub fn new(rom_bytes: Vec<u8>, rom_path: PathBuf) -> Self {
        let machine = AmigaA1200::new(rom_bytes);
        Self {
            machine,
            rom_path,
            recorder: None,
            last_recorded_tick: 0,
        }
    }

    /// Reset the machine by re-loading the ROM from `rom_path` and
    /// rebuilding the A1200. A live recording is dropped — the
    /// recorder's frame stream would otherwise see a discontinuity.
    ///
    /// # Errors
    ///
    /// Returns `std::io::Error` if the ROM file can't be re-read.
    pub fn reset(&mut self) -> std::io::Result<()> {
        let rom = std::fs::read(&self.rom_path)?;
        self.machine = AmigaA1200::new(rom);
        self.recorder = None;
        self.last_recorded_tick = 0;
        Ok(())
    }
}
