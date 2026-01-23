//! Memory model trait for different Spectrum configurations.
//!
//! Different Spectrum models have different memory sizes and behaviors:
//! - 16K: Only 16K RAM, reads above 0x7FFF return floating bus
//! - 48K: Full 48K RAM
//! - 128K: 128K banked RAM (future)

use super::ula::Ula;

/// Memory model trait defining how different Spectrum models handle memory.
pub trait MemoryModel: Default {
    /// RAM size in bytes (not including ROM).
    const RAM_SIZE: usize;

    /// Human-readable name for this model.
    const MODEL_NAME: &'static str;

    /// Read a byte from memory.
    ///
    /// # Arguments
    /// * `data` - The 64K address space
    /// * `addr` - Address to read (0x0000-0xFFFF)
    /// * `ula` - ULA state for floating bus calculation
    fn read(&self, data: &[u8; 65536], addr: u16, ula: &Ula) -> u8;

    /// Write a byte to memory.
    ///
    /// # Arguments
    /// * `data` - The 64K address space
    /// * `addr` - Address to write (0x0000-0xFFFF)
    /// * `value` - Value to write
    ///
    /// Returns true if the write was accepted (not ROM or unmapped).
    fn write(&self, data: &mut [u8; 65536], addr: u16, value: u8) -> bool;

    /// Check if an address is in contended memory.
    fn is_contended(&self, addr: u16) -> bool;

    /// Check if this model supports .SNA snapshots.
    fn supports_sna(&self) -> bool {
        // Only 48K model supports standard .SNA
        Self::RAM_SIZE >= 48 * 1024
    }
}
