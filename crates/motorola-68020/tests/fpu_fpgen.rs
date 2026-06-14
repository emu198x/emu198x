//! cpGEN reg-to-reg FMOVE / FABS / FNEG / FTST — 68020 (#112, step 3a).
//!
//! The register-to-register, non-arithmetic FPU ops. These need no
//! float-math backend — FMOVE copies the 80-bit value, FABS clears the
//! sign bit, FNEG flips it, FTST only sets condition codes — so they are
//! bit-exact against Musashi's `fpgen_rm_reg` / `SET_CONDITION_CODES`.
//! Arithmetic ops and memory/FMOVECR operands come in later steps.

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use motorola_68k_common::registers::FpReg;
use motorola_68000::bus::BusStatus;
use motorola_68000::cpu::State;
use motorola_68020::Cpu68020;

const PC: u32 = 0x0000_1000;

// FPSR condition-code bits (FPSR bits 27-24).
const FPCC_N: u32 = 1 << 27;
const FPCC_Z: u32 = 1 << 26;
const FPCC_I: u32 = 1 << 25;
const FPCC_NAN: u32 = 1 << 24;

// Representative extended-precision values (floatx80 layout).
const POS_ONE: FpReg = FpReg::new(0x3FFF, 0x8000_0000_0000_0000);
const NEG_ONE: FpReg = FpReg::new(0xBFFF, 0x8000_0000_0000_0000);
const POS_ZERO: FpReg = FpReg::new(0x0000, 0);
const NEG_ZERO: FpReg = FpReg::new(0x8000, 0);
const POS_INF: FpReg = FpReg::new(0x7FFF, 0x8000_0000_0000_0000);
const A_NAN: FpReg = FpReg::new(0x7FFF, 0xC000_0000_0000_0000);

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
}

fn service_bus(cpu: &mut Cpu68020, mem: &mut Mem) {
    if let State::BusCycle {
        addr,
        is_read,
        is_word,
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
    fp: [FpReg; 8],
    fpsr: u32,
}

/// cpGEN reg-to-reg extension word: R/M = 0, source/dest Fpn, opmode.
fn ext(src: u16, dst: u16, opmode: u16) -> u16 {
    (src << 10) | (dst << 7) | opmode
}

/// Run one cpGEN reg-to-reg op with the FP register file seeded by `seed`.
fn run(opmode: u16, src: u16, dst: u16, seed: impl FnOnce(&mut Cpu68020)) -> Out {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();
    let opcode = 0xF200; // cpID 1, op-class 0, EA bits unused for reg-to-reg
    let w2 = ext(src, dst, opmode);
    mem.write_word(PC, opcode);
    mem.write_word(PC + 2, w2);

    cpu.set_fpu_present(true);
    cpu.regs.sr |= 0x2000;
    cpu.regs.ssp = 0x0000_8000;
    cpu.regs.set_active_sp(0x0000_8000);
    seed(&mut cpu);
    cpu.regs.pc = PC + 4;
    cpu.setup_prefetch(opcode, w2);

    let start = cpu.instruction_starts;
    for _ in 0..400 {
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        if cpu.instruction_starts > start {
            return Out {
                fp: cpu.regs.fp,
                fpsr: cpu.regs.fpsr,
            };
        }
    }
    panic!("FP op did not complete");
}

#[test]
fn fmove_copies_value_and_sets_sign() {
    // FMOVE FP1,FP2 with FP1 = -1.0 → FP2 = -1.0 (exact copy), N set.
    let r = run(0x00, 1, 2, |cpu| cpu.regs.fp[1] = NEG_ONE);
    assert_eq!(r.fp[2], NEG_ONE, "FMOVE copies the 80-bit value exactly");
    assert_ne!(r.fpsr & FPCC_N, 0, "negative → N set");
    assert_eq!(r.fpsr & FPCC_Z, 0);
}

#[test]
fn fabs_clears_sign_bit() {
    // FABS FP1,FP2 with FP1 = -1.0 → FP2 = +1.0; N clear.
    let r = run(0x18, 1, 2, |cpu| cpu.regs.fp[1] = NEG_ONE);
    assert_eq!(r.fp[2], POS_ONE, "FABS clears the sign");
    assert_eq!(r.fpsr & FPCC_N, 0, "result is positive → N clear");
}

#[test]
fn fabs_of_positive_is_unchanged() {
    let r = run(0x18, 1, 2, |cpu| cpu.regs.fp[1] = POS_ONE);
    assert_eq!(r.fp[2], POS_ONE);
}

#[test]
fn fneg_flips_sign_bit() {
    // FNEG FP1,FP2 with FP1 = +1.0 → FP2 = -1.0; N set.
    let r = run(0x1A, 1, 2, |cpu| cpu.regs.fp[1] = POS_ONE);
    assert_eq!(r.fp[2], NEG_ONE, "FNEG flips the sign");
    assert_ne!(r.fpsr & FPCC_N, 0);
}

#[test]
fn fneg_of_negative_becomes_positive() {
    let r = run(0x1A, 1, 2, |cpu| cpu.regs.fp[1] = NEG_ONE);
    assert_eq!(r.fp[2], POS_ONE);
    assert_eq!(r.fpsr & FPCC_N, 0);
}

#[test]
fn ftst_sets_negative_without_writing() {
    // FTST FP1 with FP1 = -1.0 → N set; no register modified.
    let r = run(0x3A, 1, 0, |cpu| {
        cpu.regs.fp[1] = NEG_ONE;
        cpu.regs.fp[0] = POS_ONE; // dst field is 0 but FTST must not write
    });
    assert_ne!(r.fpsr & FPCC_N, 0, "FTST of −1.0 → N set");
    assert_eq!(r.fp[0], POS_ONE, "FTST must not write the dest register");
    assert_eq!(r.fp[1], NEG_ONE, "FTST must not modify the source");
}

#[test]
fn ftst_sets_zero_flag() {
    let r = run(0x3A, 1, 0, |cpu| cpu.regs.fp[1] = POS_ZERO);
    assert_ne!(r.fpsr & FPCC_Z, 0, "FTST of +0 → Z set");
    assert_eq!(r.fpsr & FPCC_N, 0);
}

#[test]
fn ftst_negative_zero_sets_z_and_n() {
    // −0.0: Z set (zero) AND N set (sign bit) — Musashi reports the raw
    // sign bit even for zero.
    let r = run(0x3A, 1, 0, |cpu| cpu.regs.fp[1] = NEG_ZERO);
    assert_ne!(r.fpsr & FPCC_Z, 0, "−0 → Z set");
    assert_ne!(r.fpsr & FPCC_N, 0, "−0 → N set (raw sign bit)");
}

#[test]
fn ftst_infinity_sets_i_flag() {
    let r = run(0x3A, 1, 0, |cpu| cpu.regs.fp[1] = POS_INF);
    assert_ne!(r.fpsr & FPCC_I, 0, "FTST of +inf → I set");
    assert_eq!(r.fpsr & FPCC_NAN, 0);
}

#[test]
fn ftst_nan_sets_nan_flag() {
    let r = run(0x3A, 1, 0, |cpu| cpu.regs.fp[1] = A_NAN);
    assert_ne!(r.fpsr & FPCC_NAN, 0, "FTST of NaN → NAN set");
    assert_eq!(r.fpsr & FPCC_I, 0);
}

#[test]
fn fmove_infinity_sets_i_flag() {
    let r = run(0x00, 1, 2, |cpu| cpu.regs.fp[1] = POS_INF);
    assert_eq!(r.fp[2], POS_INF);
    assert_ne!(r.fpsr & FPCC_I, 0);
}
