//! FSAVE / FRESTORE — 68881/2 internal-state frame (#112, #490).
//!
//! FSAVE/FRESTORE move only the FPU's *internal* state to / from a memory
//! frame (the FP data registers and FPCR/FPSR/FPIAR are saved separately by
//! FMOVEM). For our synchronous core — which has no mid-instruction
//! exception or busy state — this is the frame-format formality plus the
//! null-frame reset.
//!
//! Validated against WinUAE `fpuop_save` / `fpuop_restore` (6888x branch):
//! exact frame bytes per model, the null-frame `fpu_null` reset, the
//! null↔idle state transition, the postincrement / predecrement pointer
//! updates, and the format error (vector 14) on a bad frame version.

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use motorola_68k_common::registers::FpReg;
use motorola_68000::bus::BusStatus;
use motorola_68000::cpu::State;
use motorola_68020::Cpu68020;

const PC: u32 = 0x0000_1000;
const HANDLER: u32 = 0x0000_3000;
const INITIAL_SSP: u32 = 0x0000_8000;
const BUF: u32 = 0x0000_2000;

/// The 68881/2 created NaN — `fpu_null` fills every FP data register with
/// it (WinUAE `xhex_nan`: sign+exp $7FFF, mantissa all ones).
const NULL_NAN: FpReg = FpReg::new(0x7FFF, 0xFFFF_FFFF_FFFF_FFFF);

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
    fn write_long(&mut self, a: u32, v: u32) {
        self.write_word(a, (v >> 16) as u16);
        self.write_word(a.wrapping_add(2), v as u16);
    }
    fn byte(&self, a: u32) -> u8 {
        *self.bytes.get(&(a & 0x00FF_FFFF)).unwrap_or(&0)
    }
    fn long(&self, a: u32) -> u32 {
        (u32::from(self.byte(a)) << 24)
            | (u32::from(self.byte(a + 1)) << 16)
            | (u32::from(self.byte(a + 2)) << 8)
            | u32::from(self.byte(a + 3))
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

struct Out {
    a: [u32; 7],
    fp: [FpReg; 8],
    fpcr: u32,
    fpsr: u32,
    fpiar: u32,
    fpu_state: u8,
    vectored: bool,
}

/// Run one F-line opcode (+ EA extension words) to completion, with the
/// FPU attached. `seed` configures registers / memory / FPU model and
/// state before the run.
fn run(opcode: u16, ext_words: &[u16], seed: impl FnOnce(&mut Cpu68020, &mut Mem)) -> (Out, Mem) {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();

    // Vector 14 (format error, offset 14*4 = $38) → handler.
    mem.write_long(14 * 4, HANDLER);
    mem.write_word(HANDLER, 0x4E71); // NOP

    mem.write_word(PC, opcode);
    for (i, w) in ext_words.iter().enumerate() {
        mem.write_word(PC + 2 + (i as u32) * 2, *w);
    }

    cpu.set_fpu_present(true);
    cpu.regs.sr |= 0x2000; // supervisor
    cpu.regs.ssp = INITIAL_SSP;
    cpu.regs.set_active_sp(INITIAL_SSP);
    seed(&mut cpu, &mut mem);
    cpu.regs.pc = PC + 4;
    cpu.setup_prefetch(opcode, ext_words.first().copied().unwrap_or(0x4E71));

    let start = cpu.instruction_starts;
    for _ in 0..400 {
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        if cpu.instruction_starts > start {
            let out = Out {
                a: cpu.regs.a,
                fp: cpu.regs.fp,
                fpcr: cpu.regs.fpcr,
                fpsr: cpu.regs.fpsr,
                fpiar: cpu.regs.fpiar,
                fpu_state: cpu.variant_fpu_state,
                vectored: cpu.instr_start_pc == HANDLER,
            };
            return (out, mem);
        }
    }
    panic!("instruction did not complete");
}

// FSAVE / FRESTORE opcode bases: cpID 1 (bits 11-9), op-class 4 (FSAVE) /
// 5 (FRESTORE), then the EA mode<<3 | reg in the low six bits.
const FSAVE: u16 = 0xF300;
const FRESTORE: u16 = 0xF340;

// ─── FSAVE ──────────────────────────────────────────────────────────────

#[test]
fn fsave_null_frame_68881() {
    // Fresh FPU (state null) → the 4-byte null frame: version byte 0, size
    // byte $18, two zero bytes. Only those 4 bytes are written.
    let (out, mem) = run(FSAVE | 0x10, &[], |cpu, _| {
        cpu.regs.set_a(0, BUF);
    });
    assert_eq!(mem.long(BUF), 0x0018_0000, "68881 null frame id");
    assert_eq!(out.a[0], BUF, "(An) does not change A0");
    // No fifth byte written (the frame is exactly 4 bytes).
    assert_eq!(mem.byte(BUF + 4), 0, "no body for a null frame");
}

#[test]
fn fsave_idle_frame_68881() {
    // Idle FPU → the 28-byte idle frame: id $1F180000, then zeroed ccr /
    // exceptional-operand / operand longwords, and the BIU flags
    // ($5C0EFFFF) at offset 24.
    let (_out, mem) = run(FSAVE | 0x10, &[], |cpu, _| {
        cpu.regs.set_a(0, BUF);
        cpu.variant_fpu_state = 1;
    });
    assert_eq!(mem.long(BUF), 0x1F18_0000, "68881 idle frame id");
    for off in [4, 8, 12, 16, 20] {
        assert_eq!(mem.long(BUF + off), 0, "zeroed field at +{off}");
    }
    assert_eq!(mem.long(BUF + 24), 0x5C0E_FFFF, "BIU flags");
}

#[test]
fn fsave_idle_frame_68882() {
    // 68882 idle frame: 60 bytes, id $1F380000, 8 unused longwords after
    // the ccr, then eo/operand, and the BIU flags at offset 56.
    let (_out, mem) = run(FSAVE | 0x10, &[], |cpu, _| {
        cpu.regs.set_a(0, BUF);
        cpu.set_fpu_68882(true);
        cpu.variant_fpu_state = 1;
    });
    assert_eq!(mem.long(BUF), 0x1F38_0000, "68882 idle frame id");
    // Everything from the ccr through the operand register is zero.
    for off in (4..56).step_by(4) {
        assert_eq!(mem.long(BUF + off), 0, "zeroed field at +{off}");
    }
    assert_eq!(mem.long(BUF + 56), 0x5C0E_FFFF, "BIU flags at +56");
}

#[test]
fn fsave_predecrement_steps_back_by_frame_size() {
    // FSAVE -(A0): A0 decrements by the whole frame (28 bytes for an idle
    // 68881 frame) and the frame is written from the new A0.
    let (out, mem) = run(FSAVE | 0x20, &[], |cpu, _| {
        cpu.regs.set_a(0, BUF + 0x100);
        cpu.variant_fpu_state = 1;
    });
    let expected = BUF + 0x100 - 28;
    assert_eq!(out.a[0], expected, "-(A0) steps back by the frame size");
    assert_eq!(mem.long(expected), 0x1F18_0000, "frame written at new A0");
}

#[test]
fn fsave_does_not_change_fpu_state() {
    // FSAVE reports the state but must not change it.
    let (out, _mem) = run(FSAVE | 0x10, &[], |cpu, _| {
        cpu.regs.set_a(0, BUF);
        cpu.variant_fpu_state = 1;
    });
    assert_eq!(out.fpu_state, 1, "FSAVE leaves the FPU idle");
}

#[test]
fn fp_instruction_takes_fpu_out_of_null_state() {
    // Any executed 68881/2 FP op (here FNOP, op-class 2) moves the FPU from
    // null to idle — so a following FSAVE would emit an idle frame.
    let (out, _mem) = run(0xF280, &[0x0000], |_cpu, _| {});
    assert_eq!(out.fpu_state, 1, "FNOP flips null → idle");
}

#[test]
fn fsave_to_absolute_long() {
    // Control addressing (abs.L) goes through the core EA machinery.
    let (_out, mem) = run(FSAVE | 0x39, &[(BUF >> 16) as u16, BUF as u16], |cpu, _| {
        cpu.variant_fpu_state = 1;
    });
    assert_eq!(mem.long(BUF), 0x1F18_0000, "frame written at abs.L target");
}

// ─── FRESTORE ─────────────────────────────────────────────────────────────

#[test]
fn frestore_null_frame_resets_fpu() {
    // A null frame (version byte 0) runs fpu_null: state → null, FPCR/FPSR/
    // FPIAR → 0, every FP data register → the created NaN.
    let (out, _mem) = run(FRESTORE | 0x10, &[], |cpu, mem| {
        cpu.regs.set_a(0, BUF);
        mem.write_long(BUF, 0x0018_0000);
        cpu.variant_fpu_state = 1;
        cpu.regs.fpcr = 0x30;
        cpu.regs.fpsr = 0x0F00_0000;
        cpu.regs.fpiar = 0xDEAD_BEEF;
        for r in &mut cpu.regs.fp {
            *r = FpReg::new(0x4000, 0x8000_0000_0000_0000);
        }
    });
    assert_eq!(out.fpu_state, 0, "null frame resets to null state");
    assert_eq!(out.fpcr, 0, "FPCR cleared");
    assert_eq!(out.fpsr, 0, "FPSR cleared");
    assert_eq!(out.fpiar, 0, "FPIAR cleared");
    for (i, r) in out.fp.iter().enumerate() {
        assert_eq!(*r, NULL_NAN, "FP{i} reset to the created NaN");
    }
}

#[test]
fn frestore_idle_frame_sets_idle_state() {
    // An idle 68881 frame ($1F180000) sets the state to idle without
    // touching the FP data registers (those are restored by FMOVEM).
    let seeded = FpReg::new(0x4000, 0x8000_0000_0000_0000);
    let (out, _mem) = run(FRESTORE | 0x10, &[], |cpu, mem| {
        cpu.regs.set_a(0, BUF);
        mem.write_long(BUF, 0x1F18_0000);
        mem.write_long(BUF + 24, 0x5C0E_FFFF);
        cpu.variant_fpu_state = 0;
        for r in &mut cpu.regs.fp {
            *r = seeded;
        }
    });
    assert_eq!(out.fpu_state, 1, "idle frame → idle state");
    assert_eq!(out.fp[3], seeded, "FRESTORE idle leaves FP data registers");
}

#[test]
fn frestore_postincrement_advances_by_consumed_bytes() {
    // (A0)+ advances A0 by the number of frame bytes consumed: 4 for a null
    // frame, 28 for an idle 68881 frame.
    let (out_null, _) = run(FRESTORE | 0x18, &[], |cpu, mem| {
        cpu.regs.set_a(0, BUF);
        mem.write_long(BUF, 0x0018_0000);
    });
    assert_eq!(out_null.a[0], BUF + 4, "(A0)+ null frame advances 4");

    let (out_idle, _) = run(FRESTORE | 0x18, &[], |cpu, mem| {
        cpu.regs.set_a(0, BUF);
        mem.write_long(BUF, 0x1F18_0000);
    });
    assert_eq!(
        out_idle.a[0],
        BUF + 28,
        "(A0)+ idle 68881 frame advances 28"
    );
}

#[test]
fn frestore_68882_idle_frame() {
    // A 68882 idle frame ($1F380000) is 60 bytes; (A0)+ advances 60.
    let (out, _) = run(FRESTORE | 0x18, &[], |cpu, mem| {
        cpu.regs.set_a(0, BUF);
        mem.write_long(BUF, 0x1F38_0000);
    });
    assert_eq!(out.fpu_state, 1, "68882 idle frame → idle state");
    assert_eq!(out.a[0], BUF + 60, "(A0)+ idle 68882 frame advances 60");
}

#[test]
fn frestore_bad_version_takes_format_exception() {
    // A frame with neither a null (0) nor an idle ($1F) version byte is a
    // format error → vector 14. The postincrement pointer is not updated.
    let (out, _) = run(FRESTORE | 0x18, &[], |cpu, mem| {
        cpu.regs.set_a(0, BUF);
        mem.write_long(BUF, 0x0700_0000);
    });
    assert!(
        out.vectored,
        "bad frame version → format exception (vector 14)"
    );
    assert_eq!(out.a[0], BUF, "A0 not advanced on a fault");
}

#[test]
fn frestore_idle_version_with_bad_size_faults() {
    // Version $1F but an unrecognised size byte is also a format error.
    let (out, _) = run(FRESTORE | 0x10, &[], |cpu, mem| {
        cpu.regs.set_a(0, BUF);
        mem.write_long(BUF, 0x1F40_0000);
    });
    assert!(
        out.vectored,
        "version $1F with a bad size → format exception"
    );
}

// ─── Round trip ───────────────────────────────────────────────────────────

#[test]
fn fsave_frestore_idle_round_trip() {
    // FSAVE an idle state to a buffer, then FRESTORE it back — the FPU is
    // idle at both ends.
    let (_save, mem) = run(FSAVE | 0x10, &[], |cpu, _| {
        cpu.regs.set_a(0, BUF);
        cpu.variant_fpu_state = 1;
    });
    let saved = mem.long(BUF);
    assert_eq!(saved, 0x1F18_0000);

    let (out, _) = run(FRESTORE | 0x10, &[], |cpu, m| {
        cpu.regs.set_a(0, BUF);
        // Copy the saved frame into the FRESTORE run's memory.
        for off in 0..28 {
            m.bytes.insert(BUF + off, mem.byte(BUF + off));
        }
        cpu.variant_fpu_state = 0;
    });
    assert_eq!(out.fpu_state, 1, "round-trip preserves the idle state");
}

#[test]
fn fsave_frestore_null_round_trip() {
    // The null-frame round trip: FSAVE a null state, FRESTORE resets.
    let (_save, mem) = run(FSAVE | 0x10, &[], |cpu, _| {
        cpu.regs.set_a(0, BUF);
        // state already 0 (null) on a fresh CPU
    });
    assert_eq!(mem.long(BUF), 0x0018_0000);

    let (out, _) = run(FRESTORE | 0x10, &[], |cpu, m| {
        cpu.regs.set_a(0, BUF);
        m.write_long(BUF, mem.long(BUF));
        cpu.variant_fpu_state = 1;
    });
    assert_eq!(out.fpu_state, 0, "round-trip restores the null state");
}
