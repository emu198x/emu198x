//! Memory and I/O bus interface.

/// Memory and I/O bus interface.
///
/// Components access memory and peripherals through this trait. The bus
/// handles address decoding and routing to the appropriate device.
pub trait Bus {
    /// Read a byte from the given address.
    fn read(&mut self, address: u16) -> u8;

    /// Write a byte to the given address.
    fn write(&mut self, address: u16, value: u8);
}
