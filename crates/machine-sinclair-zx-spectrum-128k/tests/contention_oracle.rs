//! 128K memory-contention differential: the engine's measured instruction
//! cost against FUSE's, at every arrival T-state in the frame.
//!
//! The 48K has had this since #862 and the 128K has had nothing. That
//! matters more here than it would on a variant that shared the 48K's
//! geometry, because `sinclair-ula-7k010e` does not: it computes its own
//! contention window from a logical `/Border` coordinate and then carries
//! its own delay-table index offset on top.
//!
//! ```text
//! let contention_pixel = if next_scan < 192 && e.pixel >= 450 {
//!     Some(e.pixel - 450)                       // the /Border coordinate
//! } else if e.scan < 192 && e.pixel < 250 {
//!     Some(e.pixel + 6)
//! } else { None };
//! let phase = contention_pixel
//!     .map(|p| (p + 1 + 16 - HDL_TABLE_ORIGIN_SHIFT) & 0x0F);   // the offset
//! ```
//!
//! **Only their sum has ever been measured.** It was pinned against
//! HALT2INT128, a whole-program pass/fail, and #862 moved one of them and
//! compensated in the other precisely because splitting them would have
//! moved behaviour with nothing to catch it. A frame total cannot see a
//! phase error at all — the contended window is sixteen whole 8-T-state
//! groups, so a walk starting a group late retires exactly as many
//! instructions — and neither can a pass/fail.
//!
//! An arrival-resolved differential can, because it scores the shape of
//! the gate as well as its total.
//!
//! ## What it found
//!
//! **Both constants are right, jointly and individually.** The sweep has a
//! sharp isolated minimum of **17 of 375,406** and everything either side
//! of it costs about 87,000. A window whose sum was right and whose parts
//! were not would show a broad minimum or a displaced one; this shows
//! neither. The sum HALT2INT128 pinned is the sum FUSE wants, and the
//! delay-table index offset is not hiding an error in the `/Border`
//! coordinate.
//!
//! The residual is the same order as the 48K's 18 of 370,024, and it sits
//! the same one T-state below the machine's own `/INT` anchor — on a
//! machine that shares no geometry with the 48K. That is what says the one
//! T-state is the harness's arrival label rather than either engine; see
//! `ARRIVAL_LABEL_LEAD_TSTATES`.
//!
//! Separately, and not a contention result: the 128K's `/INT` edge is two
//! T-states from libspectrum's `top_left_pixel` for this ULA, where the
//! 48K's falls exactly on it.
//!
//! ```sh
//! cargo test --release -p machine-sinclair-zx-spectrum-128k \
//!     --test contention_oracle -- --ignored --nocapture
//! ```
//!
//! ## Why this shape
//!
//! Ported from the 48K's `memory_contention_matches_fuse_at_every_arrival_tstate`,
//! with the geometry re-derived for `timings_frame_ferranti_7c` rather
//! than translated:
//!
//! - The reference is FUSE's own `spectrum_contend_delay_65432100` walk,
//!   transcribed and re-checked frame-wide against its geometry.
//! - Every arrival T-state is scored, so a phase error cannot average out.
//! - The origin is pinned to the `/INT` edge and measured at **half-cycle**
//!   resolution. The 48K's version of that measurement advanced a whole
//!   T-state before sampling and so read the edge one T-state late; see
//!   `machine-sinclair-zx-spectrum-48k`'s `float_bus_oracle`.
//! - Several instruction shapes, so a shape-dependent error separates from
//!   a global one.

use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::timing::TIMING_128K;
use common_sinclair_zx_spectrum::ula::Ula;
use machine_sinclair_zx_spectrum_128k::Spectrum128K;

const ROM0_PATH_ENV: &str = "EMU198X_SPECTRUM_128K_ROM0";
const ROM1_PATH_ENV: &str = "EMU198X_SPECTRUM_128K_ROM1";

/// T-states in a 128K frame.
const FRAME_TSTATES: u32 = 70_908;
/// T-states per scan line.
const PER_LINE: u32 = 228;
/// Display lines that carry contention.
const DISPLAY_LINES: u32 = 192;
/// Contended T-states at the start of each display line.
const CONTENDED_PER_LINE: u32 = 128;
/// The canonical delay pattern across an 8-T-state contention slot.
const PATTERN: [u32; 8] = [6, 5, 4, 3, 2, 1, 0, 0];
/// Master-clock half-cycles per CPU T-state, `TIMING_128K.cpu_divisor`.
const HC_PER_TSTATE: u32 = 5;

/// First T-state at which contention applies on this ULA.
///
/// Derived from FUSE's geometry rather than restated:
/// `timings_frame_ferranti_7c` has `top_left_pixel = 14362` and 228
/// T-states per line, so `line_times[0] = 14362 - 24*228 - 16 = 8874` and
/// the first display line starts at `line_times[24] = 14346`.
/// `spectrum_contend_delay_65432100` passes offset 1, which opens the
/// window fifteen T-states into that line: 14361.
/// `the_delay_table_still_matches_fuse` checks the whole frame rather than
/// this one number.
const FIRST_DISPLAY: u32 = 14_361;

/// Contended RAM: bank 5 at `$4000`, which the 128K pages in at reset and
/// which this harness never re-pages.
const CODE_BASE: u16 = 0x4000;
const CODE_END: u16 = 0x8000;
/// Operand target inside the same contended bank.
const DATA_ADDR: u16 = 0x5000;

/// Add this to an engine frame T-state to get FUSE's.
///
/// FUSE's frame T-state 0 *is* the interrupt, and the engine raises
/// `int_active` at a T-state of its own that owes nothing to the
/// contention gate. `the_origin_is_pinned_by_the_interrupt` measures it
/// here rather than asserting it, and measures it in **half-cycles**.
///
/// That resolution is the whole point. The 48K's version of this constant
/// advanced a full T-state and *then* sampled `interrupt_active()`, so it
/// labelled an edge that fell during T-state *k* as *k+1* and came out one
/// T-state low. Fitting the offset instead would be worse still: a free
/// parameter absorbs exactly the error this differential exists to find,
/// because a gate charging one T-state late is indistinguishable from an
/// origin one T-state early.
///
/// **This is not `top_left_pixel`.** libspectrum gives
/// `timings_frame_ferranti_7c` a `top_left_pixel` of 14362, and the
/// interrupt lands two T-states away from it — where on the 48K the two
/// coincide exactly. See `the_origin_is_pinned_by_the_interrupt` for what
/// that says about `CONFIG_128K.int_start_pixel`, and why it is recorded
/// rather than moved.
const ORIGIN: i32 = 14_364;

/// The harness labels an instruction's arrival one T-state later than the
/// M-cycle FUSE charges, so the score is taken one T-state below `ORIGIN`.
///
/// This is a property of the measurement, not of the engine, and it is
/// named rather than folded into `ORIGIN` because a silently-absorbed
/// T-state is how this problem has gone wrong before. Three things say so:
///
/// - The 48K's differential needs the **same one T-state**, on a machine
///   that shares no geometry with this one — 224-T lines against 228, a
///   different frame length, a different ULA, and a contention window
///   derived a completely different way. A per-machine fudge does not come
///   out the same on both.
/// - Removing it by moving the *engine* instead was tried on the 48K and
///   rejected by both origin-independent oracles: the ZXSpectrum4.net
///   survey went from 13 failing to 17 and floatspy's diff widened from 72
///   pixels to 140.
/// - The residual either side is what a correct gate looks like — 17 here,
///   18 on the 48K — against roughly 87,000 one T-state away in either
///   direction.
const ARRIVAL_LABEL_LEAD_TSTATES: i32 = 1;

/// The offset the differential is scored at.
const SCORING_OFFSET: i32 = ORIGIN - ARRIVAL_LABEL_LEAD_TSTATES;

/// `interrupt_length` for `timings_frame_ferranti_7c`, libspectrum
/// `timings.c`.
const FUSE_INTERRUPT_LENGTH: u32 = 36;

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

/// FUSE's cost for an instruction arriving at FUSE frame T-state `t`.
///
/// `tstates += delay[tstates]; tstates += length`, once per M-cycle —
/// FUSE's whole memory contention model.
fn fuse_cost(t: u32, mcycles: &[u32]) -> u32 {
    let mut now = t;
    for length in mcycles {
        now += delay_at(now);
        now += length;
    }
    now - t
}

/// One instruction under test.
struct Case {
    name: &'static str,
    /// Bytes of the instruction, repeated to fill contended RAM.
    bytes: &'static [u8],
    /// M-cycle lengths, in order. Contention applies at each start.
    mcycles: &'static [u32],
    /// Register setup applied before the measured frame.
    setup: fn(&mut Spectrum128K),
}

fn cases() -> Vec<Case> {
    vec![
        // Anchors: single M-cycle. If either of these diverges the model
        // is wrong, not the engine.
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
            setup: |m| m.z80.regs.hl = DATA_ADDR,
        },
        // Four and five M-cycles, to show how the error scales with the
        // number of memory cycles rather than with the instruction's
        // length.
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
        // the two bytes already at `$5000` so the write is value-neutral
        // and the instruction stream stays intact.
        Case {
            name: "LD (nn),BC",
            bytes: &[0xED, 0x43, 0x00, 0x50],
            mcycles: &[4, 4, 3, 3, 3, 3],
            setup: |m| m.z80.regs.bc = 0x43ED,
        },
    ]
}

fn roms() -> Option<(Vec<u8>, Vec<u8>)> {
    let rom0 = std::fs::read(std::env::var(ROM0_PATH_ENV).ok()?).ok()?;
    let rom1 = std::fs::read(std::env::var(ROM1_PATH_ENV).ok()?).ok()?;
    Some((rom0, rom1))
}

fn tstate_in_frame(machine: &Spectrum128K) -> u32 {
    machine.hc() / TIMING_128K.cpu_divisor
}

/// A machine filled with the case's instruction, started `skew` T-states
/// past a frame boundary with the CPU aimed at contended RAM.
///
/// The skew is what gives arrival-T-state coverage. An instruction whose
/// cost is a multiple of the pattern's period revisits the same handful of
/// arrival phases forever; sweeping the start position walks the rest.
fn prepare_at(case: &Case, roms: &(Vec<u8>, Vec<u8>), skew: u32) -> Spectrum128K {
    let mut machine = Spectrum128K::new();
    machine.memory.load_roms(&roms.0, &roms.1);
    machine.reset();

    let mut addr = CODE_BASE;
    let mut index = 0usize;
    while addr < CODE_END {
        machine
            .memory
            .write(addr, case.bytes[index % case.bytes.len()]);
        index += 1;
        addr += 1;
    }

    // Align to a frame boundary so the measured window is exactly one
    // frame of contention, then aim the CPU at the filled region. The ROM
    // never runs, so IFF1 stays clear and no interrupt perturbs the count.
    while tstate_in_frame(&machine) != 0 {
        machine.advance_tstates(1);
    }
    machine.advance_tstates(skew);
    machine.z80.regs.pc = CODE_BASE;
    (case.setup)(&mut machine);
    machine
}

/// Advance until one more instruction retires; returns its cost in
/// T-states. Stepping a T-state at a time is what makes the cost exact —
/// the retired counter is the only reliable instruction boundary.
fn step_one_instruction(machine: &mut Spectrum128K) -> u32 {
    let target = machine.z80.instructions_retired() + 1;
    let mut cost = 0u32;
    while machine.z80.instructions_retired() < target {
        machine.advance_tstates(1);
        cost += 1;
        assert!(cost <= 512, "instruction should retire within 512 T-states");
    }
    cost
}

/// `(arrival T-state, measured cost)` for a frame's worth of one case.
///
/// The first two instructions of each pass are discarded: aiming `PC` at
/// the stream lands mid-M-cycle whenever the skew does, so the first
/// retirement is the tail of whatever the CPU was already doing.
fn arrival_samples(case: &Case, roms: &(Vec<u8>, Vec<u8>), skew: u32) -> Vec<(u32, u32)> {
    let mut machine = prepare_at(case, roms, skew);
    for _ in 0..2 {
        step_one_instruction(&mut machine);
    }
    let mut out = Vec::new();
    let mut spent = 0u32;
    while spent < FRAME_TSTATES {
        let arrival = tstate_in_frame(&machine);
        let pc = machine.z80.regs.pc;
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

/// The reference has to be checked before its readings mean anything.
///
/// `delay_at` above is a compact re-statement of FUSE's geometry. This is
/// FUSE's own arithmetic — `contend_delay_common` with the `65432100`
/// pattern and its offset of 1, over `timings_frame_ferranti_7c` — and
/// the two must agree at every T-state of the frame.
#[test]
fn the_delay_table_still_matches_fuse() {
    const FUSE_PATTERN: [u32; 8] = [5, 4, 3, 2, 1, 0, 0, 6];
    /// `top_left_pixel - DISPLAY_BORDER_HEIGHT * tstates_per_line - 16`
    /// = 14362 - 24*228 - 16.
    const LINE_TIMES_0: u32 = 8874;
    const LEFT_BORDER: u32 = 24;
    const HORIZONTAL_SCREEN: u32 = 128;
    /// `spectrum_contend_delay_65432100` passes 1.
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

    let bad: Vec<_> = (0..FRAME_TSTATES)
        .filter(|&t| delay_at(t) != fuse_delay(t))
        .take(8)
        .collect();
    assert!(
        bad.is_empty(),
        "the 128K delay table disagrees with FUSE at {bad:?}"
    );
}

/// The origin, measured from the interrupt rather than assumed or fitted.
///
/// Two ways to fail: where the edge falls, and how long it is held.
/// Stepping in half-cycles is deliberate — see `ORIGIN`.
///
/// ## The two T-states this records
///
/// The edge rises at engine T-state 56544, which is scan 248 pixel 0 —
/// `CONFIG_128K`'s `int_scan` and `int_start_pixel`, the same pair the
/// 48K uses. On the 48K that lands the interrupt exactly on
/// `top_left_pixel`: 69888 - 55552 = 14336. Here it lands 14364, and
/// libspectrum's `top_left_pixel` for `timings_frame_ferranti_7c` is
/// 14362. The 228-T-state line does not put scan 248 where the 224-T-state
/// one does, and `int_start_pixel` was never re-derived for it.
///
/// **Recorded, not moved.** `Float128K` reads 14364 and that is the figure
/// this engine is held to; the probe counts from the interrupt, so moving
/// the edge two T-states to sit on `top_left_pixel` would take the
/// floating bus off its own oracle to satisfy a constant nothing else
/// measures. Which of the two is right needs the 48K's `float_bus_oracle`
/// ported to this machine — a frame of screen bytes is a second anchor and
/// there is currently only one.
#[test]
#[ignore = "FIXTURE: needs EMU198X_SPECTRUM_128K_ROM0 / ROM1"]
fn the_origin_is_pinned_by_the_interrupt() {
    let Some(roms) = roms() else {
        panic!("set {ROM0_PATH_ENV} and {ROM1_PATH_ENV} to run this harness");
    };
    let mut machine = Spectrum128K::new();
    machine.memory.load_roms(&roms.0, &roms.1);
    machine.reset();
    while tstate_in_frame(&machine) != 0 {
        machine.advance_tstates(1);
    }

    let mut edges: Vec<(u32, bool)> = Vec::new();
    let mut prev = machine.ula.interrupt_active();
    for hc in 0..FRAME_TSTATES * HC_PER_TSTATE {
        machine.advance_halfcycles(1);
        let now = machine.ula.interrupt_active();
        if now != prev {
            edges.push((hc, now));
        }
        prev = now;
    }

    assert_eq!(
        edges.len(),
        2,
        "expected one interrupt assertion and one release per frame, got {edges:?}"
    );
    let (onset_hc, rising) = edges[0];
    let (release_hc, falling) = edges[1];
    assert!(rising && !falling, "edges out of order: {edges:?}");

    println!(
        "\n/INT rises at half-cycle {onset_hc} = T-state {} phase {}",
        onset_hc / HC_PER_TSTATE,
        onset_hc % HC_PER_TSTATE
    );

    assert_eq!(
        onset_hc % HC_PER_TSTATE,
        0,
        "/INT rose part-way through a T-state, which no anchor can be read off"
    );
    assert_eq!(
        (release_hc - onset_hc) / HC_PER_TSTATE,
        FUSE_INTERRUPT_LENGTH,
        "the engine holds /INT for {} T-states against FUSE's {FUSE_INTERRUPT_LENGTH}",
        (release_hc - onset_hc) / HC_PER_TSTATE
    );

    let onset = onset_hc / HC_PER_TSTATE;
    assert_eq!(
        FRAME_TSTATES as i32 - onset as i32,
        ORIGIN,
        "/INT rises at engine T-state {onset}, which puts the origin at {}, \
         not the {ORIGIN} this file scores against",
        FRAME_TSTATES as i32 - onset as i32
    );
}

/// The differential itself.
#[test]
#[ignore = "FIXTURE: differential harness; needs EMU198X_SPECTRUM_128K_ROM0 / ROM1"]
fn memory_contention_matches_fuse_at_every_arrival_tstate() {
    let Some(roms) = roms() else {
        panic!("set {ROM0_PATH_ENV} and {ROM1_PATH_ENV} to run this harness");
    };

    // Skews of 0..8 walk the arrival point across a whole period of the
    // 8-T-state pattern, so no phase goes unvisited even for an
    // instruction whose cost divides it.
    const SKEWS: u32 = 8;

    let all_cases = cases();
    let collected: Vec<Scored> = all_cases
        .iter()
        .map(|case| {
            let mut all = Vec::new();
            for skew in 0..SKEWS {
                all.extend(arrival_samples(case, &roms, skew));
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

    let total = score(SCORING_OFFSET);
    let samples_total: usize = collected.iter().map(|(_, _, all)| all.len()).sum();
    println!(
        "\n/INT origin {ORIGIN:+}, scored at {SCORING_OFFSET:+} — {total} of \
         {samples_total} samples disagree"
    );

    // The neighbourhood, so it is visible whether the pinned origin sits
    // in a plateau or beside a sharp minimum it is not on. On this machine
    // that is the finding: the distance from the pinned origin to the
    // minimum is what the delay-table index offset is carrying, because
    // the `/Border` coordinate and the offset are the only two things
    // between the raster and the gate.
    print!("\n{:<14}", "offset");
    for d in -8..=8i32 {
        print!("{:>8}", format!("{:+}", SCORING_OFFSET + d));
    }
    println!();
    print!("{:<14}", "mismatches");
    for d in -8..=8i32 {
        print!("{:>8}", score(SCORING_OFFSET + d));
    }
    println!();

    // Per case, so a shape-dependent error is separable from a global one.
    // A phase shift hits every case; a missing M-cycle hits the long ones
    // only.
    println!(
        "\n{:<14} {:>3} {:>9} {:>9} {:>7}  first divergences (t: got/want)",
        "instruction", "Ms", "samples", "wrong", "%"
    );
    println!("{}", "-".repeat(88));
    for (name, mcycles, all) in &collected {
        let wrong: Vec<_> = all
            .iter()
            .filter_map(|&(arrival, measured)| {
                let t = (arrival as i32 + SCORING_OFFSET).rem_euclid(FRAME_TSTATES as i32) as u32;
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
    // arrival T-state. A gate one T-state out of phase is wrong at the two
    // ends of the pattern and right in the middle, which reads very
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
            let t = (arrival as i32 + SCORING_OFFSET).rem_euclid(FRAME_TSTATES as i32) as u32;
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
    // anything, so every case must cost exactly its uncontended length. If
    // this fails the harness is measuring something other than the
    // instruction it thinks it is, and nothing above it means anything.
    for (name, mcycles, all) in &collected {
        let bare: u32 = mcycles.iter().sum();
        let quiet_wrong = all
            .iter()
            .filter(|&&(arrival, _)| {
                let start =
                    (arrival as i32 + SCORING_OFFSET).rem_euclid(FRAME_TSTATES as i32) as u32;
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

    // The ratchet, and it goes last so a red run still prints everything
    // above it. A ceiling, not a target: lower it in the commit that earns
    // it, never raise it.
    // 17 of 375,406, and the shape of the sweep is the result. The
    // minimum is sharp and isolated: one T-state either side costs about
    // 87,000, and the whole rest of the frame is worse still. So the
    // `/Border` coordinate and the delay-table index offset are jointly
    // right — the sum HALT2INT128 pinned is the sum FUSE wants, and
    // neither constant is carrying an error the other hides.
    //
    // The seventeen are harness residue, not engine defects, and they say
    // so in their own numbers: every one reports a cost far below the
    // instruction's uncontended length — 8 T-states for a six-M-cycle
    // `LD BC,(nn)` — which is the tail of an M-cycle already in flight
    // when the pass started. Two discarded instructions is not always
    // enough at every skew. The two single-M-cycle anchors are wrong at
    // 0 of 202,568 samples between them, which is the check that this is
    // residue rather than a shape the gate gets wrong. The 48K's
    // differential carries eighteen of exactly the same kind.
    //
    // Left visible rather than tuned away: raising the discard to silence
    // them would also silence a real short cost.
    //
    // A ceiling, not a target: lower it in the commit that earns it,
    // never raise it.
    const RATCHET: usize = 17;
    assert!(
        total <= RATCHET,
        "128K memory contention regressed against FUSE: {total} of \
         {samples_total} samples disagree, was {RATCHET}"
    );
}
