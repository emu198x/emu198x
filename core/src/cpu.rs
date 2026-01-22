use crate::Bus;

/// A CPU that can execute instructions.
///
/// The type parameter `B` is the bus type this CPU operates on.
pub trait Cpu<B: Bus> {
    /// Execute one instruction. Returns cycles consumed.
    fn step(&mut self, bus: &mut B) -> u32;

    /// Reset the CPU to its initial state.
    fn reset(&mut self, bus: &mut B);

    /// Signal a maskable interrupt.
    fn interrupt(&mut self, bus: &mut B);

    /// Signal a non-maskable interrupt.
    fn nmi(&mut self, bus: &mut B);

    /// Get the current program counter.
    fn pc(&self) -> u16;
}
