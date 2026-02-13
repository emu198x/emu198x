//! Effective address calculation for the 68000.

use super::Cpu68000;
use crate::common::addressing::AddrMode;
use crate::common::alu::Size;

impl Cpu68000 {
    /// Calculate effective address for an addressing mode using extension words.
    /// Returns the address and whether it's a register (not memory).
    pub(super) fn calc_ea(&mut self, mode: AddrMode, pc_at_ext: u32) -> (u32, bool) {
        // Track whether this is a PC-relative access for function code in address errors
        self.program_space_access = matches!(mode, AddrMode::PcDisp | AddrMode::PcIndex);
        match mode {
            AddrMode::DataReg(r) => (r as u32, true),
            AddrMode::AddrReg(r) => (r as u32, true),
            AddrMode::AddrInd(r) => (self.regs.a(r as usize), false),
            AddrMode::AddrIndPostInc(r) => {
                let addr = self.regs.a(r as usize);
                let inc = if self.size == Size::Byte && r == 7 {
                    2
                } else {
                    self.size_increment()
                };
                // Defer the increment until the memory access succeeds.
                self.deferred_postinc = Some((r, inc));
                (addr, false)
            }
            AddrMode::AddrIndPreDec(r) => {
                let dec = if self.size == Size::Byte && r == 7 {
                    2
                } else {
                    self.size_increment()
                };
                let addr = self.regs.a(r as usize).wrapping_sub(dec);
                self.regs.set_a(r as usize, addr);
                (addr, false)
            }
            AddrMode::AddrIndDisp(r) => {
                let disp = self.next_ext_word() as i16 as i32;
                let addr = (self.regs.a(r as usize) as i32).wrapping_add(disp) as u32;
                (addr, false)
            }
            AddrMode::AddrIndIndex(r) => {
                let base = self.regs.a(r as usize);
                (self.calc_index_ea(base), false)
            }
            AddrMode::AbsShort => {
                let addr = self.next_ext_word() as i16 as i32 as u32;
                (addr, false)
            }
            AddrMode::AbsLong => {
                let hi = self.next_ext_word();
                let lo = self.next_ext_word();
                let addr = (u32::from(hi) << 16) | u32::from(lo);
                (addr, false)
            }
            AddrMode::PcDisp => {
                let base_pc = pc_at_ext;
                let disp = self.next_ext_word() as i16 as i32;
                let addr = (base_pc as i32).wrapping_add(disp) as u32;
                (addr, false)
            }
            AddrMode::PcIndex => {
                let base_pc = pc_at_ext;
                (self.calc_index_ea(base_pc), false)
            }
            AddrMode::Immediate => {
                // Immediate data is in extension words, return a marker.
                (0, true)
            }
        }
    }

    /// Calculate indexed effective address: base + index + displacement.
    /// Uses the next extension word from the prefetch queue.
    pub(super) fn calc_index_ea(&mut self, base: u32) -> u32 {
        let ext = self.next_ext_word();
        let disp = (ext & 0xFF) as i8 as i32;
        let xn = ((ext >> 12) & 7) as usize;
        let is_addr = ext & 0x8000 != 0;
        let is_long = ext & 0x0800 != 0;
        let idx_val = if is_addr {
            self.regs.a(xn)
        } else {
            self.regs.d[xn]
        };
        let idx_val = if is_long {
            idx_val as i32
        } else {
            idx_val as i16 as i32
        };
        (base as i32).wrapping_add(disp).wrapping_add(idx_val) as u32
    }

    /// Get next extension word from the prefetch queue.
    /// Returns 0 if the queue is exhausted.
    pub(super) fn next_ext_word(&mut self) -> u16 {
        let idx = self.ext_idx as usize;
        if idx < self.ext_count as usize {
            self.ext_idx += 1;
            self.ext_words[idx]
        } else {
            0
        }
    }

    /// Read immediate value from extension words.
    pub(super) fn read_immediate(&mut self) -> u32 {
        match self.size {
            Size::Byte => u32::from(self.next_ext_word() & 0xFF),
            Size::Word => u32::from(self.next_ext_word()),
            Size::Long => {
                let hi = self.next_ext_word();
                let lo = self.next_ext_word();
                (u32::from(hi) << 16) | u32::from(lo)
            }
        }
    }

    /// Read value from data register based on size.
    pub(super) fn read_data_reg(&self, r: u8, size: Size) -> u32 {
        let val = self.regs.d[r as usize];
        match size {
            Size::Byte => val & 0xFF,
            Size::Word => val & 0xFFFF,
            Size::Long => val,
        }
    }

    /// Write value to data register based on size.
    pub(super) fn write_data_reg(&mut self, r: u8, value: u32, size: Size) {
        let reg = &mut self.regs.d[r as usize];
        *reg = match size {
            Size::Byte => (*reg & 0xFFFF_FF00) | (value & 0xFF),
            Size::Word => (*reg & 0xFFFF_0000) | (value & 0xFFFF),
            Size::Long => value,
        };
    }

    /// Count extension words needed for an addressing mode.
    pub(super) fn ext_words_for_mode(&self, mode: AddrMode) -> u8 {
        match mode {
            AddrMode::DataReg(_) | AddrMode::AddrReg(_) => 0,
            AddrMode::AddrInd(_) | AddrMode::AddrIndPostInc(_) | AddrMode::AddrIndPreDec(_) => 0,
            AddrMode::AddrIndDisp(_) | AddrMode::AddrIndIndex(_) => 1,
            AddrMode::AbsShort | AddrMode::PcDisp | AddrMode::PcIndex => 1,
            AddrMode::AbsLong => 2,
            AddrMode::Immediate => match self.size {
                Size::Byte | Size::Word => 1,
                Size::Long => 2,
            },
        }
    }

    /// Get the size increment for the current operation size.
    pub(super) fn size_increment(&self) -> u32 {
        match self.size {
            Size::Byte => 1,
            Size::Word => 2,
            Size::Long => 4,
        }
    }

    /// Check if the current instruction uses pre-decrement addressing mode.
    pub(super) fn uses_predec_mode(&self) -> bool {
        matches!(self.src_mode, Some(AddrMode::AddrIndPreDec(_)))
            || matches!(self.dst_mode, Some(AddrMode::AddrIndPreDec(_)))
    }
}
