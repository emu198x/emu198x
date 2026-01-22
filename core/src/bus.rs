/// A bus that supports memory read/write operations.
///
/// This is the base trait for all systems. Memory-mapped I/O systems
/// (6502-based machines like the C64 or NES) use this directly.
pub trait Bus {
    /// Read a byte from the given address.
    fn read(&self, address: u16) -> u8;

    /// Write a byte to the given address.
    fn write(&mut self, address: u16, value: u8);
}
