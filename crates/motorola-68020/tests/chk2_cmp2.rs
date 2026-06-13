//! CHK2 / CMP2 (`0000 0ss0 11 mmmrrr` + ext word) — 68020 (#114).
//!
//! Both compare a register against a pair of bounds in memory: lower at
//! `[EA]`, upper at `[EA + size]`. CMP2 sets only Z/C; CHK2 additionally
//! traps vector 6 when out of bounds. Expected flags computed by hand
//! from M68000PRM § 6.2.2 and matched to Musashi's formula (compare
//! value = register masked to size, sign-extended for Dn only; bounds
//! read signed; Z = equals either bound; C = below lower or above
//! upper).

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use motorola_68k_common::flags::{C, N, V, X, Z};
use motorola_68000::bus::BusStatus;
use motorola_68000::cpu::State;
use motorola_68020::Cpu68020;

const PC: u32 = 0x0000_1000;
const EA: u32 = 0x0000_2000;
const HANDLER: u32 = 0x0000_3000;

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
        let hi = *self.bytes.get(&(a & 0x00FF_FFFF)).unwrap_or(&0);
        let lo = *self
            .bytes
            .get(&((a.wrapping_add(1)) & 0x00FF_FFFF))
            .unwrap_or(&0);
        (u16::from(hi) << 8) | u16::from(lo)
    }
    fn write_byte(&mut self, a: u32, v: u8) {
        self.bytes.insert(a & 0x00FF_FFFF, v);
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
}

fn service_bus(cpu: &mut Cpu68020, mem: &mut Mem) {
    let resp = if let State::BusCycle {
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
                BusStatus::Ready(v)
            } else {
                BusStatus::Ready(0)
            }
        } else {
            BusStatus::Wait
        }
    } else {
        BusStatus::Wait
    };
    cpu.bus_status = resp;
}

struct Out {
    sr: u16,
    vectored: bool,
}

/// Run one CHK2/CMP2 against bounds (lo, hi) written at the size's stride
/// from EA, with `reg_seed` placing the compare value. Returns the SR
/// after the instruction and whether execution vectored to the vec-6
/// handler.
fn run(
    words: &[u16],
    size_bytes: u32,
    lo: u32,
    hi: u32,
    reg_seed: impl FnOnce(&mut Cpu68020),
) -> Out {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();
    for (i, w) in words.iter().enumerate() {
        mem.write_word(PC + (i as u32) * 2, *w);
    }
    for k in 0..3 {
        mem.write_word(PC + (words.len() as u32 + k) * 2, 0x4E71);
    }
    // Bounds at [EA] and [EA + size].
    match size_bytes {
        1 => {
            mem.write_byte(EA, lo as u8);
            mem.write_byte(EA + 1, hi as u8);
        }
        2 => {
            mem.write_word(EA, lo as u16);
            mem.write_word(EA + 2, hi as u16);
        }
        _ => {
            mem.write_long(EA, lo);
            mem.write_long(EA + 4, hi);
        }
    }
    // Vector 6 (CHK/CHK2, offset $18) → handler (NOP).
    mem.write_long(6 * 4, HANDLER);
    mem.write_word(HANDLER, 0x4E71);

    cpu.regs.sr |= 0x2000;
    cpu.regs.a[0] = EA;
    cpu.regs.ssp = 0x0000_8000;
    cpu.regs.set_active_sp(0x0000_8000);
    reg_seed(&mut cpu);
    cpu.regs.pc = PC + 4;
    cpu.setup_prefetch(words[0], words.get(1).copied().unwrap_or(0));

    let start = cpu.instruction_starts;
    for _ in 0..400 {
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        if cpu.instruction_starts > start {
            return Out {
                sr: cpu.regs.sr,
                vectored: cpu.instr_start_pc == HANDLER,
            };
        }
    }
    panic!("instruction did not complete");
}

// CHK2/CMP2 (A0),Rn. opcode = 0x00C0 | (ss<<9) | (mode 2 << 3) = +0x10.
// ss: 0=byte, 1=word, 2=long. ext = (D/A<<15)|(reg<<12)|(chk2<<11).
fn op(ss: u16) -> u16 {
    0x00C0 | (ss << 9) | 0x10
}
fn ext(is_addr: bool, reg: u16, is_chk2: bool) -> u16 {
    ((is_addr as u16) << 15) | (reg << 12) | ((is_chk2 as u16) << 11)
}

// --- CMP2: flags only ---

#[test]
fn cmp2_l_in_bounds_clears_z_and_c() {
    // bounds [10, 20], D0 = 15 → in range, not equal → Z=0, C=0.
    let words = [op(2), ext(false, 0, false)];
    let r = run(&words, 4, 10, 20, |cpu| cpu.regs.d[0] = 15);
    assert_eq!(r.sr & Z, 0, "in range → Z clear");
    assert_eq!(r.sr & C, 0, "in range → C clear");
}

#[test]
fn cmp2_l_equal_lower_sets_z() {
    // D0 = 10 (== lower) → Z=1, C=0.
    let words = [op(2), ext(false, 0, false)];
    let r = run(&words, 4, 10, 20, |cpu| cpu.regs.d[0] = 10);
    assert_ne!(r.sr & Z, 0, "equals lower bound → Z set");
    assert_eq!(r.sr & C, 0, "equals bound is in range → C clear");
}

#[test]
fn cmp2_l_equal_upper_sets_z() {
    let words = [op(2), ext(false, 0, false)];
    let r = run(&words, 4, 10, 20, |cpu| cpu.regs.d[0] = 20);
    assert_ne!(r.sr & Z, 0, "equals upper bound → Z set");
    assert_eq!(r.sr & C, 0);
}

#[test]
fn cmp2_l_above_upper_sets_c() {
    // D0 = 21 → above range → C=1, Z=0.
    let words = [op(2), ext(false, 0, false)];
    let r = run(&words, 4, 10, 20, |cpu| cpu.regs.d[0] = 21);
    assert_ne!(r.sr & C, 0, "out of range → C set");
    assert_eq!(r.sr & Z, 0);
}

#[test]
fn cmp2_l_below_lower_sets_c() {
    let words = [op(2), ext(false, 0, false)];
    let r = run(&words, 4, 10, 20, |cpu| cpu.regs.d[0] = 9);
    assert_ne!(r.sr & C, 0, "below range → C set");
    assert_eq!(r.sr & Z, 0);
}

#[test]
fn cmp2_l_signed_negative_bounds() {
    // bounds [-20, -10] (signed), D0 = -15 → in range.
    let words = [op(2), ext(false, 0, false)];
    let r = run(&words, 4, (-20i32) as u32, (-10i32) as u32, |cpu| {
        cpu.regs.d[0] = (-15i32) as u32
    });
    assert_eq!(r.sr & C, 0, "−15 ∈ [−20,−10] → in range");
    assert_eq!(r.sr & Z, 0);
}

#[test]
fn cmp2_w_word_size_uses_low_word() {
    // word bounds [10, 20]; D0 = 0xFFFF_000F (low word 15) → in range.
    let words = [op(1), ext(false, 0, false)];
    let r = run(&words, 2, 10, 20, |cpu| cpu.regs.d[0] = 0xFFFF_000F);
    assert_eq!(r.sr & C, 0, "low word 15 ∈ [10,20]");
}

#[test]
fn cmp2_b_byte_size_uses_low_byte() {
    // byte bounds [10, 20]; D0 = 0x0000_0015 (21) → above range.
    let words = [op(0), ext(false, 0, false)];
    let r = run(&words, 1, 10, 20, |cpu| cpu.regs.d[0] = 0x15);
    assert_ne!(r.sr & C, 0, "byte 0x15 = 21 > 20 → C set");
}

#[test]
fn cmp2_preserves_n_v_x() {
    // Pre-set N, V, X; CMP2 must leave them untouched.
    let words = [op(2), ext(false, 0, false)];
    let r = run(&words, 4, 10, 20, |cpu| {
        cpu.regs.d[0] = 15;
        cpu.regs.sr |= N | V | X;
    });
    assert_ne!(r.sr & N, 0, "N preserved");
    assert_ne!(r.sr & V, 0, "V preserved");
    assert_ne!(r.sr & X, 0, "X preserved");
}

// --- CHK2: traps vector 6 when out of bounds ---

#[test]
fn chk2_l_in_bounds_no_trap() {
    let words = [op(2), ext(false, 0, true)];
    let r = run(&words, 4, 10, 20, |cpu| cpu.regs.d[0] = 15);
    assert!(!r.vectored, "in range → no trap");
}

#[test]
fn chk2_l_out_of_bounds_traps_vector_6() {
    let words = [op(2), ext(false, 0, true)];
    let r = run(&words, 4, 10, 20, |cpu| cpu.regs.d[0] = 99);
    assert!(r.vectored, "out of range → vector-6 trap");
}

#[test]
fn chk2_l_equal_bound_no_trap() {
    // Equal to a bound is in range → no trap (Z set, C clear).
    let words = [op(2), ext(false, 0, true)];
    let r = run(&words, 4, 10, 20, |cpu| cpu.regs.d[0] = 20);
    assert!(!r.vectored, "equal bound is in range → no trap");
}

#[test]
fn chk2_an_register_masks_to_size_musashi_quirk() {
    // CHK2.W with An source. Musashi masks An to the operand size and
    // does NOT sign-extend it. A1 = 0x0001_0005 → low word 5. With word
    // bounds [10, 20], the masked value 5 is below 10 → out of range.
    let words = [op(1), ext(true, 1, true)];
    let r = run(&words, 2, 10, 20, |cpu| cpu.regs.a[1] = 0x0001_0005);
    assert!(r.vectored, "masked An low word 5 < 10 → trap");
}
