//! M68k bus traits with word-level access, function codes, and wait states.
//!
//! The 68000 family uses a 16-bit data bus. The `M68kBus` trait models this with:
//! - Word-level access (the natural 68000 bus width)
//! - Function codes (FC pins distinguish supervisor/user and program/data)
//! - Wait cycles returned from every access (enabling DMA cycle stealing)

/// Function code values from the 68000's FC0-FC2 pins.
///
/// These distinguish access types for memory management and bus arbitration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionCode {
    /// User data access (FC=1).
    UserData = 1,
    /// User program access (FC=2).
    UserProgram = 2,
    /// Supervisor data access (FC=5).
    SupervisorData = 5,
    /// Supervisor program access (FC=6).
    SupervisorProgram = 6,
    /// Interrupt acknowledge cycle (FC=7).
    InterruptAck = 7,
}

impl FunctionCode {
    /// Build a function code from supervisor flag and program/data flag.
    #[must_use]
    pub fn from_flags(supervisor: bool, program: bool) -> Self {
        match (supervisor, program) {
            (false, false) => Self::UserData,
            (false, true) => Self::UserProgram,
            (true, false) => Self::SupervisorData,
            (true, true) => Self::SupervisorProgram,
        }
    }

    /// Returns the 3-bit value for the function code.
    #[must_use]
    pub fn bits(self) -> u8 {
        self as u8
    }
}

/// Result of a bus access: data read, wait cycles, and bus error status.
#[derive(Debug, Clone, Copy)]
pub struct BusResult {
    /// Data read from the bus. For writes, this is 0.
    pub data: u16,
    /// Extra wait cycles inserted by the bus (DMA contention, slow memory, etc.).
    /// The CPU burns these as idle ticks before completing the access.
    pub wait_cycles: u8,
    /// True if this access caused a bus error (no DTACK response).
    /// The CPU will take a Group 0 exception (vector 2).
    pub bus_error: bool,
}

impl BusResult {
    /// Create a result with data and no wait cycles.
    #[must_use]
    pub const fn new(data: u16) -> Self {
        Self {
            data,
            wait_cycles: 0,
            bus_error: false,
        }
    }

    /// Create a result with data and wait cycles.
    #[must_use]
    pub const fn with_wait(data: u16, wait_cycles: u8) -> Self {
        Self {
            data,
            wait_cycles,
            bus_error: false,
        }
    }

    /// Create a write result (no data returned).
    #[must_use]
    pub const fn write_ok() -> Self {
        Self {
            data: 0,
            wait_cycles: 0,
            bus_error: false,
        }
    }

    /// Create a write result with wait cycles.
    #[must_use]
    pub const fn write_wait(wait_cycles: u8) -> Self {
        Self {
            data: 0,
            wait_cycles,
            bus_error: false,
        }
    }

    /// Create a bus error result (DTACK timeout).
    #[must_use]
    pub const fn error() -> Self {
        Self {
            data: 0,
            wait_cycles: 0,
            bus_error: true,
        }
    }
}

/// Bus trait for 68000-family CPUs.
///
/// All accesses are word-aligned. Byte accesses use the appropriate half of
/// the data bus. Every access returns a `BusResult` with optional wait cycles.
pub trait M68kBus {
    /// Read a word from the bus.
    fn read_word(&mut self, addr: u32, fc: FunctionCode) -> BusResult;

    /// Write a word to the bus.
    fn write_word(&mut self, addr: u32, value: u16, fc: FunctionCode) -> BusResult;

    /// Read a byte from the bus.
    fn read_byte(&mut self, addr: u32, fc: FunctionCode) -> BusResult;

    /// Write a byte to the bus.
    fn write_byte(&mut self, addr: u32, value: u8, fc: FunctionCode) -> BusResult;

    /// Assert the RESET line on the bus.
    fn reset(&mut self) {}

    /// Check if an address would cause a bus error.
    fn bus_error(&self, _addr: u32, _fc: FunctionCode) -> bool {
        false
    }

    /// Interrupt acknowledge cycle. Returns the vector number.
    /// Default implementation returns the autovector (24 + level).
    fn interrupt_ack(&mut self, level: u8) -> u8 {
        24 + level
    }
}
