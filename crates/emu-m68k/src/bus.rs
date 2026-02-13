//! M68k bus traits with word-level access, function codes, and wait states.
//!
//! The 68000 family uses a 16-bit data bus (32-bit on 68020+). Unlike the Z80/6502
//! byte-oriented buses, the 68000 performs word-aligned reads and writes natively.
//!
//! The `M68kBus` trait models this with:
//! - Word-level access (the natural 68000 bus width)
//! - Function codes (FC pins distinguish supervisor/user and program/data)
//! - Wait cycles returned from every access (enabling DMA cycle stealing)
//!
//! The `CoreBusAdapter` wraps an `emu-core::Bus` to implement `M68kBus`, allowing
//! the existing single-step test harness to work unchanged.

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

/// Result of a bus access: data read and wait cycles inserted.
#[derive(Debug, Clone, Copy)]
pub struct BusResult {
    /// Data read from the bus. For writes, this is 0.
    pub data: u16,
    /// Extra wait cycles inserted by the bus (DMA contention, slow memory, etc.).
    /// The CPU burns these as idle ticks before completing the access.
    pub wait_cycles: u8,
}

impl BusResult {
    /// Create a result with data and no wait cycles.
    #[must_use]
    pub const fn new(data: u16) -> Self {
        Self {
            data,
            wait_cycles: 0,
        }
    }

    /// Create a result with data and wait cycles.
    #[must_use]
    pub const fn with_wait(data: u16, wait_cycles: u8) -> Self {
        Self { data, wait_cycles }
    }

    /// Create a write result (no data returned).
    #[must_use]
    pub const fn write_ok() -> Self {
        Self {
            data: 0,
            wait_cycles: 0,
        }
    }

    /// Create a write result with wait cycles.
    #[must_use]
    pub const fn write_wait(wait_cycles: u8) -> Self {
        Self {
            data: 0,
            wait_cycles,
        }
    }
}

/// Bus trait for 68000-family CPUs.
///
/// All accesses are word-aligned. Byte accesses use the appropriate half of
/// the data bus (even addresses = high byte, odd = low byte on the 68000).
///
/// Every access returns a `BusResult` with optional wait cycles. This is the
/// mechanism for DMA cycle stealing: when the CPU accesses chip RAM during a
/// DMA slot, the bus returns extra wait cycles and the CPU burns them as idle
/// ticks.
pub trait M68kBus {
    /// Read a word from the bus.
    fn read_word(&mut self, addr: u32, fc: FunctionCode) -> BusResult;

    /// Write a word to the bus.
    fn write_word(&mut self, addr: u32, value: u16, fc: FunctionCode) -> BusResult;

    /// Read a byte from the bus.
    ///
    /// On the 68000, byte reads still perform a word-width bus cycle.
    /// Even addresses return the high byte, odd addresses return the low byte.
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

/// Adapter that wraps an `emu-core::Bus` as an `M68kBus`.
///
/// This allows the existing single-step test harness (which uses the byte-oriented
/// `emu-core::Bus`) to work with the new word-oriented `M68kBus` interface.
///
/// All accesses return `wait_cycles: 0` since the test bus has no contention.
pub struct CoreBusAdapter<'a, B: emu_core::Bus> {
    bus: &'a mut B,
}

impl<'a, B: emu_core::Bus> CoreBusAdapter<'a, B> {
    /// Wrap a core bus reference.
    pub fn new(bus: &'a mut B) -> Self {
        Self { bus }
    }
}

impl<B: emu_core::Bus> M68kBus for CoreBusAdapter<'_, B> {
    fn read_word(&mut self, addr: u32, _fc: FunctionCode) -> BusResult {
        let hi = self.bus.read(addr).data;
        let lo = self.bus.read(addr.wrapping_add(1)).data;
        BusResult::new(u16::from(hi) << 8 | u16::from(lo))
    }

    fn write_word(&mut self, addr: u32, value: u16, _fc: FunctionCode) -> BusResult {
        self.bus.write(addr, (value >> 8) as u8);
        self.bus.write(addr.wrapping_add(1), (value & 0xFF) as u8);
        BusResult::write_ok()
    }

    fn read_byte(&mut self, addr: u32, _fc: FunctionCode) -> BusResult {
        let data = self.bus.read(addr).data;
        BusResult::new(u16::from(data))
    }

    fn write_byte(&mut self, addr: u32, value: u8, _fc: FunctionCode) -> BusResult {
        self.bus.write(addr, value);
        BusResult::write_ok()
    }

    fn reset(&mut self) {
        self.bus.reset();
    }
}
