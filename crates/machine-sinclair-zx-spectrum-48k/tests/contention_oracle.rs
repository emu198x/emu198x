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

/// Offset from the ULA's pixel counter to the canonical C0 index.
///
/// This is the Seam 1 alignment, and it is the one number here that the
/// reference does not fix. Smith derives the wait as `CLKWAIT = C3 + C2`
/// and states the ULA counter and the Z80 T-state grid are offset by half
/// a C0 cycle, but Chapter 18 explicitly declines to pin the counter's
/// absolute phase — it requires aligning to a named test program, and
/// the wiki decision names `Float48K`. The engine's own `Float48K`
/// reading is one T-state off the published figure, which is the same
/// ambiguity seen from the other end.
///
/// The value is not fitted to the instruction costs. With this offset
/// the derived table below reproduces the gate's *measured* delay table
/// exactly — `effective_delay_table` in `ferranti-ula-6c001e` records
/// 12, 11 and 0 C0 cycles at pixel phases 3, 4 and 15, and the
/// derivation gives the same — so it is pinned by a measurement of the
/// table rather than by the numbers it is later used to predict.
const SEAM1_C0_ORIGIN: usize = 1;

// No separate sampling correction is applied, and that is deliberate.
//
// The whole-T-state version of this harness needed one, because it
// calibrated its phase against instruction costs. `SEAM1_C0_ORIGIN` is
// instead pinned by `effective_delay_table`, which samples the gate the
// same way this harness does — between ticks — so the sampling
// convention is already inside the origin. Adding a second correction
// on top double-counts it; measured, it moves every case one T-state
// the wrong way.

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
fn delay_at(t: u32) -> u32 {
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
#[ignore = "diagnostic harness; needs EMU198X_SPECTRUM_48K_ROM"]
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
#[ignore = "diagnostic harness; needs EMU198X_SPECTRUM_48K_ROM"]
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
            let c0 = ((pixel as usize) + SEAM1_C0_ORIGIN) % C0_PERIOD;
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
#[ignore = "diagnostic harness; needs EMU198X_SPECTRUM_48K_ROM"]
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
#[ignore = "diagnostic harness; needs EMU198X_SPECTRUM_48K_ROM"]
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
#[ignore = "diagnostic harness; needs EMU198X_SPECTRUM_48K_ROM"]
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
