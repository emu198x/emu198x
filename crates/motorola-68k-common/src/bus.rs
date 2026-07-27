//! 68000 bus pin definitions and types.
//!
//! The archive used a reactive `M68kBus` trait with `poll_cycle()`
//! callbacks. This port uses public pin fields on [`Cpu68000`],
//! matching the pin-level contract from
//! [cpu-bus-interface.md](../../knowledge/decisions/cpu-bus-interface.md).
//!
//! The machine layer inspects the CPU's output pins between ticks
//! and drives the input pins with the result, same shape as the
//! 6502 and Z80 ports.

use serde::{Deserialize, Serialize};

/// Function code values from the 68000's FC0-FC2 pins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BusStatus {
    /// The bus cycle is complete. For ordinary reads, contains the data word.
    ///
    /// The current interrupt-acknowledge compatibility path also uses this
    /// value for the selected autovector after the machine has collapsed
    /// VPA/AVEC termination and the CPU's internal vector generation. It does
    /// not imply literal DTACK and bus data during that special cycle.
    Ready(u16),
    /// The bus is not ready yet (DTACK not asserted).
    Wait,
    /// A bus error (BERR) occurred.
    Error,
}

/// Form the original MC68000's 24-bit CPU-space interrupt-acknowledge
/// address.
///
/// The accepted interrupt level appears on A3-A1. Every other address
/// line is high, so levels 1 through 7 map to `$FFFFF3` through
/// `$FFFFFF` on a 24-bit bus.
#[must_use]
pub fn interrupt_acknowledge_address(level: u8) -> u32 {
    assert!(
        (1..=7).contains(&level),
        "interrupt acknowledge level must be 1..=7"
    );
    0x00FF_FFF1 | (u32::from(level) << 1)
}

/// Recover the accepted interrupt level carried on A3-A1 during an
/// interrupt-acknowledge cycle.
#[must_use]
pub fn interrupt_acknowledge_level(address: u32) -> u8 {
    ((address >> 1) & 0x07) as u8
}

#[cfg(test)]
mod tests {
    use super::{interrupt_acknowledge_address, interrupt_acknowledge_level};

    #[test]
    fn interrupt_acknowledge_addresses_encode_levels_on_a3_through_a1() {
        for (level, address) in [
            (1, 0x00FF_FFF3),
            (2, 0x00FF_FFF5),
            (3, 0x00FF_FFF7),
            (4, 0x00FF_FFF9),
            (5, 0x00FF_FFFB),
            (6, 0x00FF_FFFD),
            (7, 0x00FF_FFFF),
        ] {
            assert_eq!(interrupt_acknowledge_address(level), address);
            assert_eq!(interrupt_acknowledge_level(address), level);
        }
    }
}
