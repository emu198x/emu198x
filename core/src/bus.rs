/// A bus that supports memory read/write operations.
///
/// This is the base trait for all systems. Memory-mapped I/O systems
/// (6502-based machines like the C64 or NES) use this directly.
///
/// All operations are cycle-accurate: reads and writes advance the system
/// clock, and `tick()` is used for internal CPU operations that don't
/// access memory.
pub trait Bus {
    /// Read a byte from the given address.
    ///
    /// This advances the system clock by the appropriate number of cycles
    /// (typically 3 T-states for Z80, 1 cycle for 6502), plus any additional
    /// delay from bus contention.
    fn read(&mut self, address: u32) -> u8;

    /// Write a byte to the given address.
    ///
    /// This advances the system clock by the appropriate number of cycles
    /// (typically 3 T-states for Z80, 1 cycle for 6502), plus any additional
    /// delay from bus contention.
    fn write(&mut self, address: u32, value: u8);

    /// Advance the system clock without performing a memory operation.
    ///
    /// Used for internal CPU operations (register transfers, ALU operations,
    /// etc.) that consume cycles but don't access the bus.
    fn tick(&mut self, cycles: u32);
}
