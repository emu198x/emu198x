//! Opcode dispatch and the m-cycle walker.
//!
//! Pin-level pipelined per
//! [`knowledge/decisions/cpu-bus-interface.md`](../../../../knowledge/decisions/cpu-bus-interface.md):
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
//!
//! The per-instruction bodies live in sibling modules grouped by
//! instruction class:
//!
//! - [`load`] — every `LD` / `LDH` family member
//! - [`arith`] — INC/DEC, the ALU group, 16-bit add, accumulator
//!   rotates, and the BCD/flag adjustments (DAA, CPL, SCF, CCF)
//! - [`control`] — JR/JP/CALL/RET/RETI/RST plus PUSH/POP
//! - [`misc`] — DI/EI/HALT/STOP
//!
//! The CB-prefix walker lives here next to the main dispatch since it
//! is itself a dispatcher (over the 256 `$CB xx` sub-opcodes).

mod arith;
mod control;
mod load;
mod misc;

use crate::Sm83;
use crate::cb::CbFamily;

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
            // `knowledge/decisions/cpu-bus-interface.md` and
            // `knowledge/decisions/sm83-abstraction-level.md` for the
            // accepted pin-level / m-cycle trade-offs.
            if self.ime && self.irq_pending != 0 {
                self.dispatching = true;
                self.irq_dispatch_mask = 0;
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
    /// | 3       | latch pending interrupt, push PC low (SP-=1, write) |
    /// | 4       | set PC = vector, clear IME, strobe `int_ack`,     |
    /// |         | schedule the ISR's first opcode fetch             |
    ///
    /// Priority: the lowest set bit of `irq_pending` is served first,
    /// matching VBlank → STAT → Timer → Serial → Joypad. The bit is
    /// sampled after the PC-high push has been externally serviced.
    /// If `IE` is cleared by that write such that no bit remains
    /// pending, the dispatch jumps to `$0000`. A later `IE` write
    /// during the PC-low push cannot cancel the already latched vector.
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
                self.irq_dispatch_mask = self.irq_pending & 0x1F;
                self.sp = self.sp.wrapping_sub(1);
                let pc_lo = (self.pc & 0xFF) as u8;
                self.schedule_write(self.sp, pc_lo);
                self.m_cycle = 4;
            }
            _ => {
                self.ime = false;
                let pending = self.irq_dispatch_mask & 0x1F;
                self.irq_dispatch_mask = 0;
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
    pub(super) fn finish_instruction(&mut self) {
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
    use crate::FLAG_Z;

    /// Minimal 64 KiB RAM fixture that mediates between CPU pin state
    /// and an in-process backing store. Mirrors the conversion pattern
    /// in `knowledge/decisions/cpu-bus-interface.md`: between ticks, route
    /// the CPU's pins to / from RAM and populate `data_in`.
    pub(super) struct TestBus {
        pub(super) ram: Vec<u8>,
    }

    impl TestBus {
        pub(super) fn new() -> Self {
            Self {
                ram: vec![0; 0x10000],
            }
        }

        pub(super) fn step(&mut self, cpu: &mut Sm83) {
            if cpu.mreq {
                if cpu.rd {
                    cpu.data_in = self.ram[cpu.addr as usize];
                } else if cpu.wr {
                    self.ram[cpu.addr as usize] = cpu.data;
                }
            }
            cpu.tick();
        }

        pub(super) fn run(&mut self, cpu: &mut Sm83, ticks: usize) {
            for _ in 0..ticks {
                self.step(cpu);
            }
        }
    }

    pub(super) fn boot(program: &[u8]) -> (Sm83, TestBus) {
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

    // -- Diagnostic ------------------------------------------------------

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
    pub(super) fn boot_for_dispatch() -> (Sm83, TestBus) {
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
    fn dispatch_with_zero_irq_before_pc_low_push_jumps_to_0000() {
        let (mut cpu, mut bus) = boot_for_dispatch();
        cpu.ime = true;
        cpu.irq_pending = 0x01;

        // Run 3 ticks: boundary + first two walker steps. The PC-high
        // write is now on the pins; the machine services it before the
        // next CPU tick refreshes `irq_pending`.
        bus.run(&mut cpu, 3);
        assert!(cpu.dispatching);
        cpu.irq_pending = 0; // IE cleared by the PC-high push.

        bus.step(&mut cpu); // tick 4 — sample sees zero, then pushes PC low.
        bus.step(&mut cpu); // tick 5 — final dispatch uses the latched zero.
        assert_eq!(cpu.pc, 0x0000);
        assert!(!cpu.int_ack, "no bit acknowledged in cancelled-IRQ case");
        assert!(!cpu.dispatching);
    }

    #[test]
    fn dispatch_ignores_irq_changes_after_pc_low_push_is_scheduled() {
        let (mut cpu, mut bus) = boot_for_dispatch();
        cpu.ime = true;
        cpu.irq_pending = 0x04;

        // Tick 4 samples the pending timer interrupt, then schedules
        // the PC-low push. Clearing `irq_pending` after this point is
        // too late to cancel the dispatch.
        bus.run(&mut cpu, 4);
        assert_eq!(cpu.irq_dispatch_mask, 0x04);
        cpu.irq_pending = 0;

        bus.step(&mut cpu);
        assert_eq!(cpu.pc, 0x0050);
        assert!(cpu.int_ack);
        assert_eq!(cpu.int_ack_bit, 2);
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

    // -- CB prefix --------------------------------------------------------

    #[test]
    fn bit_7_h_sets_z_when_bit_clear() {
        let (mut cpu, mut bus) = boot(&[0xCB, 0x7C]);
        cpu.h = 0x00;
        cpu.f = crate::FLAG_C;
        bus.run(&mut cpu, 2);
        assert_ne!(cpu.f & FLAG_Z, 0);
        assert_ne!(cpu.f & crate::FLAG_H, 0);
        assert_eq!(cpu.f & crate::FLAG_N, 0);
        assert_ne!(cpu.f & crate::FLAG_C, 0); // preserved
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
        assert_ne!(cpu.f & crate::FLAG_C, 0);
        assert_ne!(cpu.f & FLAG_Z, 0);
    }

    #[test]
    fn bit_7_hl_indirect_is_three_ticks() {
        let (mut cpu, mut bus) = boot(&[0xCB, 0x7E]);
        bus.ram[0x8000] = 0x80;
        cpu.set_hl(0x8000);
        bus.run(&mut cpu, 3);
        assert_eq!(cpu.f & FLAG_Z, 0);
        assert_ne!(cpu.f & crate::FLAG_H, 0);
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
        assert_ne!(cpu.f & crate::FLAG_C, 0);
        assert_ne!(cpu.f & FLAG_Z, 0);
    }
}
