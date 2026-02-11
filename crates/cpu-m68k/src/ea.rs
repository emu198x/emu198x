//! Effective address calculation for the 68000.
//!
//! EA calculation is instant — it happens inside Execute. The result is
//! an address stored in `self.addr` (or a register reference). Any
//! extension words needed are consumed from IRC via `consume_irc()`,
//! which queues FetchIRC to refill the pipeline.
//!
//! This file starts minimal — EA calculation will be fleshed out in Phase 1
//! when MOVE instructions exercise all addressing modes.

use crate::alu::Size;
use crate::cpu::Cpu68000;
use crate::addressing::AddrMode;

impl Cpu68000 {
    /// Calculate effective address, returning (address, is_register).
    ///
    /// For register modes, returns (register_index, true).
    /// For memory modes, returns (memory_address, false).
    /// Extension words are consumed from IRC as needed.
    pub(crate) fn calc_ea(&mut self, mode: AddrMode, size: Size) -> (u32, bool) {
        match mode {
            AddrMode::DataReg(r) => (u32::from(r), true),
            AddrMode::AddrReg(r) => (u32::from(r) | 0x100, true), // Flag bit to distinguish from Dn
            AddrMode::AddrInd(r) => (self.regs.a(r as usize), false),
            AddrMode::AddrIndPostInc(r) => {
                let addr = self.regs.a(r as usize);
                // Byte access to A7 increments by 2 (stack alignment)
                let inc = if size == Size::Byte && r == 7 { 2 } else { size.bytes() };
                self.regs.set_a(r as usize, addr.wrapping_add(inc));
                (addr, false)
            }
            AddrMode::AddrIndPreDec(r) => {
                // Byte access to A7 decrements by 2 (stack alignment)
                let dec = if size == Size::Byte && r == 7 { 2 } else { size.bytes() };
                let addr = self.regs.a(r as usize).wrapping_sub(dec);
                self.regs.set_a(r as usize, addr);
                (addr, false)
            }
            AddrMode::AddrIndDisp(r) => {
                let disp = self.consume_irc() as i16;
                let base = self.regs.a(r as usize);
                ((base as i32).wrapping_add(i32::from(disp)) as u32, false)
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.consume_irc();
                let base = self.regs.a(r as usize);
                (self.calc_index_ea(base, ext), false)
            }
            AddrMode::AbsShort => {
                let addr = self.consume_irc() as i16 as i32 as u32;
                (addr, false)
            }
            AddrMode::AbsLong => {
                let hi = self.consume_irc();
                let lo = self.consume_irc();
                let addr = (u32::from(hi) << 16) | u32::from(lo);
                (addr, false)
            }
            AddrMode::PcDisp => {
                // PC-relative: base is address where IRC was fetched
                let base_pc = self.irc_addr;
                let disp = self.consume_irc() as i16;
                self.program_space_access = true;
                ((base_pc as i32).wrapping_add(i32::from(disp)) as u32, false)
            }
            AddrMode::PcIndex => {
                let base_pc = self.irc_addr;
                let ext = self.consume_irc();
                self.program_space_access = true;
                (self.calc_index_ea(base_pc, ext), false)
            }
            AddrMode::Immediate => {
                // Immediate value comes from IRC
                match size {
                    Size::Byte | Size::Word => {
                        let val = self.consume_irc();
                        // For byte, the value is in the low byte of the word
                        (u32::from(val), true)
                    }
                    Size::Long => {
                        let hi = self.consume_irc();
                        let lo = self.consume_irc();
                        ((u32::from(hi) << 16) | u32::from(lo), true)
                    }
                }
            }
        }
    }

    /// Calculate indexed EA: base + d8 + Xn.
    fn calc_index_ea(&mut self, base: u32, ext: u16) -> u32 {
        let disp = ext as u8 as i8 as i32;
        let xn_reg = ((ext >> 12) & 0x0F) as usize;
        let xn_long = ext & 0x0800 != 0;

        let xn_value = if xn_reg < 8 {
            self.regs.d[xn_reg]
        } else {
            self.regs.a(xn_reg - 8)
        };

        let xn = if xn_long {
            xn_value as i32
        } else {
            xn_value as i16 as i32
        };

        (base as i32).wrapping_add(disp).wrapping_add(xn) as u32
    }
}
