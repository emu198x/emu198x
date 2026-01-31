//! CPU core trait.

use crate::Tickable;

/// A CPU core.
///
/// CPUs are tickable components that execute instructions. They access
/// memory through a bus and expose their internal state for observation.
pub trait Cpu: Tickable {
    /// The type used for register inspection.
    type Registers;

    /// Returns the current program counter.
    fn pc(&self) -> u16;

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
