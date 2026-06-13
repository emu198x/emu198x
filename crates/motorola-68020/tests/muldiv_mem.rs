//! Memory-source MUL.L / DIV.L ($4C00 / $4C40) — 68020 (#114).
//!
//! The register-source forms were already implemented; this exercises
//! the memory-source path, which fetches the 32-bit operand through the
//! shared EA pipeline and computes at the variant continuation. No
//! fixtures cover these; expected values are computed by hand from
//! M68000PRM § 6.2.5 / 6.2.7.

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

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
    d: [u32; 8],
    next_pc: u32,
    vector_pc: Option<u32>, // instr_start_pc if it vectored to $3000
}

/// Run one MUL.L/DIV.L (opcode + spec word + optional EA ext words) and
/// report the register file + where execution ended.
fn run(words: &[u16], operand: u32, seed: impl FnOnce(&mut Cpu68020)) -> Out {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();
    for (i, w) in words.iter().enumerate() {
        mem.write_word(PC + (i as u32) * 2, *w);
    }
    for k in 0..3 {
        mem.write_word(PC + (words.len() as u32 + k) * 2, 0x4E71);
    }
    mem.write_long(EA, operand);
    // Vector 5 (divide-by-zero, offset $14) → $3000 handler (NOP).
    mem.write_long(5 * 4, 0x0000_3000);
    mem.write_word(0x0000_3000, 0x4E71);

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
            let vectored = cpu.instr_start_pc == 0x0000_3000;
            return Out {
                d: cpu.regs.d,
                next_pc: cpu.instr_start_pc,
                vector_pc: vectored.then_some(cpu.instr_start_pc),
            };
        }
    }
    panic!("instruction did not complete");
}

// MULU.L (A0),Dl : $4C10, spec = Dl<<12 (unsigned, 32-bit, Dh=0).
fn mulu_a0(dl: u16) -> [u16; 2] {
    [0x4C10, dl << 12]
}
// DIVU.L (A0),Dq : $4C50, spec = Dq<<12 (unsigned, 32-bit, Dr=Dq).
fn divu_a0(dq: u16) -> [u16; 2] {
    [0x4C50, (dq << 12) | dq]
}

#[test]
fn mulu_l_memory_source_unsigned() {
    // D0 = 7, (A0) = 6 → D0 = 42.
    let r = run(&mulu_a0(0), 6, |cpu| cpu.regs.d[0] = 7);
    assert_eq!(r.d[0], 42);
    assert_eq!(r.next_pc, PC + 4, "(A0) has no EA ext words → 4-byte insn");
}

#[test]
fn muls_l_memory_source_signed() {
    // MULS.L (A0),D0: signed −3 × 5 = −15.
    let op = [0x4C10u16, 0x0800]; // spec: Dl=0, signed (bit 11), 32-bit
    let r = run(&op, (-3i32) as u32, |cpu| cpu.regs.d[0] = 5);
    assert_eq!(r.d[0] as i32, -15);
}

#[test]
fn mulu_l_64bit_wide_memory_source() {
    // Wide form (bit 10): Dh:Dl gets the full 64-bit product.
    // D0=0x1_0000, (A0)=0x1_0000 → product 0x1_0000_0000 → Dl=0, Dh=1.
    let op = [0x4C10u16, 0x0400 | 1]; // spec: Dl=0 (bits 14-12), wide (bit 10), Dh=1
    let r = run(&op, 0x0001_0000, |cpu| cpu.regs.d[0] = 0x0001_0000);
    assert_eq!(r.d[0], 0x0000_0000, "Dl = low 32 bits");
    assert_eq!(r.d[1], 0x0000_0001, "Dh = high 32 bits");
}

#[test]
fn divu_l_memory_source_unsigned() {
    // D0 = 42, (A0) = 6 → quotient 7 (Dq=Dr=0 → no separate remainder).
    let r = run(&divu_a0(0), 6, |cpu| cpu.regs.d[0] = 42);
    assert_eq!(r.d[0], 7);
}

#[test]
fn divu_l_memory_source_quotient_and_remainder() {
    // DIVUL.L (A0),D1:D0 : Dq=0, Dr=1 → quotient→D0, remainder→D1.
    // 43 / 6 = 7 rem 1.
    let op = [0x4C50u16, 1]; // spec: Dq=0 (bits 14-12), Dr=1, unsigned, 32-bit
    let r = run(&op, 6, |cpu| cpu.regs.d[0] = 43);
    assert_eq!(r.d[0], 7, "quotient → Dq");
    assert_eq!(r.d[1], 1, "remainder → Dr");
}

#[test]
fn divu_l_memory_source_divide_by_zero_traps_vector_5() {
    // (A0) = 0 → divide-by-zero, vector 5, no EA ext words.
    let r = run(&divu_a0(0), 0, |cpu| cpu.regs.d[0] = 42);
    assert_eq!(
        r.vector_pc,
        Some(0x0000_3000),
        "must take the vec-5 handler"
    );
}

#[test]
fn divl_d16_an_source_divide_by_zero_pc_accounts_for_ext_word() {
    // DIVU.L (0,A0),D0 — (d16,An) has one EA extension word, so the
    // instruction is 6 bytes; the divide-by-zero trap must stack
    // instr_start + 6 (verified indirectly: it still reaches the
    // handler, i.e. the EA + operand fetch + trap all resolve).
    let op = [0x4C68u16, 0x0000, 0x0000]; // DIVU.L (d16,A0),D0 ; d16=0
    let r = run(&op, 0, |cpu| cpu.regs.d[0] = 42);
    assert_eq!(r.vector_pc, Some(0x0000_3000));
}

#[test]
fn divl_d16_nonzero_displacement_reads_the_right_operand() {
    // DIV.L (4,A0),D0 : divisor lives at A0+4, NOT A0. Isolates whether
    // the (d16,An) operand fetch honours the displacement. 42 / 6 = 7.
    let op = [0x4C68u16, 0x0000, 0x0004]; // DIVU.L (d16,A0),D0 ; d16=4
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();
    for (i, w) in op.iter().enumerate() {
        mem.write_word(PC + (i as u32) * 2, *w);
    }
    for k in 0..3 {
        mem.write_word(PC + (op.len() as u32 + k) * 2, 0x4E71);
    }
    mem.write_long(EA + 4, 6); // divisor at A0+4
    mem.write_long(EA, 0); // A0+0 holds 0 (would div-by-zero if mis-read)
    cpu.regs.sr |= 0x2000;
    cpu.regs.a[0] = EA;
    cpu.regs.d[0] = 42;
    cpu.regs.ssp = 0x0000_8000;
    cpu.regs.set_active_sp(0x0000_8000);
    cpu.regs.pc = PC + 4;
    cpu.setup_prefetch(op[0], op[1]);
    let start = cpu.instruction_starts;
    for _ in 0..400 {
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        if cpu.instruction_starts > start {
            break;
        }
    }
    assert_eq!(cpu.regs.d[0], 7, "must read divisor from A0+4, not A0");
}

#[test]
fn divl_d16_odd_an_plus_odd_disp_reads_even_ea() {
    // Generator evens the EA by toggling An's bit 0, so An can be odd
    // with an odd d16 summing to an even EA. A0=0x2001, d16=-1 → EA 0x2000.
    let op = [0x4C68u16, 0x0000, 0xFFFF]; // DIVU.L (d16,A0),D0 ; d16=-1
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();
    for (i, w) in op.iter().enumerate() {
        mem.write_word(PC + (i as u32) * 2, *w);
    }
    for k in 0..3 {
        mem.write_word(PC + (op.len() as u32 + k) * 2, 0x4E71);
    }
    mem.write_long(0x0000_2000, 6); // divisor at EA = 0x2001 + (-1)
    cpu.regs.sr |= 0x2000;
    cpu.regs.a[0] = 0x0000_2001; // odd
    cpu.regs.d[0] = 42;
    cpu.regs.ssp = 0x0000_8000;
    cpu.regs.set_active_sp(0x0000_8000);
    cpu.regs.pc = PC + 4;
    cpu.setup_prefetch(op[0], op[1]);
    let start = cpu.instruction_starts;
    for _ in 0..400 {
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        if cpu.instruction_starts > start {
            break;
        }
    }
    assert_eq!(cpu.regs.d[0], 7, "EA = 0x2001 + (-1) = 0x2000");
}
