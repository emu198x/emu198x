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
use crate::cpu::{
    Cpu68000, TAG_EA_DST_DISP, TAG_EA_DST_LONG, TAG_EA_DST_PCDISP, TAG_EA_FF_AFTER_BD,
    TAG_EA_FF_INDIRECT_DONE, TAG_EA_FF_STREAM, TAG_EA_SRC_DISP, TAG_EA_SRC_LONG, TAG_EA_SRC_PCDISP,
    TAG_FETCH_DST_DATA, TAG_FETCH_SRC_DATA,
};
use crate::microcode::MicroOp;

impl Cpu68000 {
    /// Push the internal clocks spent calculating an indexed,
    /// predecrement, or computed effective address. The 68000 model uses
    /// a flat 2-clock approximation; when `variant_um_ea_calc_timing` is
    /// set (68020+), `clocks_020` applies instead — the M68020UM § 8.2.3
    /// "Calculate Effective Address" Cache-Case figure for the
    /// no-overlap model this engine targets (#41 Phase 4). Timing only;
    /// the computed address is identical either way.
    fn push_ea_calc_delay(&mut self, clocks_020: u8) {
        let clocks = if self.variant_um_ea_calc_timing {
            clocks_020
        } else {
            2
        };
        if clocks > 0 {
            self.micro_ops.push(MicroOp::Internal(clocks));
        }
    }

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
                self.regs
                    .set_a(r as usize, self.addr.wrapping_add(increment));
                self.ae_undo_reg = Some((r, increment, true, !is_src));
                true
            }

            // Pre-decrement: decrement register first, then use new value.
            // The 68000 spends 2 CPU clocks on the decrement calculation
            // before starting the bus read. The 68020 pipelines this.
            AddrMode::AddrIndPreDec(r) => {
                let decrement = if r == 7 && self.size == Size::Byte {
                    2
                } else {
                    self.size.bytes()
                };
                self.addr = self.regs.a(r as usize).wrapping_sub(decrement);
                self.regs.set_a(r as usize, self.addr);
                self.ae_undo_reg = Some((r, decrement, false, !is_src));
                // -(An): M68020UM § 8.2.3 Calculate EA CC = 2 (no overlap
                // benefit — same in every UM column, and the 68000 model
                // also uses 2).
                self.push_ea_calc_delay(2);
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
            // after fetching the extension word. The 68020 pipelines this.
            AddrMode::AddrIndIndex(r) => {
                let base = self.regs.a(r as usize);
                let ext = self.consume_irc();
                // Bit 8 set selects the 68020+ full extension word
                // format (base displacement / memory indirection /
                // outer displacement). On the 68000 / 68010 bit 8 is
                // part of the brief displacement and the full format
                // does not exist, so it is gated by the same
                // `variant_scaled_index` flag that enables scaling.
                if self.variant_scaled_index && ext & 0x0100 != 0 {
                    return self.ff_begin(ext, base, is_src);
                }
                // Brief extension word.
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
                // Bits 9-10: scale factor (1/2/4/8). 68020+ only —
                // gated by `variant_scaled_index`. On the 68000 /
                // 68010 these bits are "don't care" and the hardware
                // always uses scale=1.
                let scale = if self.variant_scaled_index {
                    1u32 << ((ext >> 9) & 0x3)
                } else {
                    1
                };
                self.addr = base
                    .wrapping_add(disp as u32)
                    .wrapping_add(idx.wrapping_mul(scale));
                // Brief (d8,An,Xn): M68020UM § 8.2.3 Calculate EA CC = 4.
                self.push_ea_calc_delay(4);
                true
            }

            // PC-relative with index: d8(PC,Xn)
            // The 68000 spends 2 CPU clocks computing base+disp+index
            // after fetching the extension word. The 68020 pipelines this.
            AddrMode::PcIndex => {
                self.program_space_access = true;
                let ext = self.consume_irc();
                // PC value at the extension word location — use irc_addr
                // so that prior consumed extension words are accounted for.
                let base = self.irc_addr;
                if self.variant_scaled_index && ext & 0x0100 != 0 {
                    return self.ff_begin(ext, base, is_src);
                }
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
                // Bits 9-10: scale factor (1/2/4/8). 68020+ only —
                // gated by `variant_scaled_index`.
                let scale = if self.variant_scaled_index {
                    1u32 << ((ext >> 9) & 0x3)
                } else {
                    1
                };
                self.addr = base
                    .wrapping_add(disp as u32)
                    .wrapping_add(idx.wrapping_mul(scale));
                // Brief (d8,PC,Xn): M68020UM § 8.2.3 Calculate EA CC = 4.
                self.push_ea_calc_delay(4);
                true
            } // All modes handled — DataReg/AddrReg/Immediate are instant,
              // all memory modes compute an address above.
        }
    }

    /// Scaled, sign-extended index register value from an extension
    /// word. The 68020 full format always honours the scale field
    /// (bits 10-9); this helper is only called on the 68020+ path.
    fn ext_scaled_index(&self, ext: u16) -> u32 {
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
        idx.wrapping_mul(1u32 << ((ext >> 9) & 0x3))
    }

    /// Begin a 68020 full-format indexed EA (extension word bit 8 set).
    ///
    /// Mirrors WinUAE `get_disp_ea_020` (newcpu_common.cpp). The
    /// common scaled-index case — null base displacement and no memory
    /// indirection, e.g. `(An,Xn*s)` — resolves synchronously and
    /// returns `true`. Cases that carry a base displacement, an outer
    /// displacement, or a memory-indirect long read cannot be served
    /// from the single prefetched IRC word; they stash the decoded
    /// pieces, set a `TAG_EA_FF_*` follow-up, and return `false`.
    fn ff_begin(&mut self, ext: u16, base_in: u32, is_src: bool) -> bool {
        let regd = if ext & 0x0040 != 0 {
            0 // IS: index suppress
        } else {
            self.ext_scaled_index(ext)
        };
        let base = if ext & 0x0080 != 0 {
            0 // BS: base suppress
        } else {
            base_in
        };
        // Bits 5-4: base-displacement size (00 reserved / 01 null →
        // none, 10 word, 11 long).
        let bd_words: u8 = match ext & 0x0030 {
            0x20 => 1,
            0x30 => 2,
            _ => 0,
        };
        // Bits 2-0: index/indirect selection. A non-zero low two bits
        // means a memory indirection is performed.
        let indirect = ext & 0x0003 != 0;

        if bd_words == 0 && !indirect {
            // Synchronous: EA = base + scaled index. This is the
            // full-format base+index "(B)" form — M68020UM § 8.2.3
            // Calculate EA CC = 6 (2 more than the brief (d8,An,Xn),
            // for the full extension-word decode). 68020-only path.
            self.addr = base.wrapping_add(regd);
            self.push_ea_calc_delay(6);
            return true;
        }

        self.ff_dp = ext;
        self.ff_base = base;
        self.ff_regd = regd;
        self.ff_outer = 0;
        self.ff_disp = 0;
        self.ff_is_src = is_src;
        if bd_words > 0 {
            self.ff_phase = 0;
            self.ff_stream_left = bd_words;
            self.followup_tag = TAG_EA_FF_STREAM;
        } else {
            // No base displacement, but a memory indirection remains.
            self.followup_tag = TAG_EA_FF_AFTER_BD;
        }
        false
    }

    /// Full format: the base displacement is applied (or was null).
    /// Set up the outer-displacement read, issue the memory-indirect
    /// long read, or finalise the non-indirect address. Shared by the
    /// `TAG_EA_FF_STREAM` (post-base-displacement) and
    /// `TAG_EA_FF_AFTER_BD` continuations.
    pub(crate) fn ff_after_bd(&mut self) {
        if self.ff_dp & 0x0003 != 0 {
            // Memory indirect — bits 1-0 also size the outer displacement
            // (01 null, 10 word, 11 long).
            let od_words: u8 = match self.ff_dp & 0x0003 {
                0x2 => 1,
                0x3 => 2,
                _ => 0,
            };
            if od_words > 0 {
                self.ff_phase = 1;
                self.ff_disp = 0;
                self.ff_stream_left = od_words;
                self.followup_tag = TAG_EA_FF_STREAM;
                self.micro_ops.push(MicroOp::Execute);
            } else {
                self.ff_outer = 0;
                self.ff_indirect_read();
            }
        } else {
            // No memory indirection: EA = base + scaled index.
            self.ff_base = self.ff_base.wrapping_add(self.ff_regd);
            self.addr = self.ff_base;
            self.followup_tag = if self.ff_is_src {
                TAG_FETCH_SRC_DATA
            } else {
                TAG_FETCH_DST_DATA
            };
            self.micro_ops.push(MicroOp::Execute);
        }
    }

    /// Full format: issue the memory-indirect long read of the
    /// (pre-indexed) base. The read result is picked up at
    /// `TAG_EA_FF_INDIRECT_DONE`.
    pub(crate) fn ff_indirect_read(&mut self) {
        if self.ff_dp & 0x0004 == 0 {
            // Pre-indexed: the index is added before the indirection.
            self.ff_base = self.ff_base.wrapping_add(self.ff_regd);
        }
        self.addr = self.ff_base;
        self.followup_tag = TAG_EA_FF_INDIRECT_DONE;
        self.queue_read_ops(Size::Long);
        self.micro_ops.push(MicroOp::Execute);
    }
}
