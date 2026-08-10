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
/// First T-state of the display area.
const FIRST_DISPLAY: u32 = 14336;
/// T-states per scan line.
const PER_LINE: u32 = 224;
/// Display lines that carry contention.
const DISPLAY_LINES: u32 = 192;
/// Contended T-states at the start of each display line.
const CONTENDED_PER_LINE: u32 = 128;
/// The canonical delay pattern across an 8-T-state contention slot.
const PATTERN: [u32; 8] = [6, 5, 4, 3, 2, 1, 0, 0];

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
            // The gate's own index, in T-states rather than half-cycles.
            let phase = ((pixel & 0x0F) / 2) as usize;
            let canonical = canonical_cost_from_phase(phase, case.mcycles);
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

/// Canonical cost of an instruction arriving at contention phase
/// `phase`, entirely inside the video window: each M-cycle waits
/// `PATTERN[phase]`, then runs, and the phase advances by both.
fn canonical_cost_from_phase(mut phase: usize, mcycles: &[u32]) -> u32 {
    let mut cost = 0u32;
    for length in mcycles {
        let delay = PATTERN[phase & 7];
        cost += delay + length;
        phase = (phase + (delay + length) as usize) & 7;
    }
    cost
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
    let case = cases()
        .into_iter()
        .find(|c| c.name == "LD A,(HL)")
        .expect("LD A,(HL) should be one of the cases");
    let mut machine = prepare(&case, &rom);

    // Settle into the video window and onto the locked phase.
    let mut spent = 0u32;
    loop {
        let (_, pixel, video, _, _) = machine.ula().debug_raster();
        if video && (pixel & 0x0F) / 2 == 1 && spent > 20000 {
            break;
        }
        spent += step_one_instruction(&mut machine);
    }

    println!("\n  T  pixel ph  vid clk   addr  mreq rd wr m1 rfsh    pc");
    for t in 0..26 {
        let (_, pixel, video, _, _) = machine.ula().debug_raster();
        let clk = machine.ula().cpu_clock_active();
        let z = machine.z80();
        println!(
            "{t:>3} {:>6} {:>2} {:>4} {:>3}  {:#06x} {:>5} {:>2} {:>2} {:>2} {:>4}  {:#06x}",
            pixel & 0x0F,
            (pixel & 0x0F) / 2,
            video as u8,
            clk as u8,
            z.addr,
            z.mreq as u8,
            z.rd as u8,
            z.wr as u8,
            z.m1 as u8,
            z.rfsh as u8,
            z.regs.pc
        );
        machine.advance_tstates(1);
    }
}
