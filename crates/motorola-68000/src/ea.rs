//! Effective address calculation for the 68000.
//!
//! The 68000's addressing modes range from instant (register direct) to
//! multi-cycle (absolute long requires two extension words). This module
//! handles the initial EA setup: computing the address for simple modes
//! and setting up follow-up tags for modes that need extension words.
//!
//! The follow-up tag state machine in `decode.rs` picks up where
//! `calc_ea_start` leaves off, consuming extension words from IRC as
//! they arrive from the bus.

use crate::addressing::AddrMode;
use crate::alu::Size;
use crate::cpu::{Cpu68000, TAG_EA_DST_DISP, TAG_EA_DST_LONG, TAG_EA_DST_PCDISP, TAG_EA_SRC_DISP, TAG_EA_SRC_LONG, TAG_EA_SRC_PCDISP};
use crate::microcode::MicroOp;

impl Cpu68000 {
    /// Begin effective address calculation for an addressing mode.
    ///
    /// Returns `true` if the EA is fully resolved (address in `self.addr`),
    /// `false` if extension words are needed (follow-up tag set, will
    /// resume via `continue_instruction`).
    ///
    /// For modes that resolve instantly:
    /// - `DataReg`, `AddrReg`, `Immediate`: no address needed (data comes later)
    /// - `AddrInd(r)`: address = A[r]
    /// - `AddrIndPostInc(r)`: address = A[r], then A[r] += size
    /// - `AddrIndPreDec(r)`: A[r] -= size, then address = A[r]
    /// - `AbsShort`: address = sign-extended IRC word (consumed immediately)
    ///
    /// For modes needing extension words:
    /// - `AddrIndDisp(r)`: needs d16 displacement word
    /// - `AbsLong`: needs two address words (hi then lo)
    /// - `PcDisp`: needs d16 displacement word, base = PC at extension word
    pub fn calc_ea_start(&mut self, mode: AddrMode, is_src: bool) -> bool {
        match mode {
            // Register direct and immediate: no address calculation needed
            AddrMode::DataReg(_) | AddrMode::AddrReg(_) | AddrMode::Immediate => true,

            // Address register indirect: address is register value
            AddrMode::AddrInd(r) => {
                self.addr = self.regs.a(r as usize);
                true
            }

            // Post-increment: use current value, then advance register
            AddrMode::AddrIndPostInc(r) => {
                self.addr = self.regs.a(r as usize);
                // A7 byte operations use 2 to keep SP word-aligned
                let increment = if r == 7 && self.size == Size::Byte {
                    2
                } else {
                    self.size.bytes()
                };
                self.regs.set_a(r as usize, self.addr.wrapping_add(increment));
                self.ae_undo_reg = Some((r, increment, true, !is_src));
                true
            }

            // Pre-decrement: decrement register first, then use new value.
            // The 68000 spends 2 CPU clocks on the decrement calculation
            // before starting the bus read.
            AddrMode::AddrIndPreDec(r) => {
                let decrement = if r == 7 && self.size == Size::Byte {
                    2
                } else {
                    self.size.bytes()
                };
                self.addr = self.regs.a(r as usize).wrapping_sub(decrement);
                self.regs.set_a(r as usize, self.addr);
                self.ae_undo_reg = Some((r, decrement, false, !is_src));
                self.micro_ops.push(MicroOp::Internal(2));
                true
            }

            // Displacement from address register: needs one extension word
            AddrMode::AddrIndDisp(r) => {
                self.ea_reg = r;
                self.followup_tag = if is_src {
                    TAG_EA_SRC_DISP
                } else {
                    TAG_EA_DST_DISP
                };
                false
            }

            // Absolute short: sign-extend 16-bit address from IRC
            AddrMode::AbsShort => {
                self.addr = (self.consume_irc() as i16 as i32) as u32;
                true
            }

            // Absolute long: needs two extension words (hi first, lo second)
            AddrMode::AbsLong => {
                self.addr = u32::from(self.consume_irc()) << 16;
                self.followup_tag = if is_src {
                    TAG_EA_SRC_LONG
                } else {
                    TAG_EA_DST_LONG
                };
                false
            }

            // PC with displacement: needs one extension word, base = current PC
            AddrMode::PcDisp => {
                // ea_pc captures PC value at the extension word location.
                // Use irc_addr (where the current IRC was fetched from) rather
                // than a hardcoded offset from instr_start_pc, because earlier
                // extension words (e.g. BTST #imm) may have already been consumed.
                self.ea_pc = self.irc_addr;
                self.program_space_access = true;
                self.followup_tag = if is_src {
                    TAG_EA_SRC_PCDISP
                } else {
                    TAG_EA_DST_PCDISP
                };
                false
            }

            // Address register indirect with index: d8(An,Xn)
            // Brief extension word format:
            //   bit 15: D/A (0=Dn, 1=An)
            //   bits 14-12: index register number
            //   bit 11: W/L (0=sign-extend word, 1=long)
            //   bits 7-0: signed 8-bit displacement
            // Address register indirect with index: d8(An,Xn)
            // The 68000 spends 2 CPU clocks computing base+disp+index
            // after fetching the extension word.
            AddrMode::AddrIndIndex(r) => {
                let ext = self.consume_irc();
                let base = self.regs.a(r as usize);
                let disp = (ext & 0xFF) as i8 as i32;
                let idx_reg = ((ext >> 12) & 7) as usize;
                let idx_val = if ext & 0x8000 != 0 {
                    self.regs.a(idx_reg)
                } else {
                    self.regs.d[idx_reg]
                };
                let idx = if ext & 0x0800 != 0 {
                    idx_val // long index
                } else {
                    idx_val as i16 as i32 as u32 // sign-extend word index
                };
                self.addr = base.wrapping_add(disp as u32).wrapping_add(idx);
                self.micro_ops.push(MicroOp::Internal(2));
                true
            }

            // PC-relative with index: d8(PC,Xn)
            // The 68000 spends 2 CPU clocks computing base+disp+index
            // after fetching the extension word.
            AddrMode::PcIndex => {
                self.program_space_access = true;
                let ext = self.consume_irc();
                // PC value at the extension word location — use irc_addr
                // so that prior consumed extension words are accounted for.
                let base = self.irc_addr;
                let disp = (ext & 0xFF) as i8 as i32;
                let idx_reg = ((ext >> 12) & 7) as usize;
                let idx_val = if ext & 0x8000 != 0 {
                    self.regs.a(idx_reg)
                } else {
                    self.regs.d[idx_reg]
                };
                let idx = if ext & 0x0800 != 0 {
                    idx_val // long index
                } else {
                    idx_val as i16 as i32 as u32 // sign-extend word index
                };
                self.addr = base.wrapping_add(disp as u32).wrapping_add(idx);
                self.micro_ops.push(MicroOp::Internal(2));
                true
            }

            // All modes handled — DataReg/AddrReg/Immediate are instant,
            // all memory modes compute an address above.
        }
    }
}
