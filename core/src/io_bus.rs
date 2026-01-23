use crate::Bus;

/// A bus that also supports separate I/O port operations.
///
/// The Z80 has a separate 16-bit I/O address space accessed via
/// IN and OUT instructions. Systems using the Z80 (Spectrum, Amstrad,
/// MSX, etc.) implement this trait.
///
/// I/O operations are also cycle-accurate and may be subject to
/// bus contention on systems like the ZX Spectrum.
pub trait IoBus: Bus {
    /// Read a byte from the given I/O port.
    ///
    /// This advances the system clock by the appropriate number of cycles
    /// (typically 4 T-states for Z80), plus any contention delay.
    fn read_io(&mut self, port: u16) -> u8;

    /// Write a byte to the given I/O port.
    ///
    /// This advances the system clock by the appropriate number of cycles
    /// (typically 4 T-states for Z80), plus any contention delay.
    fn write_io(&mut self, port: u16, value: u8);
}
