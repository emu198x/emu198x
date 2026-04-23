//! Opcode dispatch and the m-cycle walker.
//!
//! Pin-level pipelined per
//! [`wiki/decisions/cpu-bus-interface.md`](../../../wiki/decisions/cpu-bus-interface.md):
//! each m-cycle "consumes" `data_in` populated by the machine after the
//! previous tick's scheduled read, then schedules the next bus
//! operation by setting `addr` / `rd` / `wr` / `mreq` (or
//! [`schedule_internal`](crate::Sm83::schedule_internal) for bus-idle
//! m-cycles). The very first tick after [`reset`](crate::Sm83::reset)
//! consumes the opcode read primed during reset itself.
//!
//! The Zig source's `m_cycle 0` is a no-op transition cycle — it ran
//! after the wrapper's synchronous opcode fetch and just bumped the
//! counter. In our pipelined model the opcode is consumed at the
//! boundary (`m_cycle == 0`) and the dispatch then runs with
//! `m_cycle == 1` (and onwards). The Zig m-cycle index N therefore
//! corresponds to our m-cycle index N+1; the total number of ticks per
//! instruction is the same.

use crate::Sm83;
use crate::alu::{self, AluOp};
use crate::cb::CbFamily;
use crate::{FLAG_C, FLAG_H, FLAG_N, FLAG_Z};

impl Sm83 {
    /// Advance one m-cycle.
    ///
    /// Reads `data_in` from the previous tick's scheduled bus
    /// operation and updates the output pins (`addr`, `data`, `rd`,
    /// `wr`, `mreq`) ready for the machine to perform the next read or
    /// write.
    pub fn tick(&mut self) {
        // `int_ack` is a one-shot pulse asserted by the dispatch path
        // for exactly one m-cycle. Clear it at every tick boundary so
        // the machine sees the strobe in the gap between the asserting
        // tick and this one, never longer.
        self.int_ack = false;

        if self.stopped {
            self.schedule_internal();
            return;
        }

        if self.halt_mode {
            // HALT: keep the pins quiet until an interrupt resolves the
            // wait. The wake check uses `irq_pending` (machine refreshes
            // it between ticks).
            if self.irq_pending != 0 {
                self.halt_mode = false;
                // Fall through into the boundary path so dispatch (if
                // IME is on) or normal opcode execution (if IME is
                // off) can take over.
            } else {
                self.schedule_internal();
                return;
            }
        }

        if self.dispatching {
            self.interrupt_dispatch();
            return;
        }

        if self.m_cycle == 0 {
            // Interrupt dispatch is decided at the instruction
            // boundary, *before* the next opcode is consumed and
            // *before* `ime_pending` is promoted. That ordering is
            // what gives `EI` its one-instruction delay: the boundary
            // immediately after EI sees `ime == false` and skips
            // dispatch, then promotes; the boundary after THAT sees
            // `ime == true` and dispatches.
            //
            // The opcode byte that was already read on the bus during
            // the primed fetch is a phantom read inherent to the
            // pipelined model — see
            // `wiki/decisions/cpu-bus-interface.md` and
            // `wiki/decisions/sm83-abstraction-level.md` for the
            // accepted pin-level / m-cycle trade-offs.
            if self.ime && self.irq_pending != 0 {
                self.dispatching = true;
                self.m_cycle = 1;
                self.schedule_internal();
                return;
            }

            // EI promotes only after the dispatch check has seen the
            // pre-promotion IME value.
            if self.ime_pending {
                self.ime_pending = false;
                self.ime = true;
            }

            // The opcode byte was scheduled by either the previous
            // instruction's final m-cycle, by `reset`, or by the
            // machine layer following an interrupt-acknowledge.
            let opcode = self.data_in;

            // HALT bug: when HALT runs with IME=0 and an interrupt is
            // already pending, the next opcode byte is fetched but PC
            // does not advance past it. The duplicate fetch is the
            // observable hardware quirk that Blargg cpu_instrs §02
            // exercises.
            if self.halt_bug {
                self.halt_bug = false;
            } else {
                self.pc = self.pc.wrapping_add(1);
            }

            self.opcode = opcode;
            self.m_cycle = 1;
        }

        self.dispatch();
    }

    /// 5-m-cycle interrupt dispatch sequence. The boundary tick in
    /// [`tick`](Self::tick) accounts for the first m-cycle (one
    /// internal wait); this walker covers the remaining four:
    ///
    /// | m-cycle | work                                              |
    /// |---------|---------------------------------------------------|
    /// | 1       | second internal wait                              |
    /// | 2       | push PC high (SP-=1, write)                       |
    /// | 3       | push PC low (SP-=1, write)                        |
    /// | 4       | set PC = vector, clear IME, strobe `int_ack`,     |
    /// |         | schedule the ISR's first opcode fetch             |
    ///
    /// Priority: the lowest set bit of `irq_pending` is served first,
    /// matching VBlank → STAT → Timer → Serial → Joypad. The bit is
    /// sampled on the final dispatch m-cycle — that's the documented
    /// hardware behaviour: if `IE` is cleared during the push cycles
    /// such that no bit remains pending, the dispatch jumps to
    /// `$0000` instead of a vector.
    fn interrupt_dispatch(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_internal();
                self.m_cycle = 2;
            }
            2 => {
                self.sp = self.sp.wrapping_sub(1);
                let pc_hi = (self.pc >> 8) as u8;
                self.schedule_write(self.sp, pc_hi);
                self.m_cycle = 3;
            }
            3 => {
                self.sp = self.sp.wrapping_sub(1);
                let pc_lo = (self.pc & 0xFF) as u8;
                self.schedule_write(self.sp, pc_lo);
                self.m_cycle = 4;
            }
            _ => {
                self.ime = false;
                let pending = self.irq_pending & 0x1F;
                if pending == 0 {
                    self.pc = 0x0000;
                } else {
                    let bit = pending.trailing_zeros() as u8;
                    self.pc = 0x0040 + u16::from(bit) * 8;
                    self.int_ack = true;
                    self.int_ack_bit = bit;
                }
                self.dispatching = false;
                self.m_cycle = 0;
                self.schedule_opcode_fetch(self.pc);
            }
        }
    }

    fn dispatch(&mut self) {
        match self.opcode {
            // -- NOP / single-m-cycle accumulator ops -----------------
            0x00 => self.finish_instruction(), // NOP
            0x07 => self.op_rlca(),
            0x0F => self.op_rrca(),
            0x17 => self.op_rla(),
            0x1F => self.op_rra(),
            0x27 => self.op_daa(),
            0x2F => self.op_cpl(),
            0x37 => self.op_scf(),
            0x3F => self.op_ccf(),

            // -- 16-bit register pair loads + ADD HL, rr --------------
            0x01 | 0x11 | 0x21 | 0x31 => self.op_ld_rr_d16(),
            0x09 | 0x19 | 0x29 | 0x39 => self.op_add_hl_rr(),
            0x08 => self.op_ld_a16_sp(),

            // -- A ↔ (rr) loads ---------------------------------------
            0x02 | 0x12 => self.op_ld_pair_a(),
            0x0A | 0x1A => self.op_ld_a_pair(),
            0x22 | 0x32 => self.op_ld_hli_hld_a(),
            0x2A | 0x3A => self.op_ld_a_hli_hld(),

            // -- 8/16-bit INC and DEC ---------------------------------
            0x03 | 0x13 | 0x23 | 0x33 => self.op_inc_rr(),
            0x0B | 0x1B | 0x2B | 0x3B => self.op_dec_rr(),
            0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x3C => self.op_inc_r(),
            0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x3D => self.op_dec_r(),
            0x34 => self.op_inc_hl_indirect(),
            0x35 => self.op_dec_hl_indirect(),

            // -- LD r, d8 + LD (HL), d8 -------------------------------
            0x06 | 0x0E | 0x16 | 0x1E | 0x26 | 0x2E | 0x3E => self.op_ld_r_d8(),
            0x36 => self.op_ld_hl_d8(),

            // -- Relative jumps ---------------------------------------
            0x18 => self.op_jr(),
            0x20 | 0x28 | 0x30 | 0x38 => self.op_jr_cc(),

            // -- STOP / HALT ------------------------------------------
            0x10 => self.op_stop(),
            0x76 => self.op_halt(),

            // -- 8-bit register / (HL) transfers ----------------------
            // $40..=$7F except $76 (HALT, handled above).
            0x40..=0x75 | 0x77..=0x7F => self.op_ld_r_r(),

            // -- ALU A, r / A,(HL) ------------------------------------
            0x80..=0xBF => self.op_alu_r(),

            // -- ALU A, d8 --------------------------------------------
            0xC6 | 0xCE | 0xD6 | 0xDE | 0xE6 | 0xEE | 0xF6 | 0xFE => self.op_alu_d8(),

            // -- Conditional + unconditional control flow -------------
            0xC0 | 0xC8 | 0xD0 | 0xD8 => self.op_ret_cc(),
            0xC2 | 0xCA | 0xD2 | 0xDA => self.op_jp_cc_a16(),
            0xC3 => self.op_jp_a16(),
            0xC4 | 0xCC | 0xD4 | 0xDC => self.op_call_cc_a16(),
            0xC9 => self.op_ret(),
            0xCD => self.op_call_a16(),
            0xD9 => self.op_reti(),
            0xE9 => self.op_jp_hl(),

            // -- Stack ops --------------------------------------------
            0xC1 | 0xD1 | 0xE1 | 0xF1 => self.op_pop_rr(),
            0xC5 | 0xD5 | 0xE5 | 0xF5 => self.op_push_rr(),
            0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF => self.op_rst(),

            // -- High-RAM (a8) and (C) loads --------------------------
            0xE0 => self.op_ldh_a8_a(),
            0xF0 => self.op_ldh_a_a8(),
            0xE2 => self.op_ld_ffc_a(),
            0xF2 => self.op_ld_a_ffc(),

            // -- Absolute (a16) loads ---------------------------------
            0xEA => self.op_ld_a16_a(),
            0xFA => self.op_ld_a_a16(),

            // -- Stack-pointer arithmetic -----------------------------
            0xE8 => self.op_add_sp_r8(),
            0xF8 => self.op_ld_hl_sp_r8(),
            0xF9 => self.op_ld_sp_hl(),

            // -- Interrupt master enable ------------------------------
            0xF3 => self.op_di(),
            0xFB => self.op_ei(),

            // -- CB-prefixed sub-table --------------------------------
            0xCB => self.op_cb_prefix(),

            _ => self.diag_unimplemented_opcode(),
        }
    }

    // -- Helpers ---------------------------------------------------------

    /// Convenience for instructions whose final-m-cycle work is done in
    /// the dispatch arm itself: clears in-progress state and schedules
    /// the next opcode fetch at PC.
    #[inline]
    fn finish_instruction(&mut self) {
        self.m_cycle = 0;
        self.schedule_opcode_fetch(self.pc);
    }

    /// Mark an opcode we haven't ported yet. Real hardware has no
    /// unimplemented opcodes; this is a porting safety net so tests
    /// can detect "we hit an opcode we haven't covered" cleanly.
    #[inline]
    fn diag_unimplemented_opcode(&mut self) {
        self.diag_unimplemented = true;
        self.m_cycle = 0;
        self.schedule_opcode_fetch(self.pc);
    }

    // -- Instruction implementations -------------------------------------

    /// `LD rr, d16` — 3 m-cycles (fetch, read lo, read hi). `rr` is
    /// `BC`/`DE`/`HL`/`SP` per opcode bits 5-4.
    fn op_ld_rr_d16(&mut self) {
        match self.m_cycle {
            1 => {
                // First post-fetch tick: schedule the low-byte read.
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 3;
            }
            _ => {
                self.w = self.data_in;
                let pair = (self.opcode >> 4) & 0b11;
                let value = self.wz();
                self.write_reg16_sp(pair, value);
                self.finish_instruction();
            }
        }
    }

    /// `LD (BC), A` / `LD (DE), A` — 2 m-cycles. Picks BC or DE from
    /// opcode bit 4.
    fn op_ld_pair_a(&mut self) {
        match self.m_cycle {
            1 => {
                let addr = if (self.opcode & 0x10) == 0 {
                    self.bc()
                } else {
                    self.de()
                };
                self.schedule_write(addr, self.a);
                self.m_cycle = 2;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `LD A, (BC)` / `LD A, (DE)` — 2 m-cycles.
    fn op_ld_a_pair(&mut self) {
        match self.m_cycle {
            1 => {
                let addr = if (self.opcode & 0x10) == 0 {
                    self.bc()
                } else {
                    self.de()
                };
                self.schedule_read(addr);
                self.m_cycle = 2;
            }
            _ => {
                self.a = self.data_in;
                self.finish_instruction();
            }
        }
    }

    /// `LD (HL+), A` ($22) / `LD (HL-), A` ($32) — 2 m-cycles. Writes
    /// `A` to `(HL)` and post-increments / decrements `HL`.
    fn op_ld_hli_hld_a(&mut self) {
        match self.m_cycle {
            1 => {
                let addr = self.hl();
                self.schedule_write(addr, self.a);
                let new_hl = if self.opcode == 0x22 {
                    addr.wrapping_add(1)
                } else {
                    addr.wrapping_sub(1)
                };
                self.set_hl(new_hl);
                self.m_cycle = 2;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `LD A, (HL+)` ($2A) / `LD A, (HL-)` ($3A) — 2 m-cycles.
    fn op_ld_a_hli_hld(&mut self) {
        match self.m_cycle {
            1 => {
                let addr = self.hl();
                self.schedule_read(addr);
                let new_hl = if self.opcode == 0x2A {
                    addr.wrapping_add(1)
                } else {
                    addr.wrapping_sub(1)
                };
                self.set_hl(new_hl);
                self.m_cycle = 2;
            }
            _ => {
                self.a = self.data_in;
                self.finish_instruction();
            }
        }
    }

    /// `INC rr` — 2 m-cycles, no flags. The second m-cycle is internal
    /// (no bus op) — that's the cycle that costs a tick despite the
    /// 16-bit increment fitting the same bus width as 8-bit ops.
    fn op_inc_rr(&mut self) {
        match self.m_cycle {
            1 => {
                let pair = (self.opcode >> 4) & 0b11;
                let value = self.read_reg16_sp(pair).wrapping_add(1);
                self.write_reg16_sp(pair, value);
                self.schedule_internal();
                self.m_cycle = 2;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `DEC rr` — 2 m-cycles, no flags.
    fn op_dec_rr(&mut self) {
        match self.m_cycle {
            1 => {
                let pair = (self.opcode >> 4) & 0b11;
                let value = self.read_reg16_sp(pair).wrapping_sub(1);
                self.write_reg16_sp(pair, value);
                self.schedule_internal();
                self.m_cycle = 2;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `INC r` — 1 m-cycle. C preserved, Z/N/H per [`Sm83::alu_inc8`].
    fn op_inc_r(&mut self) {
        let reg = (self.opcode >> 3) & 0b111;
        let value = self.read_reg8(reg);
        let result = self.alu_inc8(value);
        self.write_reg8(reg, result);
        self.finish_instruction();
    }

    /// `DEC r` — 1 m-cycle. C preserved, Z/N/H per [`Sm83::alu_dec8`].
    fn op_dec_r(&mut self) {
        let reg = (self.opcode >> 3) & 0b111;
        let value = self.read_reg8(reg);
        let result = self.alu_dec8(value);
        self.write_reg8(reg, result);
        self.finish_instruction();
    }

    /// `LD r, d8` — 2 m-cycles. Excludes LD (HL),d8 at $36 (3M; landed
    /// in step 4).
    fn op_ld_r_d8(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            _ => {
                let dst = (self.opcode >> 3) & 0b111;
                self.write_reg8(dst, self.data_in);
                self.finish_instruction();
            }
        }
    }

    /// `JR r8` — unconditional relative jump. 3 m-cycles: fetch, read
    /// signed offset, internal compute.
    fn op_jr(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                let offset = i16::from(self.z as i8);
                self.pc = (self.pc as i16).wrapping_add(offset) as u16;
                self.schedule_internal();
                self.m_cycle = 3;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `JR cc, r8` — conditional relative jump. 2 m-cycles when the
    /// condition fails (offset read but not applied), 3 m-cycles when
    /// taken (offset applied via an internal compute cycle).
    fn op_jr_cc(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                if self.condition_met() {
                    let offset = i16::from(self.z as i8);
                    self.pc = (self.pc as i16).wrapping_add(offset) as u16;
                    self.schedule_internal();
                    self.m_cycle = 3;
                } else {
                    self.finish_instruction();
                }
            }
            _ => self.finish_instruction(),
        }
    }

    /// `LD r, r` / `LD r, (HL)` / `LD (HL), r` — 1 m-cycle for the
    /// pure register transfer; 2 m-cycles when either operand is
    /// `(HL)`. `$76` (HALT) is excluded by the dispatch ranges.
    fn op_ld_r_r(&mut self) {
        let dst = (self.opcode >> 3) & 0b111;
        let src = self.opcode & 0b111;

        match (self.m_cycle, dst, src) {
            // LD r, (HL) — first m-cycle: schedule read.
            (1, dst, 6) => {
                debug_assert!(dst != 6, "$76 is HALT, dispatch should have excluded it");
                self.schedule_read(self.hl());
                self.m_cycle = 2;
            }
            // LD r, (HL) — second m-cycle: latch result.
            (_, dst, 6) => {
                self.write_reg8(dst, self.data_in);
                self.finish_instruction();
            }
            // LD (HL), r — first m-cycle: schedule write.
            (1, 6, src) => {
                self.schedule_write(self.hl(), self.read_reg8(src));
                self.m_cycle = 2;
            }
            // LD (HL), r — second m-cycle.
            (_, 6, _) => self.finish_instruction(),
            // LD r, r — single m-cycle.
            (_, dst, src) => {
                let value = self.read_reg8(src);
                self.write_reg8(dst, value);
                self.finish_instruction();
            }
        }
    }

    /// `ALU A, r` / `ALU A, (HL)` ($80..$BF) — 1 m-cycle for register
    /// operands; 2 m-cycles when the operand is `(HL)`.
    fn op_alu_r(&mut self) {
        let src = self.opcode & 0b111;
        let op = AluOp::from_opcode_bits(self.opcode);

        match (self.m_cycle, src) {
            (1, 6) => {
                self.schedule_read(self.hl());
                self.m_cycle = 2;
            }
            (_, 6) => {
                let operand = self.data_in;
                self.alu(op, operand);
                self.finish_instruction();
            }
            (_, src) => {
                let operand = self.read_reg8(src);
                self.alu(op, operand);
                self.finish_instruction();
            }
        }
    }

    /// `ALU A, d8` — 2 m-cycles (fetch, immediate-byte read +
    /// in-place ALU).
    fn op_alu_d8(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            _ => {
                let op = AluOp::from_opcode_bits(self.opcode);
                let operand = self.data_in;
                self.alu(op, operand);
                self.finish_instruction();
            }
        }
    }

    /// `JP a16` — 4 m-cycles (fetch, read lo, read hi, internal
    /// branch-cost cycle). The internal cycle is the documented extra
    /// tick that distinguishes JP from a simple register-pair load.
    fn op_jp_a16(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 3;
            }
            3 => {
                self.w = self.data_in;
                self.pc = self.wz();
                self.schedule_internal();
                self.m_cycle = 4;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `JP cc, a16` — 3 m-cycles when the condition fails (imm read
    /// but not applied); 4 m-cycles when taken (extra internal cycle
    /// matches unconditional JP).
    fn op_jp_cc_a16(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 3;
            }
            3 => {
                self.w = self.data_in;
                if self.condition_met() {
                    self.pc = self.wz();
                    self.schedule_internal();
                    self.m_cycle = 4;
                } else {
                    self.finish_instruction();
                }
            }
            _ => self.finish_instruction(),
        }
    }

    /// `JP (HL)` — 1 m-cycle. Despite the mnemonic this is *not* an
    /// indirect jump; HL is loaded directly into PC without any bus
    /// dereference.
    fn op_jp_hl(&mut self) {
        self.pc = self.hl();
        self.finish_instruction();
    }

    /// `CALL a16` ($CD) — 6 m-cycles. Fetches target, then uses an
    /// internal cycle to pre-decrement SP before pushing PC high /
    /// low, finally setting PC to the fetched target.
    fn op_call_a16(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 3;
            }
            3 => {
                self.w = self.data_in;
                self.sp = self.sp.wrapping_sub(1);
                self.schedule_internal();
                self.m_cycle = 4;
            }
            4 => {
                let pc_hi = (self.pc >> 8) as u8;
                self.schedule_write(self.sp, pc_hi);
                self.sp = self.sp.wrapping_sub(1);
                self.m_cycle = 5;
            }
            5 => {
                let pc_lo = (self.pc & 0xFF) as u8;
                self.schedule_write(self.sp, pc_lo);
                self.pc = self.wz();
                self.m_cycle = 6;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `CALL cc, a16` — 3 m-cycles when the condition fails, 6 when
    /// taken (matches the unconditional CALL path).
    fn op_call_cc_a16(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 3;
            }
            3 => {
                self.w = self.data_in;
                if self.condition_met() {
                    self.sp = self.sp.wrapping_sub(1);
                    self.schedule_internal();
                    self.m_cycle = 4;
                } else {
                    self.finish_instruction();
                }
            }
            4 => {
                let pc_hi = (self.pc >> 8) as u8;
                self.schedule_write(self.sp, pc_hi);
                self.sp = self.sp.wrapping_sub(1);
                self.m_cycle = 5;
            }
            5 => {
                let pc_lo = (self.pc & 0xFF) as u8;
                self.schedule_write(self.sp, pc_lo);
                self.pc = self.wz();
                self.m_cycle = 6;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `RET` — 4 m-cycles. Pops PC from the stack, then takes one
    /// internal cycle before the next opcode fetch.
    fn op_ret(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                self.schedule_read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                self.m_cycle = 3;
            }
            3 => {
                self.w = self.data_in;
                self.pc = self.wz();
                self.schedule_internal();
                self.m_cycle = 4;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `RET cc` — 2 m-cycles when the condition fails (only the
    /// internal condition-eval cycle runs), 5 when taken (one extra
    /// condition-eval cycle on top of `RET`'s 4).
    fn op_ret_cc(&mut self) {
        match self.m_cycle {
            1 => {
                // Internal cycle evaluates the condition. The pop
                // itself only starts if taken.
                self.schedule_internal();
                self.m_cycle = 2;
            }
            2 => {
                if self.condition_met() {
                    self.schedule_read(self.sp);
                    self.sp = self.sp.wrapping_add(1);
                    self.m_cycle = 3;
                } else {
                    self.finish_instruction();
                }
            }
            3 => {
                self.z = self.data_in;
                self.schedule_read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                self.m_cycle = 4;
            }
            4 => {
                self.w = self.data_in;
                self.pc = self.wz();
                self.schedule_internal();
                self.m_cycle = 5;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `RETI` — identical to `RET` with `IME` re-enabled *immediately*
    /// (no one-instruction delay, unlike `EI`).
    fn op_reti(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                self.schedule_read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                self.m_cycle = 3;
            }
            3 => {
                self.w = self.data_in;
                self.pc = self.wz();
                self.ime = true;
                self.schedule_internal();
                self.m_cycle = 4;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `RST n` — 4 m-cycles. Pushes PC, then jumps to a fixed `$00`,
    /// `$08`, …, `$38` vector derived from opcode bits 5-3.
    fn op_rst(&mut self) {
        match self.m_cycle {
            1 => {
                self.sp = self.sp.wrapping_sub(1);
                self.schedule_internal();
                self.m_cycle = 2;
            }
            2 => {
                let pc_hi = (self.pc >> 8) as u8;
                self.schedule_write(self.sp, pc_hi);
                self.sp = self.sp.wrapping_sub(1);
                self.m_cycle = 3;
            }
            3 => {
                let pc_lo = (self.pc & 0xFF) as u8;
                self.schedule_write(self.sp, pc_lo);
                self.pc = u16::from(self.opcode & 0x38);
                self.m_cycle = 4;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `PUSH rr` — 4 m-cycles. The first is an internal SP-decrement
    /// delay before the two pushes land on the bus.
    fn op_push_rr(&mut self) {
        match self.m_cycle {
            1 => {
                self.sp = self.sp.wrapping_sub(1);
                self.schedule_internal();
                self.m_cycle = 2;
            }
            2 => {
                let pair = (self.opcode >> 4) & 0b11;
                let value = self.read_reg16_af(pair);
                let hi = (value >> 8) as u8;
                self.w = (value & 0xFF) as u8; // stash the low byte for cycle 3
                self.schedule_write(self.sp, hi);
                self.sp = self.sp.wrapping_sub(1);
                self.m_cycle = 3;
            }
            3 => {
                let lo = self.w;
                self.schedule_write(self.sp, lo);
                self.m_cycle = 4;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `POP rr` — 3 m-cycles. The low nibble of F is masked off for
    /// `POP AF`, mirroring the hardwired zero on real silicon.
    fn op_pop_rr(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                self.schedule_read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                self.m_cycle = 3;
            }
            _ => {
                self.w = self.data_in;
                let pair = (self.opcode >> 4) & 0b11;
                let value = self.wz();
                self.write_reg16_af(pair, value);
                self.finish_instruction();
            }
        }
    }

    // -- Single-m-cycle accumulator rotates (distinct from CB rotates
    //    in that they always clear Z). ------------------------------------

    fn op_rlca(&mut self) {
        let bit7 = self.a >> 7;
        self.a = (self.a << 1) | bit7;
        self.f = if bit7 != 0 { FLAG_C } else { 0 };
        self.finish_instruction();
    }

    fn op_rrca(&mut self) {
        let bit0 = self.a & 1;
        self.a = (self.a >> 1) | (bit0 << 7);
        self.f = if bit0 != 0 { FLAG_C } else { 0 };
        self.finish_instruction();
    }

    fn op_rla(&mut self) {
        let carry_in = if (self.f & FLAG_C) != 0 { 1 } else { 0 };
        let carry_out = (self.a & 0x80) != 0;
        self.a = (self.a << 1) | carry_in;
        self.f = if carry_out { FLAG_C } else { 0 };
        self.finish_instruction();
    }

    fn op_rra(&mut self) {
        let carry_in: u8 = if (self.f & FLAG_C) != 0 { 0x80 } else { 0 };
        let carry_out = (self.a & 1) != 0;
        self.a = (self.a >> 1) | carry_in;
        self.f = if carry_out { FLAG_C } else { 0 };
        self.finish_instruction();
    }

    fn op_daa(&mut self) {
        self.daa();
        self.finish_instruction();
    }

    fn op_cpl(&mut self) {
        self.a = !self.a;
        self.f = (self.f & (FLAG_Z | FLAG_C)) | FLAG_N | FLAG_H;
        self.finish_instruction();
    }

    fn op_scf(&mut self) {
        self.f = (self.f & FLAG_Z) | FLAG_C;
        self.finish_instruction();
    }

    fn op_ccf(&mut self) {
        // Preserve Z and the existing C; clear N and H; flip C.
        self.f = (self.f & (FLAG_Z | FLAG_C)) ^ FLAG_C;
        self.finish_instruction();
    }

    // -- IME control -----------------------------------------------------

    fn op_di(&mut self) {
        self.ime = false;
        self.ime_pending = false;
        self.finish_instruction();
    }

    /// `EI` arms the one-instruction delay: `ime_pending` flips at
    /// this m-cycle, and the next opcode boundary promotes it to
    /// `ime`. That's why a pending interrupt right after an `EI`
    /// isn't taken until the instruction following `EI` completes —
    /// exactly what Blargg and mooneye-gb verify.
    fn op_ei(&mut self) {
        self.ime_pending = true;
        self.finish_instruction();
    }

    // -- HALT / STOP -----------------------------------------------------

    fn op_halt(&mut self) {
        // HALT bug: with IME=0 and an interrupt already pending, the
        // CPU does NOT enter halt_mode. Instead, the byte following
        // HALT is fetched but PC doesn't advance past it, causing a
        // single-byte re-execution. The latch is consumed on the next
        // opcode boundary.
        if !self.ime && self.irq_pending != 0 {
            self.halt_bug = true;
        } else {
            self.halt_mode = true;
        }
        self.finish_instruction();
    }

    fn op_stop(&mut self) {
        // STOP is a two-byte instruction that skips the second byte
        // (documented as "usually $00"). On real hardware it halts
        // the entire SoC until a button press; here we stash the
        // condition and let the machine clear it when the joypad
        // transitions.
        self.pc = self.pc.wrapping_add(1);
        self.stopped = true;
        self.finish_instruction();
    }

    // -- LD (HL), d8 / INC (HL) / DEC (HL) -------------------------------

    fn op_ld_hl_d8(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                self.schedule_write(self.hl(), self.z);
                self.m_cycle = 3;
            }
            _ => self.finish_instruction(),
        }
    }

    fn op_inc_hl_indirect(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.hl());
                self.m_cycle = 2;
            }
            2 => {
                let value = self.data_in;
                let result = self.alu_inc8(value);
                self.schedule_write(self.hl(), result);
                self.m_cycle = 3;
            }
            _ => self.finish_instruction(),
        }
    }

    fn op_dec_hl_indirect(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.hl());
                self.m_cycle = 2;
            }
            2 => {
                let value = self.data_in;
                let result = self.alu_dec8(value);
                self.schedule_write(self.hl(), result);
                self.m_cycle = 3;
            }
            _ => self.finish_instruction(),
        }
    }

    // -- LDH / (C) / absolute loads --------------------------------------

    fn op_ldh_a8_a(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                self.schedule_write(0xFF00 | u16::from(self.z), self.a);
                self.m_cycle = 3;
            }
            _ => self.finish_instruction(),
        }
    }

    fn op_ldh_a_a8(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                self.schedule_read(0xFF00 | u16::from(self.z));
                self.m_cycle = 3;
            }
            _ => {
                self.a = self.data_in;
                self.finish_instruction();
            }
        }
    }

    fn op_ld_ffc_a(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_write(0xFF00 | u16::from(self.c), self.a);
                self.m_cycle = 2;
            }
            _ => self.finish_instruction(),
        }
    }

    fn op_ld_a_ffc(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(0xFF00 | u16::from(self.c));
                self.m_cycle = 2;
            }
            _ => {
                self.a = self.data_in;
                self.finish_instruction();
            }
        }
    }

    fn op_ld_a16_a(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 3;
            }
            3 => {
                self.w = self.data_in;
                self.schedule_write(self.wz(), self.a);
                self.m_cycle = 4;
            }
            _ => self.finish_instruction(),
        }
    }

    fn op_ld_a_a16(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 3;
            }
            3 => {
                self.w = self.data_in;
                self.schedule_read(self.wz());
                self.m_cycle = 4;
            }
            _ => {
                self.a = self.data_in;
                self.finish_instruction();
            }
        }
    }

    fn op_ld_a16_sp(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 3;
            }
            3 => {
                self.w = self.data_in;
                let sp_lo = (self.sp & 0xFF) as u8;
                self.schedule_write(self.wz(), sp_lo);
                self.m_cycle = 4;
            }
            4 => {
                let sp_hi = (self.sp >> 8) as u8;
                let addr = self.wz().wrapping_add(1);
                self.schedule_write(addr, sp_hi);
                self.m_cycle = 5;
            }
            _ => self.finish_instruction(),
        }
    }

    // -- SP / HL arithmetic ----------------------------------------------

    fn op_add_hl_rr(&mut self) {
        match self.m_cycle {
            1 => {
                let pair = (self.opcode >> 4) & 0b11;
                let hl = u32::from(self.hl());
                let rr = u32::from(self.read_reg16_sp(pair));
                let result = hl + rr;

                let mut flags = self.f & FLAG_Z; // preserve Z
                if (hl & 0xFFF) + (rr & 0xFFF) > 0xFFF {
                    flags |= FLAG_H;
                }
                if result > 0xFFFF {
                    flags |= FLAG_C;
                }
                self.set_hl(result as u16);
                self.f = flags;
                self.schedule_internal();
                self.m_cycle = 2;
            }
            _ => self.finish_instruction(),
        }
    }

    fn op_add_sp_r8(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                let (_, flags) = alu::sp_add_offset(self.sp, self.z);
                self.f = flags;
                self.schedule_internal();
                self.m_cycle = 3;
            }
            3 => {
                let (result, _) = alu::sp_add_offset(self.sp, self.z);
                self.sp = result;
                self.schedule_internal();
                self.m_cycle = 4;
            }
            _ => self.finish_instruction(),
        }
    }

    fn op_ld_hl_sp_r8(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                let (result, flags) = alu::sp_add_offset(self.sp, self.z);
                self.set_hl(result);
                self.f = flags;
                self.schedule_internal();
                self.m_cycle = 3;
            }
            _ => self.finish_instruction(),
        }
    }

    fn op_ld_sp_hl(&mut self) {
        match self.m_cycle {
            1 => {
                self.sp = self.hl();
                self.schedule_internal();
                self.m_cycle = 2;
            }
            _ => self.finish_instruction(),
        }
    }

    // -- CB prefix dispatch ---------------------------------------------

    /// CB prefix. Variable length: 2 m-cycles for register-operand
    /// ops, 3 for `BIT b,(HL)` (test only), 4 for any (HL) op that
    /// has to write back.
    fn op_cb_prefix(&mut self) {
        match self.m_cycle {
            1 => {
                // Fetch the CB sub-opcode.
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                if (self.z & 0b111) == 6 {
                    // (HL) operand — need a read m-cycle next.
                    self.schedule_read(self.hl());
                    self.m_cycle = 3;
                } else {
                    // Register operand — the work completes here.
                    let cb_op = self.z;
                    self.cb_execute_reg(cb_op);
                    self.finish_instruction();
                }
            }
            3 => {
                let cb_op = self.z;
                self.w = self.data_in;
                match CbFamily::from_cb_opcode(cb_op) {
                    CbFamily::Bit => {
                        // BIT b,(HL) — test only, no write-back.
                        let value = self.w;
                        self.cb_bit_test(cb_op, value);
                        self.finish_instruction();
                    }
                    _ => {
                        // RLC/RRC/RL/RR/SLA/SRA/SWAP/SRL/RES/SET — modify
                        // then schedule the write-back m-cycle.
                        let modified = self.cb_modify(cb_op, self.w);
                        self.schedule_write(self.hl(), modified);
                        self.m_cycle = 4;
                    }
                }
            }
            _ => self.finish_instruction(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FLAG_C, FLAG_H, FLAG_N, FLAG_Z};

    /// Minimal 64 KiB RAM fixture that mediates between CPU pin state
    /// and an in-process backing store. Mirrors the conversion pattern
    /// in `wiki/decisions/cpu-bus-interface.md`: between ticks, route
    /// the CPU's pins to / from RAM and populate `data_in`.
    struct TestBus {
        ram: Vec<u8>,
    }

    impl TestBus {
        fn new() -> Self {
            Self {
                ram: vec![0; 0x10000],
            }
        }

        fn step(&mut self, cpu: &mut Sm83) {
            if cpu.mreq {
                if cpu.rd {
                    cpu.data_in = self.ram[cpu.addr as usize];
                } else if cpu.wr {
                    self.ram[cpu.addr as usize] = cpu.data;
                }
            }
            cpu.tick();
        }

        fn run(&mut self, cpu: &mut Sm83, ticks: usize) {
            for _ in 0..ticks {
                self.step(cpu);
            }
        }
    }

    fn boot(program: &[u8]) -> (Sm83, TestBus) {
        let mut bus = TestBus::new();
        bus.ram[..program.len()].copy_from_slice(program);
        let mut cpu = Sm83::new();
        cpu.reset();
        (cpu, bus)
    }

    // -- NOP -------------------------------------------------------------

    #[test]
    fn nop_advances_pc_by_one_per_m_cycle() {
        let (mut cpu, mut bus) = boot(&[0x00]);
        bus.step(&mut cpu);
        assert_eq!(cpu.pc, 1);
        assert!(cpu.instruction_complete());
        assert!(!cpu.diag_unimplemented);
    }

    #[test]
    fn ten_consecutive_nops_advance_pc_by_ten() {
        let (mut cpu, mut bus) = boot(&[0; 10]);
        bus.run(&mut cpu, 10);
        assert_eq!(cpu.pc, 10);
        assert!(!cpu.diag_unimplemented);
    }

    // -- LD rr, d16 ------------------------------------------------------

    #[test]
    fn ld_sp_d16_takes_three_ticks() {
        let (mut cpu, mut bus) = boot(&[0x31, 0xFE, 0xFF]);
        bus.step(&mut cpu);
        assert_eq!(cpu.sp, 0); // not set yet
        bus.step(&mut cpu);
        bus.step(&mut cpu);
        assert_eq!(cpu.sp, 0xFFFE);
        assert_eq!(cpu.pc, 3);
        assert!(cpu.instruction_complete());
    }

    #[test]
    fn ld_hl_d16_loads_h_and_l() {
        let (mut cpu, mut bus) = boot(&[0x21, 0xFF, 0x9F]);
        bus.run(&mut cpu, 3);
        assert_eq!(cpu.h, 0x9F);
        assert_eq!(cpu.l, 0xFF);
    }

    #[test]
    fn ld_de_d16_loads_d_and_e() {
        let (mut cpu, mut bus) = boot(&[0x11, 0x04, 0x01]);
        bus.run(&mut cpu, 3);
        assert_eq!(cpu.d, 0x01);
        assert_eq!(cpu.e, 0x04);
    }

    // -- LD r, d8 --------------------------------------------------------

    #[test]
    fn ld_a_d8_loads_immediate_into_a() {
        let (mut cpu, mut bus) = boot(&[0x3E, 0x42]);
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.a, 0x42);
        assert_eq!(cpu.pc, 2);
    }

    #[test]
    fn ld_b_d8_loads_immediate_into_b() {
        let (mut cpu, mut bus) = boot(&[0x06, 0x99]);
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.b, 0x99);
    }

    // -- LD r, r and LD (HL),r / LD r,(HL) -------------------------------

    #[test]
    fn ld_b_a_copies_register_in_one_tick() {
        let (mut cpu, mut bus) = boot(&[0x47]);
        cpu.a = 0xAB;
        bus.step(&mut cpu);
        assert_eq!(cpu.b, 0xAB);
        assert!(cpu.instruction_complete());
    }

    #[test]
    fn ld_hl_indirect_a_writes_to_memory() {
        let (mut cpu, mut bus) = boot(&[0x77]);
        cpu.a = 0xFF;
        cpu.set_hl(0xC000);
        bus.run(&mut cpu, 2);
        assert_eq!(bus.ram[0xC000], 0xFF);
    }

    #[test]
    fn ld_a_hl_indirect_reads_from_memory() {
        let (mut cpu, mut bus) = boot(&[0x7E]);
        bus.ram[0x8000] = 0xBE;
        cpu.set_hl(0x8000);
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.a, 0xBE);
    }

    // -- LD A,(BC/DE/HL+/HL-) and writes ---------------------------------

    #[test]
    fn ld_a_de_indirect_reads_from_de() {
        let (mut cpu, mut bus) = boot(&[0x1A]);
        bus.ram[0x0104] = 0xCE;
        cpu.set_de(0x0104);
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.a, 0xCE);
    }

    #[test]
    fn ld_bc_indirect_a_writes_to_bc() {
        let (mut cpu, mut bus) = boot(&[0x02]);
        cpu.a = 0x42;
        cpu.set_bc(0xC000);
        bus.run(&mut cpu, 2);
        assert_eq!(bus.ram[0xC000], 0x42);
    }

    #[test]
    fn ld_hld_a_decrements_hl() {
        let (mut cpu, mut bus) = boot(&[0x32]);
        cpu.a = 0x42;
        cpu.set_hl(0x9FFF);
        bus.run(&mut cpu, 2);
        assert_eq!(bus.ram[0x9FFF], 0x42);
        assert_eq!(cpu.hl(), 0x9FFE);
    }

    #[test]
    fn ld_a_hli_increments_hl() {
        let (mut cpu, mut bus) = boot(&[0x2A]);
        bus.ram[0x8000] = 0x7F;
        cpu.set_hl(0x8000);
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.a, 0x7F);
        assert_eq!(cpu.hl(), 0x8001);
    }

    // -- INC / DEC -------------------------------------------------------

    #[test]
    fn inc_c_sets_half_carry_on_nibble_overflow() {
        let (mut cpu, mut bus) = boot(&[0x0C]);
        cpu.c = 0x0F;
        bus.step(&mut cpu);
        assert_eq!(cpu.c, 0x10);
        assert_ne!(cpu.f & FLAG_H, 0);
        assert_eq!(cpu.f & FLAG_N, 0);
    }

    #[test]
    fn dec_b_sets_half_carry_on_nibble_borrow() {
        let (mut cpu, mut bus) = boot(&[0x05]);
        cpu.b = 0x10;
        bus.step(&mut cpu);
        assert_eq!(cpu.b, 0x0F);
        assert_ne!(cpu.f & FLAG_H, 0);
        assert_ne!(cpu.f & FLAG_N, 0);
    }

    #[test]
    fn dec_b_to_zero_sets_zero_flag() {
        let (mut cpu, mut bus) = boot(&[0x05]);
        cpu.b = 0x01;
        bus.step(&mut cpu);
        assert_eq!(cpu.b, 0);
        assert_ne!(cpu.f & FLAG_Z, 0);
    }

    #[test]
    fn inc_c_preserves_carry() {
        let (mut cpu, mut bus) = boot(&[0x0C]);
        cpu.f = FLAG_C;
        bus.step(&mut cpu);
        assert_ne!(cpu.f & FLAG_C, 0);
    }

    #[test]
    fn inc_hl_takes_two_ticks_and_no_flags() {
        let (mut cpu, mut bus) = boot(&[0x23]);
        cpu.set_hl(0x80FF);
        cpu.f = 0;
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.hl(), 0x8100);
        assert_eq!(cpu.f, 0);
    }

    #[test]
    fn dec_bc_wraps() {
        let (mut cpu, mut bus) = boot(&[0x0B]);
        cpu.set_bc(0x0000);
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.bc(), 0xFFFF);
    }

    // -- ALU -------------------------------------------------------------

    #[test]
    fn xor_a_clears_a_and_sets_zero_flag() {
        let (mut cpu, mut bus) = boot(&[0xAF]);
        cpu.a = 0x42;
        bus.step(&mut cpu);
        assert_eq!(cpu.a, 0);
        assert_eq!(cpu.f, FLAG_Z);
    }

    #[test]
    fn and_d8_sets_half_carry_flag() {
        let (mut cpu, mut bus) = boot(&[0xE6, 0x0F]);
        cpu.a = 0xF0;
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.a, 0);
        assert_ne!(cpu.f & FLAG_Z, 0);
        assert_ne!(cpu.f & FLAG_H, 0);
    }

    #[test]
    fn cp_d8_sets_flags_without_modifying_a() {
        let (mut cpu, mut bus) = boot(&[0xFE, 0x42]);
        cpu.a = 0x42;
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.a, 0x42);
        assert_ne!(cpu.f & FLAG_Z, 0);
        assert_ne!(cpu.f & FLAG_N, 0);
    }

    #[test]
    fn add_a_b_sets_carry_on_overflow() {
        let (mut cpu, mut bus) = boot(&[0x80]);
        cpu.a = 0xFF;
        cpu.b = 0x01;
        bus.step(&mut cpu);
        assert_eq!(cpu.a, 0x00);
        assert_ne!(cpu.f & FLAG_Z, 0);
        assert_ne!(cpu.f & FLAG_H, 0);
        assert_ne!(cpu.f & FLAG_C, 0);
    }

    #[test]
    fn add_a_hl_indirect_takes_two_ticks() {
        let (mut cpu, mut bus) = boot(&[0x86]);
        bus.ram[0x8000] = 0x05;
        cpu.set_hl(0x8000);
        cpu.a = 0x10;
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.a, 0x15);
    }

    #[test]
    fn sub_a_b_underflow_sets_carry() {
        let (mut cpu, mut bus) = boot(&[0x90]);
        cpu.a = 0x00;
        cpu.b = 0x01;
        bus.step(&mut cpu);
        assert_eq!(cpu.a, 0xFF);
        assert_ne!(cpu.f & FLAG_N, 0);
        assert_ne!(cpu.f & FLAG_C, 0);
    }

    // -- JR --------------------------------------------------------------

    #[test]
    fn jr_nz_takes_branch_when_z_clear() {
        let (mut cpu, mut bus) = boot(&[0x20, 0xFD]);
        cpu.f = 0;
        bus.run(&mut cpu, 3);
        assert_eq!(cpu.pc, 0xFFFF);
        assert!(cpu.instruction_complete());
    }

    #[test]
    fn jr_nz_skips_branch_when_z_set() {
        let (mut cpu, mut bus) = boot(&[0x20, 0xFD]);
        cpu.f = FLAG_Z;
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.pc, 2);
        assert!(cpu.instruction_complete());
    }

    #[test]
    fn jr_unconditional_applies_signed_offset() {
        let (mut cpu, mut bus) = boot(&[0x18, 0x05]);
        bus.run(&mut cpu, 3);
        assert_eq!(cpu.pc, 7);
    }

    // -- JP --------------------------------------------------------------

    #[test]
    fn jp_a16_jumps_to_absolute_address() {
        let (mut cpu, mut bus) = boot(&[0xC3, 0x00, 0x01]);
        bus.run(&mut cpu, 4);
        assert_eq!(cpu.pc, 0x0100);
    }

    // -- Cross-instruction sanity ----------------------------------------

    #[test]
    fn multi_m_cycle_followed_by_nop() {
        let (mut cpu, mut bus) = boot(&[0x31, 0x34, 0x12, 0x00]);
        bus.run(&mut cpu, 3);
        assert_eq!(cpu.sp, 0x1234);
        bus.step(&mut cpu);
        assert_eq!(cpu.pc, 4);
        assert!(cpu.instruction_complete());
    }

    #[test]
    fn small_program_writes_byte_via_hl() {
        // LD HL, $C000 ; LD A, $42 ; LD (HL), A ; NOP
        let (mut cpu, mut bus) = boot(&[
            0x21, 0x00, 0xC0, // LD HL, $C000  — 3 ticks
            0x3E, 0x42, // LD A, $42     — 2 ticks
            0x77, // LD (HL), A    — 2 ticks
            0x00, // NOP           — 1 tick
        ]);
        bus.run(&mut cpu, 3 + 2 + 2 + 1);
        assert_eq!(cpu.hl(), 0xC000);
        assert_eq!(cpu.a, 0x42);
        assert_eq!(bus.ram[0xC000], 0x42);
        assert_eq!(cpu.pc, 7);
        assert!(!cpu.diag_unimplemented);
    }

    // -- Diagnostic ------------------------------------------------------

    // -- LD (HL),d8 / INC (HL) / DEC (HL) --------------------------------

    #[test]
    fn ld_hl_indirect_d8_writes_immediate_to_memory() {
        let (mut cpu, mut bus) = boot(&[0x36, 0x7F]);
        cpu.set_hl(0xC000);
        bus.run(&mut cpu, 3);
        assert_eq!(bus.ram[0xC000], 0x7F);
    }

    #[test]
    fn inc_hl_indirect_updates_memory_and_flags() {
        let (mut cpu, mut bus) = boot(&[0x34]);
        bus.ram[0x8000] = 0x0F;
        cpu.set_hl(0x8000);
        bus.run(&mut cpu, 3);
        assert_eq!(bus.ram[0x8000], 0x10);
        assert_ne!(cpu.f & FLAG_H, 0);
        assert_eq!(cpu.f & FLAG_N, 0);
    }

    #[test]
    fn dec_hl_indirect_writes_back_and_flags_borrow() {
        let (mut cpu, mut bus) = boot(&[0x35]);
        bus.ram[0x8000] = 0x10;
        cpu.set_hl(0x8000);
        bus.run(&mut cpu, 3);
        assert_eq!(bus.ram[0x8000], 0x0F);
        assert_ne!(cpu.f & FLAG_H, 0);
        assert_ne!(cpu.f & FLAG_N, 0);
    }

    // -- LDH, (C), (a16) loads -------------------------------------------

    #[test]
    fn ldh_a8_a_writes_into_ff00_page() {
        let (mut cpu, mut bus) = boot(&[0xE0, 0x47]);
        cpu.a = 0xFC;
        bus.run(&mut cpu, 3);
        assert_eq!(bus.ram[0xFF47], 0xFC);
        assert_eq!(cpu.pc, 2);
    }

    #[test]
    fn ldh_a_a8_reads_from_ff00_page() {
        let (mut cpu, mut bus) = boot(&[0xF0, 0x44]);
        bus.ram[0xFF44] = 0x90;
        bus.run(&mut cpu, 3);
        assert_eq!(cpu.a, 0x90);
    }

    #[test]
    fn ld_ffc_a_writes_via_register_c() {
        let (mut cpu, mut bus) = boot(&[0xE2]);
        cpu.a = 0x80;
        cpu.c = 0x11;
        bus.run(&mut cpu, 2);
        assert_eq!(bus.ram[0xFF11], 0x80);
    }

    #[test]
    fn ld_a_ffc_reads_via_register_c() {
        let (mut cpu, mut bus) = boot(&[0xF2]);
        bus.ram[0xFF11] = 0x55;
        cpu.c = 0x11;
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.a, 0x55);
    }

    #[test]
    fn ld_a16_a_writes_to_absolute_address() {
        let (mut cpu, mut bus) = boot(&[0xEA, 0x34, 0x12]);
        cpu.a = 0x77;
        bus.run(&mut cpu, 4);
        assert_eq!(bus.ram[0x1234], 0x77);
    }

    #[test]
    fn ld_a_a16_reads_absolute_address() {
        let (mut cpu, mut bus) = boot(&[0xFA, 0x34, 0x12]);
        bus.ram[0x1234] = 0xBE;
        bus.run(&mut cpu, 4);
        assert_eq!(cpu.a, 0xBE);
    }

    #[test]
    fn ld_a16_sp_writes_both_bytes() {
        let (mut cpu, mut bus) = boot(&[0x08, 0x00, 0xC0]);
        cpu.sp = 0xABCD;
        bus.run(&mut cpu, 5);
        assert_eq!(bus.ram[0xC000], 0xCD);
        assert_eq!(bus.ram[0xC001], 0xAB);
    }

    // -- Stack pointer arithmetic ----------------------------------------

    #[test]
    fn add_hl_rr_preserves_zero_flag() {
        let (mut cpu, mut bus) = boot(&[0x09]); // ADD HL, BC
        cpu.set_hl(0x0FFF);
        cpu.set_bc(0x0001);
        cpu.f = FLAG_Z;
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.hl(), 0x1000);
        assert_ne!(cpu.f & FLAG_Z, 0); // preserved
        assert_ne!(cpu.f & FLAG_H, 0);
        assert_eq!(cpu.f & FLAG_N, 0);
    }

    #[test]
    fn add_sp_r8_applies_signed_offset() {
        let (mut cpu, mut bus) = boot(&[0xE8, 0xFE]); // ADD SP, -2
        cpu.sp = 0xFFF8;
        bus.run(&mut cpu, 4);
        assert_eq!(cpu.sp, 0xFFF6);
    }

    #[test]
    fn ld_hl_sp_r8_preserves_sp_and_sets_hl() {
        let (mut cpu, mut bus) = boot(&[0xF8, 0x02]); // LD HL, SP+2
        cpu.sp = 0xFFF0;
        bus.run(&mut cpu, 3);
        assert_eq!(cpu.hl(), 0xFFF2);
        assert_eq!(cpu.sp, 0xFFF0);
    }

    #[test]
    fn ld_sp_hl_copies_register_pair() {
        let (mut cpu, mut bus) = boot(&[0xF9]);
        cpu.set_hl(0x1234);
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.sp, 0x1234);
    }

    // -- Accumulator rotates + DAA / CPL / SCF / CCF ---------------------

    #[test]
    fn rla_rotates_through_carry() {
        let (mut cpu, mut bus) = boot(&[0x17]);
        cpu.a = 0x80;
        cpu.f = 0;
        bus.step(&mut cpu);
        assert_eq!(cpu.a, 0x00);
        assert_ne!(cpu.f & FLAG_C, 0);
        assert_eq!(cpu.f & FLAG_Z, 0, "RLA never sets Z");
    }

    #[test]
    fn rla_carries_in_old_carry() {
        let (mut cpu, mut bus) = boot(&[0x17]);
        cpu.a = 0x00;
        cpu.f = FLAG_C;
        bus.step(&mut cpu);
        assert_eq!(cpu.a, 0x01);
        assert_eq!(cpu.f & FLAG_C, 0);
    }

    #[test]
    fn cpl_complements_a_and_sets_n_h() {
        let (mut cpu, mut bus) = boot(&[0x2F]);
        cpu.a = 0xAA;
        cpu.f = FLAG_Z;
        bus.step(&mut cpu);
        assert_eq!(cpu.a, 0x55);
        assert_ne!(cpu.f & FLAG_N, 0);
        assert_ne!(cpu.f & FLAG_H, 0);
        assert_ne!(cpu.f & FLAG_Z, 0); // preserved
    }

    #[test]
    fn scf_sets_carry_and_clears_n_h() {
        let (mut cpu, mut bus) = boot(&[0x37]);
        cpu.f = FLAG_Z | FLAG_N | FLAG_H;
        bus.step(&mut cpu);
        assert_ne!(cpu.f & FLAG_C, 0);
        assert_eq!(cpu.f & FLAG_N, 0);
        assert_eq!(cpu.f & FLAG_H, 0);
        assert_ne!(cpu.f & FLAG_Z, 0);
    }

    #[test]
    fn ccf_flips_carry_and_clears_n_h() {
        let (mut cpu, mut bus) = boot(&[0x3F]);
        cpu.f = FLAG_C | FLAG_N | FLAG_H;
        bus.step(&mut cpu);
        assert_eq!(cpu.f & FLAG_C, 0);
        assert_eq!(cpu.f & FLAG_N, 0);
        assert_eq!(cpu.f & FLAG_H, 0);
    }

    #[test]
    fn daa_adjusts_after_bcd_addition() {
        // A = 0x15 (BCD 15) + 0x27 (BCD 27) = 0x3C binary, DAA → 0x42.
        let (mut cpu, mut bus) = boot(&[0xC6, 0x27, 0x27]); // ADD A,$27 ; DAA
        cpu.a = 0x15;
        bus.run(&mut cpu, 2); // ADD
        assert_eq!(cpu.a, 0x3C);
        bus.step(&mut cpu); // DAA
        assert_eq!(cpu.a, 0x42);
    }

    // -- CALL / RET / RETI / RST / PUSH / POP ----------------------------

    #[test]
    fn call_a16_pushes_return_address_and_jumps() {
        let (mut cpu, mut bus) = boot(&[0xCD, 0x95, 0x00]);
        cpu.sp = 0xFFFE;
        bus.run(&mut cpu, 6);
        assert_eq!(cpu.pc, 0x0095);
        assert_eq!(cpu.sp, 0xFFFC);
        assert_eq!(bus.ram[0xFFFD], 0x00); // return addr hi
        assert_eq!(bus.ram[0xFFFC], 0x03); // return addr lo
    }

    #[test]
    fn call_nz_taken_matches_unconditional_call() {
        let (mut cpu, mut bus) = boot(&[0xC4, 0x95, 0x00]);
        cpu.sp = 0xFFFE;
        cpu.f = 0;
        bus.run(&mut cpu, 6);
        assert_eq!(cpu.pc, 0x0095);
        assert_eq!(cpu.sp, 0xFFFC);
    }

    #[test]
    fn call_nz_skipped_keeps_pc_and_sp() {
        let (mut cpu, mut bus) = boot(&[0xC4, 0x95, 0x00]);
        cpu.sp = 0xFFFE;
        cpu.f = FLAG_Z;
        bus.run(&mut cpu, 3);
        assert_eq!(cpu.pc, 3);
        assert_eq!(cpu.sp, 0xFFFE);
    }

    #[test]
    fn ret_pops_return_address() {
        let (mut cpu, mut bus) = boot(&[0xC9]);
        bus.ram[0xFFFC] = 0x03;
        bus.ram[0xFFFD] = 0x00;
        cpu.sp = 0xFFFC;
        bus.run(&mut cpu, 4);
        assert_eq!(cpu.pc, 0x0003);
        assert_eq!(cpu.sp, 0xFFFE);
    }

    #[test]
    fn ret_cc_taken_takes_five_ticks() {
        let (mut cpu, mut bus) = boot(&[0xC0]); // RET NZ
        bus.ram[0xFFFC] = 0x03;
        bus.ram[0xFFFD] = 0x00;
        cpu.sp = 0xFFFC;
        cpu.f = 0;
        bus.run(&mut cpu, 5);
        assert_eq!(cpu.pc, 0x0003);
    }

    #[test]
    fn ret_cc_not_taken_takes_two_ticks() {
        let (mut cpu, mut bus) = boot(&[0xC0]); // RET NZ
        cpu.sp = 0xFFFC;
        cpu.f = FLAG_Z;
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.pc, 1);
        assert_eq!(cpu.sp, 0xFFFC);
    }

    #[test]
    fn reti_pops_and_sets_ime() {
        let (mut cpu, mut bus) = boot(&[0xD9]);
        bus.ram[0xFFFC] = 0x03;
        bus.ram[0xFFFD] = 0x00;
        cpu.sp = 0xFFFC;
        cpu.ime = false;
        bus.run(&mut cpu, 4);
        assert_eq!(cpu.pc, 0x0003);
        assert!(cpu.ime);
    }

    #[test]
    fn rst_08_pushes_pc_and_jumps_to_vector() {
        let (mut cpu, mut bus) = boot(&[0xCF]); // RST $08
        cpu.sp = 0xFFFE;
        bus.run(&mut cpu, 4);
        assert_eq!(cpu.pc, 0x0008);
        assert_eq!(cpu.sp, 0xFFFC);
        assert_eq!(bus.ram[0xFFFC], 0x01); // PC was 1 when push happened
    }

    #[test]
    fn push_bc_and_pop_bc_round_trip() {
        let (mut cpu, mut bus) = boot(&[0xC5, 0xC1]);
        cpu.sp = 0xFFFE;
        cpu.b = 0x12;
        cpu.c = 0x34;
        bus.run(&mut cpu, 4); // PUSH
        assert_eq!(cpu.sp, 0xFFFC);

        cpu.b = 0;
        cpu.c = 0;
        bus.run(&mut cpu, 3); // POP
        assert_eq!(cpu.b, 0x12);
        assert_eq!(cpu.c, 0x34);
        assert_eq!(cpu.sp, 0xFFFE);
    }

    #[test]
    fn pop_af_masks_low_nibble_of_f() {
        let (mut cpu, mut bus) = boot(&[0xF1]);
        bus.ram[0xFFFC] = 0xFF; // F
        bus.ram[0xFFFD] = 0x42; // A
        cpu.sp = 0xFFFC;
        bus.run(&mut cpu, 3);
        assert_eq!(cpu.a, 0x42);
        assert_eq!(cpu.f, 0xF0);
    }

    // -- JP cc, JP (HL), HALT, STOP, DI, EI -------------------------------

    #[test]
    fn jp_z_taken_matches_unconditional_jp() {
        let (mut cpu, mut bus) = boot(&[0xCA, 0x00, 0x01]);
        cpu.f = FLAG_Z;
        bus.run(&mut cpu, 4);
        assert_eq!(cpu.pc, 0x0100);
    }

    #[test]
    fn jp_z_not_taken_is_three_ticks() {
        let (mut cpu, mut bus) = boot(&[0xCA, 0x00, 0x01]);
        cpu.f = 0;
        bus.run(&mut cpu, 3);
        assert_eq!(cpu.pc, 3);
    }

    #[test]
    fn jp_hl_loads_pc_from_hl_in_one_tick() {
        let (mut cpu, mut bus) = boot(&[0xE9]);
        cpu.set_hl(0x1234);
        bus.step(&mut cpu);
        assert_eq!(cpu.pc, 0x1234);
    }

    #[test]
    fn halt_enters_halt_mode_when_ime_clear_and_no_irq() {
        let (mut cpu, mut bus) = boot(&[0x76]);
        cpu.ime = false;
        cpu.irq_pending = 0;
        bus.step(&mut cpu);
        assert!(cpu.halt_mode);
        assert!(!cpu.halt_bug);
    }

    #[test]
    fn halt_triggers_halt_bug_when_ime_clear_and_irq_pending() {
        let (mut cpu, mut bus) = boot(&[0x76]);
        cpu.ime = false;
        cpu.irq_pending = 0x01;
        bus.step(&mut cpu);
        assert!(!cpu.halt_mode);
        assert!(cpu.halt_bug);
    }

    #[test]
    fn stop_sets_stopped_flag_and_skips_next_byte() {
        let (mut cpu, mut bus) = boot(&[0x10, 0x00]);
        bus.step(&mut cpu);
        assert!(cpu.stopped);
        assert_eq!(cpu.pc, 2);
    }

    #[test]
    fn di_clears_ime_and_pending() {
        let (mut cpu, mut bus) = boot(&[0xF3]);
        cpu.ime = true;
        cpu.ime_pending = true;
        bus.step(&mut cpu);
        assert!(!cpu.ime);
        assert!(!cpu.ime_pending);
    }

    #[test]
    fn ei_then_nop_delays_ime_by_one_instruction() {
        let (mut cpu, mut bus) = boot(&[0xFB, 0x00]); // EI ; NOP
        assert!(!cpu.ime);
        bus.step(&mut cpu); // EI
        assert!(!cpu.ime, "EI alone must not enable IME yet");
        assert!(cpu.ime_pending);
        bus.step(&mut cpu); // NOP (promotion happens at this boundary)
        assert!(cpu.ime);
    }

    // -- CB prefix --------------------------------------------------------

    #[test]
    fn bit_7_h_sets_z_when_bit_clear() {
        let (mut cpu, mut bus) = boot(&[0xCB, 0x7C]);
        cpu.h = 0x00;
        cpu.f = FLAG_C;
        bus.run(&mut cpu, 2);
        assert_ne!(cpu.f & FLAG_Z, 0);
        assert_ne!(cpu.f & FLAG_H, 0);
        assert_eq!(cpu.f & FLAG_N, 0);
        assert_ne!(cpu.f & FLAG_C, 0); // preserved
    }

    #[test]
    fn bit_7_h_clears_z_when_bit_set() {
        let (mut cpu, mut bus) = boot(&[0xCB, 0x7C]);
        cpu.h = 0x80;
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.f & FLAG_Z, 0);
    }

    #[test]
    fn rl_c_rotates_through_carry_and_sets_z() {
        let (mut cpu, mut bus) = boot(&[0xCB, 0x11]); // RL C
        cpu.c = 0x80;
        cpu.f = 0;
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.c, 0x00);
        assert_ne!(cpu.f & FLAG_C, 0);
        assert_ne!(cpu.f & FLAG_Z, 0);
    }

    #[test]
    fn bit_7_hl_indirect_is_three_ticks() {
        let (mut cpu, mut bus) = boot(&[0xCB, 0x7E]);
        bus.ram[0x8000] = 0x80;
        cpu.set_hl(0x8000);
        bus.run(&mut cpu, 3);
        assert_eq!(cpu.f & FLAG_Z, 0);
        assert_ne!(cpu.f & FLAG_H, 0);
    }

    #[test]
    fn res_3_hl_clears_bit_in_memory_over_four_ticks() {
        let (mut cpu, mut bus) = boot(&[0xCB, 0x9E]); // RES 3, (HL)
        bus.ram[0x8000] = 0xFF;
        cpu.set_hl(0x8000);
        bus.run(&mut cpu, 4);
        assert_eq!(bus.ram[0x8000], 0xF7);
    }

    #[test]
    fn set_0_hl_sets_bit_in_memory() {
        let (mut cpu, mut bus) = boot(&[0xCB, 0xC6]);
        bus.ram[0x8000] = 0x00;
        cpu.set_hl(0x8000);
        bus.run(&mut cpu, 4);
        assert_eq!(bus.ram[0x8000], 0x01);
    }

    #[test]
    fn swap_hl_swaps_nibbles() {
        let (mut cpu, mut bus) = boot(&[0xCB, 0x36]); // SWAP (HL)
        bus.ram[0x8000] = 0xAB;
        cpu.set_hl(0x8000);
        bus.run(&mut cpu, 4);
        assert_eq!(bus.ram[0x8000], 0xBA);
    }

    #[test]
    fn rl_hl_rotates_memory_byte_through_carry() {
        let (mut cpu, mut bus) = boot(&[0xCB, 0x16]); // RL (HL)
        bus.ram[0x8000] = 0x80;
        cpu.set_hl(0x8000);
        cpu.f = 0;
        bus.run(&mut cpu, 4);
        assert_eq!(bus.ram[0x8000], 0x00);
        assert_ne!(cpu.f & FLAG_C, 0);
        assert_ne!(cpu.f & FLAG_Z, 0);
    }

    #[test]
    fn illegal_opcode_sets_diagnostic_flag() {
        // $D3 is one of the SM83's illegal opcodes (no instruction
        // decoded). Real hardware would lock up; the crate's
        // diagnostic flag catches the gap so ported code can assert on
        // it instead of silently doing the wrong thing.
        let (mut cpu, mut bus) = boot(&[0xD3]);
        bus.step(&mut cpu);
        assert!(cpu.diag_unimplemented);
    }

    // -- Interrupt dispatch ---------------------------------------------

    /// Convenience for dispatch tests: boot at PC=$0100 with SP=$FFFE
    /// and a NOP at the entry point. Returns CPU, bus.
    fn boot_for_dispatch() -> (Sm83, TestBus) {
        let mut bus = TestBus::new();
        bus.ram[0x0100] = 0x00; // NOP — what the CPU would have executed
        let mut cpu = Sm83::new();
        cpu.reset_post_bootrom();
        (cpu, bus)
    }

    #[test]
    fn dispatch_jumps_to_vbl_vector_in_five_ticks() {
        let (mut cpu, mut bus) = boot_for_dispatch();
        cpu.ime = true;
        cpu.irq_pending = 0x01; // VBlank
        let return_addr = cpu.pc; // $0100
        let sp_before = cpu.sp;

        // 5 dispatch ticks: 1 boundary + 4 walker. After the 5th, the
        // next opcode fetch is primed at the vector.
        bus.run(&mut cpu, 5);

        assert_eq!(cpu.pc, 0x0040, "VBlank vector");
        assert_eq!(cpu.sp, sp_before.wrapping_sub(2));
        assert_eq!(bus.ram[sp_before.wrapping_sub(1) as usize], 0x01); // PC hi
        assert_eq!(bus.ram[sp_before.wrapping_sub(2) as usize], 0x00); // PC lo
        assert!(!cpu.ime, "IME cleared on dispatch");
        assert!(!cpu.dispatching, "dispatch completes after 5 ticks");
        // int_ack was asserted at end of tick 5; the test bus stops
        // here, so the strobe is still observable.
        assert!(cpu.int_ack);
        assert_eq!(cpu.int_ack_bit, 0);
        // The opcode fetch for the ISR is primed at the vector.
        assert_eq!(cpu.addr, 0x0040);
        assert!(cpu.rd && cpu.mreq);

        // Pushed return address matches the primed return PC.
        let pushed_lo = bus.ram[sp_before.wrapping_sub(2) as usize];
        let pushed_hi = bus.ram[sp_before.wrapping_sub(1) as usize];
        let pushed = u16::from_be_bytes([pushed_hi, pushed_lo]);
        assert_eq!(pushed, return_addr);
    }

    #[test]
    fn dispatch_priority_serves_lowest_set_bit_first() {
        let (mut cpu, mut bus) = boot_for_dispatch();
        cpu.ime = true;
        cpu.irq_pending = 0x1F;
        bus.run(&mut cpu, 5);
        assert_eq!(cpu.pc, 0x0040, "VBlank wins (bit 0 lowest)");
        assert_eq!(cpu.int_ack_bit, 0);
    }

    #[test]
    fn dispatch_serves_timer_when_only_timer_pending() {
        let (mut cpu, mut bus) = boot_for_dispatch();
        cpu.ime = true;
        cpu.irq_pending = 0x04;
        bus.run(&mut cpu, 5);
        assert_eq!(cpu.pc, 0x0050);
        assert_eq!(cpu.int_ack_bit, 2);
    }

    #[test]
    fn dispatch_int_ack_pulses_for_one_tick() {
        let (mut cpu, mut bus) = boot_for_dispatch();
        cpu.ime = true;
        cpu.irq_pending = 0x10; // Joypad
        bus.run(&mut cpu, 5);
        assert!(cpu.int_ack);
        assert_eq!(cpu.int_ack_bit, 4);

        // The next tick (first ISR opcode m-cycle 0) clears int_ack
        // first thing.
        bus.step(&mut cpu);
        assert!(!cpu.int_ack);
    }

    #[test]
    fn dispatch_with_zero_irq_after_latch_jumps_to_0000() {
        let (mut cpu, mut bus) = boot_for_dispatch();
        cpu.ime = true;
        cpu.irq_pending = 0x01;

        // Run 4 ticks: boundary + first three walker steps (push PC
        // low completes on tick 4). The bit is sampled on tick 5.
        bus.run(&mut cpu, 4);
        assert!(cpu.dispatching);
        cpu.irq_pending = 0; // IE cleared in flight

        bus.step(&mut cpu); // tick 5 — sample sees zero
        assert_eq!(cpu.pc, 0x0000);
        assert!(!cpu.int_ack, "no bit acknowledged in cancelled-IRQ case");
        assert!(!cpu.dispatching);
    }

    #[test]
    fn dispatch_does_not_fire_when_ime_clear() {
        let (mut cpu, mut bus) = boot_for_dispatch();
        cpu.ime = false;
        cpu.irq_pending = 0x01;
        bus.step(&mut cpu);
        assert_eq!(cpu.pc, 0x0101, "no dispatch — opcode runs as usual");
        assert!(!cpu.dispatching);
    }

    #[test]
    fn ei_then_nop_then_irq_dispatches_after_nop_completes() {
        let mut bus = TestBus::new();
        // EI at $0100, NOP at $0101, NOP at $0102 (return address).
        bus.ram[0x0100] = 0xFB;
        bus.ram[0x0101] = 0x00;
        bus.ram[0x0102] = 0x00;
        let mut cpu = Sm83::new();
        cpu.reset_post_bootrom();
        cpu.irq_pending = 0x01;

        bus.step(&mut cpu); // EI runs — IME still off at this boundary
        assert!(!cpu.ime);
        assert!(cpu.ime_pending);
        assert!(!cpu.dispatching);

        bus.step(&mut cpu); // NOP boundary: dispatch check sees ime=false → no dispatch; IME promotes after.
        assert!(cpu.ime);
        assert!(!cpu.dispatching, "dispatch must wait until after the NOP");
        assert_eq!(cpu.pc, 0x0102, "NOP at $0101 has completed");

        // Boundary that follows NOP: dispatch fires (5 ticks).
        bus.run(&mut cpu, 5);
        assert!(!cpu.dispatching);
        assert_eq!(cpu.pc, 0x0040);
        // Pushed return address is $0102.
        assert_eq!(bus.ram[(cpu.sp.wrapping_add(1)) as usize], 0x01);
        assert_eq!(bus.ram[cpu.sp as usize], 0x02);
    }

    // -- HALT and HALT bug ----------------------------------------------

    #[test]
    fn halt_with_ime_set_dispatches_when_irq_arrives() {
        let mut bus = TestBus::new();
        bus.ram[0x0100] = 0x76; // HALT
        bus.ram[0x0101] = 0x00; // NOP (return address)
        let mut cpu = Sm83::new();
        cpu.reset_post_bootrom();
        cpu.ime = true;

        bus.step(&mut cpu); // HALT runs
        assert!(cpu.halt_mode);
        assert!(!cpu.halt_bug);

        // CPU stays halted while no IRQ is pending.
        for _ in 0..5 {
            bus.step(&mut cpu);
        }
        assert!(cpu.halt_mode);
        assert_eq!(cpu.pc, 0x0101);

        // Inject an IRQ. The next tick wakes HALT and the boundary
        // immediately enters dispatch.
        cpu.irq_pending = 0x01;
        bus.run(&mut cpu, 5);
        assert!(!cpu.halt_mode);
        assert_eq!(cpu.pc, 0x0040);
        assert_eq!(bus.ram[cpu.sp as usize], 0x01); // pushed PC lo == $01
        assert_eq!(bus.ram[(cpu.sp.wrapping_add(1)) as usize], 0x01);
    }

    #[test]
    fn halt_with_ime_clear_wakes_without_dispatching() {
        let mut bus = TestBus::new();
        bus.ram[0x0100] = 0x76; // HALT
        bus.ram[0x0101] = 0x3E; // LD A, $42 (proves PC continues normally)
        bus.ram[0x0102] = 0x42;
        let mut cpu = Sm83::new();
        cpu.reset_post_bootrom();
        cpu.ime = false;
        cpu.irq_pending = 0;

        bus.step(&mut cpu); // HALT runs (no halt-bug — irq_pending == 0)
        assert!(cpu.halt_mode);

        bus.step(&mut cpu); // halt loop, still no IRQ
        assert!(cpu.halt_mode);

        cpu.irq_pending = 0x01; // inject IRQ
        bus.step(&mut cpu); // wake; falls through to opcode at $0101 (LD A,d8 m-cycle 1)
        assert!(!cpu.halt_mode);
        assert!(!cpu.dispatching, "no dispatch — IME is clear");

        bus.step(&mut cpu); // LD A,d8 m-cycle 2 latches A
        assert_eq!(cpu.a, 0x42);
    }

    #[test]
    fn halt_bug_causes_next_opcode_to_re_execute() {
        // HALT at $0100 with IME=0 + irq_pending=0x01 triggers the
        // HALT bug: the byte at $0101 is fetched, but PC fails to
        // advance past it the first time. So an INC A at $0101 runs
        // twice in a row.
        let mut bus = TestBus::new();
        bus.ram[0x0100] = 0x76; // HALT
        bus.ram[0x0101] = 0x3C; // INC A
        bus.ram[0x0102] = 0x00; // NOP
        let mut cpu = Sm83::new();
        cpu.reset_post_bootrom();
        cpu.a = 0x00;
        cpu.f = 0;
        cpu.ime = false;
        cpu.irq_pending = 0x01;

        bus.step(&mut cpu); // HALT runs — not halt_mode, halt_bug latched.
        assert!(!cpu.halt_mode);
        assert!(cpu.halt_bug);
        assert_eq!(cpu.pc, 0x0101);

        // First INC A — boundary clears halt_bug, PC stays at $0101.
        bus.step(&mut cpu);
        assert!(!cpu.halt_bug);
        assert_eq!(cpu.a, 0x01);
        assert_eq!(cpu.pc, 0x0101, "PC failed to advance — that's the bug");

        // Second time the byte is fetched, PC advances normally.
        bus.step(&mut cpu);
        assert_eq!(cpu.a, 0x02);
        assert_eq!(cpu.pc, 0x0102);
    }
}
