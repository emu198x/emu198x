//! Spectrum-family ULA trait.
//!
//! Source references:
//! - `knowledge/systems/spectrum/overview.md`
//! - Adapted from `../Emu198x-Older/crates/common-sinclair-zx-spectrum/src/ula.rs`

use crate::memory::MemoryBus;
use crate::timing::FrameTiming;

/// ULA trait — the heart of each Spectrum variant.
///
/// The ULA ticks twice per CPU T-state. It:
/// - Renders pixels to the framebuffer in real-time
/// - Gates the CPU's clock signal (contention)
/// - Generates the interrupt signal
/// - Tracks the floating bus value
/// - Handles port 0xFE (border, beeper, keyboard, EAR/MIC)
///
/// Each Spectrum variant has a different ULA implementation:
/// Ferranti 6C001E (48K), Sinclair 7K010E (128K), Amstrad 40077 (+2A/+3),
/// Timex SCLD (TC2048/2068), Pentagon ULA, Scorpion ULA.
pub trait Ula {
    /// Advance one CPU half-cycle edge.
    ///
    /// The ULA must be ticked BEFORE the CPU on each half-cycle.
    /// After ticking, the machine loop checks `cpu_clock_active()` to
    /// decide whether to tick the CPU.
    ///
    /// Arguments:
    /// - `memory`: memory bus for screen data fetches
    /// - `cpu_addr`: current CPU address bus value (for contention check;
    ///   also carries the refresh address `I:R` when `cpu_rfsh` is set)
    /// - `cpu_mreq`: whether the CPU's MREQ signal is active
    /// - `cpu_iorq`: whether the CPU's IORQ signal is active
    /// - `cpu_rfsh`: whether the CPU is in the refresh half of an M1
    ///   (T3/T4, `I:R` on the address bus) — drives the snow effect
    /// - `framebuffer`: pixel output buffer (palette indices, 1 byte per pixel)
    fn tick(
        &mut self,
        memory: &dyn MemoryBus,
        cpu_addr: u16,
        cpu_mreq: bool,
        cpu_iorq: bool,
        cpu_rfsh: bool,
        framebuffer: &mut [u8],
    );

    /// Is the CPU clock active this half-cycle?
    /// Returns false during contention (CPU should not tick).
    fn cpu_clock_active(&self) -> bool;

    /// Is the interrupt signal currently asserted?
    fn interrupt_active(&self) -> bool;

    /// The byte currently on the ULA's data bus (for floating bus reads).
    /// During screen fetches, this is the screen data or attribute byte.
    /// During border/blanking, returns 0xFF.
    fn floating_bus(&self) -> u8;

    /// Read port 0xFE: keyboard rows (bits 0-4) + EAR (bit 6).
    /// `port`: full 16-bit port address (high byte selects keyboard half-rows).
    /// `keyboard`: 8-element array of keyboard half-row states (active low).
    fn read_fe(&self, port: u16, keyboard: &[u8; 8]) -> u8;

    /// Write port 0xFE: border colour (bits 0-2), MIC (bit 3), EAR (bit 4).
    fn write_fe(&mut self, val: u8);

    /// Frame timing constants for this ULA variant.
    fn frame_timing(&self) -> &FrameTiming;

    /// End-of-frame housekeeping: advance flash counter, reset pixel counter.
    fn end_frame(&mut self);
}
