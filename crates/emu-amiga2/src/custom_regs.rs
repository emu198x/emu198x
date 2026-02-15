//! Amiga custom chip register offsets and helpers.
//!
//! Custom registers live at $DFF000-$DFF1FF. This module provides
//! register offset constants and the SET/CLR write logic used by
//! DMACON, INTENA, INTREQ, and ADKCON.

#![allow(clippy::cast_possible_truncation)]

/// Apply SET/CLR write logic used by DMACON, INTENA, INTREQ, ADKCON.
///
/// Bit 15 determines set (1) or clear (0) mode for bits 0-14.
pub fn set_clr_write(reg: &mut u16, val: u16) {
    if val & 0x8000 != 0 {
        *reg |= val & 0x7FFF;
    } else {
        *reg &= !(val & 0x7FFF);
    }
}

// Read registers (offsets from $DFF000):
pub const DMACONR: u16 = 0x002;
pub const VPOSR: u16 = 0x004;
pub const VHPOSR: u16 = 0x006;
pub const JOY0DAT: u16 = 0x00A;
pub const JOY1DAT: u16 = 0x00C;
pub const ADKCONR: u16 = 0x010;
pub const POTGOR: u16 = 0x016;
pub const SERDATR: u16 = 0x018;
pub const INTENAR: u16 = 0x01C;
pub const INTREQR: u16 = 0x01E;

// Write registers:
pub const VPOSW: u16 = 0x02A;
pub const VHPOSW: u16 = 0x02C;
pub const COPCON: u16 = 0x02E;
pub const SERDAT: u16 = 0x030;
pub const SERPER: u16 = 0x032;
pub const BLTCON0: u16 = 0x040;
pub const BLTCON1: u16 = 0x042;
pub const BLTAFWM: u16 = 0x044;
pub const BLTALWM: u16 = 0x046;
pub const BLTCPTH: u16 = 0x048;
pub const BLTCPTL: u16 = 0x04A;
pub const BLTBPTH: u16 = 0x04C;
pub const BLTBPTL: u16 = 0x04E;
pub const BLTAPTH: u16 = 0x050;
pub const BLTAPTL: u16 = 0x052;
pub const BLTDPTH: u16 = 0x054;
pub const BLTDPTL: u16 = 0x056;
pub const BLTSIZE: u16 = 0x058;
pub const BLTCMOD: u16 = 0x060;
pub const BLTBMOD: u16 = 0x062;
pub const BLTAMOD: u16 = 0x064;
pub const BLTDMOD: u16 = 0x066;
pub const BLTCDAT: u16 = 0x070;
pub const BLTBDAT: u16 = 0x072;
pub const BLTADAT: u16 = 0x074;
pub const COP1LCH: u16 = 0x080;
pub const COP1LCL: u16 = 0x082;
pub const COP2LCH: u16 = 0x084;
pub const COP2LCL: u16 = 0x086;
pub const COPJMP1: u16 = 0x088;
pub const COPJMP2: u16 = 0x08A;
pub const DIWSTRT: u16 = 0x08E;
pub const DIWSTOP: u16 = 0x090;
pub const DDFSTRT: u16 = 0x092;
pub const DDFSTOP: u16 = 0x094;
pub const DMACON: u16 = 0x096;
pub const INTENA: u16 = 0x09A;
pub const INTREQ: u16 = 0x09C;
pub const BPL1PTH: u16 = 0x0E0;
pub const BPL1PTL: u16 = 0x0E2;
pub const BPL2PTH: u16 = 0x0E4;
pub const BPL2PTL: u16 = 0x0E6;
pub const BPL3PTH: u16 = 0x0E8;
pub const BPL3PTL: u16 = 0x0EA;
pub const BPL4PTH: u16 = 0x0EC;
pub const BPL4PTL: u16 = 0x0EE;
pub const BPL5PTH: u16 = 0x0F0;
pub const BPL5PTL: u16 = 0x0F2;
pub const BPL6PTH: u16 = 0x0F4;
pub const BPL6PTL: u16 = 0x0F6;
pub const BPLCON0: u16 = 0x100;
pub const BPLCON1: u16 = 0x102;
pub const BPLCON2: u16 = 0x104;
pub const BPL1MOD: u16 = 0x108;
pub const BPL2MOD: u16 = 0x10A;
pub const BPL1DAT: u16 = 0x110;
pub const BPL2DAT: u16 = 0x112;
pub const BPL3DAT: u16 = 0x114;
pub const BPL4DAT: u16 = 0x116;
pub const BPL5DAT: u16 = 0x118;
pub const BPL6DAT: u16 = 0x11A;
#[allow(dead_code)]
pub const COLOR00: u16 = 0x180;

// Disk registers
pub const DSKBYTR: u16 = 0x01A;
pub const DSKPTH: u16 = 0x020;
pub const DSKPTL: u16 = 0x022;
pub const DSKLEN: u16 = 0x024;

// DMACON bits
pub const DMAF_DMAEN: u16 = 1 << 9;
pub const DMAF_BPLEN: u16 = 1 << 8;
pub const DMAF_COPEN: u16 = 1 << 7;
#[allow(dead_code)]
pub const DMAF_BLTEN: u16 = 1 << 6;
pub const DMAF_SPREN: u16 = 1 << 5;
pub const DMAF_DSKEN: u16 = 1 << 4;
pub const DMAF_AUD3EN: u16 = 1 << 3;
pub const DMAF_AUD2EN: u16 = 1 << 2;
pub const DMAF_AUD1EN: u16 = 1 << 1;
pub const DMAF_AUD0EN: u16 = 1 << 0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_clr_set_mode() {
        let mut reg = 0u16;
        set_clr_write(&mut reg, 0x8003);
        assert_eq!(reg, 0x0003);
    }

    #[test]
    fn set_clr_clear_mode() {
        let mut reg = 0x001Fu16;
        set_clr_write(&mut reg, 0x0003);
        assert_eq!(reg, 0x001C);
    }

    #[test]
    fn set_clr_preserves_other_bits() {
        let mut reg = 0x00FFu16;
        set_clr_write(&mut reg, 0x8100);
        assert_eq!(reg, 0x01FF);
    }
}
