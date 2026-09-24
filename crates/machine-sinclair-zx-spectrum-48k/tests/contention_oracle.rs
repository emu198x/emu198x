//! Per-instruction contention oracle: measure what the engine actually
//! costs, and diff it against the canonical delay pattern.
//!
//! The ZXSpectrum4.net timing suite grades a whole loop pass/fail. That is
//! enough to say "contention is wrong" and nothing at all about *which*
//! cycles are wrong, which is why several structural changes to the gate
//! were tried and reverted without ever being scored at a useful
//! granularity. This harness closes that gap: it runs one known
//! instruction out of contended RAM for exactly one frame and compares the
//! retired-instruction count against the count the canonical model
//! predicts.
//!
//! The canonical side is computed here rather than taken from an external
//! table, from the delay pattern and display geometry in
//! `reference/by-system/sinclair-zx-spectrum/ula-timing-expanded.md`:
//! `[6,5,4,3,2,1,0,0]` repeating over the first 128 T-states of each of the
//! 192 display lines. Contention is applied once at the start of each
//! M-cycle, per Smith's `CLKWAIT = (C3 OR C2) AND /Border AND A14 AND /A15
//! AND /MREQT23` (Chapter 18) — the wait holds until the access commits,
//! so an M-cycle is charged one delay, not one per T-state.
//!
//! The model is anchored by two instructions the engine is already known
//! to get exactly right — `NOP` and `INC BC`, both single-M-cycle — so a
//! divergence on the multi-M-cycle cases is a statement about the engine,
//! not about the model.
//!
//! Report-only by design: it prints a table and asserts only that every
//! case ran. Turning any individual case into a gate is a separate
//! decision, taken once the engine agrees with the oracle.
//!
//! Two views, and they are not equally authoritative.
//! `contention_matches_the_canonical_model_per_instruction` compares
//! whole-frame instruction counts and matches exactly for every case.
//! `contention_cost_by_arrival_phase` breaks the same work down by
//! contention phase and still carries a one-T-state residual on
//! multi-M-cycle instructions — see `PHASE_SAMPLE_OFFSET`. Read the
//! frame totals as the result and the phase table as a diagnostic.
//!
//! ```sh
//! cargo test --release -p machine-sinclair-zx-spectrum-48k \
//!     --test contention_oracle -- --ignored --nocapture
//! ```

use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::ula::Ula;
use machine_sinclair_zx_spectrum_48k::Spectrum48k;

const ROM_PATH_ENV: &str = "EMU198X_SPECTRUM_48K_ROM";

/// T-states in a 48K frame.
const FRAME_TSTATES: u32 = 69888;
/// First T-state at which contention applies.
///
/// **Not** the first display *fetch* — contention opens one T-state
/// earlier. Verified against FUSE's `spectrum_contend_delay_65432100`
/// frame-wide by `matches_fuse_contention_across_the_whole_frame`: at
/// 14336 the two models disagreed at 21,504 of 69,888 T-states in a
/// clean one-T-state lag; at 14335 they agree everywhere.
///
/// Our own 128K ULA already notes the distinction — "contention follows
/// /Border rather than the later video-fetch window" — while the 48K
/// Ferranti gates on `e.video`, the fetch window. That is the same
/// conflation the floating-bus pattern made.
const FIRST_DISPLAY: u32 = 14335;
/// T-states per scan line.
const PER_LINE: u32 = 224;
/// Display lines that carry contention.
const DISPLAY_LINES: u32 = 192;
/// Contended T-states at the start of each display line.
const CONTENDED_PER_LINE: u32 = 128;
/// The canonical delay pattern across an 8-T-state contention slot.
const PATTERN: [u32; 8] = [6, 5, 4, 3, 2, 1, 0, 0];

/// C0 cycles (half T-states) in one contention period.
const C0_PERIOD: usize = 16;

/// The control edge uses the upcoming counter phase. SpecIde independently
/// asserts its delay mask at physical pixels 3..14; Smith's C2|C3 expression
/// is high at phases 4..15. This maps the observation conventions, without
/// claiming identical HDL scheduling semantics.
const CONTROL_PHASE_LEAD: usize = 1;

/// Is the wait asserted at canonical C0 index `c0`?
///
/// Straight from `CLKWAIT = C3 + C2` (Smith Chapter 18, p. 192): `C2` is
/// bit 2 of the master counter and toggles every 4 C0 cycles, `C3` is bit
/// 3 and toggles every 8, so the wait is low only while both are low —
/// 4 free cycles in every 16, matching the "6 of every 8 C0 cycles"
/// figure and the two zero entries of `[6,5,4,3,2,1,0,0]`.
const fn clkwait_asserted(c0: usize) -> bool {
    let c2 = (c0 >> 2) & 1;
    let c3 = (c0 >> 3) & 1;
    c2 == 1 || c3 == 1
}

/// C0 cycles an access arriving at `c0` waits before the gate frees it.
fn wait_c0(c0: usize) -> usize {
    (0..C0_PERIOD)
        .find(|d| !clkwait_asserted((c0 + d) % C0_PERIOD))
        .unwrap_or(0)
}

/// Canonical cost of an instruction arriving at canonical C0 index
/// `c0`, in whole T-states.
///
/// The walk runs in C0 cycles because that is the resolution the gate
/// works at: Smith has the ULA hold the clock high during the *first
/// half* of `T1`, with wait transitions aligned to the negative edge of
/// C0. A whole-T-state walk cannot represent that, which is why the
/// earlier version of this function mis-scored every multi-M-cycle
/// instruction by exactly one T-state — a second M-cycle can land on
/// either C0 parity, and half a cycle can only be spent as a whole one.
/// That rounding is the final `div_ceil`, and it is the whole point.
///
/// **Known residual.** This gets all five multi-M-cycle cases exact and
/// leaves the two single-M-cycle anchors one T-state high. The leading
/// explanation is that the walk treats the wait as level-sensitive on
/// every C0 cycle, whereas the gate only evaluates while
/// `z80_clock_high` — so contention fires on alternate C0 cycles, and an
/// arrival on the other parity waits up to two C0 less than this model
/// charges. That term is not added here on purpose: three formulations
/// have now been tried, each fixing one group and breaking the other,
/// and adding a fourth constant without an independent way to pin it
/// would be fitting the model to the answer.
fn canonical_cost_c0(mut c0: usize, mcycles: &[u32]) -> u32 {
    let mut cost = 0usize;
    for length in mcycles {
        let wait = wait_c0(c0);
        let span = wait + (*length as usize) * 2;
        cost += span;
        c0 = (c0 + span) % C0_PERIOD;
    }
    (cost as u32).div_ceil(2)
}

/// Contended RAM: the whole lower 16K.
const CODE_BASE: u16 = 0x4000;
const CODE_END: u16 = 0x8000;

/// Delay a contended M-cycle starting at frame T-state `t` incurs.
///
/// Wraps, because a cost walk starting late in the frame runs off the end
/// of it and the raster does not stop there.
fn delay_at(t: u32) -> u32 {
    let t = t % FRAME_TSTATES;
    if t < FIRST_DISPLAY {
        return 0;
    }
    let into_display = t - FIRST_DISPLAY;
    if into_display / PER_LINE >= DISPLAY_LINES {
        return 0;
    }
    let in_line = into_display % PER_LINE;
    if in_line >= CONTENDED_PER_LINE {
        return 0;
    }
    PATTERN[(in_line % 8) as usize]
}

/// Instructions the canonical model completes in one frame, given an
/// instruction's M-cycle lengths — contention charged once per M-cycle.
fn canonical_per_frame(mcycles: &[u32]) -> u64 {
    let mut t = 0u32;
    let mut retired = 0u64;
    while t < FRAME_TSTATES {
        for length in mcycles {
            t += delay_at(t);
            t += length;
        }
        retired += 1;
    }
    retired
}

/// One instruction under test.
struct Case {
    name: &'static str,
    /// Bytes of the instruction, repeated to fill contended RAM.
    bytes: &'static [u8],
    /// M-cycle lengths, in order. Contention applies at each start.
    mcycles: &'static [u32],
    /// Register setup applied before the measured frame.
    setup: fn(&mut Spectrum48k),
}

fn cases() -> Vec<Case> {
    vec![
        // Anchors: single M-cycle, already known correct. If either of
        // these diverges the model is wrong, not the engine.
        Case {
            name: "NOP",
            bytes: &[0x00],
            mcycles: &[4],
            setup: |_| {},
        },
        Case {
            name: "INC BC",
            bytes: &[0x03],
            mcycles: &[6],
            setup: |_| {},
        },
        // Two M-cycles: fetch plus one contended read.
        Case {
            name: "LD A,(HL)",
            bytes: &[0x7E],
            mcycles: &[4, 3],
            setup: |m| m.z80_mut().regs.hl = 0x5000,
        },
        // Four and five M-cycles, to show how the error scales with the
        // number of memory cycles rather than with the instruction's
        // length. Operand bytes address $5000, itself contended.
        Case {
            name: "LD A,(nn)",
            bytes: &[0x3A, 0x00, 0x50],
            mcycles: &[4, 3, 3, 3],
            setup: |_| {},
        },
        Case {
            name: "LD HL,(nn)",
            bytes: &[0x2A, 0x00, 0x50],
            mcycles: &[4, 3, 3, 3, 3],
            setup: |_| {},
        },
        // `IN A,(n)` — M1, operand fetch, then a 4-T-state IO cycle (the
        // Z80 inserts one automatic wait state in IO). Port `$FF` is the
        // floating-bus port floatspy and Float48K both read.
        Case {
            name: "IN A,(0FFh)",
            bytes: &[0xDB, 0xFF],
            mcycles: &[4, 3, 4],
            setup: |_| {},
        },
        // Six M-cycles. This is the shape the failing suite cases use.
        Case {
            name: "LD BC,(nn)",
            bytes: &[0xED, 0x4B, 0x00, 0x50],
            mcycles: &[4, 4, 3, 3, 3, 3],
            setup: |_| {},
        },
        // Same shape, writing rather than reading. BC is preloaded with
        // the two bytes already at $5000 so the write is value-neutral
        // and the instruction stream stays intact.
        Case {
            name: "LD (nn),BC",
            bytes: &[0xED, 0x43, 0x00, 0x50],
            mcycles: &[4, 4, 3, 3, 3, 3],
            setup: |m| m.z80_mut().regs.bc = 0x43ED,
        },
    ]
}

fn rom_bytes() -> Option<Vec<u8>> {
    let path = std::env::var(ROM_PATH_ENV).ok()?;
    std::fs::read(path).ok()
}

/// A machine filled with the case's instruction, aligned to a frame
/// boundary, with the CPU aimed at contended RAM.
fn prepare(case: &Case, rom: &[u8]) -> Spectrum48k {
    prepare_at(case, rom, 0)
}

/// The same, started `skew` T-states past the frame boundary.
///
/// The skew is what gives arrival-T-state coverage. An instruction whose
/// cost is a multiple of the pattern's period revisits the same handful of
/// arrival phases forever; sweeping the start position walks the rest.
fn prepare_at(case: &Case, rom: &[u8], skew: u32) -> Spectrum48k {
    let mut machine = Spectrum48k::new();
    machine.load_rom_bytes(rom).expect("48K ROM should load");
    machine.reset();

    // Fill all contended RAM with the instruction under test, so every
    // fetch and every operand access is contended.
    let mut addr = CODE_BASE;
    let mut index = 0usize;
    while addr < CODE_END {
        machine
            .memory_mut()
            .write(addr, case.bytes[index % case.bytes.len()]);
        index += 1;
        addr += 1;
    }

    // Align to a frame boundary so the measured window is exactly one
    // frame of contention, then aim the CPU at the filled region. The ROM
    // never runs, so IFF1 stays clear and no interrupt perturbs the count.
    while machine.tstate_in_frame() != 0 {
        machine.advance_tstates(1);
    }
    machine.advance_tstates(skew);
    machine.z80_mut().regs.pc = CODE_BASE;
    (case.setup)(&mut machine);
    machine
}

/// Run one case for exactly one frame and return instructions retired.
fn measure(case: &Case, rom: &[u8]) -> u64 {
    let mut machine = prepare(case, rom);
    let before = machine.z80().instructions_retired();
    machine.advance_tstates(FRAME_TSTATES);
    machine.z80().instructions_retired() - before
}

/// Advance until one more instruction retires; returns its cost in
/// T-states. Stepping a T-state at a time is what makes the cost exact —
/// the retired counter is the only reliable instruction boundary.
fn step_one_instruction(machine: &mut Spectrum48k) -> u32 {
    let target = machine.z80().instructions_retired() + 1;
    let start = machine.tstate_in_frame();
    let mut cost = 0u32;
    while machine.z80().instructions_retired() < target {
        machine.advance_tstates(1);
        cost += 1;
        assert!(cost <= 512, "instruction should retire within 512 T-states");
    }
    debug_assert_eq!(
        cost,
        (machine.tstate_in_frame() + FRAME_TSTATES - start) % FRAME_TSTATES
    );
    cost
}

#[test]
#[ignore = "DIAGNOSTIC: diagnostic harness; needs EMU198X_SPECTRUM_48K_ROM"]
fn contention_matches_the_canonical_model_per_instruction() {
    let Some(rom) = rom_bytes() else {
        panic!("set {ROM_PATH_ENV} to the 48K ROM to run this harness");
    };

    println!(
        "\n{:<12} {:>3} {:>9} {:>9} {:>8} {:>9}  M-cycles",
        "instruction", "Ms", "canonical", "measured", "excess", "extra T"
    );
    println!("{}", "-".repeat(76));

    let mut ran = 0;
    for case in cases() {
        let canonical = canonical_per_frame(case.mcycles);
        let measured = measure(&case, &rom);
        // Fewer instructions retired means more time lost to waits. The
        // T-state form is the diagnostic one: it is comparable across
        // instructions of different length, where the percentage is not.
        let excess = (canonical as f64 - measured as f64) / canonical as f64 * 100.0;
        let extra_t =
            FRAME_TSTATES as f64 / measured as f64 - FRAME_TSTATES as f64 / canonical as f64;
        println!(
            "{:<12} {:>3} {:>9} {:>9} {:>7.1}% {:>9.3}  {:?}",
            case.name,
            case.mcycles.len(),
            canonical,
            measured,
            excess,
            extra_t,
            case.mcycles
        );
        ran += 1;
    }

    assert_eq!(ran, cases().len(), "every case should have been measured");
}

/// Cost by the arrival phase the *gate itself* uses.
///
/// An earlier version of this derived the phase from `tstate_in_frame`
/// and produced a table that could not be reconciled with the frame
/// totals. The reason is that the gate indexes `DELAY_TABLE_48K` by the
/// ULA's pixel counter, whose origin need not coincide with the frame
/// T-state origin — and the known one-T-state `Float48K` discrepancy
/// lives in exactly that gap. Reading the engine's own phase and `video`
/// flag through `debug_raster()` removes the assumption rather than
/// guessing at it, and needs no instrumentation.
///
/// Sampling is confined to instructions that begin and end inside the
/// video window, so no sample straddles a boundary and the canonical
/// cost is a pure phase walk.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic harness; needs EMU198X_SPECTRUM_48K_ROM"]
fn contention_cost_by_arrival_phase() {
    let Some(rom) = rom_bytes() else {
        panic!("set {ROM_PATH_ENV} to the 48K ROM to run this harness");
    };

    for case in cases() {
        let mut machine = prepare(&case, &rom);

        // `[(count, measured_sum, canonical_sum, diff_min, diff_max)]`.
        let mut by_phase = [(0u32, 0u32, 0u32, i64::MAX, i64::MIN); 8];
        let mut spent = 0u32;
        while spent < FRAME_TSTATES {
            let (_, pixel, video, _, _) = machine.ula().debug_raster();
            // Canonical C0 index, and the T-state phase it falls in.
            let c0 = ((pixel as usize) + CONTROL_PHASE_LEAD) % C0_PERIOD;
            let phase = c0 / 2;
            let canonical = canonical_cost_c0(c0, case.mcycles);
            let measured = step_one_instruction(&mut machine);
            spent += measured;
            let (_, _, still_video, _, _) = machine.ula().debug_raster();
            if !video || !still_video {
                continue;
            }
            let diff = measured as i64 - canonical as i64;
            let slot = &mut by_phase[phase];
            slot.0 += 1;
            slot.1 += measured;
            slot.2 += canonical;
            slot.3 = slot.3.min(diff);
            slot.4 = slot.4.max(diff);
        }

        println!("\n{}  {:?}", case.name, case.mcycles);
        println!(
            "{:>5} {:>6} {:>7} {:>10} {:>10} {:>8} {:>12}",
            "phase", "delay", "samples", "canonical", "measured", "diff", "diff range"
        );
        println!("{}", "-".repeat(66));
        for (phase, (count, measured, canonical, lo, hi)) in by_phase.iter().enumerate() {
            if *count == 0 {
                println!(
                    "{phase:>5} {:>6} {:>7} {:>10} {:>10} {:>8} {:>12}",
                    PATTERN[phase], 0, "-", "-", "-", "-"
                );
                continue;
            }
            let m = *measured as f64 / *count as f64;
            let c = *canonical as f64 / *count as f64;
            println!(
                "{phase:>5} {:>6} {count:>7} {c:>10.2} {m:>10.2} {:>+8.2} {:>12}",
                PATTERN[phase],
                m - c,
                format!("{lo:+} .. {hi:+}")
            );
        }
    }
}

/// The instrument has to be checked before its readings mean anything.
/// Stepping a T-state at a time must produce exactly the same execution
/// as one bulk advance: same instruction count, and per-instruction costs
/// that sum to the frame. If the driver batched work per call, or the
/// retired counter lagged, the per-phase table would be measuring the
/// harness rather than the engine.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic harness; needs EMU198X_SPECTRUM_48K_ROM"]
fn stepping_one_tstate_agrees_with_a_bulk_advance() {
    let Some(rom) = rom_bytes() else {
        panic!("set {ROM_PATH_ENV} to the 48K ROM to run this harness");
    };

    for case in cases() {
        let bulk = measure(&case, &rom);

        let mut machine = prepare(&case, &rom);
        let mut stepped = 0u64;
        let mut spent = 0u32;
        while spent < FRAME_TSTATES {
            spent += step_one_instruction(&mut machine);
            stepped += 1;
        }

        // The loop runs one instruction past the frame, so `spent`
        // overshoots by that instruction's cost and `stepped` counts it.
        // `bulk` counts instructions *retired* inside the frame, so the
        // straddler is the one legitimate unit of difference.
        let overshoot = spent - FRAME_TSTATES;
        println!(
            "{:<12} bulk={bulk:>6} stepped={stepped:>6} overshoot={overshoot:>3}T",
            case.name
        );
        assert!(
            overshoot < 64,
            "{}: overshoot {overshoot} should be one instruction's worth",
            case.name
        );
        assert!(
            stepped == bulk || stepped == bulk + 1,
            "{}: stepping executed {stepped} against a bulk {bulk}; they must \
             agree bar the instruction straddling the frame boundary",
            case.name
        );
    }
}

/// Half-open trace of one instruction, T-state by T-state.
///
/// This is what turned "multi-M-cycle instructions cost one slot too
/// many" into a mechanism. It shows the operand read at `$5000` stalled
/// *twice*: once before the access commits, and again immediately after,
/// because `MREQ` deasserts while the contended address is still on the
/// bus and the gate re-arms. `M1` escapes only because what follows its
/// access is the refresh cycle, whose address is uncontended — which is
/// why single-M-cycle instructions come out exact.
///
/// Kept because the gate's behaviour is easier to argue about from a
/// trace than from a pass count, and because any change to the gate
/// should be read here first.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic harness; needs EMU198X_SPECTRUM_48K_ROM"]
fn trace_one_instruction() {
    let Some(rom) = rom_bytes() else {
        panic!("set {ROM_PATH_ENV} to the 48K ROM to run this harness");
    };
    // Which instruction to trace; defaults to the two-M-cycle case that
    // first exposed the double stall.
    let want = std::env::var("EMU198X_TRACE_CASE").unwrap_or_else(|_| "LD A,(HL)".to_string());
    let case = cases()
        .into_iter()
        .find(|c| c.name == want)
        .unwrap_or_else(|| panic!("no such case: {want}"));
    let mut machine = prepare(&case, &rom);

    // Settle into the video window, staying inside this frame.
    //
    // An earlier version waited for a specific arrival phase, which is
    // fine for an instruction that locks onto it and a trap for one that
    // does not: `NOP` never visits phase 1, so the loop ran past the
    // frame boundary, took the interrupt the ROM enables during
    // alignment, and traced the ISR instead. The trace looked plausible —
    // uncontended addresses and a clock that never stalls — which is
    // exactly why the guard below exists.
    let mut spent = 0u32;
    loop {
        let (_, _, video, _, _) = machine.ula().debug_raster();
        if video && spent > FRAME_TSTATES / 4 {
            break;
        }
        spent += step_one_instruction(&mut machine);
        assert!(
            spent < FRAME_TSTATES,
            "settle ran past the frame boundary without finding the video window"
        );
    }
    assert!(
        (CODE_BASE..CODE_END).contains(&machine.z80().regs.pc),
        "trace left the instruction stream (pc {:#06x}) — it would be \
         tracing the ROM, not the case under test",
        machine.z80().regs.pc
    );

    println!("\n  T  pixel ph  vid clk   addr  mreq iorq rd wr m1 rfsh    pc");
    for t in 0..26 {
        let (_, pixel, video, _, _) = machine.ula().debug_raster();
        let clk = machine.ula().cpu_clock_active();
        let z = machine.z80();
        println!(
            "{t:>3} {:>6} {:>2} {:>4} {:>3}  {:#06x} {:>5} {:>4} {:>2} {:>2} {:>2} {:>4}  {:#06x}",
            pixel & 0x0F,
            (pixel & 0x0F) / 2,
            video as u8,
            clk as u8,
            z.addr,
            z.mreq as u8,
            z.iorq as u8,
            z.rd as u8,
            z.wr as u8,
            z.m1 as u8,
            z.rfsh as u8,
            z.regs.pc
        );
        machine.advance_tstates(1);
    }
}

/// Does the measured frame actually execute the instruction under test?
///
/// The trace showed the CPU sitting in ROM at `$11d2`, which would mean
/// the oracle had been scoring the ROM rather than the filled stream.
/// This samples the PC across a measured frame and reports how much of
/// it ran inside contended RAM.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic harness; needs EMU198X_SPECTRUM_48K_ROM"]
fn measured_frame_runs_the_instruction_under_test() {
    let Some(rom) = rom_bytes() else {
        panic!("set {ROM_PATH_ENV} to the 48K ROM to run this harness");
    };

    for case in cases() {
        let mut machine = prepare(&case, &rom);
        let (mut inside, mut outside) = (0u32, 0u32);
        let mut spent = 0u32;
        let mut first_escape = None;
        while spent < FRAME_TSTATES {
            let pc = machine.z80().regs.pc;
            if (CODE_BASE..CODE_END).contains(&pc) {
                inside += 1;
            } else {
                outside += 1;
                if first_escape.is_none() {
                    first_escape = Some((spent, pc));
                }
            }
            spent += step_one_instruction(&mut machine);
        }
        let total = inside + outside;
        println!(
            "{:<12} in-RAM {inside:>6}/{total:<6} ({:>5.1}%)  first escape: {:?}",
            case.name,
            inside as f64 / total as f64 * 100.0,
            first_escape
        );
    }
}

// ---------------------------------------------------------------------
// The arrival-resolved memory differential.
//
// Everything above this line is report-only or compares two *models*.
// That is the gap this section closes, and it is not a theoretical one:
// a global one-T-state phase shift in the engine's contention charge
// survived a whole phase of work because nothing here could fail on it.
// Frame totals are structurally blind to phase — the contended window is
// sixteen whole 8-T-state groups, so a walk that starts one T-state late
// retires the same number of instructions — and
// `contention_cost_by_arrival_phase` prints a table nobody has to read.
// ---------------------------------------------------------------------

/// FUSE's first bus byte is 14338; the physical counter exposes it at
/// T=4 (C=8). Transaction coordinates therefore add 14338-4.
/// This mapping is separate from the /INT pin edge.
const ORIGIN: i32 = 14334;

/// FUSE's cost for an instruction arriving at FUSE frame T-state `t`.
///
/// `tstates += delay[tstates]; tstates += length`, once per M-cycle —
/// FUSE's whole memory contention model, and the same walk
/// `canonical_per_frame` uses. What is new here is that the arrival
/// T-state is the engine's own, mapped through `ORIGIN`, rather than a
/// walk of the model against itself.
fn fuse_cost(t: u32, mcycles: &[u32]) -> u32 {
    let mut now = t;
    for length in mcycles {
        now += delay_at(now);
        now += length;
    }
    now - t
}

/// `(arrival T-state, measured cost)` for a frame's worth of one case.
///
/// The first two instructions of each pass are discarded: aiming `PC` at
/// the stream lands mid-M-cycle whenever the skew does, so the first
/// retirement is the tail of whatever the CPU was already doing.
fn arrival_samples(case: &Case, rom: &[u8], skew: u32) -> Vec<(u32, u32)> {
    let mut machine = prepare_at(case, rom, skew);
    for _ in 0..2 {
        step_one_instruction(&mut machine);
    }
    let mut out = Vec::new();
    let mut spent = 0u32;
    while spent < FRAME_TSTATES {
        let arrival = machine.tstate_in_frame();
        let pc = machine.z80().regs.pc;
        assert!(
            (CODE_BASE..CODE_END).contains(&pc),
            "{}: execution left the instruction stream at pc {pc:#06x} — an \
             interrupt or a stray jump would make every later sample a \
             measurement of the ROM",
            case.name
        );
        let cost = step_one_instruction(&mut machine);
        out.push((arrival, cost));
        spent += cost;
    }
    out
}

/// A scored case: name, M-cycle lengths, and its raw arrival samples.
///
/// Named for the same reason `io_contention_oracle`'s `Scored` is: the
/// tuple is passed through four separate reporting passes, and spelling it
/// out at each one is where a field silently swaps places.
type Scored = (&'static str, &'static [u32], Vec<(u32, u32)>);

fn mismatches(samples: &[(u32, u32)], mcycles: &[u32], offset: i32) -> usize {
    samples
        .iter()
        .filter(|&&(arrival, measured)| {
            let t = (arrival as i32 + offset).rem_euclid(FRAME_TSTATES as i32) as u32;
            fuse_cost(t, mcycles) != measured
        })
        .count()
}

/// Physical IRQ edge and pulse width, independent of FUSE pattern coordinates.
#[test]
#[ignore = "FIXTURE: needs EMU198X_SPECTRUM_48K_ROM"]
fn the_interrupt_pin_is_pinned_in_master_ticks() {
    use common_sinclair_zx_spectrum::ula::Ula;
    let mut machine = Spectrum48k::new();
    let mut edges = Vec::new();
    let mut previous = false;
    for tick in 0..FRAME_TSTATES * 4 {
        machine.advance_halfcycles(1);
        let active = machine.ula().interrupt_active();
        if active != previous {
            edges.push((tick, active));
        }
        previous = active;
    }
    // Counter phase 1, before advance; 32-T pulse.
    assert_eq!(edges, [(55_552 * 4 + 2, true), (55_584 * 4 + 2, false)]);
}

/// Memory contention against FUSE, at every arrival T-state in the frame.
///
/// The counterpart of `io_contention_oracle`'s differential for a plain
/// contended memory access, and the instrument the I/O side's offset sweep
/// asked for. Six instruction shapes, one to six M-cycles, all executing
/// out of contended RAM and reading contended RAM, each scored against
/// FUSE's per-M-cycle walk at the `/INT`-pinned origin.
///
/// The offset sweep is printed for the same reason the I/O one is: a
/// disagreement between the fitted winner and the pinned origin is itself
/// a finding — it says the engine's contention phase has moved against its
/// own interrupt — and is never a reason to rescore.
#[test]
#[ignore = "FIXTURE: differential harness; needs EMU198X_SPECTRUM_48K_ROM"]
fn memory_contention_matches_fuse_at_every_arrival_tstate() {
    let Some(rom) = rom_bytes() else {
        panic!("set {ROM_PATH_ENV} to the 48K ROM to run this harness");
    };

    // Skews of 0..8 walk the arrival point across a whole period of the
    // 8-T-state pattern, so no phase goes unvisited even for an
    // instruction whose cost divides it.
    const SKEWS: u32 = 8;

    // `IN A,(n)` is excluded: its third M-cycle is a port cycle, which
    // FUSE charges through `ula_contend_port_early`/`_late` rather than
    // through the memory walk. It belongs to `io_contention_oracle`, and
    // mixing it in here would let a known-red I/O gate mask the memory
    // result this test exists to isolate.
    let cases: Vec<Case> = cases()
        .into_iter()
        .filter(|c| !c.name.starts_with("IN "))
        .collect();

    let collected: Vec<Scored> = cases
        .iter()
        .map(|case| {
            let mut all = Vec::new();
            for skew in 0..SKEWS {
                all.extend(arrival_samples(case, &rom, skew));
            }
            (case.name, case.mcycles, all)
        })
        .collect();

    let score = |offset: i32| -> usize {
        collected
            .iter()
            .map(|(_, mcycles, all)| mismatches(all, mcycles, offset))
            .sum()
    };

    let total = score(ORIGIN);
    let samples_total: usize = collected.iter().map(|(_, _, all)| all.len()).sum();
    println!("\norigin offset {ORIGIN:+} — {total} of {samples_total} samples disagree");

    // The neighbourhood, so it is visible whether the pinned origin sits
    // in a plateau or next to a sharp minimum it is not on.
    print!("\n{:<14}", "offset");
    for d in -4..=4i32 {
        print!("{:>9}", format!("{:+}", ORIGIN + d));
    }
    println!();
    print!("{:<14}", "mismatches");
    for d in -4..=4i32 {
        print!("{:>9}", score(ORIGIN + d));
    }
    println!();

    // Per case, so a shape-dependent error is separable from a global
    // one. A phase shift hits every case; a missing M-cycle hits the long
    // ones only.
    println!(
        "\n{:<14} {:>3} {:>9} {:>9} {:>7}  first divergences (t: got/want)",
        "instruction", "Ms", "samples", "wrong", "%"
    );
    println!("{}", "-".repeat(88));
    for (name, mcycles, all) in &collected {
        let wrong: Vec<_> = all
            .iter()
            .filter_map(|&(arrival, measured)| {
                let t = (arrival as i32 + ORIGIN).rem_euclid(FRAME_TSTATES as i32) as u32;
                let want = fuse_cost(t, mcycles);
                (want != measured).then_some((t, measured, want))
            })
            .collect();
        let head: Vec<String> = wrong
            .iter()
            .take(3)
            .map(|(t, got, want)| format!("{t}: {got}/{want}"))
            .collect();
        println!(
            "{name:<14} {:>3} {:>9} {:>9} {:>6.1}%  {}",
            mcycles.len(),
            all.len(),
            wrong.len(),
            wrong.len() as f64 / all.len() as f64 * 100.0,
            head.join("  ")
        );
    }

    // Where the disagreement sits, by the delay the raster owed at the
    // arrival T-state. A gate one T-state out of phase is wrong at the
    // two ends of the pattern and right in the middle, which reads very
    // differently from a gate that charges the wrong amount everywhere.
    println!(
        "\n{:<14} {}",
        "instruction",
        (0..8)
            .map(|p| format!("{:>7}", format!("d={}", PATTERN[p])))
            .collect::<String>()
    );
    println!("{}", "-".repeat(72));
    for (name, mcycles, all) in &collected {
        let mut wrong = [0u32; 8];
        let mut seen = [0u32; 8];
        for &(arrival, measured) in all {
            let t = (arrival as i32 + ORIGIN).rem_euclid(FRAME_TSTATES as i32) as u32;
            if delay_at(t) == 0 && delay_at(t + 4) == 0 {
                continue;
            }
            let slot = ((t + FRAME_TSTATES - FIRST_DISPLAY) % 8) as usize;
            seen[slot] += 1;
            if fuse_cost(t, mcycles) != measured {
                wrong[slot] += 1;
            }
        }
        print!("{name:<14} ");
        for p in 0..8 {
            print!("{:>6.0}%", wrong[p] as f64 / seen[p].max(1) as f64 * 100.0);
        }
        println!();
    }

    // The self-check. Outside the contended window neither model charges
    // anything, so every case must cost exactly its uncontended length.
    // If this fails the harness is measuring something other than the
    // instruction it thinks it is, and nothing above it means anything.
    for (name, mcycles, all) in &collected {
        let bare: u32 = mcycles.iter().sum();
        let quiet_wrong = all
            .iter()
            .filter(|&&(arrival, _)| {
                let start = (arrival as i32 + ORIGIN).rem_euclid(FRAME_TSTATES as i32) as u32;
                (0..FIRST_DISPLAY.saturating_sub(64)).contains(&start)
            })
            .filter(|&&(_, measured)| measured != bare)
            .count();
        assert_eq!(
            quiet_wrong, 0,
            "{name}: {quiet_wrong} samples outside the contended window did \
             not cost the bare {bare} T-states"
        );
    }

    // The ratchet, and it goes last so that a red run still prints
    // everything above it. `io_contention_oracle` asserted before its
    // diagnostics for a whole phase, which is how a sharp minimum one
    // T-state off the pinned origin went unread.
    //
    // A ceiling, not a target. Lower it in the commit that earns it;
    // never raise it.
    //
    // 18 of 370,024, from 88,871. The window's phase closed most of it
    // and the window's *edge* closed the rest: every remaining
    // disagreement at 9,858 was within a couple of T-states of a display
    // line's boundary, because the engine gated contention on the fetch
    // window rather than on the sixteen whole fetch cycles the HDL's
    // `Border_n` spans.
    //
    // The eighteen that are left are harness residue rather than engine
    // defects: each reports a cost far below the instruction's
    // uncontended length — 8 T-states for a four-M-cycle `LD A,(nn)` —
    // which is the tail of an M-cycle already in flight when the pass
    // started. Two discarded instructions is not always enough at every
    // skew. Left visible rather than tuned away, because raising the
    // discard to silence them would also silence a real short cost.
    const RATCHET: usize = 18;
    assert!(
        total <= RATCHET,
        "memory contention regressed against FUSE: {total} of {samples_total} \
         samples disagree, was {RATCHET}. If this change is right and the \
         reference is wrong, say so explicitly and move the ratchet in the \
         same commit — do not widen it silently."
    );
    if total < RATCHET {
        println!("\nRATCHET: {total} of {samples_total} — improved on {RATCHET}.");
    }
}

/// Our canonical contention model must agree with FUSE's frame-wide.
///
/// The oracle scores the engine against `delay_at`. If that reference is
/// itself wrong, "exact" means nothing — which is precisely what happened
/// while `FIRST_DISPLAY` was 14336.
#[test]
fn matches_fuse_contention_across_the_whole_frame() {
    // FUSE spec48 wires `spectrum_contend_delay_65432100`: pattern
    // {5,4,3,2,1,0,0,6} at offset 1, which rotated is our
    // [6,5,4,3,2,1,0,0]. Geometry from `machine.c`, where
    // `line_times[0] = top_left_pixel - 24*224 - 16 = 8944`.
    const FUSE_PATTERN: [u32; 8] = [5, 4, 3, 2, 1, 0, 0, 6];
    const LINE_TIMES_0: u32 = 8944;
    const LEFT_BORDER: u32 = 24;
    const HORIZONTAL_SCREEN: u32 = 128;
    const OFFSET: u32 = 1;
    const BORDER_HEIGHT: u32 = 24;

    fn fuse_delay(t: u32) -> u32 {
        if t < LINE_TIMES_0 {
            return 0;
        }
        let line = (t - LINE_TIMES_0) / PER_LINE;
        if !(BORDER_HEIGHT..BORDER_HEIGHT + DISPLAY_LINES).contains(&line) {
            return 0;
        }
        let through = (t - LINE_TIMES_0 + (LEFT_BORDER - 16)) % PER_LINE;
        if !(LEFT_BORDER - OFFSET..LEFT_BORDER + HORIZONTAL_SCREEN - OFFSET).contains(&through) {
            return 0;
        }
        FUSE_PATTERN[(through % 8) as usize]
    }

    let mismatches: Vec<_> = (0..FRAME_TSTATES)
        .filter(|&t| delay_at(t) != fuse_delay(t))
        .take(8)
        .collect();
    assert!(
        mismatches.is_empty(),
        "canonical contention disagrees with FUSE at {:?}...",
        mismatches
    );
}
