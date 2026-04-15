//! 68000 bus pin definitions and types.
//!
//! The archive used a reactive `M68kBus` trait with `poll_cycle()`
//! callbacks. This port uses public pin fields on [`Cpu68000`],
//! matching the pin-level contract from
//! [cpu-bus-interface.md](../../wiki/decisions/cpu-bus-interface.md).
//!
//! The machine layer inspects the CPU's output pins between ticks
//! and drives the input pins with the result, same shape as the
//! 6502 and Z80 ports.

/// Function code values from the 68000's FC0-FC2 pins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionCode {
    UserData = 1,
    UserProgram = 2,
    SupervisorData = 5,
    SupervisorProgram = 6,
    InterruptAck = 7,
}

impl FunctionCode {
    /// Return the 3-bit function code value.
    #[must_use]
    pub fn bits(self) -> u8 {
        self as u8
    }
}

/// The status of a bus request. The machine layer writes this to the
/// CPU's input pins after performing the memory operation.
///
/// In pin terms:
/// - `Ready` = DTACK asserted, data valid on the bus.
/// - `Wait` = DTACK not asserted, CPU holds in BusCycle state.
/// - `Error` = BERR asserted, CPU enters bus error exception.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusStatus {
    /// The bus cycle is complete. For reads, contains the data word.
    Ready(u16),
    /// The bus is not ready yet (DTACK not asserted).
    Wait,
    /// A bus error (BERR) occurred.
    Error,
}
