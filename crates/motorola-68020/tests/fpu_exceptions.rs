//! FP exception trap delivery — 68881/2 (#112, #488).
//!
//! When an FP instruction raises an exception that is *enabled* in the FPCR
//! exception-enable byte (bits 15-8), the FPU traps to one of vectors 48-54.
//! Two delivery models, both per the 68881:
//!
//!   - Arithmetic exceptions (SNAN/OPERR/OVFL/UNFL/DZ/INEX) are *post-
//!     instruction*: the instruction completes, FPIAR latches its address,
//!     and the trap is delivered at the instruction boundary with the
//!     stacked PC pointing at the following instruction.
//!   - BSUN (an unordered conditional) is *pre-instruction*: the branch /
//!     set does not execute and the stacked PC is the conditional itself.
//!
//! No single-step oracle exists (Musashi never traps); validated against
//! WinUAE `fpsr_get_vector` / `fpsr_check_arithmetic_exception` /
//! `fpsr_set_bsun` for the vector mapping, FPIAR latching, and the
//! pre/post stacked-PC distinction.

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use motorola_68k_common::registers::FpReg;
use motorola_68000::bus::BusStatus;
use motorola_68000::cpu::State;
use motorola_68020::Cpu68020;

const PC: u32 = 0x0000_1000;
const INITIAL_SSP: u32 = 0x0000_8000;

// FPCR exception-enable byte (bits 15-8).
const EN_BSUN: u32 = 0x8000;
const EN_SNAN: u32 = 0x4000;
const EN_OPERR: u32 = 0x2000;
const EN_OVFL: u32 = 0x1000;
const EN_DZ: u32 = 0x0400;
const EN_INEX2: u32 = 0x0200;

// FPSR exception-status byte (bits 15-8).
const EXC_DZ: u32 = 0x0400; // bit 10
const EXC_BSUN: u32 = 0x8000; // bit 15

// Representative floatx80 values.
const POS_ONE: FpReg = FpReg::new(0x3FFF, 0x8000_0000_0000_0000);
const NEG_ONE: FpReg = FpReg::new(0xBFFF, 0x8000_0000_0000_0000);
const POS_ZERO: FpReg = FpReg::new(0x0000, 0);
// Signalling NaN: max exponent, quiet bit (62) clear, another fraction bit set.
const SNAN_VAL: FpReg = FpReg::new(0x7FFF, 0xA000_0000_0000_0000);
// Largest finite extended value — used to force an overflow.
const HUGE: FpReg = FpReg::new(0x7FFE, 0xFFFF_FFFF_FFFF_FFFF);
// An odd value that cannot be represented exactly in single precision.
const INEXACT_SRC: FpReg = FpReg::new(0x3FFF, 0x8000_0000_0000_0001);

struct Mem {
    bytes: HashMap<u32, u8>,
}

impl Mem {
    fn new() -> Self {
        Self {
            bytes: HashMap::new(),
        }
    }
    fn read_word(&self, a: u32) -> u16 {
        let a = a & 0x00FF_FFFF;
        match (self.bytes.get(&a), self.bytes.get(&(a.wrapping_add(1)))) {
            (None, None) => 0x4E71, // NOP fill
            (hi, lo) => (u16::from(*hi.unwrap_or(&0)) << 8) | u16::from(*lo.unwrap_or(&0)),
        }
    }
    fn write_word(&mut self, a: u32, v: u16) {
        self.bytes.insert(a & 0x00FF_FFFF, (v >> 8) as u8);
        self.bytes
            .insert((a.wrapping_add(1)) & 0x00FF_FFFF, v as u8);
    }
    fn write_long(&mut self, a: u32, v: u32) {
        self.write_word(a, (v >> 16) as u16);
        self.write_word(a.wrapping_add(2), v as u16);
    }
    fn long(&self, a: u32) -> u32 {
        (u32::from(self.read_word(a)) << 16) | u32::from(self.read_word(a + 2))
    }
}

fn service_bus(cpu: &mut Cpu68020, mem: &mut Mem) {
    if let State::BusCycle {
        addr,
        is_read,
        is_word,
        data,
        cycle_count,
        ..
    } = &cpu.state
    {
        if *cycle_count >= 3 {
            if *is_read {
                let v = if *is_word {
                    mem.read_word(*addr)
                } else {
                    u16::from((mem.read_word(*addr) >> 8) as u8)
                };
                cpu.bus_status = BusStatus::Ready(v);
            } else {
                if let Some(d) = data {
                    if *is_word {
                        mem.write_word(*addr, *d);
                    } else {
                        mem.bytes.insert(*addr & 0x00FF_FFFF, (*d & 0xFF) as u8);
                    }
                }
                cpu.bus_status = BusStatus::Ready(0);
            }
        } else {
            cpu.bus_status = BusStatus::Wait;
        }
    } else {
        cpu.bus_status = BusStatus::Wait;
    }
}

struct Out {
    /// The trap vector taken (read from the stacked format/vector word), or
    /// `None` if the instruction completed without trapping.
    vector: Option<u8>,
    /// The PC stacked in the exception frame (if a trap was taken).
    stacked_pc: u32,
    fpiar: u32,
    fpsr: u32,
    fp: [FpReg; 8],
    d0: u32,
}

/// Run one F-line opcode (+ extension words) to completion with the FPU
/// attached. `seed` sets up FP registers / FPCR before the run. Detects an
/// FP trap from the stacked Format-$0 exception frame.
fn run(opcode: u16, ext_words: &[u16], seed: impl FnOnce(&mut Cpu68020)) -> Out {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();

    // Point every FP exception vector (48-54) and the NOP-fill handler space
    // at NOPs so execution continues cleanly after the trap.
    for v in 48..=54u32 {
        mem.write_long(v * 4, 0x0000_2000 + v * 0x40);
        mem.write_word(0x0000_2000 + v * 0x40, 0x4E71);
    }

    mem.write_word(PC, opcode);
    for (i, w) in ext_words.iter().enumerate() {
        mem.write_word(PC + 2 + (i as u32) * 2, *w);
    }

    cpu.set_fpu_present(true);
    cpu.regs.sr |= 0x2000; // supervisor
    cpu.regs.ssp = INITIAL_SSP;
    cpu.regs.set_active_sp(INITIAL_SSP);
    seed(&mut cpu);
    cpu.regs.pc = PC + 4;
    cpu.setup_prefetch(opcode, ext_words.first().copied().unwrap_or(0x4E71));

    let start = cpu.instruction_starts;
    for _ in 0..600 {
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        if cpu.instruction_starts > start {
            // A Format-$0 frame (8 bytes) at the new SSP: SR, PC (long),
            // format/vector word. The vector word's low 12 bits = vector*4.
            let frame = INITIAL_SSP - 8;
            let fmt_word = mem.read_word(frame + 6);
            let trapped = cpu.regs.ssp == INITIAL_SSP - 8;
            return Out {
                vector: trapped.then_some(((fmt_word & 0x0FFF) / 4) as u8),
                stacked_pc: mem.long(frame + 2),
                fpiar: cpu.regs.fpiar,
                fpsr: cpu.regs.fpsr,
                fp: cpu.regs.fp,
                d0: cpu.regs.d[0],
            };
        }
    }
    panic!("instruction did not complete");
}

/// cpGEN reg-to-reg extension word: R/M = 0, source/dest Fpn, opmode.
fn ext(src: u16, dst: u16, opmode: u16) -> u16 {
    (src << 10) | (dst << 7) | opmode
}

// ─── Arithmetic exceptions: disabled vs enabled ───────────────────────────

#[test]
fn divide_by_zero_disabled_sets_flag_no_trap() {
    // FDIV FP0(=0.0) into FP1(=1.0): 1.0 / 0.0 → DZ. With DZ disabled, only
    // the FPSR flag is set; no trap.
    let r = run(0xF200, &[ext(0, 1, 0x20)], |cpu| {
        cpu.regs.fp[1] = POS_ONE;
        cpu.regs.fp[0] = POS_ZERO;
    });
    assert_eq!(r.vector, None, "DZ disabled → no trap");
    assert_ne!(r.fpsr & EXC_DZ, 0, "FPSR DZ flag still set");
}

#[test]
fn divide_by_zero_enabled_traps_vector_50() {
    // Same FDIV with DZ enabled → trap to vector 50; stacked PC is the next
    // instruction (post-instruction), FPIAR is the FDIV address.
    let r = run(0xF200, &[ext(0, 1, 0x20)], |cpu| {
        cpu.regs.fp[1] = POS_ONE;
        cpu.regs.fp[0] = POS_ZERO;
        cpu.regs.fpcr = EN_DZ;
    });
    assert_eq!(r.vector, Some(50), "DZ enabled → vector 50");
    assert_eq!(
        r.stacked_pc,
        PC + 4,
        "post-instruction: PC = next instruction"
    );
    assert_eq!(r.fpiar, PC, "FPIAR latched to the FDIV address");
    assert_ne!(r.fpsr & EXC_DZ, 0, "FPSR DZ flag set");
}

#[test]
fn operr_enabled_traps_vector_52() {
    // FSQRT of -1.0 → OPERR. Enabled → vector 52.
    let r = run(0xF200, &[ext(0, 1, 0x04)], |cpu| {
        cpu.regs.fp[0] = NEG_ONE;
        cpu.regs.fpcr = EN_OPERR;
    });
    assert_eq!(r.vector, Some(52), "OPERR enabled → vector 52");
    assert_eq!(r.fpiar, PC);
}

#[test]
fn snan_enabled_traps_vector_54() {
    // FADD with a signalling-NaN source → SNAN. Enabled → vector 54.
    let r = run(0xF200, &[ext(0, 1, 0x22)], |cpu| {
        cpu.regs.fp[1] = POS_ONE;
        cpu.regs.fp[0] = SNAN_VAL;
        cpu.regs.fpcr = EN_SNAN;
    });
    assert_eq!(r.vector, Some(54), "SNAN enabled → vector 54");
}

#[test]
fn overflow_enabled_traps_vector_53() {
    // HUGE + HUGE overflows the extended range → OVFL. Enabled → vector 53.
    let r = run(0xF200, &[ext(0, 1, 0x22)], |cpu| {
        cpu.regs.fp[1] = HUGE;
        cpu.regs.fp[0] = HUGE;
        cpu.regs.fpcr = EN_OVFL;
    });
    assert_eq!(r.vector, Some(53), "OVFL enabled → vector 53");
}

#[test]
fn inexact_enabled_traps_vector_49() {
    // FMOVE of an extended value that does not fit single precision, with the
    // single-precision prefix (FSMOVE, opmode 0x40) → INEX2. Enabled → 49.
    let r = run(0xF200, &[ext(0, 1, 0x40)], |cpu| {
        cpu.regs.fp[0] = INEXACT_SRC;
        cpu.regs.fpcr = EN_INEX2;
    });
    assert_eq!(r.vector, Some(49), "INEX2 enabled → vector 49");
}

#[test]
fn ftst_signalling_nan_enabled_traps_vector_54() {
    // FTST of a signalling NaN raises SNAN even though it writes no register
    // (the #488 FTST gap). Enabled → vector 54.
    let r = run(0xF200, &[ext(0, 0, 0x3A)], |cpu| {
        cpu.regs.fp[0] = SNAN_VAL;
        cpu.regs.fpcr = EN_SNAN;
    });
    assert_eq!(r.vector, Some(54), "FTST SNAN enabled → vector 54");
}

#[test]
fn highest_priority_enabled_exception_wins() {
    // 0.0 / 0.0 raises OPERR (invalid). Enable both OPERR and INEX2; OPERR
    // (higher priority) selects the vector.
    let r = run(0xF200, &[ext(0, 1, 0x20)], |cpu| {
        cpu.regs.fp[1] = POS_ZERO;
        cpu.regs.fp[0] = POS_ZERO;
        cpu.regs.fpcr = EN_OPERR | EN_INEX2;
    });
    assert_eq!(r.vector, Some(52), "OPERR outranks INEX → vector 52");
}

#[test]
fn enabled_but_unraised_exception_does_not_trap() {
    // A clean FADD (1.0 + 1.0) with every exception enabled raises nothing.
    let r = run(0xF200, &[ext(0, 1, 0x22)], |cpu| {
        cpu.regs.fp[1] = POS_ONE;
        cpu.regs.fp[0] = POS_ONE;
        cpu.regs.fpcr = 0xFF00;
    });
    assert_eq!(r.vector, None, "no exception raised → no trap");
    assert_eq!(
        r.fp[1],
        FpReg::new(0x4000, 0x8000_0000_0000_0000),
        "1+1 = 2.0"
    );
}

// ─── BSUN: pre-instruction trap ───────────────────────────────────────────

// The IEEE-*nonaware* NE predicate ($1E, bit 4 set) raises BSUN when the
// NAN condition code is set; the aware NE ($0E) does not. Condition lives in
// the opcode for FBcc, in the extension word for FScc / FTRAPcc.
const COND_NE_NONAWARE: u16 = 0x1E;

#[test]
fn fbcc_bsun_disabled_sets_flag_and_branches() {
    // FBNE.W (nonaware) with the NAN condition set. BSUN disabled → flag set,
    // branch proceeds, no trap.
    let r = run(0xF280 | COND_NE_NONAWARE, &[0x0040], |cpu| {
        cpu.regs.fpsr = 0x0100_0000; // NAN condition code
    });
    assert_eq!(r.vector, None, "BSUN disabled → no trap");
    assert_ne!(r.fpsr & EXC_BSUN, 0, "BSUN flag still set");
}

#[test]
fn fbcc_bsun_enabled_traps_vector_48() {
    // Same FBcc with BSUN enabled → vector 48, pre-instruction: the stacked
    // PC is the FBcc instruction itself (re-executed on RTE).
    let r = run(0xF280 | COND_NE_NONAWARE, &[0x0040], |cpu| {
        cpu.regs.fpsr = 0x0100_0000; // NAN
        cpu.regs.fpcr = EN_BSUN;
    });
    assert_eq!(r.vector, Some(48), "BSUN enabled → vector 48");
    assert_eq!(r.stacked_pc, PC, "pre-instruction: PC = the FBcc itself");
}

#[test]
fn fscc_bsun_enabled_traps_and_leaves_register() {
    // FSNE D0 (nonaware) with the NAN condition + BSUN enabled traps; the
    // byte is not written (pre-instruction).
    let r = run(0xF240, &[COND_NE_NONAWARE], |cpu| {
        cpu.regs.d[0] = 0x1234_5678;
        cpu.regs.fpsr = 0x0100_0000; // NAN
        cpu.regs.fpcr = EN_BSUN;
    });
    assert_eq!(r.vector, Some(48), "FScc BSUN enabled → vector 48");
    assert_eq!(r.d0, 0x1234_5678, "FScc byte not written when BSUN traps");
}

#[test]
fn ftrapcc_bsun_enabled_traps_vector_48_not_7() {
    // FTRAPNE (no operand, nonaware) with NAN + BSUN enabled: BSUN (48) takes
    // precedence over the conditional TRAP (vector 7).
    let r = run(0xF27C, &[COND_NE_NONAWARE], |cpu| {
        cpu.regs.fpsr = 0x0100_0000; // NAN
        cpu.regs.fpcr = EN_BSUN;
    });
    assert_eq!(r.vector, Some(48), "FTRAPcc BSUN → vector 48, not 7");
}
