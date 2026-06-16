//! MC68881/2 FP arithmetic cycle timing — 68020 (#112, step #494).
//!
//! A *bounded UM-derived* timing model: each arithmetic FP op queues an
//! internal execution delay drawn from MC68881UM tables 8-14 (dyadic) and
//! 8-15 (monadic) — the normalized-operand common case — plus a fixed
//! interface overhead. These tests don't assert clock-exact figures (the
//! model deliberately approximates the operand-class matrices and the 68882's
//! execution concurrency); they assert the delay is *present*, *ordered* by
//! the UM calculation cost, and lands in the right ballpark. A simple op
//! (FADD) must be far cheaper than a transcendental (FSIN), which in turn must
//! be cheaper than the dearest transcendental (FATANH).

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use motorola_68k_common::registers::FpReg;
use motorola_68000::bus::BusStatus;
use motorola_68000::cpu::State;
use motorola_68020::Cpu68020;

const PC: u32 = 0x0000_1000;

// floatx80 layout of the operands the ops chew on.
const POS_TWO: FpReg = FpReg::new(0x4000, 0x8000_0000_0000_0000);
const POS_HALF: FpReg = FpReg::new(0x3FFE, 0x8000_0000_0000_0000);

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

/// cpGEN reg-to-reg extension word: R/M = 0, source/dest Fpn, opmode.
fn ext(src: u16, dst: u16, opmode: u16) -> u16 {
    (src << 10) | (dst << 7) | opmode
}

/// Run one cpGEN reg-to-reg op and return the number of ticks taken to
/// retire it (the instruction-boundary count).
fn ticks_for(opmode: u16) -> u32 {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();
    let opcode = 0xF200; // cpID 1, op-class 0
    let w2 = ext(2, 0, opmode); // src = FP2, dst = FP0
    mem.write_word(PC, opcode);
    mem.write_word(PC + 2, w2);

    cpu.set_fpu_present(true);
    cpu.regs.sr |= 0x2000;
    cpu.regs.ssp = 0x0000_8000;
    cpu.regs.set_active_sp(0x0000_8000);
    // Seed both operands with finite, in-range values so every op takes its
    // normalized-operand path (and the trig family stays inside ±9).
    cpu.regs.fp[0] = POS_TWO;
    cpu.regs.fp[2] = POS_HALF;
    cpu.regs.pc = PC + 4;
    cpu.setup_prefetch(opcode, w2);

    let start = cpu.instruction_starts;
    for n in 0..4000 {
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        if cpu.instruction_starts > start {
            return n;
        }
    }
    panic!("FP op did not retire within the tick budget");
}

const FADD: u16 = 0x22;
const FMUL: u16 = 0x23;
const FDIV: u16 = 0x20;
const FSQRT: u16 = 0x04;
const FSIN: u16 = 0x0E;
const FATANH: u16 = 0x0D;
const FMOVE: u16 = 0x00;

#[test]
fn arithmetic_ops_carry_an_internal_delay() {
    // FMOVE (2 calc clocks) is the cheapest op, but still pays the fixed
    // interface overhead — so even it costs more than a bare prefetch pair.
    let fmove = ticks_for(FMOVE);
    let fadd = ticks_for(FADD);
    // FADD (24 calc) must visibly cost more than FMOVE (2 calc).
    assert!(fadd > fmove, "FADD ({fadd}) should outlast FMOVE ({fmove})");
}

#[test]
fn cost_is_ordered_by_um_calculation_time() {
    let fadd = ticks_for(FADD); // 24
    let fmul = ticks_for(FMUL); // 46
    let fdiv = ticks_for(FDIV); // 78
    let fsin = ticks_for(FSIN); // 360
    let fatanh = ticks_for(FATANH); // 662
    assert!(fadd < fmul, "FADD {fadd} < FMUL {fmul}");
    assert!(fmul < fdiv, "FMUL {fmul} < FDIV {fdiv}");
    assert!(fdiv < fsin, "FDIV {fdiv} < FSIN {fsin}");
    assert!(fsin < fatanh, "FSIN {fsin} < FATANH {fatanh}");
}

#[test]
fn transcendental_delay_matches_the_um_figure() {
    // FSIN: 360 calc + 35 overhead = 395 internal clocks, plus the prefetch
    // bus traffic surrounding the instruction. The retire count must clear the
    // calculation figure and stay within a bus-overhead margin of the total.
    let fsin = ticks_for(FSIN);
    assert!(
        fsin >= 395,
        "FSIN {fsin} should cover its 360+35 model clocks"
    );
    assert!(
        fsin < 460,
        "FSIN {fsin} should not balloon past the model + bus"
    );

    // FSQRT: 76 + 35 = 111 model clocks.
    let fsqrt = ticks_for(FSQRT);
    assert!(
        fsqrt >= 111,
        "FSQRT {fsqrt} should cover its 76+35 model clocks"
    );
    assert!(
        fsqrt < 180,
        "FSQRT {fsqrt} should not balloon past the model + bus"
    );
}
