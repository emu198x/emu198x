//! What an interrupt costs on the CPC, against the Compendium's figures.
//!
//! Longshot's *Amstrad CPC CRTC Compendium* v1.10 §27.4 states both halves of
//! the same operation:
//!
//! > The Z80A RST #38 instruction lasts 4 µsec when called by code.
//! > When an interrupt occurs, the call in #38 lasts 5 µsec.
//!
//! At 4 MHz that is **16 and 20 T-states**. The pair is the useful part: a
//! `RST $38` written in code and an interrupt-driven call to the same address
//! do the same architectural work, and the interrupt costs one microsecond
//! more for the acknowledge cycle. An engine that got the Gate Array's
//! instruction stretching wrong would move both; one that got the acknowledge
//! wrong would move only the second. Measuring them together tells those
//! apart.
//!
//! This is deliberately a *direct* assertion rather than a reading of SHAKER's
//! screen. SHAKER measures the same territory, but what its value notation
//! means is still unread (see the `shaker` harness), and a documented figure
//! in T-states needs no interpretation.
//!
//! Nothing here needs the SHAKER disc — only the firmware, for a machine that
//! boots far enough to have a running Gate Array.
//!
//! ```text
//! cargo test -p machine-amstrad-cpc --test interrupt_timing -- --ignored
//! ```

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_amstrad_cpc::AmstradCpc;

/// T-states per microsecond: the Z80A runs at 4 MHz on a CPC.
const TSTATES_PER_USEC: u64 = 4;
/// §27.4: `RST #38` from code, on a CPC — 16 t-states at 4 MHz.
const RST38_BY_CODE_USEC: u64 = 4;
/// §27.4: the same call, caused by an interrupt — 20 t-states.
const RST38_BY_INTERRUPT_USEC: u64 = 5;

/// What this engine measures today, pending #959.
///
/// Both are the bare Z80's own figures: `RST` is 11 t-states and an IM 1
/// acknowledge is 13. The CPC's Gate Array stretches every M-cycle onto a
/// 1 µsec grid, which is what turns those into 16 and 20 — and
/// `amstrad-gate-array` does not model `/WAIT` at all, which its own module
/// docs say outright.
///
/// Recorded exactly rather than asserted as a range. The gate has to fail
/// when this moves in *either* direction: closing #959 should break these
/// tests and update the numbers in the same commit, and anything else that
/// moves them is news.
const MEASURED_RST38_BY_CODE: u64 = 11;
const MEASURED_RST38_BY_INTERRUPT: u64 = 13;

/// Where the test code is planted. Always RAM on a CPC, and clear of both the
/// firmware's variables and the screen.
const CODE: u16 = 0x8000;
/// The IM 1 vector, and where `RST $38` lands.
const HANDLER: u16 = 0x0038;

/// `RST $38`.
const OP_RST38: u8 = 0xFF;
/// `NOP` — one microsecond on a CPC, which `cpu_rate.rs` already pins.
const OP_NOP: u8 = 0x00;

/// Frames to reach a booted machine with the Gate Array counting HSyncs.
const BOOT_FRAMES: usize = 150;

fn firmware_path() -> PathBuf {
    if let Some(p) = env::var_os("EMU198X_CPC_ROM") {
        return PathBuf::from(p);
    }
    PathBuf::from(env::var("HOME").expect("HOME")).join(".emu198x/roms/amstrad-cpc/cpc464.rom")
}

/// Boot, plant `code` at [`CODE`], and enter it on an instruction boundary.
///
/// The boundary matters: `run_frame` stops on a frame boundary, which says
/// nothing about where the CPU is inside an instruction, and writing `PC`
/// mid-instruction lets the in-flight instruction finish against the new `PC`.
/// That is #943.
fn boot_with(firmware: &[u8], code: &[u8]) -> AmstradCpc {
    let mut cpc = AmstradCpc::new(firmware).expect("build machine");
    for _ in 0..BOOT_FRAMES {
        cpc.run_frame();
    }
    for (i, &b) in code.iter().enumerate() {
        cpc.poke(CODE.wrapping_add(u16::try_from(i).expect("fits")), b);
    }
    let mut guard = 0;
    while !cpc.z80().instruction_complete() {
        cpc.advance_tstates(1);
        guard += 1;
        assert!(guard < 256, "no instruction boundary within 256 t-states");
    }
    cpc.z80_mut().regs.pc = CODE;
    cpc
}

/// Run one instruction, returning the T-states it took and where `PC` ended.
///
/// Measured by retirement, not by watching `PC`. `PC` moves *inside* an
/// instruction — during a `RST` it takes the target address partway through,
/// before the pushes are done — so "advance until `PC` reads `$0038`" reports
/// 5 t-states for a 16-t-state instruction. `instructions_retired` is the
/// boundary that means what it says.
fn run_one_instruction(cpc: &mut AmstradCpc, limit: u64) -> (u64, u16) {
    let retired = cpc.z80().instructions_retired();
    let start = cpc.cpu_tstates();
    for _ in 0..limit {
        cpc.advance_tstates(1);
        if cpc.z80().instructions_retired() != retired {
            return (cpc.cpu_tstates() - start, cpc.z80().regs.pc);
        }
    }
    panic!("no instruction retired within {limit} t-states");
}

/// `RST $38` written in code, with interrupts off so nothing else can reach
/// `$0038` first.
#[test]
#[ignore = "needs the CPC464 firmware — run with --ignored"]
fn rst38_from_code_is_recorded_against_the_compendium() {
    let rom = firmware_path();
    if !rom.exists() {
        emu198x_test_skip::skip!("cpc464.rom not staged (EMU198X_CPC_ROM)");
    }
    let firmware = fs::read(&rom).expect("read firmware");
    let mut cpc = boot_with(&firmware, &[OP_RST38]);
    {
        let z80 = cpc.z80_mut();
        z80.regs.iff1 = false;
        z80.regs.iff2 = false;
    }

    let (taken, pc) = run_one_instruction(&mut cpc, 1_000);
    assert_eq!(
        pc, HANDLER,
        "`RST $38` should land on $0038, ended at ${pc:04X}"
    );
    let documented = RST38_BY_CODE_USEC * TSTATES_PER_USEC;
    assert_eq!(
        taken, MEASURED_RST38_BY_CODE,
        "`RST $38` from code moved. Compendium §27.4 puts it at \
         {RST38_BY_CODE_USEC} µsec ({documented} t-states at 4 MHz); this \
         engine has been recording {MEASURED_RST38_BY_CODE}, the bare Z80's \
         own figure, because the Gate Array's /WAIT stretching is not modelled \
         (#959). Measured {taken}."
    );
}

/// The same arrival at `$0038`, caused by the Gate Array rather than by an
/// opcode.
///
/// Runs `NOP`s with interrupts enabled and waits for the Gate Array's HSync
/// counter to raise `/INT` on its own — no pin is forced, so this exercises
/// the real assertion path.
#[test]
#[ignore = "needs the CPC464 firmware — run with --ignored"]
fn the_interrupt_call_is_recorded_against_the_compendium() {
    let rom = firmware_path();
    if !rom.exists() {
        emu198x_test_skip::skip!("cpc464.rom not staged (EMU198X_CPC_ROM)");
    }
    let firmware = fs::read(&rom).expect("read firmware");
    // A NOP field that loops. Interrupts arrive every 52 HSyncs — a few
    // thousand NOPs apart — so a straight run would leave the field long
    // before one landed.
    let mut field = [OP_NOP; 256];
    field[253] = 0xC3; // JP CODE
    field[254] = (CODE & 0xFF) as u8;
    field[255] = (CODE >> 8) as u8;
    let mut cpc = boot_with(&firmware, &field);
    {
        let z80 = cpc.z80_mut();
        z80.regs.im = 1;
        z80.regs.iff1 = true;
        z80.regs.iff2 = true;
    }

    // Step instruction by instruction until one of them lands on $0038. That
    // one is the interrupt response: the NOPs before it each retire in their
    // own time, and the response begins where the last NOP ended — which is
    // the boundary sampling this measures.
    let mut cost = None;
    for _ in 0..20_000u32 {
        let (taken, pc) = run_one_instruction(&mut cpc, 4_000);
        if pc == HANDLER {
            cost = Some(taken);
            break;
        }
        assert!(
            (CODE..CODE.wrapping_add(256)).contains(&pc),
            "ran out of the NOP field to ${pc:04X} before any interrupt"
        );
    }

    let taken = cost.expect("the Gate Array never raised an interrupt");
    let documented = RST38_BY_INTERRUPT_USEC * TSTATES_PER_USEC;
    assert_eq!(
        taken, MEASURED_RST38_BY_INTERRUPT,
        "the interrupt call to #38 moved. Compendium §27.4 puts it at \
         {RST38_BY_INTERRUPT_USEC} µsec ({documented} t-states at 4 MHz); this \
         engine has been recording {MEASURED_RST38_BY_INTERRUPT}, the bare \
         Z80's IM 1 acknowledge, because the Gate Array's /WAIT stretching is \
         not modelled (#959). Measured {taken}."
    );
}

/// The acknowledge's own cost, which survives both figures being wrong
/// together.
///
/// The Compendium has an interrupt costing exactly one microsecond more than
/// the equivalent `RST $38` — four t-states. This engine's two figures differ
/// by **two**, the bare Z80's gap, because neither is on the CPC's
/// microsecond grid. So the shortfall is not a constant offset that could be
/// waved away as a measurement origin: the acknowledge is short by a different
/// amount than the instruction is, and only modelling `/WAIT` fixes both.
#[test]
fn the_acknowledge_gap_is_recorded_as_well_as_the_totals() {
    let documented_gap = (RST38_BY_INTERRUPT_USEC - RST38_BY_CODE_USEC) * TSTATES_PER_USEC;
    let measured_gap = MEASURED_RST38_BY_INTERRUPT - MEASURED_RST38_BY_CODE;
    assert_eq!(documented_gap, 4, "§27.4's own figures differ by 1 µsec");
    assert_eq!(
        measured_gap, 2,
        "the recorded gap changed; if #959 landed, update all three constants \
         together"
    );
}
