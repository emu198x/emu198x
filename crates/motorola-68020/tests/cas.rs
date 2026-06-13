//! CAS (`0000 1ss0 11 mmmrrr` + ext word) — 68020 (#114).
//!
//! Atomic compare-and-swap: read the destination at `[EA]`, compare it
//! with the compare register Dc (ext bits 2-0). On equal, write the
//! update register Du (ext bits 8-6) back to `[EA]`; on not-equal, load
//! the read value into Dc. Flags are the comparison `dest - Dc` (CMP
//! semantics: N/Z/V/C set, X preserved). M68000PRM § 6.2.3; matched to
//! Musashi (`m68k_in.c` cas).

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use motorola_68k_common::flags::{C, N, X, Z};
use motorola_68000::bus::BusStatus;
use motorola_68000::cpu::State;
use motorola_68020::Cpu68020;

const PC: u32 = 0x0000_1000;
const EA: u32 = 0x0000_2000;

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
    fn read_long(&self, a: u32) -> u32 {
        (u32::from(self.read_word(a)) << 16) | u32::from(self.read_word(a.wrapping_add(2)))
    }
    fn write_long(&mut self, a: u32, v: u32) {
        self.write_word(a, (v >> 16) as u16);
        self.write_word(a.wrapping_add(2), v as u16);
    }
}

/// Service one bus cycle, capturing writes back into `mem`.
fn service_bus(cpu: &mut Cpu68020, mem: &mut Mem) {
    let resp = if let State::BusCycle {
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
                BusStatus::Ready(v)
            } else {
                // Write: store it.
                let v = data.unwrap_or(0);
                if *is_word {
                    mem.write_word(*addr, v);
                } else {
                    mem.write_byte(*addr, v as u8);
                }
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
    d: [u32; 8],
    sr: u16,
    /// The size-width destination at `[EA]` after the instruction, in the
    /// low bits (byte → low 8, word → low 16, long → all 32).
    mem_at_ea: u32,
}

/// Run one CAS (opcode + ext word) with `seed` setting registers and the
/// size-width `dest` placed at `[EA]`. `size_bytes` selects how the
/// destination is written and read back (CAS reads byte/word/long at the
/// front of `[EA]`, big-endian).
fn run(words: &[u16], size_bytes: u32, dest: u32, seed: impl FnOnce(&mut Cpu68020)) -> Out {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();
    for (i, w) in words.iter().enumerate() {
        mem.write_word(PC + (i as u32) * 2, *w);
    }
    for k in 0..3 {
        mem.write_word(PC + (words.len() as u32 + k) * 2, 0x4E71);
    }
    match size_bytes {
        1 => mem.write_byte(EA, dest as u8),
        2 => mem.write_word(EA, dest as u16),
        _ => mem.write_long(EA, dest),
    }

    cpu.regs.sr |= 0x2000;
    cpu.regs.a[0] = EA;
    cpu.regs.ssp = 0x0000_8000;
    cpu.regs.set_active_sp(0x0000_8000);
    seed(&mut cpu);
    cpu.regs.pc = PC + 4;
    cpu.setup_prefetch(words[0], words.get(1).copied().unwrap_or(0));

    let start = cpu.instruction_starts;
    for _ in 0..400 {
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        if cpu.instruction_starts > start {
            let mem_at_ea = match size_bytes {
                1 => u32::from(mem.read_word(EA) >> 8),
                2 => u32::from(mem.read_word(EA)),
                _ => mem.read_long(EA),
            };
            return Out {
                d: cpu.regs.d,
                sr: cpu.regs.sr,
                mem_at_ea,
            };
        }
    }
    panic!("instruction did not complete");
}

// CAS (A0): mode 2 reg 0 → ea bits 0x10. Base by size.
const CAS_B: u16 = 0x0AC0 | 0x10;
const CAS_W: u16 = 0x0CC0 | 0x10;
const CAS_L: u16 = 0x0EC0 | 0x10;
// ext: Dc in bits 2-0, Du in bits 8-6.
fn ext(dc: u16, du: u16) -> u16 {
    (du << 6) | dc
}

#[test]
fn cas_l_equal_writes_du_to_memory() {
    // [EA] = 0x1111_1111 == Dc (D1) → write Du (D2) back, Z set, Dc kept.
    let words = [CAS_L, ext(1, 2)];
    let r = run(&words, 4, 0x1111_1111, |cpu| {
        cpu.regs.d[1] = 0x1111_1111;
        cpu.regs.d[2] = 0x2222_2222;
    });
    assert_eq!(r.mem_at_ea, 0x2222_2222, "equal → Du written to [EA]");
    assert_eq!(r.d[1], 0x1111_1111, "Dc unchanged on a match");
    assert_ne!(r.sr & Z, 0, "equal → Z set");
}

#[test]
fn cas_l_not_equal_loads_dc_leaves_memory() {
    // [EA] = 0x9999_9999 != Dc → Dc ← [EA], memory unchanged, Z clear.
    let words = [CAS_L, ext(1, 2)];
    let r = run(&words, 4, 0x9999_9999, |cpu| {
        cpu.regs.d[1] = 0x1111_1111;
        cpu.regs.d[2] = 0x2222_2222;
    });
    assert_eq!(r.mem_at_ea, 0x9999_9999, "no match → memory unchanged");
    assert_eq!(r.d[1], 0x9999_9999, "no match → Dc loaded with [EA]");
    assert_eq!(r.sr & Z, 0, "not equal → Z clear");
}

#[test]
fn cas_w_not_equal_loads_low_word_only() {
    // CAS.W: on mismatch only the low word of Dc is replaced. [EA] word
    // is 0xBEEF; Dc low word 0x1234 ≠ 0xBEEF → Dc low word ← 0xBEEF.
    let words = [CAS_W, ext(1, 2)];
    let r = run(&words, 2, 0xBEEF, |cpu| {
        cpu.regs.d[1] = 0xAAAA_1234;
        cpu.regs.d[2] = 0x0000_5678;
    });
    assert_eq!(r.d[1], 0xAAAA_BEEF, "high word of Dc preserved");
    assert_eq!(r.sr & Z, 0);
}

#[test]
fn cas_b_not_equal_loads_low_byte_only() {
    // [EA] byte = 0x7F; Dc low byte 0x12 ≠ 0x7F → Dc low byte ← 0x7F.
    let words = [CAS_B, ext(1, 2)];
    let r = run(&words, 1, 0x7F, |cpu| {
        cpu.regs.d[1] = 0xAAAA_AA12;
        cpu.regs.d[2] = 0x0000_0034;
    });
    assert_eq!(r.d[1], 0xAAAA_AA7F, "only low byte of Dc replaced");
    assert_eq!(r.sr & Z, 0);
}

#[test]
fn cas_w_equal_writes_low_word_of_du() {
    // CAS.W equal: only the low word of Du is written to [EA]. [EA] word
    // 0x1234 == Dc low word → write Du low word 0x5678.
    let words = [CAS_W, ext(1, 2)];
    let r = run(&words, 2, 0x1234, |cpu| {
        cpu.regs.d[1] = 0x0000_1234;
        cpu.regs.d[2] = 0xFFFF_5678;
    });
    assert_eq!(r.mem_at_ea, 0x5678, "low word of Du written to [EA]");
    assert_ne!(r.sr & Z, 0);
}

#[test]
fn cas_l_flags_match_cmp_on_mismatch() {
    // Dc = 0, [EA] = 1 → res = 1 - 0 = 1 → N=0, Z=0, C=0.
    let words = [CAS_L, ext(1, 2)];
    let r = run(&words, 4, 1, |cpu| cpu.regs.d[1] = 0);
    assert_eq!(r.sr & Z, 0);
    assert_eq!(r.sr & N, 0);
    assert_eq!(r.sr & C, 0, "1 - 0 borrows nothing → C clear");
}

#[test]
fn cas_l_carry_set_when_dest_below_compare() {
    // Dc = 5, [EA] = 1 → res = 1 - 5 borrows → C set, N set.
    let words = [CAS_L, ext(1, 2)];
    let r = run(&words, 4, 1, |cpu| cpu.regs.d[1] = 5);
    assert_ne!(r.sr & C, 0, "dest < Dc → borrow → C set");
    assert_ne!(r.sr & N, 0, "negative result → N set");
}

#[test]
fn cas_preserves_x_flag() {
    // X is not affected by CAS (CMP semantics).
    let words = [CAS_L, ext(1, 2)];
    let r = run(&words, 4, 1, |cpu| {
        cpu.regs.d[1] = 5;
        cpu.regs.sr |= X;
    });
    assert_ne!(r.sr & X, 0, "X preserved across CAS");
}

#[test]
fn cas_same_register_compare_and_update() {
    // Dc == Du (common idiom). Equal case writes Dc's value back.
    let words = [CAS_L, ext(3, 3)];
    let r = run(&words, 4, 0x4242_4242, |cpu| cpu.regs.d[3] = 0x4242_4242);
    assert_eq!(r.mem_at_ea, 0x4242_4242);
    assert_ne!(r.sr & Z, 0);
}
