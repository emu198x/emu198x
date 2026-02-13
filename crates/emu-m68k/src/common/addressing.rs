//! Addressing mode definitions for the 68000 family.
//!
//! The base 68000 has 12 addressing modes. The 68020+ adds scaled index
//! and memory indirect modes, which will be added in their respective modules.

/// Addressing mode for 68000 instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddrMode {
    /// Data register direct: Dn
    DataReg(u8),
    /// Address register direct: An
    AddrReg(u8),
    /// Address register indirect: (An)
    AddrInd(u8),
    /// Address register indirect with postincrement: (An)+
    AddrIndPostInc(u8),
    /// Address register indirect with predecrement: -(An)
    AddrIndPreDec(u8),
    /// Address register indirect with displacement: d16(An)
    AddrIndDisp(u8),
    /// Address register indirect with index: d8(An,Xn)
    AddrIndIndex(u8),
    /// Absolute short: (xxx).W
    AbsShort,
    /// Absolute long: (xxx).L
    AbsLong,
    /// Program counter with displacement: d16(PC)
    PcDisp,
    /// Program counter with index: d8(PC,Xn)
    PcIndex,
    /// Immediate: #<data>
    Immediate,
}

impl AddrMode {
    /// Decode addressing mode from mode/register fields.
    #[must_use]
    pub fn decode(mode: u8, reg: u8) -> Option<Self> {
        match mode & 0x07 {
            0 => Some(Self::DataReg(reg & 0x07)),
            1 => Some(Self::AddrReg(reg & 0x07)),
            2 => Some(Self::AddrInd(reg & 0x07)),
            3 => Some(Self::AddrIndPostInc(reg & 0x07)),
            4 => Some(Self::AddrIndPreDec(reg & 0x07)),
            5 => Some(Self::AddrIndDisp(reg & 0x07)),
            6 => Some(Self::AddrIndIndex(reg & 0x07)),
            7 => match reg & 0x07 {
                0 => Some(Self::AbsShort),
                1 => Some(Self::AbsLong),
                2 => Some(Self::PcDisp),
                3 => Some(Self::PcIndex),
                4 => Some(Self::Immediate),
                _ => None,
            },
            _ => None,
        }
    }

    /// Check if this mode is a data alterable destination.
    #[must_use]
    pub fn is_data_alterable(&self) -> bool {
        matches!(
            self,
            Self::DataReg(_)
                | Self::AddrInd(_)
                | Self::AddrIndPostInc(_)
                | Self::AddrIndPreDec(_)
                | Self::AddrIndDisp(_)
                | Self::AddrIndIndex(_)
                | Self::AbsShort
                | Self::AbsLong
        )
    }

    /// Check if this mode is memory alterable.
    #[must_use]
    pub fn is_memory_alterable(&self) -> bool {
        matches!(
            self,
            Self::AddrInd(_)
                | Self::AddrIndPostInc(_)
                | Self::AddrIndPreDec(_)
                | Self::AddrIndDisp(_)
                | Self::AddrIndIndex(_)
                | Self::AbsShort
                | Self::AbsLong
        )
    }
}
