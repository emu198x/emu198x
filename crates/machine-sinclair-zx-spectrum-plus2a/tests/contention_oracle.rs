//! +2A/+3 memory-contention differential: the engine's measured
//! instruction cost against FUSE's, at every arrival T-state in the frame.
//!
//! This is what #856 asked for and could not build. The gate array has had
//! a frame-wide differential since `amstrad-ula-40077`'s
//! `fuse_differential`, but that harness drives the gate directly with a
//! synthetic M-cycle and compares a frame **maximum** — engine 1 T-state
//! against FUSE's 7. A maximum can say the mask is too short. It cannot
//! say where the mask should start, and #856 records four experiments plus
//! three re-runs against the corrected `/MREQ` that all founder on the
//! same rock:
//!
//! > `z80_clock_high` freezes during a stall, so the mask's phase against
//! > the arming parity is not fixed and cannot be settled from a frame
//! > maximum. Choosing a phase because it makes the number reach 7 would
//! > be a fit.
//!
//! An arrival-resolved differential can settle it, because it scores every
//! arrival phase separately instead of collapsing them. It also measures
//! the gate through a real CPU rather than a hand-driven pin sequence,
//! which is where two of #856's experiments went wrong.
//!
//! Nothing here consults `CONTENTION_PATTERN_PLUS2A` or
//! `DELAY_TABLE_PLUS2A`. The reference is FUSE and the subject is the
//! engine; a test that reads the table it is checking cannot fail.
//!
//! ## What it found: the phase is not the free parameter
//!
//! **The offset sweep is flat.** Across seventeen origins it runs 123,927
//! to 157,334 of 442,666, with no minimum anywhere near the pinned origin
//! or anywhere else. Put beside the 128K's differential — 17 at its
//! minimum and about 87,000 one T-state either side — that is a different
//! kind of answer entirely. There is no phase at which this gate agrees
//! with FUSE, so no rotation of the mask can be the fix, and #856's open
//! question is closed in the negative rather than by a fit.
//!
//! What the gate does instead is undercharge, everywhere:
//!
//! | instruction | Ms | engine mean | FUSE mean |
//! |---|---|---|---|
//! | `NOP`        | 1 | **4.00** | 9.00 |
//! | `INC BC`     | 1 | 6.38 | 8.58 |
//! | `LD A,(HL)`  | 2 | 7.94 | 14.77 |
//! | `LD A,(nn)`  | 4 | 14.56 | 28.56 |
//! | `LD BC,(nn)` | 6 | 22.25 | 42.54 |
//!
//! `NOP` reads 4.00 to the last digit: a single-M-cycle instruction is
//! **never** contended, at any arrival T-state in the frame. Only 81,646
//! of 442,666 samples are charged anything at all, where FUSE charges on
//! seven of every eight T-states inside the window. And the per-slot table
//! is 100% wrong at every slot each case actually visits.
//!
//! So the finding is not "the mask is one T-state out". It is that the
//! gate arms far too rarely and charges too little when it does, and that
//! `DELAY_TABLE_PLUS2A`'s three trues against the fourteen the pattern
//! needs — #856's derivable half — is the whole story rather than half of
//! one. Nothing is landed here: this is the instrument, and it now says
//! what a fix has to achieve and what it cannot be.
//!
//! ```sh
//! cargo test --release -p machine-sinclair-zx-spectrum-plus2a \
//!     --test contention_oracle -- --ignored --nocapture
//! ```
//!
//! ## The gate contends the other way round
//!
//! ```rust,ignore
//! let contention = contended_addr && cpu_mreq && e.z80_clock_high;
//! ```
//!
//! The Sinclair ULAs withhold the clock edge that would *drop* `/MREQ`;
//! the Amstrad gate array contends while `/MREQ` is asserted. That is why
//! the 48K's probes could not be adapted to it and why this file is a port
//! of the *differential* — which drives a real instruction and reads its
//! cost — rather than of any of the 48K's gate-level probes.
//!
//! FUSE's numbers come from the vendored source, not from memory:
//!
//! - `fuse-1.7.0/spectrum.c` — `contention_pattern_76543210 =
//!   {5,4,3,2,1,0,7,6}` via `contend_delay_common(time, pattern, 4)`. Both
//!   `machines/specplus2a.c` and `machines/specplus3e.c` select it.
//! - `fuse-emulator-libspectrum/timings.c` — `timings_frame_amstrad_asic`:
//!   24/128/24/52 horizontal (228 per line), 48/192/48/23 vertical (311
//!   lines), `interrupt_length` 32, `top_left_pixel` 14365.

use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::timing::TIMING_PLUS2A;
use common_sinclair_zx_spectrum::ula::Ula;
use machine_sinclair_zx_spectrum_plus2a::SpectrumPlus2A;
use std::path::PathBuf;

const ROM_DIR_ENV: &str = "EMU198X_SPECTRUM_PLUS3_ROM_DIR";

/// T-states in a +2A frame: 228 x 311.
const FRAME_TSTATES: u32 = 70_908;
/// T-states per scan line.
const PER_LINE: u32 = 228;
/// Display lines that carry contention.
const DISPLAY_LINES: u32 = 192;
/// Contended T-states at the start of each display line.
const CONTENDED_PER_LINE: u32 = 128;
/// Master-clock half-cycles per CPU T-state, `TIMING_PLUS2A.cpu_divisor`.
const HC_PER_TSTATE: u32 = 5;

/// The delay pattern across an 8-T-state slot, read from `FIRST_DISPLAY`.
///
/// This is FUSE's `{5,4,3,2,1,0,7,6}` rotated onto the T-state the window
/// opens at, not a transcription of `CONTENTION_PATTERN_PLUS2A`.
/// `the_delay_table_still_matches_fuse` re-derives the whole frame from
/// FUSE's own arithmetic rather than trusting the rotation.
const PATTERN: [u32; 8] = [1, 0, 7, 6, 5, 4, 3, 2];

/// First T-state at which contention applies.
///
/// `top_left_pixel` 14365 over 228-T lines with a 48-line top border gives
/// `line_times[0] = 14365 - 48*228 - 16 = 3405`, so the first display line
/// starts at `line_times[48] = 14349`. `contend_delay_common`'s offset of
/// 4 opens the window twelve T-states into that line.
const FIRST_DISPLAY: u32 = 14_361;

/// Contended RAM: bank 5 at `$4000`, which the +2A pages in at reset and
/// which this harness never re-pages.
const CODE_BASE: u16 = 0x4000;
const CODE_END: u16 = 0x8000;
/// Operand target inside the same contended bank.
const DATA_ADDR: u16 = 0x5000;

/// Add this to an engine frame T-state to get FUSE's.
///
/// FUSE's frame T-state 0 *is* the interrupt.
/// `the_origin_is_pinned_by_the_interrupt` measures the engine's edge at
/// **half-cycle** resolution and asserts this number, rather than fitting
/// the offset — a free origin absorbs exactly the phase error this
/// differential exists to resolve, which is the whole reason #856 could
/// not be closed from a frame maximum.
///
/// **This is not `top_left_pixel`**, which libspectrum gives as 14365 for
/// `timings_frame_amstrad_asic`. The edge lands one T-state from it. The
/// 128K lands two T-states from its own, and the 48K lands exactly on its
/// own; all three share `int_scan` 248 and `int_start_pixel` 1, and only
/// the 48K's 224-T-state line makes that come out on `top_left_pixel`.
/// Recorded, not moved — see the 128K's `contention_oracle` for why.
const ORIGIN: i32 = 14_364;

/// The harness labels an instruction's arrival one T-state later than the
/// M-cycle FUSE charges, so the score is taken one T-state below `ORIGIN`.
///
/// A property of the measurement, not of the engine, and the same one the
/// 48K and 128K differentials carry — three machines, three ULAs, two of
/// which contend on opposite polarities of `/MREQ`. See
/// `machine-sinclair-zx-spectrum-128k`'s `ARRIVAL_LABEL_LEAD_TSTATES` for
/// the evidence that it is the label rather than the engines.
const ARRIVAL_LABEL_LEAD_TSTATES: i32 = 1;

/// The offset the differential is scored at.
const SCORING_OFFSET: i32 = ORIGIN - ARRIVAL_LABEL_LEAD_TSTATES;

/// `interrupt_length` for `timings_frame_amstrad_asic`.
const FUSE_INTERRUPT_LENGTH: u32 = 32;

/// Delay a contended M-cycle starting at frame T-state `t` incurs.
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

/// Which entry of `PATTERN` a frame T-state falls on, or `None` outside
/// the contended window.
///
/// Separate from `delay_at` because the differential's per-slot table
/// needs the index rather than the value, and because a zero delay is a
/// legitimate slot rather than "outside the window".
fn pattern_slot(t: u32) -> Option<usize> {
    let t = t % FRAME_TSTATES;
    if t < FIRST_DISPLAY {
        return None;
    }
    let into_display = t - FIRST_DISPLAY;
    if into_display / PER_LINE >= DISPLAY_LINES {
        return None;
    }
    let in_line = into_display % PER_LINE;
    if in_line >= CONTENDED_PER_LINE {
        return None;
    }
    Some((in_line % 8) as usize)
}

/// FUSE's cost for an instruction arriving at FUSE frame T-state `t`.
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
    bytes: &'static [u8],
    mcycles: &'static [u32],
    setup: fn(&mut SpectrumPlus2A),
}

fn cases() -> Vec<Case> {
    vec![
        // Anchors: single M-cycle. If either diverges the model is wrong,
        // not the engine.
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
        Case {
            name: "LD A,(HL)",
            bytes: &[0x7E],
            mcycles: &[4, 3],
            setup: |m| m.z80.regs.hl = DATA_ADDR,
        },
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
        Case {
            name: "LD BC,(nn)",
            bytes: &[0xED, 0x4B, 0x00, 0x50],
            mcycles: &[4, 4, 3, 3, 3, 3],
            setup: |_| {},
        },
        Case {
            name: "LD (nn),BC",
            bytes: &[0xED, 0x43, 0x00, 0x50],
            mcycles: &[4, 4, 3, 3, 3, 3],
            setup: |m| m.z80.regs.bc = 0x43ED,
        },
    ]
}

fn rom_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(ROM_DIR_ENV) {
        return Some(PathBuf::from(dir));
    }
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".emu198x/roms/amstrad-zx-spectrum-plus3"))
}

/// The four +3 ROMs, or `None` if they are not installed.
fn roms() -> Option<Vec<Vec<u8>>> {
    let dir = rom_dir()?;
    (0..4)
        .map(|i| std::fs::read(dir.join(format!("plus3-{i}.rom"))).ok())
        .collect()
}

fn tstate_in_frame(machine: &SpectrumPlus2A) -> u32 {
    machine.hc() / TIMING_PLUS2A.cpu_divisor
}

fn fresh(roms: &[Vec<u8>]) -> SpectrumPlus2A {
    let mut machine = SpectrumPlus2A::new();
    machine
        .memory
        .load_roms(&roms[0], &roms[1], &roms[2], &roms[3]);
    machine.reset();
    machine
}

/// A machine filled with the case's instruction, started `skew` T-states
/// past a frame boundary with the CPU aimed at contended RAM.
fn prepare_at(case: &Case, roms: &[Vec<u8>], skew: u32) -> SpectrumPlus2A {
    let mut machine = fresh(roms);

    let mut addr = CODE_BASE;
    let mut index = 0usize;
    while addr < CODE_END {
        machine
            .memory
            .write(addr, case.bytes[index % case.bytes.len()]);
        index += 1;
        addr += 1;
    }

    while tstate_in_frame(&machine) != 0 {
        machine.advance_tstates(1);
    }
    machine.advance_tstates(skew);
    machine.z80.regs.pc = CODE_BASE;
    (case.setup)(&mut machine);
    machine
}

/// Advance until one more instruction retires; returns its cost.
fn step_one_instruction(machine: &mut SpectrumPlus2A) -> u32 {
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
fn arrival_samples(case: &Case, roms: &[Vec<u8>], skew: u32) -> Vec<(u32, u32)> {
    let mut machine = prepare_at(case, roms, skew);
    // Aiming `PC` at the stream lands mid-M-cycle whenever the skew does,
    // so the first retirement is the tail of whatever was already running.
    for _ in 0..2 {
        step_one_instruction(&mut machine);
    }
    let mut out = Vec::new();
    let mut spent = 0u32;
    while spent < FRAME_TSTATES {
        // Wrap the stream rather than run off the end of it. A frame holds
        // more instructions than the 16 KiB bank does whenever contention
        // is light, and on this gate array it is very light — that is the
        // defect under measurement, so the harness must not fail *because*
        // of it. `CODE_BASE` is always an instruction boundary: the fill
        // repeats from there, and PC is only ever reset here at a retire.
        if machine.z80.regs.pc >= CODE_END - 8 {
            machine.z80.regs.pc = CODE_BASE;
        }
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
/// `delay_at` is a compact re-statement rotated onto `FIRST_DISPLAY`. This
/// is FUSE's own arithmetic — `contend_delay_common` with the `76543210`
/// pattern and its offset of 4, over `timings_frame_amstrad_asic` — and
/// the two must agree at every T-state of the frame.
#[test]
fn the_delay_table_still_matches_fuse() {
    const FUSE_PATTERN: [u32; 8] = [5, 4, 3, 2, 1, 0, 7, 6];
    const TOP_LEFT_PIXEL: u32 = 14_365;
    const BORDER_HEIGHT: u32 = 48;
    const LEFT_BORDER: u32 = 24;
    const HORIZONTAL_SCREEN: u32 = 128;
    /// `spectrum_contend_delay_76543210` passes 4.
    const OFFSET: u32 = 4;
    /// `machine.c`: `top_left_pixel - border * per_line - 16`.
    const LINE_TIMES_0: u32 = TOP_LEFT_PIXEL - BORDER_HEIGHT * PER_LINE - 16;

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
        "the +2A delay table disagrees with FUSE at {bad:?}"
    );
}

/// The origin, measured from the interrupt rather than assumed or fitted.
///
/// Two ways to fail: where the edge falls, and how long it is held.
/// Half-cycle resolution is deliberate — a T-state-resolution version of
/// this measurement on the 48K read its edge one T-state late for months.
#[test]
#[ignore = "needs the +3 ROM set"]
fn the_origin_is_pinned_by_the_interrupt() {
    let Some(roms) = roms() else {
        // Not a bare `return`. A harness that passes when its fixture is
        // absent gates nothing, and this one exists because #856 went
        // unmeasured for months. `skip!` fails under
        // `EMU198X_STRICT_FIXTURES`, which the nightly sets.
        emu198x_test_skip::skip!("+3 ROM set not staged; set {}", ROM_DIR_ENV);
    };
    let mut machine = fresh(&roms);
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
#[ignore = "differential harness; needs the +3 ROM set"]
fn memory_contention_matches_fuse_at_every_arrival_tstate() {
    let Some(roms) = roms() else {
        // Not a bare `return`. A harness that passes when its fixture is
        // absent gates nothing, and this one exists because #856 went
        // unmeasured for months. `skip!` fails under
        // `EMU198X_STRICT_FIXTURES`, which the nightly sets.
        emu198x_test_skip::skip!("+3 ROM set not staged; set {}", ROM_DIR_ENV);
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

    // The neighbourhood. This is the part a frame maximum cannot produce
    // and #856 needs: if the mask's phase is wrong the minimum sits away
    // from the pinned origin, and the distance names the correction.
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

    // Per case, so a shape-dependent error separates from a global one.
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

    // The delay the raster owed at the arrival T-state, against how often
    // the engine got that arrival wrong. **This is the table #856 needs.**
    // A mask that is too short is wrong on the large delays and right on
    // the small ones; a mask in the wrong phase is wrong on a rotation of
    // them. A frame maximum shows neither.
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
            // The slot has to come from the position *within the line*, not
            // from the frame T-state. 228 is not a multiple of 8, so a
            // frame-relative index rotates by four every scan line and
            // smears all eight slots together. The 48K's version of this
            // table gets away with the shorter form because 224 is.
            let Some(slot) = pattern_slot(t) else {
                continue;
            };
            seen[slot] += 1;
            if fuse_cost(t, mcycles) != measured {
                wrong[slot] += 1;
            }
        }
        // An unvisited slot must not print as 0% — "always right" and
        // "never measured" are the two readings this table exists to
        // separate, and a `max(1)` denominator renders them identically.
        print!("{name:<14} ");
        for p in 0..8 {
            if seen[p] == 0 {
                print!("{:>7}", "-");
            } else {
                print!("{:>6.0}%", wrong[p] as f64 / seen[p] as f64 * 100.0);
            }
        }
        print!("   n={}", seen.iter().sum::<u32>());
        println!();
    }

    // How much contention the engine charges against how much FUSE does,
    // inside the window. #856's headline is a maximum; this is the mean,
    // which a phase error moves and a length error moves differently.
    println!(
        "\n{:<14} {:>14} {:>14}",
        "instruction", "mean got (in)", "mean want (in)"
    );
    println!("{}", "-".repeat(46));
    for (name, mcycles, all) in &collected {
        let (mut got, mut want, mut n) = (0u64, 0u64, 0u64);
        for &(arrival, measured) in all {
            let t = (arrival as i32 + SCORING_OFFSET).rem_euclid(FRAME_TSTATES as i32) as u32;
            if pattern_slot(t).is_none() {
                continue;
            }
            got += measured as u64;
            want += fuse_cost(t, mcycles) as u64;
            n += 1;
        }
        println!(
            "{name:<14} {:>14.2} {:>14.2}",
            got as f64 / n.max(1) as f64,
            want as f64 / n.max(1) as f64
        );
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

    // The second self-check, and the one #856's history demands. The
    // known state of this gate is that it undercharges — 1 T-state where
    // FUSE reaches 7. A run that reports no contention anywhere is the
    // harness failing to arm the gate, which is exactly how the first
    // version of `amstrad-ula-40077`'s `fuse_differential` measured a
    // gate that never stalls.
    let contended_samples: usize = collected
        .iter()
        .map(|(_, mcycles, all)| {
            let bare: u32 = mcycles.iter().sum();
            all.iter().filter(|&&(_, measured)| measured > bare).count()
        })
        .sum();
    assert!(
        contended_samples > 0,
        "harness fault, not a finding: no sample anywhere in the frame cost \
         more than its uncontended length, so the gate never armed"
    );
    println!("\n{contended_samples} of {samples_total} samples were charged anything at all");

    // The ratchet, last so a red run still prints everything above it.
    //
    // 149,185 of 442,666, which is a floor to work down from rather than a
    // near-miss to protect. The sweep's flatness is the load-bearing
    // result and it is not a number this ratchet can express: every other
    // origin in the neighbourhood scores between 123,927 and 157,334, so a
    // change that merely moved the phase would land inside that band while
    // fixing nothing.
    //
    // A ceiling, not a target: lower it in the commit that earns it,
    // never raise it.
    const RATCHET: usize = 149_185;
    assert!(
        total <= RATCHET,
        "+2A memory contention regressed against FUSE: {total} of \
         {samples_total} samples disagree, was {RATCHET}"
    );
}
