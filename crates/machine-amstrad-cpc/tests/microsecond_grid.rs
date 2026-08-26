//! The CPC's microsecond grid.
//!
//! The Gate Array stretches every Z80 M-cycle onto a 1 µs boundary, so on this
//! machine **every instruction costs a whole number of microseconds** and every
//! instruction boundary lands on the same T-state of the microsecond. That is
//! the firmware guide's claim stated as an invariant rather than as a rate:
//!
//! > Accesses to memory are synchronised with the video logic — they are
//! > constrained to occur on microsecond boundaries. This has the effect of
//! > stretching each Z80 M cycle (machine cycle) to be a multiple of 4 T states
//! > (clock cycles).
//!
//! Worth testing as an invariant because it localises a defect that a rate
//! cannot. `cpu_rate.rs` pins a `NOP` at 4 T-states and passes whether `/WAIT`
//! is modelled or not, because a `NOP` is natively 4 and already on the grid.
//! The invariant here fails the moment anything runs off the grid, and names
//! the instruction that did it.
//!
//! Individual *M-cycles* are not each a multiple of 4 in this engine — tracing
//! shows 3, 5, 6 and 9 — and that is not a defect on its own. The stretching
//! redistributes within an instruction, and the instruction totals below are
//! what the machine is documented by and what software observes.

use std::path::PathBuf;
use std::{env, fs};

use machine_amstrad_cpc::AmstradCpc;

/// Where the test code is planted: RAM, clear of the firmware's variables.
const CODE: u16 = 0x8000;
/// Stack parked well below the code, so a field of `PUSH` cannot walk into it.
const STACK: u16 = 0x7000;
/// The IM 1 vector.
const HANDLER: u16 = 0x0038;
const T_PER_USEC: u64 = 4;
const BOOT_FRAMES: usize = 150;

fn firmware_path() -> PathBuf {
    if let Some(p) = env::var_os("EMU198X_CPC_ROM") {
        return PathBuf::from(p);
    }
    PathBuf::from(env::var("HOME").expect("HOME")).join(".emu198x/roms/amstrad-cpc/cpc464.rom")
}

/// Boot, plant a field of `code` repeated up to a jump back to the start, and
/// enter it on an instruction boundary.
fn boot_with_field(firmware: &[u8], code: &[u8]) -> AmstradCpc {
    let mut cpc = AmstradCpc::new(firmware).expect("build machine");
    for _ in 0..BOOT_FRAMES {
        cpc.run_frame();
    }
    let mut addr = CODE;
    while usize::from(addr - CODE) + code.len() + 3 < 0x100 {
        for &b in code {
            cpc.poke(addr, b);
            addr += 1;
        }
    }
    cpc.poke(addr, 0xC3);
    cpc.poke(addr + 1, (CODE & 0xFF) as u8);
    cpc.poke(addr + 2, (CODE >> 8) as u8);

    let mut guard = 0;
    while !cpc.z80().instruction_complete() {
        cpc.advance_tstates(1);
        guard += 1;
        assert!(guard < 256, "no instruction boundary within 256 t-states");
    }
    cpc.z80_mut().regs.pc = CODE;
    cpc.z80_mut().regs.sp = STACK;
    cpc
}

/// Run one instruction. Returns its cost and the `PC` it left behind.
///
/// Bounded by retirement, not by `PC`: `PC` moves *inside* an instruction, so
/// a `PC` test reports 5 t-states for a 16-t-state `RST`.
fn run_one(cpc: &mut AmstradCpc) -> (u64, u16, u64) {
    let retired = cpc.z80().instructions_retired();
    let start = cpc.cpu_tstates();
    for _ in 0..4_000 {
        cpc.advance_tstates(1);
        if cpc.z80().instructions_retired() != retired {
            return (
                cpc.cpu_tstates() - start,
                cpc.z80().regs.pc,
                start % T_PER_USEC,
            );
        }
    }
    panic!("no instruction retired within 4000 t-states");
}

/// Every instruction costs a whole number of microseconds.
///
/// Bare-Z80 lengths are given alongside so the stretching is visible: six of
/// the nine are *not* multiples of four unstretched, and every one of them is
/// wrong on this machine without `/WAIT` (#959).
#[test]
#[ignore = "FIXTURE: needs the CPC464 firmware — run with --ignored"]
fn every_instruction_costs_whole_microseconds() {
    let rom = firmware_path();
    if !rom.exists() {
        emu198x_test_skip::skip!("cpc464.rom not staged (EMU198X_CPC_ROM)");
    }
    let firmware = fs::read(&rom).expect("read firmware");

    // (mnemonic, encoding, bare Z80 t-states, CPC microseconds)
    let cases: &[(&str, &[u8], u64, u64)] = &[
        ("NOP", &[0x00], 4, 1),
        ("INC A", &[0x3C], 4, 1),
        ("LD A,n", &[0x3E, 0x12], 7, 2),
        ("LD (HL),A", &[0x77], 7, 2),
        ("LD HL,nn", &[0x21, 0x34, 0x12], 10, 3),
        ("JP nn", &[0xC3, 0x00, 0x80], 10, 3),
        // `RST`'s shape: a five-t-state M1 and two writes. §27.4 puts this
        // family at 4 µsec, which is where the `RST $38` figure comes from.
        ("PUSH BC", &[0xC5], 11, 4),
        ("LD A,(nn)", &[0x3A, 0x00, 0x90], 13, 4),
        ("EX (SP),HL", &[0xE3], 19, 6),
    ];

    let mut wrong = Vec::new();
    for &(name, code, bare, usec) in cases {
        let mut cpc = boot_with_field(&firmware, code);
        cpc.z80_mut().regs.iff1 = false;
        cpc.z80_mut().regs.iff2 = false;
        // Settle into the field before timing anything.
        for _ in 0..16 {
            run_one(&mut cpc);
        }
        let want = usec * T_PER_USEC;
        for _ in 0..32 {
            let (cost, pc, _) = run_one(&mut cpc);
            // The jump back closes each pass of the field; it is timed by its
            // own case rather than as part of this one.
            if code[0] != 0xC3 && pc == CODE {
                continue;
            }
            if cost != want {
                wrong.push(format!("{name}: {cost} (bare {bare}, want {want})"));
                break;
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "instructions off the microsecond grid: {wrong:#?}"
    );
}

/// Instruction cost is alignment-dependent, and should not be.
///
/// A `JP nn` costs 12 t-states in a field of `JP`s and **11** in a field of
/// `NOP`s. On a CPC it is 3 µsec either way: the Gate Array stretches each
/// M-cycle to a multiple of four, so an instruction cannot cost less because
/// of what ran before it.
///
/// What happens instead is that the `JP` comes up one short, leaves the
/// boundary off the microsecond grid, and the *next* instruction absorbs the
/// difference — the `NOP` after it costs 5 and the grid is restored:
///
/// ```text
/// cost=4   pc=$80FD  phase=1     NOPs, grid-locked
/// cost=11  pc=$8000  phase=1     JP nn — one short, off the grid
/// cost=5   pc=$8001  phase=0     the next NOP absorbs it
/// cost=4   pc=$8002  phase=1     re-locked
/// ```
///
/// So the totals come out right over any run of instructions, which is why
/// `every_instruction_costs_whole_microseconds` passes — it measures
/// homogeneous fields, where every instruction has the same alignment — and
/// why SHAKER, which measures over many instructions, agrees on all six of its
/// figures. It is individual costs in mixed streams that are wrong.
///
/// This is the same defect as the interrupt response's 19 (#971), not a
/// separate one: the response is an M-cycle sequence that comes up one short
/// in a stream of `NOP`s. Pinned exactly here, wrong values and all, so that
/// fixing it fails this test loudly.
#[test]
#[ignore = "FIXTURE: needs the CPC464 firmware — run with --ignored"]
fn instruction_cost_depends_on_alignment_and_should_not() {
    let rom = firmware_path();
    if !rom.exists() {
        emu198x_test_skip::skip!("cpc464.rom not staged (EMU198X_CPC_ROM)");
    }
    let firmware = fs::read(&rom).expect("read firmware");

    // A field of `NOP`s closed by `JP CODE`, so the jump runs among `NOP`s
    // rather than among its own kind.
    let mut cpc = boot_with_field(&firmware, &[0x00]);
    cpc.z80_mut().regs.iff1 = false;
    cpc.z80_mut().regs.iff2 = false;
    for _ in 0..16 {
        run_one(&mut cpc);
    }

    let mut jump = None;
    let mut after = None;
    for _ in 0..600 {
        let (cost, pc, _) = run_one(&mut cpc);
        if pc == CODE {
            jump = Some(cost);
            after = Some(run_one(&mut cpc).0);
            break;
        }
        assert_eq!(cost, 4, "a NOP in the field cost {cost} t-states");
    }

    assert_eq!(
        jump.expect("the field never wrapped"),
        11,
        "`JP nn` among NOPs. It is 3 µsec — 12 t-states — on a CPC, and this \
         engine measures 12 for the same instruction in a field of its own \
         kind. If this now reads 12, the alignment dependence is fixed (#971) \
         — update this test and the response test below with it."
    );
    assert_eq!(
        after.expect("nothing ran after the jump"),
        5,
        "the instruction after the jump absorbs what the jump was short by. A \
         `NOP` is 1 µsec on a CPC and cannot cost 5; it does here only because \
         the jump left the boundary off the grid."
    );
}

/// The interrupt response is the one thing on this machine that leaves the
/// grid — and this records it rather than asserting it is fine.
///
/// The response costs 19 t-states from a boundary that is on the grid, so it
/// ends off it and every instruction after it is off it too: costs of 5 and 7
/// appear in the handler, which cannot happen on a CPC. Twenty would restore
/// the invariant exactly.
///
/// That is independent evidence for §27.4's 5 µsec, arrived at without
/// consulting it: the machine's own quantisation says 19 is impossible.
/// Caprice32 agrees from a third direction — `z80_int_handler` sets
/// `iCycleCount = 20` for IM 0/1 (`src/z80.cpp`).
///
/// Pinned exactly, both the cost and the fact that the phase shifts, so
/// fixing #971 breaks this test and it is updated deliberately.
///
/// Same root cause as [`instruction_cost_depends_on_alignment_and_should_not`].
#[test]
#[ignore = "FIXTURE: needs the CPC464 firmware — run with --ignored"]
fn the_interrupt_response_leaves_the_grid() {
    let rom = firmware_path();
    if !rom.exists() {
        emu198x_test_skip::skip!("cpc464.rom not staged (EMU198X_CPC_ROM)");
    }
    let firmware = fs::read(&rom).expect("read firmware");
    let mut cpc = boot_with_field(&firmware, &[0x00]);
    {
        let z80 = cpc.z80_mut();
        z80.regs.im = 1;
        z80.regs.iff1 = true;
        z80.regs.iff2 = true;
    }

    // The field's own jump wraps every 253 instructions and costs one short,
    // so the instruction after it is a five-t-state `NOP` — that is the defect
    // recorded in `instruction_cost_depends_on_alignment_and_should_not`, not
    // something this test is measuring. Take the grid phase from the last
    // `NOP` that ran at its proper 4.
    let mut before = None;
    for _ in 0..20_000u32 {
        let (cost, pc, at) = run_one(&mut cpc);
        if pc == HANDLER {
            let entered_at: u64 = before.expect("no grid-locked NOP before the response");
            assert_eq!(
                at, entered_at,
                "the response should begin on the microsecond grid; if it does \
                 not, this test is measuring the wrong thing"
            );
            assert_eq!(
                cost, 19,
                "the interrupt response cost changed. §27.4 puts it at 5 µsec \
                 (20 t-states); Caprice32 charges 20; and 20 is what keeps the \
                 boundary on the grid. If this now reads 20, #971 is fixed — \
                 update this test and the phase assertion below with it."
            );
            let (_, _, next) = run_one(&mut cpc);
            let _ = entered_at;
            assert_ne!(
                next, entered_at,
                "the response no longer moves the boundary off the grid, which \
                 is the outcome #971 wants — update this test"
            );
            assert_eq!(
                (entered_at + cost) % T_PER_USEC,
                next,
                "the boundary after the response should follow from its cost"
            );
            return;
        }
        if cost == 4 {
            before = Some(at);
        }
    }
    panic!("the Gate Array never raised an interrupt");
}
