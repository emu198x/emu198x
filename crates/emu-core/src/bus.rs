//! Memory and I/O bus interface.

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
pub trait Bus {
    /// Read a byte from memory.
    ///
    /// Returns the data and any wait states due to contention.
    fn read(&mut self, addr: u16) -> ReadResult;

    /// Write a byte to memory.
    ///
    /// Returns the number of wait states due to contention.
    fn write(&mut self, addr: u16, value: u8) -> u8;

    /// Read a byte from an I/O port.
    ///
    /// Returns the data and any wait states.
    fn io_read(&mut self, addr: u16) -> ReadResult;

    /// Write a byte to an I/O port.
    ///
    /// Returns the number of wait states.
    fn io_write(&mut self, addr: u16, value: u8) -> u8;
}

/// Simple bus implementation for testing - no contention.
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
}

impl Bus for SimpleBus {
    fn read(&mut self, addr: u16) -> ReadResult {
        ReadResult::new(self.memory[addr as usize])
    }

    fn write(&mut self, addr: u16, value: u8) -> u8 {
        self.memory[addr as usize] = value;
        0 // No wait states
    }

    fn io_read(&mut self, _addr: u16) -> ReadResult {
        ReadResult::new(0xFF) // Floating bus
    }

    fn io_write(&mut self, _addr: u16, _value: u8) -> u8 {
        0 // No wait states
    }
}
