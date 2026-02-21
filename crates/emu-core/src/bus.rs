//! Memory and I/O bus interface.

use crate::{Observable, Value};

/// Result of a bus read operation.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReadResult {
    /// The data read from the bus.
    pub data: u8,
    /// Number of wait states (T-states) to add before the access completes.
    /// Used for memory contention on systems like the ZX Spectrum.
    pub wait: u8,
}

impl ReadResult {
    /// Create a read result with no wait states.
    #[must_use]
    pub const fn new(data: u8) -> Self {
        Self { data, wait: 0 }
    }

    /// Create a read result with wait states.
    #[must_use]
    pub const fn with_wait(data: u8, wait: u8) -> Self {
        Self { data, wait }
    }
}

impl From<u8> for ReadResult {
    fn from(data: u8) -> Self {
        Self::new(data)
    }
}

/// Memory and I/O bus interface.
///
/// Components access memory and peripherals through this trait. The bus
/// handles address decoding, routing to the appropriate device, and
/// memory contention.
///
/// For systems with memory contention (e.g., ZX Spectrum), read/write
/// operations return the number of wait states to inject.
///
/// Addresses use 32-bit values to support systems with larger address spaces
/// (e.g., 68000 with 24-bit addresses, Amiga with 32-bit). Systems with
/// smaller address spaces (e.g., Z80 with 16-bit) use only the low bits.
pub trait Bus {
    /// Read a byte from memory.
    ///
    /// Returns the data and any wait states due to contention.
    fn read(&mut self, addr: u32) -> ReadResult;

    /// Write a byte to memory.
    ///
    /// Returns the number of wait states due to contention.
    fn write(&mut self, addr: u32, value: u8) -> u8;

    /// Read a byte from an I/O port.
    ///
    /// Returns the data and any wait states.
    fn io_read(&mut self, addr: u32) -> ReadResult;

    /// Write a byte to an I/O port.
    ///
    /// Returns the number of wait states.
    fn io_write(&mut self, addr: u32, value: u8) -> u8;

    /// Assert a reset signal to attached devices.
    ///
    /// Default implementation does nothing.
    fn reset(&mut self) {}
}

/// Extension for 16-bit data bus systems (68000, SH-2).
///
/// The 68000 also does byte accesses (e.g. CIA registers), so this extends
/// `Bus` rather than replacing it.
pub trait WordBus: Bus {
    /// Read a 16-bit word from memory.
    ///
    /// Address must be word-aligned. Unaligned reads cause an address error
    /// on the 68000.
    fn read_word(&mut self, address: u32) -> u16;

    /// Write a 16-bit word to memory.
    ///
    /// Address must be word-aligned.
    fn write_word(&mut self, address: u32, value: u16);
}

/// Simple bus implementation for testing - 64KB, no contention.
///
/// This is primarily for Z80-based systems. For 68000 systems, use a bus
/// with a larger address space.
pub struct SimpleBus {
    memory: [u8; 65536],
}

impl Default for SimpleBus {
    fn default() -> Self {
        Self::new()
    }
}

impl SimpleBus {
    #[must_use]
    #[allow(clippy::large_stack_arrays)] // Intentional: 64KB is the full Z80 address space
    pub fn new() -> Self {
        Self {
            memory: [0; 65536],
        }
    }

    /// Load data into memory at the given address.
    pub fn load(&mut self, addr: u16, data: &[u8]) {
        let start = addr as usize;
        let end = start + data.len();
        self.memory[start..end].copy_from_slice(data);
    }

    /// Get a slice of memory.
    #[must_use]
    pub fn slice(&self, start: u16, len: u16) -> &[u8] {
        let s = start as usize;
        let e = s + len as usize;
        &self.memory[s..e]
    }

    /// Read a byte without side effects (for observation).
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.memory[addr as usize]
    }

    /// Write a byte to memory without side effects.
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.memory[addr as usize] = value;
    }

    /// Parse an address from a query path.
    ///
    /// Accepts hex (0x1234, $1234) or decimal (4660).
    fn parse_address(path: &str) -> Option<u32> {
        if let Some(hex) = path.strip_prefix("0x").or_else(|| path.strip_prefix("0X")) {
            u32::from_str_radix(hex, 16).ok()
        } else if let Some(hex) = path.strip_prefix('$') {
            u32::from_str_radix(hex, 16).ok()
        } else {
            path.parse().ok()
        }
    }
}

impl Bus for SimpleBus {
    fn read(&mut self, addr: u32) -> ReadResult {
        // Mask to 16-bit address space
        ReadResult::new(self.memory[(addr & 0xFFFF) as usize])
    }

    fn write(&mut self, addr: u32, value: u8) -> u8 {
        // Mask to 16-bit address space
        self.memory[(addr & 0xFFFF) as usize] = value;
        0 // No wait states
    }

    fn io_read(&mut self, _addr: u32) -> ReadResult {
        ReadResult::new(0xFF) // Floating bus
    }

    fn io_write(&mut self, _addr: u32, _value: u8) -> u8 {
        0 // No wait states
    }
}

impl Observable for SimpleBus {
    fn query(&self, path: &str) -> Option<Value> {
        // Memory queries: "0x4000", "$4000", "16384"
        Self::parse_address(path).map(|addr| self.memory[(addr & 0xFFFF) as usize].into())
    }

    fn query_paths(&self) -> &'static [&'static str] {
        // Memory is queryable by address, not by fixed paths
        &["<address>"]
    }
}
