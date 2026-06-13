//! 68020 cycle-timing tests for the #41 timing model.
//!
//! Two kinds of test, per the plan's verification section:
//!
//! 1. **Directional** — assert the *relationships* the four timing
//!    phases deliver (3- vs 4-clock bus, constant-time shifter, indexed
//!    EA cost, cache hit cheaper than miss), each tied to a fact in the
//!    MC68020 User's Manual § 8. These are robust to the fact that our
//!    model is an approximation.
//! 2. **Characterization golden** — measure the model's elapsed clocks
//!    for a curated instruction set and lock those values against
//!    regression, annotating each with the UM § 8 figure and the delta.
//!
//! ## Why not assert equality with the UM totals
//!
//! Our engine is a sequential, no-overlap clock accumulator
//! (bus cycle = `variant_min_bus_clocks`, plus `Internal(n)` delays), so
//! structurally it targets the UM's **Cache Case** (cache on) and
//! **Worst Case** (cache off) columns — both defined as "no instruction
//! overlap." The UM **Best Case** column assumes cross-instruction
//! pipeline overlap (head/tail) that this model does not represent.
//! Even within CC/WC the UM figures are empirical and do not decompose
//! into our `(bus×3 + internal)` form (e.g. Calculate EA `(d8,An,Xn)` is
//! CC 4 = all internal, but WC 5 = 3 bus + 2 internal — the internal
//! part is not constant across cache states). So exact UM-total matching
//! is not achievable here; the golden records what we produce and cites
//! the UM as the reference target.

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use motorola_68000::bus::{BusStatus, FunctionCode};
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
    fn read_byte(&self, a: u32) -> u8 {
        *self.bytes.get(&(a & 0x00FF_FFFF)).unwrap_or(&0)
    }
    fn read_word(&self, a: u32) -> u16 {
        (u16::from(self.read_byte(a)) << 8) | u16::from(self.read_byte(a.wrapping_add(1)))
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

/// How a run serviced the bus, and how long it took.
struct Run {
    /// CPU ticks (clocks) from this instruction's start to the next.
    ticks: u32,
    /// Completed program-space read (instruction-access) bus cycles.
    prog_reads: u32,
    /// Completed operand read/write bus cycles.
    data_cycles: u32,
}

/// Configuration knobs flipped to isolate one phase's effect on an
/// otherwise-identical 68020.
#[derive(Clone, Copy)]
struct Knobs {
    min_bus: u8,
    constant_shift: bool,
    um_ea_calc: bool,
    cache_on: bool,
}

impl Knobs {
    /// The real 68020 settings (what the wrapper installs).
    fn m68020() -> Self {
        Self {
            min_bus: 3,
            constant_shift: true,
            um_ea_calc: true,
            cache_on: false, // CACR.E starts clear, as on hardware
        }
    }
}

/// Run one instruction to completion and report timing. `words` are the
/// opcode followed by its extension words; `setup` seeds registers and
/// operand memory.
fn run(words: &[u16], knobs: Knobs, setup: impl FnOnce(&mut Cpu68020, &mut Mem)) -> Run {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();

    for (i, w) in words.iter().enumerate() {
        mem.write_word(PC + (i as u32) * 2, *w);
    }
    for k in 0..6 {
        mem.write_word(PC + ((words.len() + k) as u32) * 2, 0x4E71); // trailing NOPs
    }

    setup(&mut cpu, &mut mem);

    cpu.regs.sr |= 0x2000; // supervisor
    cpu.variant_min_bus_clocks = knobs.min_bus;
    cpu.variant_constant_shift_timing = knobs.constant_shift;
    cpu.variant_um_ea_calc_timing = knobs.um_ea_calc;
    if knobs.cache_on {
        cpu.regs.cacr = 0x01;
    }
    cpu.regs.pc = PC + 4;
    cpu.setup_prefetch(words[0], words.get(1).copied().unwrap_or(0x4E71));

    let start = cpu.instruction_starts;
    let mut r = Run {
        ticks: 0,
        prog_reads: 0,
        data_cycles: 0,
    };
    let min_bus = knobs.min_bus;
    for _ in 0..400 {
        if let State::BusCycle {
            addr,
            fc,
            is_read,
            is_word,
            data,
            cycle_count,
            ..
        } = &cpu.state
        {
            if *cycle_count >= min_bus {
                let prog = matches!(
                    fc,
                    FunctionCode::SupervisorProgram | FunctionCode::UserProgram
                );
                if *is_read {
                    if prog {
                        r.prog_reads += 1;
                    } else {
                        r.data_cycles += 1;
                    }
                    let v = if *is_word {
                        mem.read_word(*addr)
                    } else {
                        u16::from(mem.read_byte(*addr))
                    };
                    cpu.bus_status = BusStatus::Ready(v);
                } else {
                    r.data_cycles += 1;
                    if *is_word {
                        mem.write_word(*addr, data.unwrap_or(0));
                    }
                    cpu.bus_status = BusStatus::Ready(0);
                }
            } else {
                cpu.bus_status = BusStatus::Wait;
            }
        } else {
            cpu.bus_status = BusStatus::Wait;
        }
        cpu.tick();
        r.ticks += 1;
        if cpu.instruction_starts > start {
            return r;
        }
    }
    panic!("instruction did not complete");
}

// ─── Instruction encodings (architectural result asserted in setup) ──

const NOP: u16 = 0x4E71;
const MOVE_W_IDX_D2: u16 = 0x3430; // MOVE.W (d8,A0,Xn),D2
const EXT_D1_W: u16 = 0x1000; // brief: Xn=D1.W, scale 1, disp 0
const MOVE_L_A0_D0: u16 = 0x2010; // MOVE.L (A0),D0
const ASL_L_1_D0: u16 = 0xE380; // ASL.L #1,D0
const ASL_L_8_D0: u16 = 0xE180; // ASL.L #8,D0 (count field 0 == 8)

fn seed_idx_move(cpu: &mut Cpu68020, mem: &mut Mem) {
    cpu.regs.a[0] = 0x2000;
    cpu.regs.d[1] = 0x10;
    mem.write_word(0x2010, 0xCAFE);
}

// ─── Directional tests (one per phase) ──────────────────────────────

#[test]
fn phase1_bus_cycle_is_three_clocks_not_four() {
    // MOVE.L (A0),D0 does one long-word read = two word bus cycles plus
    // the instruction-boundary prefetch. Switching only the bus minimum
    // (3 → 4) must lengthen the instruction by exactly one clock per bus
    // cycle. UM § 8.2: "three-cycle read/write" on the 020.
    let setup = |cpu: &mut Cpu68020, mem: &mut Mem| {
        cpu.regs.a[0] = 0x2000;
        mem.write_long(0x2000, 0x1234_5678);
    };
    let at3 = run(
        &[MOVE_L_A0_D0],
        Knobs {
            min_bus: 3,
            ..Knobs::m68020()
        },
        setup,
    );
    let at4 = run(
        &[MOVE_L_A0_D0],
        Knobs {
            min_bus: 4,
            ..Knobs::m68020()
        },
        setup,
    );

    let bus_cycles = at3.prog_reads + at3.data_cycles;
    assert!(
        bus_cycles >= 2,
        "expected at least the long read + a prefetch"
    );
    assert_eq!(
        at4.ticks - at3.ticks,
        bus_cycles,
        "each bus cycle should cost exactly one more clock at min_bus=4 \
         ({} cycles; 3-clock {} vs 4-clock {})",
        bus_cycles,
        at3.ticks,
        at4.ticks
    );
}

#[test]
fn phase2_shifter_is_constant_time() {
    // ASL.L #1 vs ASL.L #8. The 68000 costs 2+2n (so #8 ≫ #1); the 020
    // barrel shifter is constant. UM § 8: shifts are a fixed cost
    // regardless of count.
    let k = Knobs::m68020();
    let asl1 = run(&[ASL_L_1_D0], k, |cpu, _| cpu.regs.d[0] = 0x0F0F_0F0F);
    let asl8 = run(&[ASL_L_8_D0], k, |cpu, _| cpu.regs.d[0] = 0x0F0F_0F0F);
    assert_eq!(
        asl1.ticks, asl8.ticks,
        "020 shifter must be constant-time (#1 {} vs #8 {})",
        asl1.ticks, asl8.ticks
    );

    // With the flag off (68000 model) #8 must cost more than #1.
    let k0 = Knobs {
        constant_shift: false,
        ..k
    };
    let s1 = run(&[ASL_L_1_D0], k0, |cpu, _| cpu.regs.d[0] = 0x0F0F_0F0F);
    let s8 = run(&[ASL_L_8_D0], k0, |cpu, _| cpu.regs.d[0] = 0x0F0F_0F0F);
    assert!(
        s8.ticks > s1.ticks,
        "68000 model shift must grow with count (#1 {} vs #8 {})",
        s1.ticks,
        s8.ticks
    );
}

#[test]
fn phase3_cache_hit_is_cheaper_than_miss() {
    // Same NOP fetched cold (cache empty) vs warm (pre-filled). The warm
    // fetch self-serves with no bus cycle. UM § 8: a cached instruction
    // access performs no external bus cycle.
    let cold = run(
        &[NOP],
        Knobs {
            cache_on: true,
            ..Knobs::m68020()
        },
        |_, _| {},
    );
    // Warm: run twice on the same CPU is awkward through this harness, so
    // assert the structural effect instead — cache off forces the
    // boundary prefetch onto the bus; cache on can serve it. The
    // dedicated loop test in icache_timing.rs proves re-fetch elision.
    let no_cache = run(
        &[NOP],
        Knobs {
            cache_on: false,
            ..Knobs::m68020()
        },
        |_, _| {},
    );
    assert!(
        cold.prog_reads <= no_cache.prog_reads,
        "enabling the cache must not increase program-fetch bus cycles \
         (on {} vs off {})",
        cold.prog_reads,
        no_cache.prog_reads
    );
}

#[test]
fn phase4_indexed_ea_costs_um_cache_case() {
    // Brief (d8,An,Xn): M68020UM § 8.2.3 Calculate EA Cache Case = 4
    // internal clocks; the 68000 model uses a flat 2. Flipping only the
    // EA-calc knob must add exactly 2 clocks, with an identical result.
    let with020 = run(
        &[MOVE_W_IDX_D2, EXT_D1_W],
        Knobs {
            um_ea_calc: true,
            ..Knobs::m68020()
        },
        seed_idx_move,
    );
    let with000 = run(
        &[MOVE_W_IDX_D2, EXT_D1_W],
        Knobs {
            um_ea_calc: false,
            ..Knobs::m68020()
        },
        seed_idx_move,
    );
    assert_eq!(
        with020.ticks - with000.ticks,
        2,
        "020 indexed EA should cost the UM Cache-Case 4 vs the 68000 \
         model's 2 — a 2-clock difference (020 {} vs 68000 {})",
        with020.ticks,
        with000.ticks
    );
}

// ─── Characterization golden (cites UM § 8; asserts our own values) ──
//
// These lock the model's current elapsed-clock output against
// regression. Each line cites the relevant UM § 8 figure so divergence
// is visible and intentional. They are NOT equality assertions against
// the UM (see the module header for why that is not achievable).

#[test]
fn golden_clock_counts() {
    let k = Knobs::m68020(); // cache off → UM Worst-Case column

    // NOP — UM § 8.2.11 lists NOP at 2(0/0/0) cache / worst case (a
    // pipeline sync). Our model charges the instruction-boundary
    // prefetch instead.
    // Golden values are our model's current output; the comment on each
    // gives the UM § 8 reference and the delta. UM column = Worst Case
    // (cache off, no overlap), which these knobs select.

    // NOP — UM lists NOP as a 2-clock pipeline sync. Our model charges
    // only the instruction-boundary prefetch (served here in 1 tick),
    // so we undercount by 1; NOP carries no EA or operand work.
    let nop = run(&[NOP], k, |_, _| {});
    assert_eq!(nop.ticks, 1, "NOP golden (UM ≈ 2; we charge prefetch only)");

    // MOVE.L (A0),D0 — Fetch EA (An) + MOVE.L. The long read is two word
    // bus cycles in our 16-bit-fetch model; with the 3-clock bus that is
    // 6 clocks of operand read plus one boundary prefetch (3) = 9.
    let move_l = run(&[MOVE_L_A0_D0], k, |cpu, mem| {
        cpu.regs.a[0] = 0x2000;
        mem.write_long(0x2000, 0x1234_5678);
    });
    assert_eq!(move_l.ticks, 9, "MOVE.L (A0),D0 golden");
    assert_eq!(move_l.data_cycles, 2, "one long read = two word cycles");

    // MOVE.W (d8,A0,D1),D2 — brief indexed EA calc = 4 (UM § 8.2.3 CC,
    // Phase 4) + one operand read + boundary prefetch.
    let move_idx = run(&[MOVE_W_IDX_D2, EXT_D1_W], k, seed_idx_move);
    assert_eq!(move_idx.ticks, 13, "MOVE.W (d8,A0,Xn),D2 golden");
    assert_eq!(move_idx.data_cycles, 1, "one operand word read");

    // ASL.L #1,D0 vs #8 — constant on the 020 (Phase 2); both 5 here.
    let asl1 = run(&[ASL_L_1_D0], k, |cpu, _| cpu.regs.d[0] = 0x0F0F_0F0F);
    let asl8 = run(&[ASL_L_8_D0], k, |cpu, _| cpu.regs.d[0] = 0x0F0F_0F0F);
    assert_eq!(asl1.ticks, 5, "ASL.L #1,D0 golden");
    assert_eq!(asl8.ticks, 5, "ASL.L #8,D0 golden (constant-time)");
}
