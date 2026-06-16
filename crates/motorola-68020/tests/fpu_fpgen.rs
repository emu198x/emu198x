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

/// Read `n` big-endian bytes back from memory.
fn read_bytes(mem: &Mem, addr: u32, n: usize) -> Vec<u8> {
    (0..n)
        .map(|i| {
            *mem.bytes
                .get(&((addr + i as u32) & 0x00FF_FFFF))
                .unwrap_or(&0)
        })
        .collect()
}

struct Out {
    fp: [FpReg; 8],
    fpsr: u32,
    a: [u32; 7],
}

/// cpGEN reg-to-reg extension word: R/M = 0, source/dest Fpn, opmode.
fn ext(src: u16, dst: u16, opmode: u16) -> u16 {
    (src << 10) | (dst << 7) | opmode
}

/// Run one cpGEN reg-to-reg op with the FP register file seeded by `seed`.
fn run(opmode: u16, src: u16, dst: u16, seed: impl FnOnce(&mut Cpu68020)) -> Out {
    run_raw(ext(src, dst, opmode), seed)
}

/// Run one cpGEN op with a fully-specified extension word (for forms that
/// set the R/M bit, e.g. FMOVECR).
fn run_raw(w2: u16, seed: impl FnOnce(&mut Cpu68020)) -> Out {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();
    let opcode = 0xF200; // cpID 1, op-class 0, EA bits unused for reg-to-reg
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
                a: cpu.regs.a,
            };
        }
    }
    panic!("FP op did not complete");
}

/// FMOVECR extension word: R/M = 1, source specifier 7, dest Fpn, 7-bit
/// ROM offset.
fn fmovecr_ext(dst: u16, offset: u16) -> u16 {
    0x4000 | (7 << 10) | (dst << 7) | offset
}

/// Memory-source extension word: R/M = 1, format specifier, dest Fpn,
/// opmode.
fn mem_ext(format: u16, dst: u16, opmode: u16) -> u16 {
    0x4000 | (format << 10) | (dst << 7) | opmode
}

/// Control register set captured after a FMOVE FPcr ↔ ea.
struct Ctrl {
    fpcr: u32,
    fpsr: u32,
    fpiar: u32,
    d: [u32; 8],
    a: [u32; 7],
}

/// FMOVE-control extension word. `dir`: 0 = ea → reg (sub-op 4), 1 = reg →
/// ea (sub-op 5). `reg_mask`: bit 2 = FPCR, bit 1 = FPSR, bit 0 = FPIAR.
fn ctrl_ext(dir: u16, reg_mask: u16) -> u16 {
    0x8000 | (dir << 13) | (reg_mask << 10)
}

/// Run a FMOVE FPcr ↔ ea op (opcode carries the EA bits) and return the
/// control + data + address registers.
fn run_ctrl(opcode: u16, w2: u16, seed: impl FnOnce(&mut Cpu68020)) -> Ctrl {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();
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
            return Ctrl {
                fpcr: cpu.regs.fpcr,
                fpsr: cpu.regs.fpsr,
                fpiar: cpu.regs.fpiar,
                d: cpu.regs.d,
                a: cpu.regs.a,
            };
        }
    }
    panic!("FMOVE control op did not complete");
}

/// Run a FMOVE ea → FPcr that loads from memory at `addr`, returning the
/// control + data + address registers.
fn run_ctrl_mem(
    opcode: u16,
    w2: u16,
    addr: u32,
    bytes: &[u8],
    seed: impl FnOnce(&mut Cpu68020),
) -> Ctrl {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();
    mem.write_word(PC, opcode);
    mem.write_word(PC + 2, w2);
    for (i, &b) in bytes.iter().enumerate() {
        mem.bytes.insert((addr + i as u32) & 0x00FF_FFFF, b);
    }

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
            return Ctrl {
                fpcr: cpu.regs.fpcr,
                fpsr: cpu.regs.fpsr,
                fpiar: cpu.regs.fpiar,
                d: cpu.regs.d,
                a: cpu.regs.a,
            };
        }
    }
    panic!("FMOVE control-mem op did not complete");
}

#[test]
fn fmove_dn_to_fpcr() {
    // FMOVE.L D0,FPCR — opcode $F200 (D0), ea→reg, mask = FPCR.
    let r = run_ctrl(0xF200, ctrl_ext(0, 4), |cpu| cpu.regs.d[0] = 0x0000_0030);
    assert_eq!(r.fpcr, 0x0000_0030, "D0 → FPCR");
}

#[test]
fn fmove_memory_to_fpcr() {
    // FMOVE.L (A0),FPCR — opcode $F210 (A0), ea→reg, mask = FPCR.
    let r = run_ctrl_mem(
        0xF210,
        ctrl_ext(0, 4),
        DATA,
        &[0x00, 0x00, 0x00, 0x30],
        |cpu| {
            cpu.regs.set_a(0, DATA);
        },
    );
    assert_eq!(r.fpcr, 0x0000_0030, "(A0) → FPCR");
}

#[test]
fn fmove_fpcr_to_memory() {
    // FMOVE.L FPCR,(A0) → 4 bytes at (A0). reg→ea, mask = FPCR.
    let (_r, mem) = run_store(0xF210, ctrl_ext(1, 4), |cpu| {
        cpu.regs.set_a(0, DATA);
        cpu.regs.fpcr = 0x0000_0030;
    });
    assert_eq!(read_bytes(&mem, DATA, 4), [0x00, 0x00, 0x00, 0x30]);
}

#[test]
fn fmove_fpsr_postincrement_from_memory() {
    // FMOVE.L (A0)+,FPSR — opcode $F218 (mode 3), ea→reg. A0 advances 4.
    let r = run_ctrl_mem(
        0xF218,
        ctrl_ext(0, 2),
        DATA,
        &[0x0F, 0x00, 0x00, 0x00],
        |cpu| {
            cpu.regs.set_a(0, DATA);
        },
    );
    assert_eq!(r.fpsr, 0x0F00_0000, "(A0)+ → FPSR");
    assert_eq!(r.a[0], DATA + 4, "(A0)+ steps by 4");
}

#[test]
fn fmove_fpiar_to_predecrement_memory() {
    // FMOVE.L FPIAR,-(A0) — opcode $F220 (mode 4), reg→ea. A0 = DATA+4 →
    // pre-decrements to DATA.
    let (r, mem) = run_store(0xF220, ctrl_ext(1, 1), |cpu| {
        cpu.regs.set_a(0, DATA + 4);
        cpu.regs.fpiar = 0x0001_2340;
    });
    assert_eq!(r.a[0], DATA, "-(A0) steps back by 4");
    assert_eq!(read_bytes(&mem, DATA, 4), [0x00, 0x01, 0x23, 0x40]);
}

// --- FScc Dn (op-class 1): set a byte on the FP condition ---

#[test]
fn fscc_true_sets_low_byte() {
    // FST D0 (condition T = $0F) → D0 low byte all-ones, upper preserved.
    let r = run_ctrl(0xF240, 0x000F, |cpu| cpu.regs.d[0] = 0x1234_5600);
    assert_eq!(r.d[0], 0x1234_56FF, "FST sets the low byte to $FF");
}

#[test]
fn fscc_false_clears_low_byte() {
    // FSF D0 (condition F = $00) → D0 low byte cleared, upper preserved.
    let r = run_ctrl(0xF240, 0x0000, |cpu| cpu.regs.d[0] = 0x1234_56FF);
    assert_eq!(r.d[0], 0x1234_5600, "FSF clears the low byte");
}

#[test]
fn fseq_follows_fpsr_z() {
    // FSEQ D1 (condition EQ = $01) tracks FPSR Z.
    let r = run_ctrl(0xF241, 0x0001, |cpu| {
        cpu.regs.d[1] = 0;
        cpu.regs.fpsr = 1 << 26; // Z set
    });
    assert_eq!(r.d[1] & 0xFF, 0xFF, "Z set → FSEQ true");

    let r = run_ctrl(0xF241, 0x0001, |cpu| {
        cpu.regs.d[1] = 0;
        cpu.regs.fpsr = 0;
    });
    assert_eq!(r.d[1] & 0xFF, 0x00, "Z clear → FSEQ false");
}

#[test]
fn fscc_true_to_memory() {
    // FST (A0) → byte $FF at (A0). Opcode $F250 (op-class 1, mode 2).
    let (_r, mem) = run_store(0xF250, 0x000F, |cpu| cpu.regs.set_a(0, DATA));
    assert_eq!(read_bytes(&mem, DATA, 1), [0xFF]);
}

#[test]
fn fscc_false_to_memory() {
    // FSF (A0) → byte $00.
    let (_r, mem) = run_store(0xF250, 0x0000, |cpu| cpu.regs.set_a(0, DATA));
    assert_eq!(read_bytes(&mem, DATA, 1), [0x00]);
}

#[test]
fn fscc_to_memory_postincrement() {
    // FST (A0)+ ($F258) → $FF, A0 advances by 1.
    let (r, mem) = run_store(0xF258, 0x000F, |cpu| cpu.regs.set_a(0, DATA));
    assert_eq!(read_bytes(&mem, DATA, 1), [0xFF]);
    assert_eq!(r.a[0], DATA + 1, "(A0)+ steps by 1 for a byte store");
}

#[test]
fn fscc_to_d16_an() {
    // FST $0010(A0) ($F268) → $FF at A0 + 0x10. A0 = 0x1FF0 → 0x2000.
    let (_r, mem) = run_store_ea(0xF268, 0x000F, &[0x0010], |cpu| {
        cpu.regs.set_a(0, 0x0000_1FF0);
    });
    assert_eq!(read_bytes(&mem, 0x0000_2000, 1), [0xFF]);
}

#[test]
fn fmove_dn_to_fpsr_and_fpiar() {
    let r = run_ctrl(0xF201, ctrl_ext(0, 2), |cpu| cpu.regs.d[1] = 0x0F00_0000);
    assert_eq!(r.fpsr, 0x0F00_0000, "D1 → FPSR");

    let r = run_ctrl(0xF202, ctrl_ext(0, 1), |cpu| cpu.regs.d[2] = 0x0000_1000);
    assert_eq!(r.fpiar, 0x0000_1000, "D2 → FPIAR");
}

#[test]
fn fmove_fpcr_to_dn() {
    // FMOVE.L FPCR,D3 — reg→ea, mask = FPCR.
    let r = run_ctrl(0xF203, ctrl_ext(1, 4), |cpu| {
        cpu.regs.fpcr = 0x0000_0020;
        cpu.regs.d[3] = 0xDEAD_BEEF;
    });
    assert_eq!(r.d[3], 0x0000_0020, "FPCR → D3");
}

#[test]
fn fmove_fpsr_to_dn() {
    let r = run_ctrl(0xF204, ctrl_ext(1, 2), |cpu| {
        cpu.regs.fpsr = 0x0800_0000; // some FPCC bits
        cpu.regs.d[4] = 0;
    });
    assert_eq!(r.d[4], 0x0800_0000, "FPSR → D4");
}

#[test]
fn fmove_control_to_an() {
    // FMOVE.L FPIAR,A1 — reg→ea with An destination (mode 1, opcode
    // $F209). FPIAR holds an instruction address.
    let r = run_ctrl(0xF209, ctrl_ext(1, 1), |cpu| {
        cpu.regs.fpiar = 0x0001_2340;
        cpu.regs.set_a(1, 0);
    });
    assert_eq!(r.a[1], 0x0001_2340, "FPIAR → A1");
}

#[test]
fn fmove_dn_to_fpcr_changes_rounding() {
    // Writing FPCR sets the rounding mode used by subsequent ops. Mode
    // bits are 5-4; 0x10 = toward zero. We only check the register lands;
    // the rounding behaviour is covered by the softfloat tests.
    let r = run_ctrl(0xF205, ctrl_ext(0, 4), |cpu| cpu.regs.d[5] = 0x0000_0010);
    assert_eq!(r.fpcr & 0x30, 0x10, "FPCR MODE field = toward-zero");
}

/// Run a cpGEN op with a memory source: `opcode` carries the EA bits,
/// `w2` the FP extension word, `a` the address-register values, and
/// `bytes` the big-endian operand placed at `0x2000`.
fn run_mem(
    opcode: u16,
    w2: u16,
    areg: usize,
    addr: u32,
    bytes: &[u8],
    seed: impl FnOnce(&mut Cpu68020),
) -> Out {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();
    mem.write_word(PC, opcode);
    mem.write_word(PC + 2, w2);
    for (i, &b) in bytes.iter().enumerate() {
        mem.bytes.insert((addr + i as u32) & 0x00FF_FFFF, b);
    }

    cpu.set_fpu_present(true);
    cpu.regs.sr |= 0x2000;
    cpu.regs.ssp = 0x0000_8000;
    cpu.regs.set_active_sp(0x0000_8000);
    cpu.regs.set_a(areg, addr);
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
                a: cpu.regs.a,
            };
        }
    }
    panic!("FP memory op did not complete");
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

// --- FADD / FSUB (opmode 0x22 / 0x28), via the SoftFloat floatx80 port.
// `run` computes dst = dst <op> src, mirroring Musashi's
// `floatx80_add(REG_FP[dst], source)`. Default FPCR → round-to-nearest. ---

const POS_TWO: FpReg = FpReg::new(0x4000, 0x8000_0000_0000_0000);
const POS_THREE: FpReg = FpReg::new(0x4000, 0xC000_0000_0000_0000);

#[test]
fn fadd_one_plus_one_is_two() {
    // FADD FP1,FP2 with FP2 = 1.0, FP1 = 1.0 → FP2 = 2.0.
    let r = run(0x22, 1, 2, |cpu| {
        cpu.regs.fp[1] = POS_ONE;
        cpu.regs.fp[2] = POS_ONE;
    });
    assert_eq!(r.fp[2], POS_TWO, "1.0 + 1.0 = 2.0");
    assert_eq!(r.fpsr & FPCC_N, 0);
    assert_eq!(r.fpsr & FPCC_Z, 0);
}

#[test]
fn fadd_two_plus_one_is_three() {
    let r = run(0x22, 1, 2, |cpu| {
        cpu.regs.fp[1] = POS_ONE;
        cpu.regs.fp[2] = POS_TWO;
    });
    assert_eq!(r.fp[2], POS_THREE, "2.0 + 1.0 = 3.0");
}

#[test]
fn fsub_three_minus_one_is_two() {
    // FSUB FP1,FP2 with FP2 = 3.0, FP1 = 1.0 → FP2 = 3.0 − 1.0 = 2.0.
    let r = run(0x28, 1, 2, |cpu| {
        cpu.regs.fp[1] = POS_ONE;
        cpu.regs.fp[2] = POS_THREE;
    });
    assert_eq!(r.fp[2], POS_TWO, "3.0 − 1.0 = 2.0");
}

#[test]
fn fsub_equal_is_zero() {
    // FSUB FP1,FP2 with FP2 = FP1 = 1.0 → +0.0, Z set.
    let r = run(0x28, 1, 2, |cpu| {
        cpu.regs.fp[1] = POS_ONE;
        cpu.regs.fp[2] = POS_ONE;
    });
    assert_eq!(r.fp[2], POS_ZERO, "1.0 − 1.0 = +0.0");
    assert_ne!(r.fpsr & FPCC_Z, 0, "zero result → Z set");
}

#[test]
fn fadd_negates_to_negative_result() {
    // FADD FP1,FP2 with FP2 = 1.0, FP1 = −3.0 → 1.0 + (−3.0) = −2.0.
    let r = run(0x22, 1, 2, |cpu| {
        cpu.regs.fp[1] = FpReg::new(0xC000, 0xC000_0000_0000_0000); // −3.0
        cpu.regs.fp[2] = POS_ONE;
    });
    assert_eq!(r.fp[2], FpReg::new(0xC000, 0x8000_0000_0000_0000), "= −2.0");
    assert_ne!(r.fpsr & FPCC_N, 0, "negative result → N set");
}

#[test]
fn fsadd_prefix_normalises_to_fadd() {
    // FSADD (opmode 0x62 = FADD | single-prefix bit 0x40) must decode to
    // FADD at extended precision, matching Musashi's opmode stripping.
    let r = run(0x62, 1, 2, |cpu| {
        cpu.regs.fp[1] = POS_ONE;
        cpu.regs.fp[2] = POS_ONE;
    });
    assert_eq!(r.fp[2], POS_TWO, "FSADD strips to FADD → 2.0");
}

// --- FMUL / FDIV / FSQRT / FINT / FINTRZ / FCMP (the rest of the wired
// arithmetic, via the SoftFloat floatx80 port). ---

const POS_SIX: FpReg = FpReg::new(0x4001, 0xC000_0000_0000_0000); // 6.0
const POS_FOUR: FpReg = FpReg::new(0x4001, 0x8000_0000_0000_0000); // 4.0
const POS_FIVE: FpReg = FpReg::new(0x4001, 0xA000_0000_0000_0000); // 5.0
const POS_TEN: FpReg = FpReg::new(0x4002, 0xA000_0000_0000_0000); // 10.0

#[test]
fn fmul_two_times_three_is_six() {
    let r = run(0x23, 1, 2, |cpu| {
        cpu.regs.fp[1] = POS_THREE;
        cpu.regs.fp[2] = POS_TWO;
    });
    assert_eq!(r.fp[2], POS_SIX, "2.0 × 3.0 = 6.0");
}

#[test]
fn fdiv_ten_by_two_is_five() {
    let r = run(0x20, 1, 2, |cpu| {
        cpu.regs.fp[1] = POS_TWO;
        cpu.regs.fp[2] = POS_TEN;
    });
    assert_eq!(r.fp[2], POS_FIVE, "10.0 / 2.0 = 5.0");
}

#[test]
fn fsqrt_of_four_is_two() {
    // FSQRT is unary on the source → dst.
    let r = run(0x04, 1, 2, |cpu| {
        cpu.regs.fp[1] = POS_FOUR;
        cpu.regs.fp[2] = NEG_ONE; // overwritten
    });
    assert_eq!(r.fp[2], POS_TWO, "√4 = 2.0");
    assert_eq!(r.fpsr & FPCC_N, 0);
}

#[test]
fn fint_rounds_to_nearest_integer() {
    // FINT of 2.5 → 2.0 (round-to-nearest-even, default FPCR).
    let two_point_five = FpReg::new(0x4000, 0xA000_0000_0000_0000);
    let r = run(0x01, 1, 2, |cpu| cpu.regs.fp[1] = two_point_five);
    assert_eq!(r.fp[2], POS_TWO, "FINT 2.5 → 2.0");
}

#[test]
fn fintrz_truncates_toward_zero() {
    // FINTRZ of 2.5 → 2.0 regardless of FPCR mode.
    let two_point_five = FpReg::new(0x4000, 0xA000_0000_0000_0000);
    let r = run(0x03, 1, 2, |cpu| cpu.regs.fp[1] = two_point_five);
    assert_eq!(r.fp[2], POS_TWO, "FINTRZ 2.5 → 2.0");
}

#[test]
fn fcmp_sets_codes_without_writing() {
    // FCMP FP1,FP2 with FP2 = 1.0, FP1 = 1.0 → equal → Z set, dst kept.
    let r = run(0x38, 1, 2, |cpu| {
        cpu.regs.fp[1] = POS_ONE;
        cpu.regs.fp[2] = POS_ONE;
    });
    assert_ne!(r.fpsr & FPCC_Z, 0, "1.0 vs 1.0 → equal → Z set");
    assert_eq!(r.fp[2], POS_ONE, "FCMP must not write the destination");
}

#[test]
fn fcmp_less_than_sets_negative() {
    // FCMP FP1,FP2 with FP2 = 1.0, FP1 = 2.0 → 1.0 − 2.0 < 0 → N set.
    let r = run(0x38, 1, 2, |cpu| {
        cpu.regs.fp[1] = POS_TWO;
        cpu.regs.fp[2] = POS_ONE;
    });
    assert_ne!(r.fpsr & FPCC_N, 0, "dst < source → N set");
    assert_eq!(r.fpsr & FPCC_Z, 0);
}

#[test]
fn fcmp_infinities_equal_sets_n_and_z() {
    // FCMP +inf,+inf → equal via the inf special path → Z set.
    let r = run(0x38, 1, 2, |cpu| {
        cpu.regs.fp[1] = POS_INF;
        cpu.regs.fp[2] = POS_INF;
    });
    assert_ne!(r.fpsr & FPCC_Z, 0, "+inf vs +inf → equal → Z set");
}

#[test]
fn fsqrt_of_negative_sets_nan() {
    // √(−1) → default NaN → NAN code set.
    let r = run(0x04, 1, 2, |cpu| cpu.regs.fp[1] = NEG_ONE);
    assert_ne!(r.fpsr & FPCC_NAN, 0, "√(−1) → NaN");
}

#[test]
fn fgetexp_extracts_exponent() {
    // FGETEXP (opmode 0x1E) is unary on the source → dst. 6.0 = 1.5 × 2^2 →
    // exponent 2.0.
    let six = FpReg::new(0x4001, 0xC000_0000_0000_0000);
    let two = FpReg::new(0x4000, 0x8000_0000_0000_0000);
    let r = run(0x1E, 1, 2, |cpu| {
        cpu.regs.fp[1] = six;
        cpu.regs.fp[2] = NEG_ONE; // overwritten
    });
    assert_eq!(r.fp[2], two, "FGETEXP 6.0 → 2.0");
}

#[test]
fn fscale_scales_by_power_of_two() {
    // FSCALE (opmode 0x26) computes dst × 2^(int part of source): 1.0 scaled
    // by 3.0 → 8.0.
    let one = FpReg::new(0x3FFF, 0x8000_0000_0000_0000);
    let three = FpReg::new(0x4000, 0xC000_0000_0000_0000);
    let eight = FpReg::new(0x4002, 0x8000_0000_0000_0000);
    let r = run(0x26, 1, 2, |cpu| {
        cpu.regs.fp[1] = three; // source = scale amount
        cpu.regs.fp[2] = one; // dst = value scaled, in place
    });
    assert_eq!(r.fp[2], eight, "FSCALE 1.0 by 3 → 8.0");
}

#[test]
fn fgetman_extracts_mantissa() {
    // FGETMAN (opmode 0x1F) is unary on the source → dst. 6.0 = 1.5 × 2^2,
    // so the mantissa in [1.0, 2.0) is 1.5.
    let six = FpReg::new(0x4001, 0xC000_0000_0000_0000);
    let one_point_five = FpReg::new(0x3FFF, 0xC000_0000_0000_0000);
    let r = run(0x1F, 1, 2, |cpu| {
        cpu.regs.fp[1] = six;
        cpu.regs.fp[2] = NEG_ONE; // overwritten
    });
    assert_eq!(r.fp[2], one_point_five, "FGETMAN 6.0 → 1.5");
    assert_eq!(r.fpsr & FPCC_N, 0);
    assert_eq!(r.fpsr & FPCC_NAN, 0);
}

// --- FMOVECR (R/M = 1, src = 7): on-chip constant ROM load. ---

#[test]
fn fmovecr_loads_one() {
    // FMOVECR #$32,FP3 → FP3 = 1.0.
    let r = run_raw(fmovecr_ext(3, 0x32), |_| {});
    assert_eq!(r.fp[3], POS_ONE, "ROM offset 0x32 = 1.0");
    assert_eq!(r.fpsr & FPCC_Z, 0);
    assert_eq!(r.fpsr & FPCC_N, 0);
}

#[test]
fn fmovecr_loads_pi() {
    // FMOVECR #$00,FP0 → FP0 = π (0x4000:C90FDAA22168C235).
    let r = run_raw(fmovecr_ext(0, 0x00), |_| {});
    assert_eq!(r.fp[0], FpReg::new(0x4000, 0xC90F_DAA2_2168_C235), "π");
}

#[test]
fn fmovecr_loads_zero_sets_z() {
    // FMOVECR #$0F,FP5 → FP5 = +0.0 → Z set.
    let r = run_raw(fmovecr_ext(5, 0x0F), |cpu| cpu.regs.fp[5] = NEG_ONE);
    assert_eq!(r.fp[5], POS_ZERO, "ROM offset 0x0F = 0.0");
    assert_ne!(r.fpsr & FPCC_Z, 0, "zero constant → Z set");
}

#[test]
fn fmovecr_loads_ten() {
    // FMOVECR #$33,FP1 → FP1 = 10.0 (= int32_to_floatx80(10)).
    let r = run_raw(fmovecr_ext(1, 0x33), |_| {});
    assert_eq!(r.fp[1], FpReg::new(0x4002, 0xA000_0000_0000_0000), "10.0");
}

#[test]
fn fmovecr_unlisted_offset_reads_zero() {
    // An unpopulated ROM slot reads +0.0, matching Musashi's default.
    let r = run_raw(fmovecr_ext(2, 0x01), |cpu| cpu.regs.fp[2] = NEG_ONE);
    assert_eq!(r.fp[2], POS_ZERO, "unlisted offset → +0.0");
}

// --- Memory-source operands (R/M = 1) via the EA-fetch pipeline.
// Opcode $F210 = cpGEN, EA = (A0). Each format widens to floatx80. ---

const DATA: u32 = 0x0000_2000;

#[test]
fn fmove_long_memory_to_fp() {
    // FMOVE.L (A0),FP0 — big-endian 32-bit integer 5 → 5.0.
    let r = run_mem(
        0xF210,
        mem_ext(0, 0, 0x00),
        0,
        DATA,
        &[0x00, 0x00, 0x00, 0x05],
        |_| {},
    );
    assert_eq!(r.fp[0], int_fx(5), "FMOVE.L 5 → 5.0");
}

#[test]
fn fmove_single_memory_to_fp() {
    // FMOVE.S (A0),FP0 — 0x3F800000 = 1.0f → 1.0.
    let r = run_mem(
        0xF210,
        mem_ext(1, 0, 0x00),
        0,
        DATA,
        &[0x3F, 0x80, 0x00, 0x00],
        |_| {},
    );
    assert_eq!(r.fp[0], POS_ONE, "FMOVE.S 1.0f → 1.0");
}

#[test]
fn fmove_double_memory_to_fp() {
    // FMOVE.D (A0),FP0 — 0x3FF0000000000000 = 1.0 → 1.0.
    let r = run_mem(
        0xF210,
        mem_ext(5, 0, 0x00),
        0,
        DATA,
        &[0x3F, 0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        |_| {},
    );
    assert_eq!(r.fp[0], POS_ONE, "FMOVE.D 1.0 → 1.0");
}

#[test]
fn fmove_extended_memory_to_fp() {
    // FMOVE.X (A0),FP0 — 96-bit extended 2.0: high 0x4000, pad 0x0000,
    // mantissa 0x8000000000000000.
    let r = run_mem(
        0xF210,
        mem_ext(2, 0, 0x00),
        0,
        DATA,
        &[
            0x40, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ],
        |_| {},
    );
    assert_eq!(r.fp[0], POS_TWO, "FMOVE.X 2.0 → 2.0");
}

#[test]
fn fmove_word_memory_sign_extends() {
    // FMOVE.W (A0),FP0 — 0xFFFF = −1 → −1.0.
    let r = run_mem(0xF210, mem_ext(4, 0, 0x00), 0, DATA, &[0xFF, 0xFF], |_| {});
    assert_eq!(r.fp[0], NEG_ONE, "FMOVE.W −1 → −1.0");
}

#[test]
fn fmove_byte_memory_sign_extends() {
    // FMOVE.B (A0),FP0 — 0x07 = 7 → 7.0.
    let r = run_mem(0xF210, mem_ext(6, 0, 0x00), 0, DATA, &[0x07], |_| {});
    assert_eq!(r.fp[0], int_fx(7), "FMOVE.B 7 → 7.0");
}

#[test]
fn fadd_long_memory_operand() {
    // FADD.L (A0),FP0 with FP0 = 1.0, mem = 2 → 3.0.
    let r = run_mem(
        0xF210,
        mem_ext(0, 0, 0x22),
        0,
        DATA,
        &[0x00, 0x00, 0x00, 0x02],
        |cpu| {
            cpu.regs.fp[0] = POS_ONE;
        },
    );
    assert_eq!(r.fp[0], POS_THREE, "1.0 + (mem) 2 = 3.0");
}

#[test]
fn fmove_single_postincrement_advances_areg() {
    // FMOVE.S (A0)+,FP0 — opcode $F218 (mode 3). A0 advances by 4.
    let r = run_mem(
        0xF218,
        mem_ext(1, 0, 0x00),
        0,
        DATA,
        &[0x3F, 0x80, 0x00, 0x00],
        |_| {},
    );
    assert_eq!(r.fp[0], POS_ONE);
    assert_eq!(r.a[0], DATA + 4, "(A0)+ steps by the 4-byte single size");
}

#[test]
fn fmove_long_predecrement_steps_back() {
    // FMOVE.L -(A0),FP0 — opcode $F220 (mode 4). A0 starts at DATA+4,
    // pre-decrements by 4 to DATA, reads 5.
    let r = run_mem(
        0xF220,
        mem_ext(0, 0, 0x00),
        0,
        DATA,
        &[0x00, 0x00, 0x00, 0x05],
        |cpu| {
            cpu.regs.set_a(0, DATA + 4);
        },
    );
    assert_eq!(r.fp[0], int_fx(5));
    assert_eq!(r.a[0], DATA, "-(A0) steps back by the 4-byte long size");
}

#[test]
fn fmove_extended_postincrement_steps_by_12() {
    // FMOVE.X (A0)+,FP0 — A0 advances by the 12-byte extended size.
    let r = run_mem(
        0xF218,
        mem_ext(2, 0, 0x00),
        0,
        DATA,
        &[
            0x40, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ],
        |_| {},
    );
    assert_eq!(r.fp[0], POS_TWO);
    assert_eq!(r.a[0], DATA + 12, "(A0)+ steps by 12 for extended");
}

/// `n.0` for a small integer, via the exact int→extended path.
fn int_fx(n: i32) -> FpReg {
    use motorola_68k_common::softfloat::int32_to_floatx80;
    int32_to_floatx80(n)
}

/// Run a cpGEN op whose EA needs extension words. `ea_words` are placed
/// after the FP extension word; `setup` seeds registers and writes the
/// operand bytes into memory.
fn run_mem_ea(
    opcode: u16,
    w2: u16,
    ea_words: &[u16],
    setup: impl FnOnce(&mut Cpu68020, &mut Mem),
) -> Out {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();
    mem.write_word(PC, opcode);
    mem.write_word(PC + 2, w2);
    for (i, &w) in ea_words.iter().enumerate() {
        mem.write_word(PC + 4 + (i as u32) * 2, w);
    }

    cpu.set_fpu_present(true);
    cpu.regs.sr |= 0x2000;
    cpu.regs.ssp = 0x0000_8000;
    cpu.regs.set_active_sp(0x0000_8000);
    setup(&mut cpu, &mut mem);
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
                a: cpu.regs.a,
            };
        }
    }
    panic!("FP memory-EA op did not complete");
}

fn write_bytes(mem: &mut Mem, addr: u32, bytes: &[u8]) {
    for (i, &b) in bytes.iter().enumerate() {
        mem.bytes.insert((addr + i as u32) & 0x00FF_FFFF, b);
    }
}

/// Run a cpGEN store (FMOVE FPn → ea) and return the final memory + the
/// register snapshot so the test can read back the written operand.
fn run_store(opcode: u16, w2: u16, seed: impl FnOnce(&mut Cpu68020)) -> (Out, Mem) {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();
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
            let out = Out {
                fp: cpu.regs.fp,
                fpsr: cpu.regs.fpsr,
                a: cpu.regs.a,
            };
            return (out, mem);
        }
    }
    panic!("FP store did not complete");
}

/// FMOVE FPn → ea extension word: sub-op 3 (bits 15-13 = 011), dest
/// format, source Fpn, k-factor (0 for non-packed).
fn store_ext(format: u16, src: u16, kfactor: u16) -> u16 {
    0x6000 | (format << 10) | (src << 7) | kfactor
}

/// Run a store whose destination EA needs extension words (placed after
/// the FP extension word). Returns the final memory + registers.
fn run_store_ea(
    opcode: u16,
    w2: u16,
    ea_words: &[u16],
    seed: impl FnOnce(&mut Cpu68020),
) -> (Out, Mem) {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();
    mem.write_word(PC, opcode);
    mem.write_word(PC + 2, w2);
    for (i, &w) in ea_words.iter().enumerate() {
        mem.write_word(PC + 4 + (i as u32) * 2, w);
    }

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
            let out = Out {
                fp: cpu.regs.fp,
                fpsr: cpu.regs.fpsr,
                a: cpu.regs.a,
            };
            return (out, mem);
        }
    }
    panic!("FP store-EA did not complete");
}

#[test]
fn fmove_store_long_to_d16_an() {
    // FMOVE.L FP0,$0010(A0) — opcode $F228 (mode 5, A0). A0 = 0x1FF0,
    // disp +0x0010 → EA 0x2000. FP0 = 5.
    let (_r, mem) = run_store_ea(0xF228, store_ext(0, 0, 0), &[0x0010], |cpu| {
        cpu.regs.set_a(0, 0x0000_1FF0);
        cpu.regs.fp[0] = int_fx(5);
    });
    assert_eq!(read_bytes(&mem, 0x0000_2000, 4), [0x00, 0x00, 0x00, 0x05]);
}

#[test]
fn fmove_store_double_to_abs_short() {
    // FMOVE.D FP1,($2000).W — opcode $F238 (mode 7, reg 0). FP1 = 2.0.
    let (_r, mem) = run_store_ea(0xF238, store_ext(5, 1, 0), &[0x2000], |cpu| {
        cpu.regs.fp[1] = POS_TWO;
    });
    assert_eq!(
        read_bytes(&mem, 0x0000_2000, 8),
        [0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
    );
}

#[test]
fn fmove_store_extended_to_d16_an() {
    // FMOVE.X FP0,$0000(A1) — a 12-byte store via a displacement EA.
    let (_r, mem) = run_store_ea(0xF229, store_ext(2, 0, 0), &[0x0000], |cpu| {
        cpu.regs.set_a(1, 0x0000_2000);
        cpu.regs.fp[0] = POS_TWO;
    });
    assert_eq!(
        read_bytes(&mem, 0x0000_2000, 12),
        [
            0x40, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
        ]
    );
}

#[test]
fn fmove_store_long_to_memory() {
    // FMOVE.L FP0,(A0) with FP0 = 5.0 → big-endian 5 at (A0). Opcode
    // $F210 (mode 2, A0).
    let (_r, mem) = run_store(0xF210, store_ext(0, 0, 0), |cpu| {
        cpu.regs.set_a(0, DATA);
        cpu.regs.fp[0] = int_fx(5);
    });
    assert_eq!(read_bytes(&mem, DATA, 4), [0x00, 0x00, 0x00, 0x05]);
}

#[test]
fn fmove_store_single_to_memory() {
    // FMOVE.S FP0,(A0) with FP0 = 1.0 → 0x3F800000.
    let (_r, mem) = run_store(0xF210, store_ext(1, 0, 0), |cpu| {
        cpu.regs.set_a(0, DATA);
        cpu.regs.fp[0] = POS_ONE;
    });
    assert_eq!(read_bytes(&mem, DATA, 4), [0x3F, 0x80, 0x00, 0x00]);
}

#[test]
fn fmove_store_double_to_memory() {
    // FMOVE.D FP0,(A0) with FP0 = 2.0 → 0x4000000000000000.
    let (_r, mem) = run_store(0xF210, store_ext(5, 0, 0), |cpu| {
        cpu.regs.set_a(0, DATA);
        cpu.regs.fp[0] = POS_TWO;
    });
    assert_eq!(
        read_bytes(&mem, DATA, 8),
        [0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
    );
}

#[test]
fn fmove_store_extended_to_memory() {
    // FMOVE.X FP0,(A0) with FP0 = 2.0 → 96-bit extended (high 0x4000,
    // pad 0x0000, mantissa 0x8000000000000000).
    let (_r, mem) = run_store(0xF210, store_ext(2, 0, 0), |cpu| {
        cpu.regs.set_a(0, DATA);
        cpu.regs.fp[0] = POS_TWO;
    });
    assert_eq!(
        read_bytes(&mem, DATA, 12),
        [
            0x40, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
        ]
    );
}

#[test]
fn fmove_load_packed_decimal() {
    // FMOVE.P (A0),FP0 — 2.5 as a 96-bit packed-decimal real: significand
    // 2.5 (integer digit 2, first fraction digit 5), exponent 0.
    let bytes = [
        0x00, 0x00, 0x00, 0x02, 0x50, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    let r = run_mem(0xF210, mem_ext(3, 0, 0), 0, DATA, &bytes, |_| {});
    assert_eq!(r.fp[0], FpReg::new(0x4000, 0xA000_0000_0000_0000), "2.5");
}

#[test]
fn fmove_store_packed_decimal_static_k() {
    // FMOVE.P FP0,(A0){#17} — store 2.5 with the maximum 17-digit static
    // k-factor, then decode the written BCD back: it must read as 2.5.
    let two_point_five = FpReg::new(0x4000, 0xA000_0000_0000_0000);
    let (_r, mem) = run_store(0xF210, store_ext(3, 0, 17), |cpu| {
        cpu.regs.set_a(0, DATA);
        cpu.regs.fp[0] = two_point_five;
    });
    let b = read_bytes(&mem, DATA, 12);
    let wrd = [
        u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
        u32::from_be_bytes([b[4], b[5], b[6], b[7]]),
        u32::from_be_bytes([b[8], b[9], b[10], b[11]]),
    ];
    let back = motorola_68k_common::softfloat::pack_decimal_to_floatx80(
        wrd,
        motorola_68k_common::softfloat::RoundingMode::NearestEven,
    );
    assert_eq!(back, two_point_five, "stored BCD decodes back to 2.5");
}

#[test]
fn fmove_store_packed_decimal_dynamic_k() {
    // FMOVE.P FP0,(A0){D1} — dynamic k-factor (format 7, ext bits 6-4 = D1).
    // D1 = 17 → same 17-digit BCD as the static case.
    let two_point_five = FpReg::new(0x4000, 0xA000_0000_0000_0000);
    // store (bits 15-13 = 011), format 7, source FP0 (bits 9-7 = 0), D1 (bits 6-4).
    let w2 = 0x6000 | (7 << 10) | (1 << 4);
    let (_r, mem) = run_store(0xF210, w2, |cpu| {
        cpu.regs.set_a(0, DATA);
        cpu.regs.d[1] = 17;
        cpu.regs.fp[0] = two_point_five;
    });
    let b = read_bytes(&mem, DATA, 12);
    let wrd = [
        u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
        u32::from_be_bytes([b[4], b[5], b[6], b[7]]),
        u32::from_be_bytes([b[8], b[9], b[10], b[11]]),
    ];
    let back = motorola_68k_common::softfloat::pack_decimal_to_floatx80(
        wrd,
        motorola_68k_common::softfloat::RoundingMode::NearestEven,
    );
    assert_eq!(back, two_point_five, "dynamic-k store decodes back to 2.5");
}

#[test]
fn fmove_store_word_and_byte() {
    // FMOVE.W FP0,(A0) with FP0 = −1 → 0xFFFF.
    let (_r, mem) = run_store(0xF210, store_ext(4, 0, 0), |cpu| {
        cpu.regs.set_a(0, DATA);
        cpu.regs.fp[0] = NEG_ONE;
    });
    assert_eq!(read_bytes(&mem, DATA, 2), [0xFF, 0xFF]);

    // FMOVE.B FP0,(A0) with FP0 = 7 → 0x07.
    let (_r, mem) = run_store(0xF210, store_ext(6, 0, 0), |cpu| {
        cpu.regs.set_a(0, DATA);
        cpu.regs.fp[0] = int_fx(7);
    });
    assert_eq!(read_bytes(&mem, DATA, 1), [0x07]);
}

#[test]
fn fmove_store_postincrement_steps_areg() {
    // FMOVE.L FP0,(A0)+ — opcode $F218 (mode 3). A0 advances by 4.
    let (r, mem) = run_store(0xF218, store_ext(0, 0, 0), |cpu| {
        cpu.regs.set_a(0, DATA);
        cpu.regs.fp[0] = int_fx(5);
    });
    assert_eq!(read_bytes(&mem, DATA, 4), [0x00, 0x00, 0x00, 0x05]);
    assert_eq!(r.a[0], DATA + 4, "(A0)+ steps by 4 for a long store");
}

#[test]
fn fmove_store_predecrement_extended_steps_back() {
    // FMOVE.X FP0,-(A0) — opcode $F220 (mode 4). A0 starts at DATA+12,
    // pre-decrements by 12 to DATA.
    let (r, mem) = run_store(0xF220, store_ext(2, 0, 0), |cpu| {
        cpu.regs.set_a(0, DATA + 12);
        cpu.regs.fp[0] = POS_TWO;
    });
    assert_eq!(r.a[0], DATA, "-(A0) steps back by 12 for an extended store");
    assert_eq!(
        read_bytes(&mem, DATA, 2),
        [0x40, 0x00],
        "high word at the new base"
    );
}

#[test]
fn fmove_store_round_trips_through_load() {
    // Store π as a double, load it back — exercises the narrowing/widening
    // pair end to end.
    let pi = FpReg::new(0x4000, 0xC90F_DAA2_2168_C235);
    let (_r, mem) = run_store(0xF210, store_ext(5, 0, 0), |cpu| {
        cpu.regs.set_a(0, DATA);
        cpu.regs.fp[0] = pi;
    });
    let bytes = read_bytes(&mem, DATA, 8);
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&bytes);
    let r = run_mem(0xF210, mem_ext(5, 1, 0x00), 0, DATA, &arr, |_| {});
    // π narrowed to double then widened back is π rounded to 53-bit
    // mantissa — the canonical double value of π in extended form.
    assert_eq!(r.fp[1].high, 0x4000, "exponent preserved");
}

#[test]
fn fmove_d16_an_memory() {
    // FMOVE.S $0100(A0),FP0 — opcode $F228 (mode 5, A0). A0 = 0x1F00,
    // disp +0x0100 → EA 0x2000. Operand 1.0f.
    let r = run_mem_ea(0xF228, mem_ext(1, 0, 0x00), &[0x0100], |cpu, mem| {
        cpu.regs.set_a(0, 0x0000_1F00);
        write_bytes(mem, 0x0000_2000, &[0x3F, 0x80, 0x00, 0x00]);
    });
    assert_eq!(r.fp[0], POS_ONE, "$0100(A0) single 1.0f → 1.0");
}

#[test]
fn fmove_d16_an_negative_disp() {
    // FMOVE.L $FFF0(A0),FP0 — disp −16. A0 = 0x2010 → EA 0x2000. Operand 5.
    let r = run_mem_ea(0xF228, mem_ext(0, 0, 0x00), &[0xFFF0], |cpu, mem| {
        cpu.regs.set_a(0, 0x0000_2010);
        write_bytes(mem, 0x0000_2000, &[0x00, 0x00, 0x00, 0x05]);
    });
    assert_eq!(r.fp[0], int_fx(5), "−16(A0) long 5 → 5.0");
}

#[test]
fn fmove_abs_short_memory() {
    // FMOVE.L ($2000).W,FP0 — opcode $F238 (mode 7, reg 0).
    let r = run_mem_ea(0xF238, mem_ext(0, 0, 0x00), &[0x2000], |_cpu, mem| {
        write_bytes(mem, 0x0000_2000, &[0x00, 0x00, 0x00, 0x05]);
    });
    assert_eq!(r.fp[0], int_fx(5), "(0x2000).W long 5 → 5.0");
}

#[test]
fn fadd_d16_an_double_memory() {
    // FADD.D $0000(A0),FP0 with FP0 = 1.0, mem = 2.0 → 3.0. Exercises an
    // 8-byte operand at a displacement EA.
    let r = run_mem_ea(0xF228, mem_ext(5, 0, 0x22), &[0x0000], |cpu, mem| {
        cpu.regs.set_a(0, 0x0000_2000);
        cpu.regs.fp[0] = POS_ONE;
        write_bytes(
            mem,
            0x0000_2000,
            &[0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        );
    });
    assert_eq!(r.fp[0], POS_THREE, "1.0 + (mem.D) 2.0 = 3.0");
}

#[test]
fn fmove_extended_abs_short_memory() {
    // FMOVE.X ($2000).W,FP0 — a 12-byte operand via a static EA.
    let r = run_mem_ea(0xF238, mem_ext(2, 0, 0x00), &[0x2000], |_cpu, mem| {
        write_bytes(
            mem,
            0x0000_2000,
            &[
                0x40, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ],
        );
    });
    assert_eq!(r.fp[0], POS_TWO, "(0x2000).W extended 2.0 → 2.0");
}

// --- Immediate (#data) source operands: opcode $F23C (mode 7, reg 4).
// The operand words follow the FP extension word inline. ---

#[test]
fn fmove_immediate_single() {
    // FMOVE.S #1.0,FP0 — 0x3F800000.
    let r = run_mem_ea(0xF23C, mem_ext(1, 0, 0x00), &[0x3F80, 0x0000], |_, _| {});
    assert_eq!(r.fp[0], POS_ONE, "FMOVE.S #1.0f → 1.0");
}

#[test]
fn fmove_immediate_long() {
    // FMOVE.L #5,FP0.
    let r = run_mem_ea(0xF23C, mem_ext(0, 0, 0x00), &[0x0000, 0x0005], |_, _| {});
    assert_eq!(r.fp[0], int_fx(5), "FMOVE.L #5 → 5.0");
}

#[test]
fn fmove_immediate_double() {
    // FMOVE.D #2.0,FP0 — 0x4000000000000000 (4 words).
    let r = run_mem_ea(
        0xF23C,
        mem_ext(5, 0, 0x00),
        &[0x4000, 0x0000, 0x0000, 0x0000],
        |_, _| {},
    );
    assert_eq!(r.fp[0], POS_TWO, "FMOVE.D #2.0 → 2.0");
}

#[test]
fn fmove_immediate_extended() {
    // FMOVE.X #2.0,FP0 — 6 words: high 0x4000, pad 0x0000, mantissa
    // 0x8000000000000000.
    let r = run_mem_ea(
        0xF23C,
        mem_ext(2, 0, 0x00),
        &[0x4000, 0x0000, 0x8000, 0x0000, 0x0000, 0x0000],
        |_, _| {},
    );
    assert_eq!(r.fp[0], POS_TWO, "FMOVE.X #2.0 → 2.0");
}

#[test]
fn fmove_immediate_word_and_byte() {
    // FMOVE.W #-1,FP0 (one word, 0xFFFF) → −1.0.
    let r = run_mem_ea(0xF23C, mem_ext(4, 0, 0x00), &[0xFFFF], |_, _| {});
    assert_eq!(r.fp[0], NEG_ONE, "FMOVE.W #-1 → −1.0");

    // FMOVE.B #7,FP0 (one word, low byte 0x07) → 7.0.
    let r = run_mem_ea(0xF23C, mem_ext(6, 0, 0x00), &[0x0007], |_, _| {});
    assert_eq!(r.fp[0], int_fx(7), "FMOVE.B #7 → 7.0");
}

#[test]
fn fadd_immediate_long_operand() {
    // FADD.L #2,FP0 with FP0 = 1.0 → 3.0.
    let r = run_mem_ea(0xF23C, mem_ext(0, 0, 0x22), &[0x0000, 0x0002], |cpu, _| {
        cpu.regs.fp[0] = POS_ONE;
    });
    assert_eq!(r.fp[0], POS_THREE, "1.0 + #2 = 3.0");
}

#[test]
fn fmove_pc_relative_memory() {
    // FMOVE.L (d16,PC),FP0 — opcode $F23A (mode 7, reg 2). The
    // displacement is relative to the extension-word address (PC+4 =
    // 0x1004); d16 = 0x2000 − 0x1004 = 0x0FFC targets the constant at
    // 0x2000. This is how compilers emit FP literals.
    let r = run_mem_ea(0xF23A, mem_ext(0, 0, 0x00), &[0x0FFC], |_cpu, mem| {
        write_bytes(mem, 0x0000_2000, &[0x00, 0x00, 0x00, 0x05]);
    });
    assert_eq!(r.fp[0], int_fx(5), "(d16,PC) long 5 → 5.0");
}

// --- FMOVEM register list <-> memory (cpGEN sub-op 6/7) ---

#[test]
fn fmovem_store_predecrement() {
    // FMOVEM FP0/FP1,-(A0) — reglist $03 (predec: bit i -> FPi). A0 =
    // 0x2018; FP0 lands at A0-12 (0x200C), FP1 at A0-24 (0x2000); A0 -= 24.
    let (r, mem) = run_store(0xF220, 0xE003, |cpu| {
        cpu.regs.set_a(0, 0x0000_2018);
        cpu.regs.fp[0] = POS_ONE;
        cpu.regs.fp[1] = POS_TWO;
    });
    assert_eq!(
        read_bytes(&mem, 0x0000_200C, 12),
        [0x3F, 0xFF, 0, 0, 0x80, 0, 0, 0, 0, 0, 0, 0],
        "FP0 (1.0) at A0-12"
    );
    assert_eq!(
        read_bytes(&mem, 0x0000_2000, 12),
        [0x40, 0x00, 0, 0, 0x80, 0, 0, 0, 0, 0, 0, 0],
        "FP1 (2.0) at A0-24"
    );
    assert_eq!(r.a[0], 0x0000_2000, "A0 -= 24");
}

#[test]
fn fmovem_load_postincrement() {
    // FMOVEM (A0)+,<FP6,FP7> — reglist $03 (postinc: bit i -> FP[7-i], so
    // bit 0 -> FP7, bit 1 -> FP6). FP7 reads from A0, FP6 from A0+12.
    let mut bytes = vec![0x3F, 0xFF, 0, 0, 0x80, 0, 0, 0, 0, 0, 0, 0]; // 1.0 -> FP7
    bytes.extend_from_slice(&[0x40, 0x00, 0, 0, 0x80, 0, 0, 0, 0, 0, 0, 0]); // 2.0 -> FP6
    let r = run_mem(0xF218, 0xD003, 0, 0x0000_2000, &bytes, |_| {});
    assert_eq!(r.fp[7], POS_ONE, "bit 0 -> FP7");
    assert_eq!(r.fp[6], POS_TWO, "bit 1 -> FP6");
    assert_eq!(r.a[0], 0x0000_2000 + 24, "A0 += 24");
}

#[test]
fn fmovem_round_trip_all_registers() {
    // Save FP0..FP7 with predecrement, then restore with postincrement,
    // and confirm every register and the pointer round-trip exactly.
    let vals: [FpReg; 8] = [
        int_fx(1),
        int_fx(2),
        int_fx(3),
        int_fx(4),
        int_fx(5),
        int_fx(6),
        int_fx(7),
        int_fx(8),
    ];
    // FMOVEM FP0-FP7,-(A0): reglist $FF, A0 = 0x2060 (= 0x2000 + 96).
    let (r1, mem) = run_store(0xF220, 0xE0FF, |cpu| {
        cpu.regs.set_a(0, 0x0000_2060);
        for (i, v) in vals.iter().enumerate() {
            cpu.regs.fp[i] = *v;
        }
    });
    assert_eq!(r1.a[0], 0x0000_2000, "A0 -= 96 after storing 8 registers");

    // Read the 96 stored bytes and restore: FMOVEM (A0)+,FP0-FP7 ($FF).
    let stored = read_bytes(&mem, 0x0000_2000, 96);
    let r2 = run_mem(0xF218, 0xD0FF, 0, 0x0000_2000, &stored, |_| {});
    assert_eq!(r2.a[0], 0x0000_2060, "A0 += 96 after loading 8 registers");
    for (i, v) in vals.iter().enumerate() {
        assert_eq!(r2.fp[i], *v, "FP{i} round-trips");
    }
}

#[test]
fn fmovem_empty_list_is_noop() {
    // FMOVEM with an empty register list transfers nothing and leaves the
    // address register unchanged.
    let (r, _mem) = run_store(0xF220, 0xE000, |cpu| cpu.regs.set_a(0, 0x0000_2000));
    assert_eq!(r.a[0], 0x0000_2000, "empty list → A0 unchanged");
}

// --- FPSR exception bytes (#112, step 5c) ------------------------------
//
// The arithmetic raises IEEE exceptions through the SoftFloat port (its flag
// computation is validated bit-for-bit against softfloat.c by the common
// crate's C-diff). These check the flag → FPSR EXC/AEXC mapping + the core
// wiring end-to-end, which the Musashi corpus cannot (Musashi never sets the
// FPSR exception bytes). EXC byte = (fpsr >> 8) & 0xFF, AEXC = fpsr & 0xFF.

fn exc(fpsr: u32) -> u8 {
    ((fpsr >> 8) & 0xFF) as u8
}
fn aexc(fpsr: u32) -> u8 {
    (fpsr & 0xFF) as u8
}

const ONE: FpReg = FpReg::new(0x3FFF, 0x8000_0000_0000_0000);
const HUGE: FpReg = FpReg::new(0x7FFE, 0xFFFF_FFFF_FFFF_FFFF);

#[test]
fn exact_add_sets_no_exceptions() {
    let r = run(0x22, 0, 1, |cpu| {
        cpu.regs.fp[0] = ONE;
        cpu.regs.fp[1] = ONE;
    });
    assert_eq!(exc(r.fpsr), 0, "1+1 raises no exceptions");
    assert_eq!(aexc(r.fpsr), 0);
}

#[test]
fn overflow_sets_ovfl() {
    // HUGE + HUGE overflows. The 68881/2 raises OVFL unconditionally but
    // INEX2 only when significand bits are discarded; 2×HUGE is exact, so
    // EXC = OVFL(0x10) alone. AEXC.INEX still accrues from EXC.OVFL, so
    // AEXC = OVFL(0x40) | INEX(0x08).
    let r = run(0x22, 0, 1, |cpu| {
        cpu.regs.fp[0] = HUGE;
        cpu.regs.fp[1] = HUGE;
    });
    assert_eq!(exc(r.fpsr), 0x10, "EXC = OVFL");
    assert_eq!(aexc(r.fpsr), 0x48, "AEXC = OVFL | INEX");
}

#[test]
fn divide_by_zero_sets_dz() {
    // dst / src = 1 / 0 (FDIV computes dst op source).
    let r = run(0x20, 0, 1, |cpu| {
        cpu.regs.fp[0] = FpReg::new(0, 0); // source = 0
        cpu.regs.fp[1] = ONE; // dst = 1
    });
    assert_eq!(exc(r.fpsr), 0x04, "EXC = DZ");
    assert_eq!(aexc(r.fpsr), 0x10, "AEXC = DZ");
}

#[test]
fn sqrt_of_negative_sets_operr_not_snan() {
    // sqrt(-1): an operational invalid → OPERR(0x20), not SNAN.
    let r = run(0x04, 0, 1, |cpu| {
        cpu.regs.fp[0] = FpReg::new(0xBFFF, 0x8000_0000_0000_0000); // -1.0
    });
    assert_eq!(exc(r.fpsr), 0x20, "EXC = OPERR");
    assert_eq!(aexc(r.fpsr), 0x80, "AEXC = IOP");
}

#[test]
fn signaling_nan_operand_sets_snan_not_operr() {
    // A signalling-NaN input → SNAN(0x40), distinct from OPERR.
    let snan = FpReg::new(0x7FFF, 0x8000_0000_0000_0001); // integer bit set, quiet bit clear
    let r = run(0x22, 0, 1, |cpu| {
        cpu.regs.fp[0] = snan;
        cpu.regs.fp[1] = ONE;
    });
    assert_eq!(exc(r.fpsr), 0x40, "EXC = SNAN");
    assert_eq!(aexc(r.fpsr), 0x80, "AEXC = IOP");
}

// --- FSGLMUL / FSGLDIV (#486, Phase 1) ---------------------------------
//
// Single-precision multiply/divide via the dedicated floatx80_sglmul/sgldiv
// (FSGLMUL also truncates its operands to single precision first). The result
// is rounded to single precision (kept in extended format), so the low 40
// mantissa bits are clear. Bit-exact vs WinUAE's SOFTFLOAT_68K (C-diff,
// validation/run_fpsp.sh ops 11/12).

#[test]
fn fsglmul_rounds_result_to_single_precision() {
    use motorola_68k_common::softfloat::{RoundingMode::NearestEven, floatx80_sglmul};
    let pi = FpReg::new(0x4000, 0xC90F_DAA2_2168_C235);
    let r = run(0x27, 0, 1, |cpu| {
        cpu.regs.fp[0] = pi;
        cpu.regs.fp[1] = ONE;
    });
    assert_eq!(
        r.fp[1].low & 0x0000_00FF_FFFF_FFFF,
        0,
        "FSGLMUL result is representable in single precision"
    );
    // dst (1.0) FSGLMUL src (pi).
    assert_eq!(r.fp[1], floatx80_sglmul(NearestEven, ONE, pi));
}

#[test]
fn fsgldiv_rounds_result_to_single_precision() {
    use motorola_68k_common::softfloat::{RoundingMode::NearestEven, floatx80_sgldiv};
    let three = FpReg::new(0x4000, 0xC000_0000_0000_0000); // 3.0
    let r = run(0x24, 0, 1, |cpu| {
        cpu.regs.fp[0] = three;
        cpu.regs.fp[1] = ONE;
    });
    assert_eq!(
        r.fp[1].low & 0x0000_00FF_FFFF_FFFF,
        0,
        "FSGLDIV result is representable in single precision"
    );
    // dst (1.0) FSGLDIV src (3.0).
    assert_eq!(r.fp[1], floatx80_sgldiv(NearestEven, ONE, three));
}

// --- FMOD / FREM (#486, Phase 1) — value + FPSR quotient byte --------------

#[test]
fn fmod_truncates_quotient_and_sets_quotient_byte() {
    // 5 FMOD 3: trunc(5/3) = 1, 5 − 1·3 = 2; quotient byte = 1 (sign 0).
    let five = FpReg::new(0x4001, 0xA000_0000_0000_0000);
    let three = FpReg::new(0x4000, 0xC000_0000_0000_0000);
    let two = FpReg::new(0x4000, 0x8000_0000_0000_0000);
    let r = run(0x21, 0, 1, |cpu| {
        cpu.regs.fp[0] = three; // source
        cpu.regs.fp[1] = five; // dst, reduced in place
    });
    assert_eq!(r.fp[1], two, "5 FMOD 3 = 2");
    assert_eq!((r.fpsr >> 16) & 0xFF, 1, "FPSR quotient byte = 1");
}

#[test]
fn frem_rounds_quotient_to_nearest_and_sets_quotient_byte() {
    // 5 FREM 3: round(5/3) = 2, 5 − 2·3 = −1; quotient byte = 2 (sign 0).
    let five = FpReg::new(0x4001, 0xA000_0000_0000_0000);
    let three = FpReg::new(0x4000, 0xC000_0000_0000_0000);
    let neg_one = FpReg::new(0xBFFF, 0x8000_0000_0000_0000);
    let r = run(0x25, 0, 1, |cpu| {
        cpu.regs.fp[0] = three;
        cpu.regs.fp[1] = five;
    });
    assert_eq!(r.fp[1], neg_one, "5 FREM 3 = -1");
    assert_eq!((r.fpsr >> 16) & 0xFF, 2, "FPSR quotient byte = 2");
}

// --- FPCR rounding precision + FSxxx/FDxxx prefix (#487) -------------------

#[test]
fn fadd_honours_fpcr_single_precision() {
    use motorola_68k_common::softfloat::{RoundingMode::NearestEven, floatx80_add};
    // FPCR rounding precision = single (bits 7-6 = 01). The sum rounds to
    // single precision — its low 40 mantissa bits are clear.
    let a = FpReg::new(0x4000, 0xC90F_DAA2_2168_C235);
    let r = run(0x22, 0, 1, |cpu| {
        cpu.regs.fpcr = 0x40; // single
        cpu.regs.fp[0] = a;
        cpu.regs.fp[1] = ONE;
    });
    assert_eq!(r.fp[1].low & 0x0000_00FF_FFFF_FFFF, 0, "single-rounded");
    assert_eq!(r.fp[1], floatx80_add(32, NearestEven, ONE, a));
}

#[test]
fn fmove_honours_fpcr_double_precision() {
    use motorola_68k_common::softfloat::{RoundingMode::NearestEven, floatx80_move};
    // FPCR rounding precision = double (bits 7-6 = 10): FMOVE rounds the
    // source to double precision (low 11 mantissa bits clear).
    let a = FpReg::new(0x4000, 0xC90F_DAA2_2168_C235);
    let r = run(0x00, 0, 1, |cpu| {
        cpu.regs.fpcr = 0x80; // double
        cpu.regs.fp[0] = a;
    });
    assert_eq!(r.fp[1].low & 0x0000_0000_0000_07FF, 0, "double-rounded");
    assert_eq!(r.fp[1], floatx80_move(64, NearestEven, a));
}

#[test]
fn fsadd_prefix_forces_single_precision() {
    // FSADD (opmode 0x22 | 0x44 = 0x66) forces single rounding even with the
    // FPCR precision left at extended.
    let a = FpReg::new(0x4000, 0xC90F_DAA2_2168_C235);
    let r = run(0x66, 0, 1, |cpu| {
        cpu.regs.fp[0] = a;
        cpu.regs.fp[1] = ONE;
    });
    assert_eq!(
        r.fp[1].low & 0x0000_00FF_FFFF_FFFF,
        0,
        "single-rounded via the FSxxx prefix"
    );
}

// ─── FPSP exponential transcendentals (#492) ──────────────────────────────
//
// The result values are validated bit-exact against WinUAE in
// `validation/run_fpsp.sh`; these check the 020 cpGEN dispatch routes each
// opmode to the right `softfloat_fpsp` function (and a known anchor value).

#[test]
fn fetox_of_zero_is_one() {
    use motorola_68k_common::softfloat::RoundingMode::NearestEven;
    use motorola_68k_common::softfloat_fpsp::floatx80_etox;
    // FETOX (opmode 0x10): e^0 = 1.0.
    let r = run(0x10, 0, 1, |cpu| cpu.regs.fp[0] = FpReg::new(0, 0));
    assert_eq!(r.fp[1], ONE, "e^0 = 1.0");
    // And FETOX(1.0) matches the backend exactly.
    let r = run(0x10, 0, 1, |cpu| cpu.regs.fp[0] = ONE);
    assert_eq!(
        r.fp[1],
        floatx80_etox(80, NearestEven, ONE),
        "FETOX → floatx80_etox"
    );
}

#[test]
fn fetoxm1_dispatches() {
    use motorola_68k_common::softfloat::RoundingMode::NearestEven;
    use motorola_68k_common::softfloat_fpsp::floatx80_etoxm1;
    // FETOXM1 (opmode 0x08): e^0 - 1 = 0.
    let r = run(0x08, 0, 1, |cpu| cpu.regs.fp[0] = FpReg::new(0, 0));
    assert_eq!(r.fp[1], FpReg::new(0, 0), "e^0 - 1 = 0");
    let r = run(0x08, 0, 1, |cpu| cpu.regs.fp[0] = ONE);
    assert_eq!(r.fp[1], floatx80_etoxm1(80, NearestEven, ONE));
}

#[test]
fn ftwotox_of_known_values() {
    use motorola_68k_common::softfloat::RoundingMode::NearestEven;
    use motorola_68k_common::softfloat_fpsp::floatx80_twotox;
    // FTWOTOX (opmode 0x11): 2^3 = 8.0.
    let three = FpReg::new(0x4000, 0xC000_0000_0000_0000);
    let r = run(0x11, 0, 1, |cpu| cpu.regs.fp[0] = three);
    assert_eq!(
        r.fp[1],
        FpReg::new(0x4002, 0x8000_0000_0000_0000),
        "2^3 = 8.0"
    );
    let r = run(0x11, 0, 1, |cpu| cpu.regs.fp[0] = three);
    assert_eq!(r.fp[1], floatx80_twotox(80, NearestEven, three));
}

#[test]
fn ftentox_of_known_values() {
    use motorola_68k_common::softfloat::RoundingMode::NearestEven;
    use motorola_68k_common::softfloat_fpsp::floatx80_tentox;
    // FTENTOX (opmode 0x12): 10^2 = 100.0.
    let two = FpReg::new(0x4000, 0x8000_0000_0000_0000);
    let r = run(0x12, 0, 1, |cpu| cpu.regs.fp[0] = two);
    assert_eq!(
        r.fp[1],
        FpReg::new(0x4005, 0xC800_0000_0000_0000),
        "10^2 = 100.0"
    );
    let r = run(0x12, 0, 1, |cpu| cpu.regs.fp[0] = two);
    assert_eq!(r.fp[1], floatx80_tentox(80, NearestEven, two));
}

// ─── FPSP logarithms (#492) ───────────────────────────────────────────────

#[test]
fn flogn_of_one_is_zero() {
    use motorola_68k_common::softfloat::RoundingMode::NearestEven;
    use motorola_68k_common::softfloat_fpsp::floatx80_logn;
    // FLOGN (opmode 0x14): ln(1) = 0.
    let r = run(0x14, 0, 1, |cpu| cpu.regs.fp[0] = ONE);
    assert_eq!(r.fp[1], FpReg::new(0, 0), "ln(1) = 0");
    let e = FpReg::new(0x4000, 0xADF8_5458_A2BB_4A9A); // ~2.71828
    let r = run(0x14, 0, 1, |cpu| cpu.regs.fp[0] = e);
    assert_eq!(
        r.fp[1],
        floatx80_logn(80, NearestEven, e),
        "FLOGN → floatx80_logn"
    );
}

#[test]
fn flognp1_of_zero_is_zero() {
    use motorola_68k_common::softfloat_fpsp::floatx80_lognp1;
    let _ = floatx80_lognp1; // referenced for the doc link
    // FLOGNP1 (opmode 0x06): ln(1+0) = 0.
    let r = run(0x06, 0, 1, |cpu| cpu.regs.fp[0] = FpReg::new(0, 0));
    assert_eq!(r.fp[1], FpReg::new(0, 0), "ln(1+0) = 0");
}

#[test]
fn flog10_of_hundred_is_two() {
    // FLOG10 (opmode 0x15): log10(100) = 2.0.
    let hundred = FpReg::new(0x4005, 0xC800_0000_0000_0000);
    let r = run(0x15, 0, 1, |cpu| cpu.regs.fp[0] = hundred);
    assert_eq!(
        r.fp[1],
        FpReg::new(0x4000, 0x8000_0000_0000_0000),
        "log10(100) = 2.0"
    );
}

#[test]
fn flog2_of_eight_is_three() {
    // FLOG2 (opmode 0x16): log2(8) = 3.0 (the exact 2^k path).
    let eight = FpReg::new(0x4002, 0x8000_0000_0000_0000);
    let r = run(0x16, 0, 1, |cpu| cpu.regs.fp[0] = eight);
    assert_eq!(
        r.fp[1],
        FpReg::new(0x4000, 0xC000_0000_0000_0000),
        "log2(8) = 3.0"
    );
}

// ─── FPSP trigonometric (#492) ────────────────────────────────────────────

#[test]
fn fsin_fcos_ftan_of_zero() {
    use motorola_68k_common::softfloat::RoundingMode::NearestEven;
    use motorola_68k_common::softfloat_fpsp::{floatx80_cos, floatx80_sin, floatx80_tan};
    // FSIN (0x0E): sin(0) = 0; FCOS (0x1D): cos(0) = 1; FTAN (0x0F): tan(0) = 0.
    let r = run(0x0E, 0, 1, |cpu| cpu.regs.fp[0] = FpReg::new(0, 0));
    assert_eq!(r.fp[1], FpReg::new(0, 0), "sin(0) = 0");
    let r = run(0x1D, 0, 1, |cpu| cpu.regs.fp[0] = FpReg::new(0, 0));
    assert_eq!(r.fp[1], ONE, "cos(0) = 1");
    let r = run(0x0F, 0, 1, |cpu| cpu.regs.fp[0] = FpReg::new(0, 0));
    assert_eq!(r.fp[1], FpReg::new(0, 0), "tan(0) = 0");
    // Dispatch matches the backend on a non-trivial value.
    let half = FpReg::new(0x3FFE, 0x8000_0000_0000_0000); // 0.5
    let r = run(0x0E, 0, 1, |cpu| cpu.regs.fp[0] = half);
    assert_eq!(r.fp[1], floatx80_sin(80, NearestEven, half));
    let r = run(0x1D, 0, 1, |cpu| cpu.regs.fp[0] = half);
    assert_eq!(r.fp[1], floatx80_cos(80, NearestEven, half));
    let r = run(0x0F, 0, 1, |cpu| cpu.regs.fp[0] = half);
    assert_eq!(r.fp[1], floatx80_tan(80, NearestEven, half));
}

#[test]
fn fsincos_writes_both_registers() {
    use motorola_68k_common::softfloat::RoundingMode::NearestEven;
    use motorola_68k_common::softfloat_fpsp::floatx80_sincos;
    // FSINCOS (opmode 0110ccc): sine → FP1 (dst), cosine → FP2 (c). Use a
    // raw extension word: R/M=0, src=0, dst=1, opmode = 0x32 (cos reg 2).
    let half = FpReg::new(0x3FFE, 0x8000_0000_0000_0000); // 0.5
    let r = run(0x32, 0, 1, |cpu| cpu.regs.fp[0] = half);
    let (s, c) = floatx80_sincos(80, NearestEven, half);
    assert_eq!(r.fp[1], s, "sine → dst FP1");
    assert_eq!(r.fp[2], c, "cosine → FP2");
}

#[test]
fn fsincos_same_register_keeps_sine() {
    use motorola_68k_common::softfloat::RoundingMode::NearestEven;
    use motorola_68k_common::softfloat_fpsp::floatx80_sincos;
    // When FPc == FPs (both reg 1: dst=1, opmode 0x31), the sine result wins.
    let half = FpReg::new(0x3FFE, 0x8000_0000_0000_0000);
    let r = run(0x31, 0, 1, |cpu| cpu.regs.fp[0] = half);
    let (s, _c) = floatx80_sincos(80, NearestEven, half);
    assert_eq!(r.fp[1], s, "FPc == FPs → sine kept");
}
