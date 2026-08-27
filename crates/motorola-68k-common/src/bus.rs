//! 68000 bus pin definitions and types.
//!
//! The archive used a reactive `M68kBus` trait with `poll_cycle()`
//! callbacks. This port uses public pin fields on [`Cpu68000`],
//! matching the pin-level contract from
//! [cpu-bus-interface.md](../../../knowledge/decisions/cpu-bus-interface.md).
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

/// Number of bytes that remain in an MC68020/MC68030 operand transfer.
///
/// This is the logical value driven on SIZ1/SIZ0. It is not necessarily the
/// number of bytes accepted during the current physical bus cycle: alignment
/// and the DSACK-selected responder width can make the cycle narrower.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TransferSize {
    /// One byte remains (SIZ1/SIZ0 = 0/1).
    Byte,
    /// Two bytes remain (SIZ1/SIZ0 = 1/0).
    #[default]
    Word,
    /// Three bytes remain (SIZ1/SIZ0 = 1/1).
    ThreeBytes,
    /// Four bytes remain (SIZ1/SIZ0 = 0/0).
    Long,
}

impl TransferSize {
    /// Construct the SIZ value for a remaining byte count.
    ///
    /// # Panics
    ///
    /// Panics when `bytes` is outside the hardware-defined range `1..=4`.
    #[must_use]
    pub const fn from_bytes(bytes: u8) -> Self {
        match bytes {
            1 => Self::Byte,
            2 => Self::Word,
            3 => Self::ThreeBytes,
            4 => Self::Long,
            _ => panic!("MC68020 transfer size must contain 1..=4 bytes"),
        }
    }

    /// Return the number of bytes represented by this SIZ value.
    #[must_use]
    pub const fn bytes(self) -> u8 {
        match self {
            Self::Byte => 1,
            Self::Word => 2,
            Self::ThreeBytes => 3,
            Self::Long => 4,
        }
    }

    /// Return `(SIZ1, SIZ0)` as asserted-high logical pin values.
    #[must_use]
    pub const fn siz_pins(self) -> (bool, bool) {
        match self {
            Self::Byte => (false, true),
            Self::Word => (true, false),
            Self::ThreeBytes => (true, true),
            Self::Long => (false, false),
        }
    }
}

/// Data-port width reported by DSACK1/DSACK0 on an MC68020/MC68030 cycle.
///
/// The enum stores the decoded responder width rather than electrical pin
/// levels. DSACK pins are active-low: a byte port asserts DSACK0, a word port
/// asserts DSACK1, and a long-word port asserts both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataPortSize {
    /// Eight-bit responder connected to D31-D24.
    Byte,
    /// Sixteen-bit responder connected to D31-D16.
    Word,
    /// Thirty-two-bit responder connected to D31-D0.
    Long,
}

impl DataPortSize {
    /// Return the responder width in bytes.
    #[must_use]
    pub const fn bytes(self) -> u8 {
        match self {
            Self::Byte => 1,
            Self::Word => 2,
            Self::Long => 4,
        }
    }

    /// Return `(DSACK1 asserted, DSACK0 asserted)`.
    #[must_use]
    pub const fn asserted_pins(self) -> (bool, bool) {
        match self {
            Self::Byte => (false, true),
            Self::Word => (true, false),
            Self::Long => (true, true),
        }
    }
}

/// Number of sequential operand bytes accepted by one completed dynamic-sized
/// bus cycle.
///
/// The processor attempts the entire remaining transfer. A responder can
/// accept only the bytes that fit between the current address and the next
/// boundary of its own fixed-width port.
#[must_use]
pub const fn dynamic_transfer_bytes(
    remaining: TransferSize,
    address: u32,
    port: DataPortSize,
) -> u8 {
    let port_bytes = port.bytes();
    let offset = (address as u8) & (port_bytes - 1);
    let available = port_bytes - offset;
    if remaining.bytes() < available {
        remaining.bytes()
    } else {
        available
    }
}

/// Place sequential read bytes on the fixed lanes used by a dynamic-sized
/// responder.
///
/// `value` is right-justified and big-endian within `transferred` bytes. A
/// word value `$1234`, for example, is passed as `$00001234`. The returned
/// value is the physical D31-D0 bus image: byte ports use D31-D24, word ports
/// use D31-D16, and long-word ports use all four lanes.
///
/// # Panics
///
/// Panics when `transferred` is zero or extends past the responder's current
/// fixed-width port boundary.
#[must_use]
pub const fn place_dynamic_read_data(
    value: u32,
    transferred: u8,
    address: u32,
    port: DataPortSize,
) -> u32 {
    let port_bytes = port.bytes();
    let start_lane = (address as u8) & (port_bytes - 1);
    assert!(
        transferred >= 1 && transferred <= port_bytes - start_lane,
        "dynamic bus transfer must fit the responder's current port boundary"
    );
    let mut bus_data = 0u32;
    let mut index = 0u8;
    while index < transferred {
        let source_shift = 8 * (transferred - index - 1);
        let byte = (value >> source_shift) & 0xFF;
        let destination_lane = start_lane + index;
        let destination_shift = 8 * (3 - destination_lane);
        bus_data |= byte << destination_shift;
        index += 1;
    }
    bus_data
}

/// Extract the sequential bytes accepted by a responder from D31-D0.
///
/// The result is right-justified and big-endian within `transferred` bytes.
/// This is used both by the processor's read-data multiplexer and by a machine
/// committing the active byte lanes of a write cycle.
///
/// # Panics
///
/// Panics when `transferred` is zero or extends past the responder's current
/// fixed-width port boundary.
#[must_use]
pub const fn extract_dynamic_bus_data(
    bus_data: u32,
    transferred: u8,
    address: u32,
    port: DataPortSize,
) -> u32 {
    let port_bytes = port.bytes();
    let start_lane = (address as u8) & (port_bytes - 1);
    assert!(
        transferred >= 1 && transferred <= port_bytes - start_lane,
        "dynamic bus transfer must fit the responder's current port boundary"
    );
    let mut value = 0u32;
    let mut index = 0u8;
    while index < transferred {
        let source_lane = start_lane + index;
        let source_shift = 8 * (3 - source_lane);
        value = (value << 8) | ((bus_data >> source_shift) & 0xFF);
        index += 1;
    }
    value
}

/// Build the MC68020/MC68030 write-data duplication pattern on D31-D0.
///
/// The processor does not know the responder width until DSACK terminates the
/// cycle. It therefore duplicates operand bytes so an 8-, 16-, or 32-bit
/// responder sees the correct data on its fixed lanes. `operand` contains the
/// complete byte/word/long operand right-justified in a 32-bit value.
#[must_use]
pub const fn dynamic_write_data(operand: u32, remaining: TransferSize, address: u32) -> u32 {
    let op0 = (operand >> 24) as u8;
    let op1 = (operand >> 16) as u8;
    let op2 = (operand >> 8) as u8;
    let op3 = operand as u8;
    let offset = (address & 3) as usize;

    let lanes = match remaining {
        TransferSize::Byte => [op3, op3, op3, op3],
        TransferSize::Word if address & 1 == 0 => [op2, op3, op2, op3],
        TransferSize::Word => [op2, op2, op3, op2],
        TransferSize::ThreeBytes => [
            [op1, op2, op3, op0],
            [op1, op1, op2, op3],
            [op1, op2, op1, op2],
            [op1, op1, op2, op1],
        ][offset],
        TransferSize::Long => [
            [op0, op1, op2, op3],
            [op0, op0, op1, op2],
            [op0, op1, op0, op1],
            [op0, op0, op1, op0],
        ][offset],
    };

    (lanes[0] as u32) << 24 | (lanes[1] as u32) << 16 | (lanes[2] as u32) << 8 | lanes[3] as u32
}

/// The status of a bus request. The machine layer writes this to the
/// CPU's input pins after performing the memory operation.
///
/// In pin terms:
/// - `Ready` = DTACK asserted, data valid on the bus.
/// - `Wait` = DTACK not asserted, CPU holds in BusCycle state.
/// - `Error` = BERR asserted. Ordinary cycles enter the bus-error exception;
///   interrupt acknowledge selects the spurious interrupt response.
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
    /// A terminal bus error (BERR without a retry request) occurred. During
    /// interrupt acknowledge this selects the spurious interrupt response
    /// instead of the ordinary bus-error path. This abstraction does not
    /// represent the BERR-plus-HALT retry handshake.
    Error,
    /// An MC68020/MC68030 dynamic-sized cycle completed.
    ///
    /// `data` is the physical D31-D0 bus image. `port` is the responder width
    /// encoded by DSACK1/DSACK0. The processor applies its input multiplexer
    /// using the current address and SIZ pins, then starts another physical
    /// phase if bytes remain.
    ///
    /// Appended after the pre-existing variants so serialized `Wait` and
    /// `Error` discriminants remain stable.
    ReadySized { data: u32, port: DataPortSize },
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
    use super::{
        DataPortSize, TransferSize, dynamic_transfer_bytes, dynamic_write_data,
        extract_dynamic_bus_data, interrupt_acknowledge_address, interrupt_acknowledge_level,
        place_dynamic_read_data,
    };

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

    #[test]
    fn transfer_size_encodes_siz_pins_and_byte_counts() {
        for (size, bytes, pins) in [
            (TransferSize::Byte, 1, (false, true)),
            (TransferSize::Word, 2, (true, false)),
            (TransferSize::ThreeBytes, 3, (true, true)),
            (TransferSize::Long, 4, (false, false)),
        ] {
            assert_eq!(TransferSize::from_bytes(bytes), size);
            assert_eq!(size.bytes(), bytes);
            assert_eq!(size.siz_pins(), pins);
        }
    }

    #[test]
    fn data_port_size_decodes_asserted_dsack_pins() {
        assert_eq!(DataPortSize::Byte.asserted_pins(), (false, true));
        assert_eq!(DataPortSize::Word.asserted_pins(), (true, false));
        assert_eq!(DataPortSize::Long.asserted_pins(), (true, true));
    }

    #[test]
    fn dynamic_transfer_counts_reproduce_manual_alignment_table() {
        let expected_word_cycles = [
            (DataPortSize::Long, [1, 1, 1, 2]),
            (DataPortSize::Word, [1, 2, 1, 2]),
            (DataPortSize::Byte, [2, 2, 2, 2]),
        ];
        let expected_long_cycles = [
            (DataPortSize::Long, [1, 2, 2, 2]),
            (DataPortSize::Word, [2, 3, 2, 3]),
            (DataPortSize::Byte, [4, 4, 4, 4]),
        ];

        for (port, expected) in expected_word_cycles {
            for offset in 0..4u32 {
                let mut address = offset;
                let mut remaining = 2u8;
                let mut cycles = 0;
                while remaining != 0 {
                    let transferred =
                        dynamic_transfer_bytes(TransferSize::from_bytes(remaining), address, port);
                    address += u32::from(transferred);
                    remaining -= transferred;
                    cycles += 1;
                }
                assert_eq!(cycles, expected[offset as usize]);
            }
        }

        for (port, expected) in expected_long_cycles {
            for offset in 0..4u32 {
                let mut address = offset;
                let mut remaining = 4u8;
                let mut cycles = 0;
                while remaining != 0 {
                    let transferred =
                        dynamic_transfer_bytes(TransferSize::from_bytes(remaining), address, port);
                    address += u32::from(transferred);
                    remaining -= transferred;
                    cycles += 1;
                }
                assert_eq!(cycles, expected[offset as usize]);
            }
        }
    }

    #[test]
    fn read_lane_placement_round_trips_for_every_port_and_alignment() {
        for port in [DataPortSize::Byte, DataPortSize::Word, DataPortSize::Long] {
            for address in 0..4u32 {
                for remaining in 1..=4u8 {
                    let remaining = TransferSize::from_bytes(remaining);
                    let transferred = dynamic_transfer_bytes(remaining, address, port);
                    let mask = u32::MAX >> (32 - u32::from(transferred) * 8);
                    let value = 0x1234_5678 & mask;
                    let bus_data = place_dynamic_read_data(value, transferred, address, port);
                    assert_eq!(
                        extract_dynamic_bus_data(bus_data, transferred, address, port),
                        value
                    );
                }
            }
        }
    }

    #[test]
    #[should_panic(
        expected = "dynamic bus transfer must fit the responder's current port boundary"
    )]
    fn read_lane_placement_rejects_zero_transferred_bytes() {
        let _ = place_dynamic_read_data(0, 0, 0, DataPortSize::Long);
    }

    #[test]
    #[should_panic(
        expected = "dynamic bus transfer must fit the responder's current port boundary"
    )]
    fn read_lane_extraction_rejects_a_phase_past_the_port_boundary() {
        let _ = extract_dynamic_bus_data(0, 2, 3, DataPortSize::Long);
    }

    #[test]
    fn write_lane_patterns_match_the_manual_table() {
        let operand = 0x1020_3040;

        for address in 0..4u32 {
            assert_eq!(
                dynamic_write_data(operand, TransferSize::Byte, address),
                0x4040_4040
            );
        }
        assert_eq!(
            dynamic_write_data(operand, TransferSize::Word, 0),
            0x3040_3040
        );
        assert_eq!(
            dynamic_write_data(operand, TransferSize::Word, 1),
            0x3030_4030
        );
        assert_eq!(
            dynamic_write_data(operand, TransferSize::ThreeBytes, 0),
            0x2030_4010
        );
        assert_eq!(
            dynamic_write_data(operand, TransferSize::ThreeBytes, 1),
            0x2020_3040
        );
        assert_eq!(
            dynamic_write_data(operand, TransferSize::ThreeBytes, 2),
            0x2030_2030
        );
        assert_eq!(
            dynamic_write_data(operand, TransferSize::ThreeBytes, 3),
            0x2020_3020
        );
        assert_eq!(
            dynamic_write_data(operand, TransferSize::Long, 0),
            0x1020_3040
        );
        assert_eq!(
            dynamic_write_data(operand, TransferSize::Long, 1),
            0x1010_2030
        );
        assert_eq!(
            dynamic_write_data(operand, TransferSize::Long, 2),
            0x1020_1020
        );
        assert_eq!(
            dynamic_write_data(operand, TransferSize::Long, 3),
            0x1010_2010
        );
    }
}
