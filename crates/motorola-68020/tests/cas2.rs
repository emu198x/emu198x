//! CAS2 (`$0CFC` / `$0EFC` + two extension words) — 68020 (#114).
//!
//! Dual-address atomic compare-and-swap. Two register-held pointers
//! Rn1/Rn2 address the destinations; each is compared with its compare
//! register Dc1/Dc2. If *both* match, Du1/Du2 are written back;
//! otherwise both read values are loaded into Dc1/Dc2. The flags reflect
//! operand 1's comparison, or operand 2's if operand 1 matched.
//! M68000PRM § 6.2.4; matched to Musashi (`m68k_in.c` cas2).
//!
//! Spec word (32 bits, high half = operand 1, low half = operand 2),
//! each 16-bit half: bit 15 = D/A of Rn, bits 14-12 = Rn number,
//! bits 8-6 = Du, bits 2-0 = Dc.

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use motorola_68k_common::flags::Z;
use motorola_68000::bus::BusStatus;
use motorola_68000::cpu::State;
use motorola_68020::Cpu68020;

const PC: u32 = 0x0000_1000;
const EA1: u32 = 0x0000_2000;
const EA2: u32 = 0x0000_3000;

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
    fn write_sized(&mut self, a: u32, size: u32, v: u32) {
        match size {
            2 => self.write_word(a, v as u16),
            _ => self.write_long(a, v),
        }
    }
    fn read_sized(&self, a: u32, size: u32) -> u32 {
        match size {
            2 => u32::from(self.read_word(a)),
            _ => self.read_long(a),
        }
    }
}

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
    mem1: u32,
    mem2: u32,
}

/// One 16-bit half of the spec word.
fn half(da: u16, rn: u16, du: u16, dc: u16) -> u16 {
    (da << 15) | (rn << 12) | (du << 6) | dc
}

const CAS2_W: u16 = 0x0CFC;
const CAS2_L: u16 = 0x0EFC;

/// Run one CAS2. `size_bytes` is 2 (word) or 4 (long). dest1/dest2 are
/// written at EA1/EA2; A0=EA1, A1=EA2 are the pointers used by the test
/// spec words. Returns registers, SR, and the size-width values at
/// EA1/EA2 after.
fn run(
    opcode: u16,
    ext1: u16,
    ext2: u16,
    size_bytes: u32,
    dest1: u32,
    dest2: u32,
    seed: impl FnOnce(&mut Cpu68020),
) -> Out {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();
    mem.write_word(PC, opcode);
    mem.write_word(PC + 2, ext1);
    mem.write_word(PC + 4, ext2);
    for k in 0..3 {
        mem.write_word(PC + 6 + k * 2, 0x4E71);
    }
    mem.write_sized(EA1, size_bytes, dest1);
    mem.write_sized(EA2, size_bytes, dest2);

    cpu.regs.sr |= 0x2000;
    cpu.regs.a[0] = EA1;
    cpu.regs.a[1] = EA2;
    cpu.regs.ssp = 0x0000_8000;
    cpu.regs.set_active_sp(0x0000_8000);
    seed(&mut cpu);
    cpu.regs.pc = PC + 4;
    cpu.setup_prefetch(opcode, ext1);

    let start = cpu.instruction_starts;
    for _ in 0..400 {
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        if cpu.instruction_starts > start {
            return Out {
                d: cpu.regs.d,
                sr: cpu.regs.sr,
                mem1: mem.read_sized(EA1, size_bytes),
                mem2: mem.read_sized(EA2, size_bytes),
            };
        }
    }
    panic!("CAS2 did not complete");
}

// Operand 1: A0 pointer (da=1,rn=0), Du1=D2, Dc1=D0.
// Operand 2: A1 pointer (da=1,rn=1), Du2=D3, Dc2=D1.
fn ext1_a0() -> u16 {
    half(1, 0, 2, 0)
}
fn ext2_a1() -> u16 {
    half(1, 1, 3, 1)
}

#[test]
fn cas2_l_both_match_writes_both() {
    // [A0] == D0 and [A1] == D1 → write D2 to [A0], D3 to [A1]; Z set.
    let r = run(
        CAS2_L,
        ext1_a0(),
        ext2_a1(),
        4,
        0x1111_1111,
        0x2222_2222,
        |cpu| {
            cpu.regs.d[0] = 0x1111_1111; // Dc1
            cpu.regs.d[1] = 0x2222_2222; // Dc2
            cpu.regs.d[2] = 0xAAAA_AAAA; // Du1
            cpu.regs.d[3] = 0xBBBB_BBBB; // Du2
        },
    );
    assert_eq!(r.mem1, 0xAAAA_AAAA, "Du1 → [A0]");
    assert_eq!(r.mem2, 0xBBBB_BBBB, "Du2 → [A1]");
    assert_ne!(r.sr & Z, 0, "both match → Z set");
}

#[test]
fn cas2_l_first_mismatch_loads_both_no_write() {
    // [A0] != D0 → no write; load [A0]→D0, [A1]→D1; flags from operand 1.
    let r = run(
        CAS2_L,
        ext1_a0(),
        ext2_a1(),
        4,
        0x9999_9999,
        0x2222_2222,
        |cpu| {
            cpu.regs.d[0] = 0x1111_1111;
            cpu.regs.d[1] = 0x2222_2222;
            cpu.regs.d[2] = 0xAAAA_AAAA;
            cpu.regs.d[3] = 0xBBBB_BBBB;
        },
    );
    assert_eq!(r.mem1, 0x9999_9999, "no write on mismatch");
    assert_eq!(r.mem2, 0x2222_2222, "no write on mismatch");
    assert_eq!(r.d[0], 0x9999_9999, "Dc1 ← [A0]");
    assert_eq!(r.d[1], 0x2222_2222, "Dc2 ← [A1]");
    assert_eq!(r.sr & Z, 0, "operand-1 mismatch → Z clear");
}

#[test]
fn cas2_l_first_match_second_mismatch_loads_no_write() {
    // [A0] == D0 but [A1] != D1 → no write; load both; flags reflect
    // operand 2's comparison (Z clear).
    let r = run(
        CAS2_L,
        ext1_a0(),
        ext2_a1(),
        4,
        0x1111_1111,
        0x7777_7777,
        |cpu| {
            cpu.regs.d[0] = 0x1111_1111;
            cpu.regs.d[1] = 0x2222_2222; // != [A1] = 0x7777_7777
            cpu.regs.d[2] = 0xAAAA_AAAA;
            cpu.regs.d[3] = 0xBBBB_BBBB;
        },
    );
    assert_eq!(r.mem1, 0x1111_1111, "no write when operand 2 fails");
    assert_eq!(r.mem2, 0x7777_7777, "no write when operand 2 fails");
    assert_eq!(r.d[0], 0x1111_1111, "Dc1 ← [A0] (== old, unchanged)");
    assert_eq!(r.d[1], 0x7777_7777, "Dc2 ← [A1]");
    assert_eq!(r.sr & Z, 0, "operand-2 mismatch → Z clear");
}

#[test]
fn cas2_w_both_match_writes_low_words() {
    // Word size: only low words compared and written.
    let r = run(CAS2_W, ext1_a0(), ext2_a1(), 2, 0x1234, 0x5678, |cpu| {
        cpu.regs.d[0] = 0xFFFF_1234; // low word matches [A0]
        cpu.regs.d[1] = 0x0000_5678;
        cpu.regs.d[2] = 0xDEAD_AAAA; // Du1 low word written
        cpu.regs.d[3] = 0xBEEF_BBBB;
    });
    assert_eq!(r.mem1, 0xAAAA, "Du1 low word → [A0]");
    assert_eq!(r.mem2, 0xBBBB, "Du2 low word → [A1]");
    assert_ne!(r.sr & Z, 0);
}

#[test]
fn cas2_w_mismatch_loads_low_word_sign_extends_for_a_pointer() {
    // Word + mismatch: with an address-register pointer (D/A bit set),
    // Musashi sign-extends the loaded value into the data compare reg.
    // [A0] = 0x8001 (negative word) != D0 low word → D0 ← sign-extend.
    let r = run(CAS2_W, ext1_a0(), ext2_a1(), 2, 0x8001, 0x5678, |cpu| {
        cpu.regs.d[0] = 0x0000_1234;
        cpu.regs.d[1] = 0x0000_5678; // matches [A1] but op1 already failed
        cpu.regs.d[2] = 0;
        cpu.regs.d[3] = 0;
    });
    assert_eq!(r.d[0], 0xFFFF_8001, "A-pointer word load sign-extends Dc1");
    assert_eq!(r.sr & Z, 0);
}

#[test]
fn cas2_w_mismatch_loads_low_word_preserves_high_for_d_pointer() {
    // With a DATA-register pointer (D/A bit clear), the high word of the
    // compare register is preserved on a word mismatch load.
    // Operand 1: D0 pointer (da=0, rn=0) → but D0 is also Dc... use D4
    // as the pointer to avoid aliasing. da=0, rn=4 (D4).
    let ext1 = half(0, 4, 2, 0); // Rn1=D4, Du1=D2, Dc1=D0
    let ext2 = half(1, 1, 3, 1); // Rn2=A1, Du2=D3, Dc2=D1
    let r = run(CAS2_W, ext1, ext2, 2, 0xBEEF, 0x5678, |cpu| {
        cpu.regs.d[4] = EA1; // D4 holds the pointer
        cpu.regs.d[0] = 0xAAAA_1234; // low word 0x1234 != 0xBEEF
        cpu.regs.d[1] = 0x0000_5678;
        cpu.regs.d[2] = 0;
        cpu.regs.d[3] = 0;
    });
    assert_eq!(
        r.d[0], 0xAAAA_BEEF,
        "D-pointer word load preserves high word"
    );
    assert_eq!(r.sr & Z, 0);
}

#[test]
fn cas2_l_same_pointer_for_both_operands() {
    // Rn1 == Rn2 (both A0). Both reads hit [A0]; on a double match both
    // writes hit [A0] (the second wins). dest1==dest2 since same address.
    let ext1 = half(1, 0, 2, 0); // A0, Du1=D2, Dc1=D0
    let ext2 = half(1, 0, 3, 1); // A0, Du2=D3, Dc2=D1
    let r = run(CAS2_L, ext1, ext2, 4, 0x4242_4242, 0x4242_4242, |cpu| {
        cpu.regs.d[0] = 0x4242_4242; // Dc1 matches
        cpu.regs.d[1] = 0x4242_4242; // Dc2 matches
        cpu.regs.d[2] = 0x1111_1111; // Du1
        cpu.regs.d[3] = 0x9999_9999; // Du2 (written second → wins)
    });
    assert_eq!(
        r.mem1, 0x9999_9999,
        "second write (Du2) wins at shared address"
    );
    assert_ne!(r.sr & Z, 0);
}
