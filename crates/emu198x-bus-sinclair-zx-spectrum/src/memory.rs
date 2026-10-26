//! The memory surface a Spectrum-family ULA reads the screen through.
//!
//! Source references:
//! - `knowledge/systems/spectrum/overview.md`
//! - `knowledge/systems/spectrum/contention.md`

/// Spectrum-family memory surface used by the machine and ULA layers.
pub trait MemoryBus {
    /// Reads one byte from the machine address space.
    fn read(&self, addr: u16) -> u8;

    /// Writes one byte to the machine address space.
    ///
    /// ROM writes are silently ignored.
    fn write(&mut self, addr: u16, value: u8);

    /// Returns `true` if the address lies in contended RAM.
    fn is_contended(&self, addr: u16) -> bool;

    /// Reads one byte from the ULA-visible screen bank.
    ///
    /// On the 48K machine this is identical to `read`.
    fn read_screen(&self, addr: u16) -> u8 {
        self.read(addr)
    }
}
