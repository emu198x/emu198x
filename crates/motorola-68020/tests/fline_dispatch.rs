//! F-line ($Fxxx) dispatch + FBcc.W / FNOP — 68020 (#112, steps 1-2).
//!
//! Two behaviours are pinned here:
//!
//! 1. **No FPU (default):** every F-line opcode takes the vector-11
//!    F-line emulator trap — the 68000/68010 and the 68EC020 (A1200/
//!    CD32, no coprocessor) behaviour. Protects the route's fallback.
//! 2. **FPU present:** cpID-1 op-class-2 (cpBcc.W) executes — FBcc.W
//!    branches on the FPSR condition; FNOP (FBF.W, never) falls through.
//!    Target = instr_start + 2 + disp, matching Musashi's `fbcc16`.

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use motorola_68000::bus::BusStatus;
use motorola_68000::cpu::State;
use motorola_68020::Cpu68020;

const PC: u32 = 0x0000_1000;
const HANDLER: u32 = 0x0000_3000;
const INITIAL_SSP: u32 = 0x0000_8000;

struct Mem {
    bytes: HashMap<u32, u8>,
}

impl Mem {
    fn new() -> Self {
        Self {
            bytes: HashMap::new(),
        }
    }
    // Unprogrammed memory reads as NOP ($4E71) so fall-through and branch
    // targets land on a valid instruction.
    fn read_word(&self, a: u32) -> u16 {
        let a = a & 0x00FF_FFFF;
        match (self.bytes.get(&a), self.bytes.get(&(a.wrapping_add(1)))) {
            (None, None) => 0x4E71,
            (hi, lo) => (u16::from(*hi.unwrap_or(&0)) << 8) | u16::from(*lo.unwrap_or(&0)),
        }
    }
    fn read_byte(&self, a: u32) -> u8 {
        (self.read_word(a) >> 8) as u8
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
                    mem.bytes.insert(*addr & 0x00FF_FFFF, v as u8);
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
    next_instr_pc: u32,
    vectored: bool,
}

/// Run one F-line opcode (+ extension words). `fpu` enables the FPU; with
/// it off, the vector-11 fallback is exercised. `fpsr` seeds the FP
/// condition codes for FBcc tests.
fn run(opcode: u16, ext_words: &[u16], fpu: bool, fpsr: u32) -> Out {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();

    // Vector 11 (F-line, offset 11*4 = $2C) → handler.
    mem.write_long(11 * 4, HANDLER);
    mem.write_word(HANDLER, 0x4E71); // NOP

    mem.write_word(PC, opcode);
    for (i, w) in ext_words.iter().enumerate() {
        mem.write_word(PC + 2 + (i as u32) * 2, *w);
    }

    cpu.set_fpu_present(fpu);
    cpu.regs.fpsr = fpsr;
    cpu.regs.sr |= 0x2000; // supervisor
    cpu.regs.ssp = INITIAL_SSP;
    cpu.regs.set_active_sp(INITIAL_SSP);
    cpu.regs.pc = PC + 4;
    cpu.setup_prefetch(opcode, ext_words.first().copied().unwrap_or(0x4E71));

    let start = cpu.instruction_starts;
    for _ in 0..400 {
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        if cpu.instruction_starts > start {
            return Out {
                next_instr_pc: cpu.instr_start_pc,
                vectored: cpu.instr_start_pc == HANDLER,
            };
        }
    }
    panic!("instruction did not complete");
}

// --- No FPU: F-line traps vector 11 ---

#[test]
fn fpu_general_opcode_traps_vector_11_without_fpu() {
    // $F200 = cpID 1, cpGEN. No FPU → vector 11.
    assert!(run(0xF200, &[0x0000], false, 0).vectored);
}

#[test]
fn low_cpid_traps_vector_11_even_with_fpu() {
    // $F080 = cpID 0 — never an FPU op → vector 11 regardless of FPU.
    assert!(run(0xF080, &[0x0000], true, 0).vectored);
}

#[test]
fn fnop_traps_vector_11_without_fpu() {
    // FNOP ($F280 $0000) with no FPU attached → vector 11.
    assert!(run(0xF280, &[0x0000], false, 0).vectored);
}

#[test]
fn unimplemented_fpu_arithmetic_traps_vector_11() {
    // $F200 = cpID 1, cpGEN; extension word $000E = FSIN (reg-to-reg,
    // opmode 0x0E). The non-transcendental ops (arithmetic + FMOD/FREM/
    // FSCALE/FGETEXP/FGETMAN/FSGLMUL/FSGLDIV) now execute via the SoftFloat
    // port, but the transcendentals are not wired yet, so they still
    // decline → vector 11. (The wired ops DO execute now — see
    // tests/fpu_fpgen.rs.)
    assert!(run(0xF200, &[0x000E], true, 0).vectored);
}

#[test]
fn fpu_memory_operand_invalid_mode_traps_vector_11() {
    // cpGEN R/M = 1 (ext bit 14) with EA = Dn (mode 0) is not a valid
    // memory operand, so it declines → vector 11. The memory modes
    // ((An)/(An)+/-(An), d16(An), indexed, abs, PC-relative) DO execute
    // now — see tests/fpu_fpgen.rs.
    assert!(run(0xF200, &[0x4000], true, 0).vectored);
}

#[test]
fn fpu_packed_decimal_format_executes() {
    // cpGEN R/M = 1 with the packed-decimal format (3, ext bits 12-10 =
    // 011 → $4C00) now executes (FMOVE.P #imm,FP0 reads the 12-byte BCD
    // operand). Value-checked load/store coverage is in tests/fpu_fpgen.rs.
    assert!(!run(0xF23C, &[0x4C00], true, 0).vectored);
}

// --- FPU present: FBcc.W / FNOP execute ---

#[test]
fn fnop_executes_with_fpu() {
    // FNOP = FBF.W (condition F, never) with zero displacement. Falls
    // through to the next instruction (past the 4-byte FNOP) — no trap.
    let r = run(0xF280, &[0x0000], true, 0);
    assert!(!r.vectored, "FNOP must not trap when an FPU is present");
    assert_eq!(r.next_instr_pc, PC + 4, "FNOP falls through to instr+4");
}

#[test]
fn fbf_w_with_displacement_falls_through() {
    // FBF.W (never taken) with a non-zero displacement still falls
    // through — the displacement is skipped, not branched to.
    let r = run(0xF280, &[0x0040], true, 0);
    assert!(!r.vectored);
    assert_eq!(r.next_instr_pc, PC + 4, "not taken → past the disp word");
}

#[test]
fn fbt_w_always_branches() {
    // FBT.W ($F28F, condition T) → always taken. Target = instr+2+disp.
    let r = run(0xF28F, &[0x0040], true, 0);
    assert_eq!(
        r.next_instr_pc,
        PC + 2 + 0x40,
        "FBT.W branches to instr+2+disp"
    );
}

#[test]
fn fbt_w_negative_displacement() {
    let r = run(0xF28F, &[0xFFF0], true, 0); // disp = -16
    assert_eq!(r.next_instr_pc, (PC + 2).wrapping_sub(16));
}

#[test]
fn fbeq_w_taken_when_fpsr_z_set() {
    // FBEQ.W ($F281, condition EQ = Z). FPSR Z is bit 26 (cc nibble bit 2
    // at bits 27-24). With Z set → taken.
    let r = run(0xF281, &[0x0040], true, 1 << 26);
    assert_eq!(r.next_instr_pc, PC + 2 + 0x40, "Z set → FBEQ taken");
}

#[test]
fn fbeq_w_not_taken_when_fpsr_z_clear() {
    // FBEQ.W with Z clear → not taken → falls through.
    let r = run(0xF281, &[0x0040], true, 0);
    assert!(!r.vectored);
    assert_eq!(r.next_instr_pc, PC + 4, "Z clear → FBEQ falls through");
}

// --- FBcc.L (op-class 3): 32-bit displacement ---

#[test]
fn fbt_l_always_branches() {
    // FBT.L ($F2CF, condition T) with disp 0x00000040 → instr+2+disp.
    let r = run(0xF2CF, &[0x0000, 0x0040], true, 0);
    assert_eq!(
        r.next_instr_pc,
        PC + 2 + 0x40,
        "FBT.L branches to instr+2+disp"
    );
}

#[test]
fn fbt_l_large_displacement() {
    // A displacement that needs the full 32 bits: high word 0x0001.
    let r = run(0xF2CF, &[0x0001, 0x0000], true, 0);
    assert_eq!(
        r.next_instr_pc,
        PC + 2 + 0x0001_0000,
        "32-bit disp resolves"
    );
}

#[test]
fn fbeq_l_taken_when_fpsr_z_set() {
    // FBEQ.L ($F2C1, condition EQ = Z) with Z set → taken.
    let r = run(0xF2C1, &[0x0000, 0x0040], true, 1 << 26);
    assert_eq!(r.next_instr_pc, PC + 2 + 0x40, "Z set → FBEQ.L taken");
}

#[test]
fn fbeq_l_not_taken_falls_through() {
    // FBEQ.L with Z clear → not taken → falls through past both disp words
    // to the next instruction (6 bytes: opcode + 2 displacement words).
    let r = run(0xF2C1, &[0x0000, 0x0040], true, 0);
    assert!(!r.vectored);
    assert_eq!(
        r.next_instr_pc,
        PC + 6,
        "Z clear → FBEQ.L falls through to instr+6"
    );
}

#[test]
fn fbt_l_negative_displacement() {
    // disp = −16 as a 32-bit value: 0xFFFFFFF0.
    let r = run(0xF2CF, &[0xFFFF, 0xFFF0], true, 0);
    assert_eq!(r.next_instr_pc, (PC + 2).wrapping_sub(16));
}

// --- FDBcc / FTRAPcc / BSUN (#491; #488 BSUN flag) ---

const TRAP7: u32 = 0x0000_4000;

/// Run one F-line instruction with full seeding + CPU inspection. Installs the
/// vector-7 (TRAPcc) and vector-11 (F-line) handlers; returns the CPU after the
/// instruction completes.
fn run_seeded(opcode: u16, ext_words: &[u16], seed: impl FnOnce(&mut Cpu68020)) -> Cpu68020 {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();
    mem.write_long(11 * 4, HANDLER);
    mem.write_word(HANDLER, 0x4E71);
    mem.write_long(7 * 4, TRAP7);
    mem.write_word(TRAP7, 0x4E71);
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
    for _ in 0..400 {
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        if cpu.instruction_starts > start {
            return cpu;
        }
    }
    panic!("instruction did not complete");
}

// FDBcc D0 = $F248; condition word then 16-bit displacement.
#[test]
fn fdbcc_condition_false_decrements_and_branches() {
    // FDBF D0 (condition F = always false) with D0.w = 5, disp = +4. F is
    // false → decrement D0 to 4 and branch to instr_start + 4 + 4 = PC + 8.
    let cpu = run_seeded(0xF248, &[0x0000, 0x0004], |c| c.regs.d[0] = 0x1234_0005);
    assert_eq!(cpu.regs.d[0], 0x1234_0004, "D0.w decremented");
    assert_eq!(cpu.instr_start_pc, PC + 8, "branch to disp-word + disp");
}

#[test]
fn fdbcc_count_exhausted_falls_through() {
    // FDBF D0 with D0.w = 0 → decrement to -1 (0xFFFF) → loop done, fall
    // through past opcode+condition+disp (6 bytes).
    let cpu = run_seeded(0xF248, &[0x0000, 0x0004], |c| c.regs.d[0] = 0x1234_0000);
    assert_eq!(cpu.regs.d[0], 0x1234_FFFF, "D0.w underflowed to -1");
    assert_eq!(cpu.instr_start_pc, PC + 6, "falls through to instr+6");
}

#[test]
fn fdbcc_condition_true_falls_through_without_decrement() {
    // FDBT D0 (condition T = always true) → no operation, D0 unchanged,
    // fall through.
    let cpu = run_seeded(0xF248, &[0x000F, 0x0004], |c| c.regs.d[0] = 0x1234_0005);
    assert_eq!(
        cpu.regs.d[0], 0x1234_0005,
        "D0 unchanged when condition true"
    );
    assert_eq!(cpu.instr_start_pc, PC + 6, "falls through to instr+6");
}

// FTRAPcc (no operand) = $F27C; condition word follows.
#[test]
fn ftrapcc_condition_true_takes_vector_7() {
    // FTRAPT (condition T) → trap, vector 7.
    let cpu = run_seeded(0xF27C, &[0x000F], |_| {});
    assert_eq!(cpu.instr_start_pc, TRAP7, "FTRAPT traps to vector 7");
}

#[test]
fn ftrapcc_condition_false_falls_through() {
    // FTRAPF (condition F) → no trap, fall through past opcode + condition.
    let cpu = run_seeded(0xF27C, &[0x0000], |_| {});
    assert_eq!(cpu.instr_start_pc, PC + 4, "falls through to instr+4");
}

// FTRAPcc.W = $F27A; condition word + one operand word.
#[test]
fn ftrapcc_word_operand_false_skips_operand() {
    let cpu = run_seeded(0xF27A, &[0x0000, 0xDEAD], |_| {});
    assert_eq!(cpu.instr_start_pc, PC + 6, "skips the operand word");
}

#[test]
fn fbcc_nonaware_predicate_sets_bsun_on_nan() {
    // FBGT.W ($F280 | 0x12) is an IEEE-nonaware predicate. With the NAN
    // condition code set, it must set BSUN (FPSR bit 15) + AEXC IOP (bit 7).
    let cpu = run_seeded(0xF292, &[0x0004], |c| c.regs.fpsr = 0x0100_0000); // NAN
    assert_ne!(cpu.regs.fpsr & 0x0000_8000, 0, "BSUN set");
    assert_ne!(cpu.regs.fpsr & 0x0000_0080, 0, "AEXC IOP set");
}

#[test]
fn fbcc_aware_predicate_does_not_set_bsun() {
    // FBOGT.W ($F280 | 0x02) is IEEE-aware → never sets BSUN, even with NAN.
    let cpu = run_seeded(0xF282, &[0x0004], |c| c.regs.fpsr = 0x0100_0000);
    assert_eq!(
        cpu.regs.fpsr & 0x0000_8000,
        0,
        "BSUN not set for aware test"
    );
}
