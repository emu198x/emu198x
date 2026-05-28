//! 68020 full-format extension word effective-address tests.
//!
//! The brief extension word `(d8,An,Xn)` is exercised by the
//! m68k-generated corpus, but that generator only ever emits brief
//! words (bit 8 clear), and the real-68000 Tom Harte harness skips
//! every indexed-addressing case outright. So the 68020 full format —
//! base displacement, scaled index, base/index suppression, and
//! memory indirection — had **no** coverage anywhere, which is how the
//! AGA Workbench palette bug (a full-format `lea` mis-decoded as
//! brief) survived a "100%" Tom Harte pass.
//!
//! Every expected address here is computed by hand from the 68020
//! User's Manual §2.2 / WinUAE `get_disp_ea_020`
//! (newcpu_common.cpp). The instruction under test is
//! `lea <ea>, A5`, so the destination A-register holds the computed
//! effective address after one instruction.

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use motorola_68000::bus::BusStatus;
use motorola_68000::cpu::State;
use motorola_68020::Cpu68020;

/// First instruction word sits here; extension words follow.
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
    fn read_byte(&self, a: u32) -> u8 {
        *self.bytes.get(&(a & 0x00FF_FFFF)).unwrap_or(&0)
    }
    fn read_word(&self, a: u32) -> u16 {
        (u16::from(self.read_byte(a)) << 8) | u16::from(self.read_byte(a.wrapping_add(1)))
    }
    fn write_byte(&mut self, a: u32, v: u8) {
        self.bytes.insert(a & 0x00FF_FFFF, v);
    }
    fn write_word(&mut self, a: u32, v: u16) {
        self.write_byte(a, (v >> 8) as u8);
        self.write_byte(a.wrapping_add(1), v as u8);
    }
    fn write_long(&mut self, a: u32, v: u32) {
        self.write_word(a, (v >> 16) as u16);
        self.write_word(a.wrapping_add(2), v as u16);
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
                    u16::from(mem.read_byte(*addr))
                };
                cpu.bus_status = BusStatus::Ready(v);
            } else {
                let v = data.unwrap_or(0);
                if *is_word {
                    mem.write_word(*addr, v);
                } else {
                    mem.write_byte(*addr, v as u8);
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

/// Run `lea <ea>, A5` once and return the resulting A5.
///
/// `opcode` is the LEA word (encoding the EA mode/register and A5 as
/// destination). `words` are the extension words in stream order
/// (format word first, then base/outer displacement words). `setup`
/// seeds registers and any memory the indirection reads.
fn run_lea(opcode: u16, words: &[u16], setup: impl FnOnce(&mut Cpu68020, &mut Mem)) -> u32 {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();

    mem.write_word(PC, opcode);
    for (i, w) in words.iter().enumerate() {
        mem.write_word(PC + 2 + (i as u32) * 2, *w);
    }
    // Trailing NOPs so the prefetch past the instruction is benign.
    for k in 0..4 {
        mem.write_word(PC + 2 + ((words.len() + k) as u32) * 2, 0x4E71);
    }

    setup(&mut cpu, &mut mem);

    cpu.regs.sr |= 0x2000; // supervisor
    cpu.regs.pc = PC + 4;
    cpu.setup_prefetch(opcode, words.first().copied().unwrap_or(0x4E71));

    let start = cpu.instruction_starts;
    for _ in 0..400 {
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        if cpu.instruction_starts > start {
            return cpu.regs.a[5];
        }
    }
    panic!("instruction did not complete");
}

// LEA (d8,A3,Xn),A5 — EA mode 6, base register A3, dest A5.
const LEA_A3_A5: u16 = 0x4BF3;
// LEA (d8,A2,Xn),A5 — base register A2.
const LEA_A2_A5: u16 = 0x4BF2;
// LEA (d8,PC,Xn),A5 — EA mode 7 reg 3 (PC-relative indexed).
const LEA_PC_A5: u16 = 0x4BFB;

#[test]
fn synchronous_null_bd_scale2_word_index() {
    // ext $0310: full format, index D0.w, scale ×2, null base
    // displacement, no memory indirect — the exact Workbench 3.1
    // palette `lea (A3,D0.w*2),A5`. EA = A3 + D0.w*2.
    let a5 = run_lea(LEA_A3_A5, &[0x0310], |cpu, _| {
        cpu.regs.a[3] = 0x0000_1B78; // ColorTable
        cpu.regs.d[0] = 0x0003_0000; // D0.w = 0 (firstcolor)
    });
    assert_eq!(a5, 0x0000_1B78, "firstcolor 0 must land at entry 0");

    let a5 = run_lea(LEA_A3_A5, &[0x0310], |cpu, _| {
        cpu.regs.a[3] = 0x0000_1B78;
        cpu.regs.d[0] = 0x0000_0004; // D0.w = 4
    });
    assert_eq!(a5, 0x0000_1B80, "4 * scale 2 = +8 bytes");
}

#[test]
fn synchronous_negative_word_index_sign_extends() {
    // D0.w = -2 → sign-extended, ×2 = -4.
    let a5 = run_lea(LEA_A3_A5, &[0x0310], |cpu, _| {
        cpu.regs.a[3] = 0x0000_1B78;
        cpu.regs.d[0] = 0x0000_FFFE;
    });
    assert_eq!(a5, 0x0000_1B74);
}

#[test]
fn synchronous_long_index_scale4() {
    // ext $1D10: full, index D1.l, scale ×4, null BD, no indirect.
    let a5 = run_lea(LEA_A2_A5, &[0x1D10], |cpu, _| {
        cpu.regs.a[2] = 0x0001_0000;
        cpu.regs.d[1] = 0x0000_0003; // long index 3, ×4 = 12
    });
    assert_eq!(a5, 0x0001_000C);
}

#[test]
fn word_base_displacement() {
    // ext $0120: full, index D0.w scale ×1, word base displacement.
    let a5 = run_lea(LEA_A3_A5, &[0x0120, 0x0100], |cpu, _| {
        cpu.regs.a[3] = 0x0000_2000;
        cpu.regs.d[0] = 0x0000_0002;
    });
    assert_eq!(a5, 0x0000_2102, "A3 + 0x100 + 2");

    // Negative word displacement sign-extends.
    let a5 = run_lea(LEA_A3_A5, &[0x0120, 0xFFF0], |cpu, _| {
        cpu.regs.a[3] = 0x0000_2000;
        cpu.regs.d[0] = 0x0000_0002;
    });
    assert_eq!(a5, 0x0000_1FF2, "A3 - 16 + 2");
}

#[test]
fn long_base_displacement() {
    // ext $0130: full, index D0.w scale ×1, long base displacement.
    let a5 = run_lea(LEA_A3_A5, &[0x0130, 0x0001, 0x0000], |cpu, _| {
        cpu.regs.a[3] = 0x0000_2000;
        cpu.regs.d[0] = 0x0000_0000;
    });
    assert_eq!(a5, 0x0001_2000, "A3 + 0x0001_0000");
}

#[test]
fn base_suppress_ignores_an() {
    // ext $0190: full, base suppress (BS), index D0.w scale ×1, null BD.
    let a5 = run_lea(LEA_A3_A5, &[0x0190], |cpu, _| {
        cpu.regs.a[3] = 0x9999_9999; // must be ignored
        cpu.regs.d[0] = 0x0000_0005;
    });
    assert_eq!(a5, 0x0000_0005, "base suppressed: EA = index only");
}

#[test]
fn index_suppress_ignores_xn() {
    // ext $0160: full, index suppress (IS), word base displacement.
    let a5 = run_lea(LEA_A3_A5, &[0x0160, 0x0040], |cpu, _| {
        cpu.regs.a[3] = 0x0000_3000;
        cpu.regs.d[0] = 0x7777_7777; // must be ignored
    });
    assert_eq!(a5, 0x0000_3040, "index suppressed: EA = An + BD");
}

#[test]
fn memory_indirect_preindexed_word_outer() {
    // ext $0122: full, index D0.w scale ×1, word BD, pre-indexed
    // memory indirect (bit 2 = 0), word outer displacement.
    // intermediate = A3 + BD + index = 0x4000 + 0x10 + 2 = 0x4012
    // EA = get_long(0x4012) + OD = 0x5000 + 4 = 0x5004.
    let a5 = run_lea(LEA_A3_A5, &[0x0122, 0x0010, 0x0004], |cpu, mem| {
        cpu.regs.a[3] = 0x0000_4000;
        cpu.regs.d[0] = 0x0000_0002;
        mem.write_long(0x0000_4012, 0x0000_5000);
    });
    assert_eq!(a5, 0x0000_5004);
}

#[test]
fn memory_indirect_postindexed_null_outer() {
    // ext $1315: full, index D1.w scale ×2, null BD, post-indexed
    // memory indirect (bit 2 = 1), null outer displacement.
    // base = get_long(A3) = 0x7000, then + index (3 ×2 = 6).
    let a5 = run_lea(LEA_A3_A5, &[0x1315], |cpu, mem| {
        cpu.regs.a[3] = 0x0000_6000;
        cpu.regs.d[1] = 0x0000_0003;
        mem.write_long(0x0000_6000, 0x0000_7000);
    });
    assert_eq!(a5, 0x0000_7006);
}

#[test]
fn pc_relative_full_format_synchronous() {
    // ext $0310 on PC-relative indexed: base = address of the
    // extension word (PC + 2 = 0x1002), index D0.w ×2.
    let a5 = run_lea(LEA_PC_A5, &[0x0310], |cpu, _| {
        cpu.regs.d[0] = 0x0000_0004;
    });
    assert_eq!(a5, 0x0000_100A, "0x1002 + 4*2");
}
