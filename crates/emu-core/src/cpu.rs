//! CPU core trait.

use crate::Bus;

/// A CPU core.
///
/// CPUs execute instructions and access memory through a bus. Unlike other
/// `Tickable` components, CPUs take a bus reference in their tick method
/// because they need to access memory on specific cycles.
///
/// CPUs expose their internal state for observation and debugging.
pub trait Cpu {
    /// The type used for register inspection.
    type Registers;

    /// Advance the CPU by one T-state.
    ///
    /// The bus is passed in, not owned, so it can be shared with other
    /// components (e.g., video chip). The bus may return wait states for
    /// contended memory accesses.
    fn tick<B: Bus>(&mut self, bus: &mut B);

    /// Returns the current program counter.
    ///
    /// Returns `u32` to support all CPU address widths: 16-bit (6502, Z80),
    /// 24-bit (68000), and 32-bit (ARM7TDMI). Narrower CPUs zero-extend.
    fn pc(&self) -> u32;

    /// Returns a snapshot of all registers for inspection.
    fn registers(&self) -> Self::Registers;

    /// Returns true if the CPU is halted.
    fn is_halted(&self) -> bool;

    /// Request an interrupt. Returns true if accepted.
    fn interrupt(&mut self) -> bool;

    /// Request a non-maskable interrupt.
    fn nmi(&mut self);

    /// Reset the CPU to its initial state.
    fn reset(&mut self);
}
