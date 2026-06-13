//! PACK / UNPK ($8140 / $8180 + adjustment word) — 68020 BCD ↔ ASCII
//! conversion (#114). Register forms `Dy,Dx,#adj`.
//!
//! No fixtures cover these; expected values are hand-computed from
//! M68000PRM § 6.2.27:
//!   PACK: byte = ((src16 + adj) packed at nibbles [11:8] and [3:0]).
//!   UNPK: word = (src8 nibbles spread to [11:8] and [3:0]) + adj.
//! Neither affects condition codes.

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use motorola_68000::bus::BusStatus;
use motorola_68000::cpu::State;
use motorola_68020::Cpu68020;

const PC: u32 = 0x0000_1000;

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
    fn write_word(&mut self, a: u32, v: u16) {
        self.bytes.insert(a & 0x00FF_FFFF, (v >> 8) as u8);
        self.bytes
            .insert((a.wrapping_add(1)) & 0x00FF_FFFF, v as u8);
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

/// Run one PACK/UNPK and return the resulting register file.
fn run(opcode: u16, adj: u16, seed: impl FnOnce(&mut Cpu68020)) -> [u32; 8] {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();
    mem.write_word(PC, opcode);
    mem.write_word(PC + 2, adj);
    for k in 0..3 {
        mem.write_word(PC + 4 + (k as u32) * 2, 0x4E71); // trailing NOPs
    }
    cpu.regs.sr |= 0x2000;
    seed(&mut cpu);
    cpu.regs.pc = PC + 4;
    cpu.setup_prefetch(opcode, adj);

    let start = cpu.instruction_starts;
    for _ in 0..400 {
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        if cpu.instruction_starts > start {
            return cpu.regs.d;
        }
    }
    panic!("instruction did not complete");
}

// PACK Dy,Dx,#adj = $8140 | (Dx<<9) | Dy.  UNPK = $8180 | (Dx<<9) | Dy.
fn pack(dx: u16, dy: u16) -> u16 {
    0x8140 | (dx << 9) | dy
}
fn unpk(dx: u16, dy: u16) -> u16 {
    0x8180 | (dx << 9) | dy
}

#[test]
fn pack_two_ascii_digits_to_bcd() {
    // Dy = 0x3539 ('5','9'), adj 0 → packed BCD $59 in Dx low byte.
    let d = run(pack(2, 1), 0x0000, |cpu| cpu.regs.d[1] = 0x3539);
    assert_eq!(d[2] & 0xFF, 0x59);
}

#[test]
fn pack_adjustment_is_added_before_packing() {
    // Dy = 0, adj = 0x1234 → src 0x1234 → nibbles [11:8]=2,[3:0]=4 → $24.
    let d = run(pack(2, 1), 0x1234, |cpu| cpu.regs.d[1] = 0x0000);
    assert_eq!(d[2] & 0xFF, 0x24);
}

#[test]
fn pack_preserves_upper_24_bits_of_dest() {
    let d = run(pack(2, 1), 0x0000, |cpu| {
        cpu.regs.d[1] = 0x3539;
        cpu.regs.d[2] = 0xAABB_CCDD;
    });
    assert_eq!(d[2], 0xAABB_CC59, "only the low byte changes");
}

#[test]
fn pack_ignores_high_word_of_source() {
    // Only Dy[15:0] participates; Dy[31:16] must not leak in.
    let d = run(pack(2, 1), 0x0000, |cpu| cpu.regs.d[1] = 0xFFFF_3539);
    assert_eq!(d[2] & 0xFF, 0x59);
}

#[test]
fn unpk_bcd_to_two_ascii_digits() {
    // Dy low = $59, adj 0x3030 → 0x0509 + 0x3030 = 0x3539 ('5','9').
    let d = run(unpk(2, 1), 0x3030, |cpu| cpu.regs.d[1] = 0x0000_0059);
    assert_eq!(d[2] & 0xFFFF, 0x3539);
}

#[test]
fn unpk_zero_adjustment_spreads_nibbles() {
    // Dy low = $59 → 0x0509 with no adjustment.
    let d = run(unpk(2, 1), 0x0000, |cpu| cpu.regs.d[1] = 0x0000_0059);
    assert_eq!(d[2] & 0xFFFF, 0x0509);
}

#[test]
fn unpk_preserves_upper_16_bits_of_dest() {
    let d = run(unpk(2, 1), 0x0000, |cpu| {
        cpu.regs.d[1] = 0x59;
        cpu.regs.d[2] = 0xAABB_CCDD;
    });
    assert_eq!(d[2], 0xAABB_0509, "only the low word changes");
}

#[test]
fn pack_does_not_touch_flags() {
    // Seed all CCR flags set; PACK must leave them untouched.
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();
    let op = pack(2, 1);
    mem.write_word(PC, op);
    mem.write_word(PC + 2, 0);
    for k in 0..3 {
        mem.write_word(PC + 4 + (k as u32) * 2, 0x4E71);
    }
    cpu.regs.sr = 0x2000 | 0x1F; // all CCR bits set
    cpu.regs.d[1] = 0x3539;
    cpu.regs.pc = PC + 4;
    cpu.setup_prefetch(op, 0);
    let start = cpu.instruction_starts;
    for _ in 0..400 {
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        if cpu.instruction_starts > start {
            break;
        }
    }
    assert_eq!(cpu.regs.sr & 0x1F, 0x1F, "PACK must not change the CCR");
}
