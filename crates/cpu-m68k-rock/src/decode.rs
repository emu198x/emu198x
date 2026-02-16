//! Instruction decoding helpers.

use crate::addressing::AddrMode;

/// Decode effective address mode and register.
pub fn decode_ea(mode: u8, reg: u8) -> Option<AddrMode> {
    match mode {
        0 => Some(AddrMode::DataReg(reg)),
        1 => Some(AddrMode::AddrReg(reg)),
        2 => Some(AddrMode::AddrIndirect(reg)),
        3 => Some(AddrMode::AddrIndirectPostInc(reg)),
        4 => Some(AddrMode::AddrIndirectPreDec(reg)),
        5 => Some(AddrMode::AddrIndirectDisp(reg)),
        6 => Some(AddrMode::AddrIndirectIndex(reg)),
        7 => match reg {
            0 => Some(AddrMode::AbsShort),
            1 => Some(AddrMode::AbsLong),
            2 => Some(AddrMode::PcIndirectDisp),
            3 => Some(AddrMode::PcIndirectIndex),
            4 => Some(AddrMode::Immediate),
            _ => None,
        },
        _ => None,
    }
}
