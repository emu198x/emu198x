//! Instruction decoding helpers.

use crate::addressing::AddrMode;

/// Decode effective address mode and register.
pub fn decode_ea(mode: u8, reg: u8) -> Option<AddrMode> {
    AddrMode::decode(mode, reg)
}
