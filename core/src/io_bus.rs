use crate::Bus;

/// A bus that also supports separate I/O port operations.
///
/// The Z80 has a separate 16-bit I/O address space accessed via
/// IN and OUT instructions. Systems using the Z80 (Spectrum, Amstrad,
/// MSX, etc.) implement this trait.
pub trait IoBus: Bus {
    /// Read a byte from the given I/O port.
    fn read_io(&self, port: u32) -> u8;

    /// Write a byte to the given I/O port.
    fn write_io(&mut self, port: u32, value: u8);
}
