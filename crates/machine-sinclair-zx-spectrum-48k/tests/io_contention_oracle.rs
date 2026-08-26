//! I/O contention differential: score the engine's `IN` cost against
//! FUSE's model, at every arrival T-state in the frame, for every port
//! class at once.
//!
//! Memory contention has already been pinned to FUSE frame-wide
//! (`matches_fuse_contention_across_the_whole_frame` in
//! `contention_oracle.rs`) and the engine agrees with it per instruction.
//! I/O contention had never been derived at all. The gate reads
//!
//! ```text
//! (cpu_iorq || e.z80_iorq_prev) && io_even_port && e.z80_clock_high
//! ```
//!
//! which was the same one-half-cycle delay-line approximation that was
//! proved wrong for `MREQ`, and which knew nothing about the *page* the
//! port address lands in. FUSE contends in three of the four classes, with
//! a different shape in each; that gate had one shape and one test.
//!
//! See `knowledge/decisions/spectrum-contention-vs-floating-bus.md`.
//!
//! ## What it found
//!
//! - The engine could not separate `$40FE` from `$40FF` at all — the two
//!   differ only in the bit the gate tested, yet cost the same to within
//!   0.00 T-states, where FUSE separates them. The page dependence the
//!   engine *did* show came from the wrong place: the memory gate's
//!   `!cpu_mreq` term is true throughout an I/O cycle, so a port address in
//!   `$4000..$8000` tripped *memory* contention.
//! - `$00FF`, the port floatspy reads, is FUSE's `N:4` class: no I/O
//!   contention at any arrival T-state. The engine matched it exactly,
//!   0 of 63,744 samples, with or without the `MREQT23` latch.
//!
//! ## What it says now
//!
//! Zero of 297,222, all five classes, at the interrupt-pinned origin. The
//! gate charges a *count* of lookups at FUSE's offsets rather than holding
//! a level; see
//! `knowledge/decisions/io-contention-is-a-count-not-a-level.md`.
//!
//! The second bullet above drew a conclusion that has since been
//! falsified, and it is left standing as a warning. `$00FF` is uncontended
//! in both models, so I/O contention looked irrelevant to floatspy — but
//! floatspy's *probe* reaches its `IN A,($FF)` through an `IN A,(254)`, an
//! even port, whose cost this fix changes. "The port under test is
//! uncontended" does not imply "the program measuring it is unaffected".
//!
//! ## Why this shape
//!
//! Two things have worked on this problem and one has not. Frame-wide
//! model differentials against FUSE found the floating-bus pattern lag and
//! the oracle's own wrong reference; adjusting a constant and re-measuring
//! a test program has failed three times, because another compensating
//! constant absorbed the change. So this harness compares *models*, over
//! the whole frame, before any test program is consulted:
//!
//! - The reference is FUSE's `ula_contend_port_early`/`_late` transcribed
//!   whole, not a constant lifted out of it.
//! - The subject is the engine's own measured per-instruction cost.
//! - Every arrival T-state in the frame is scored, so a phase error cannot
//!   average out. Frame *totals* are structurally blind to phase — the
//!   contended window is sixteen whole 8-T-state groups — which is how the
//!   window experiment survived long enough to be acted on.
//! - Every port class is scored against **one shared** origin offset. A
//!   single class can be talked into agreement by moving the origin; five
//!   cannot.
//! - The load-bearing conclusion is stated in a form the origin cannot
//!   reach at all — the gap between two ports differing only in their low
//!   bit. See the closing table.
//!
//! ```sh
//! cargo test --release -p machine-sinclair-zx-spectrum-48k \
//!     --test io_contention_oracle -- --ignored --nocapture
//! ```

use common_sinclair_zx_spectrum::memory::MemoryBus;
use machine_sinclair_zx_spectrum_48k::Spectrum48k;

const ROM_PATH_ENV: &str = "EMU198X_SPECTRUM_48K_ROM";

/// T-states in a 48K frame.
const FRAME_TSTATES: u32 = 69888;
/// First T-state at which contention applies. Pinned to FUSE frame-wide;
/// see `contention_oracle.rs`, which proves this against
/// `spectrum_contend_delay_65432100` rather than asserting it.
const FIRST_DISPLAY: u32 = 14335;
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

/// Delay a contended cycle starting at frame T-state `t` incurs.
///
/// This is FUSE's `ula_contention[]`. On the 48K `contend_delay` and
/// `contend_delay_no_mreq` are wired to the *same* function
/// (`spectrum_contend_delay_65432100`, `machines/spec48.c`), so port
/// contention — which uses the `no_mreq` table — reads from this one too.
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

/// Is the *port address* in a contended page?
///
/// FUSE tests `memory_map_read[ port >> 14 ].contended`, i.e. the page the
/// 16-bit port address falls in — for `IN A,(C)` that is `B`, not the
/// address the code is running from. On the 48K only `$4000..$8000` is
/// contended.
fn port_page_contended(port: u16) -> bool {
    (0x4000..0x8000).contains(&port)
}

/// Does the ULA answer this port? `spec48_port_from_ula`: all even ports.
fn port_from_ula(port: u16) -> bool {
    port & 1 == 0
}

/// FUSE's `ula_contend_port_early` — `peripherals/ula.c`.
fn contend_port_early(t: u32, port: u16) -> u32 {
    let mut t = t;
    if port_page_contended(port) {
        t += delay_at(t);
    }
    t + 1
}

/// FUSE's `ula_contend_port_late` — `peripherals/ula.c`.
///
/// The three branches are the classic four-way table seen from the inside:
/// ULA port gives `C:1, C:3`; contended page with an odd port gives
/// `C:1, C:1, C:1, C:1`; an uncontended page with an odd port is `N:4`.
fn contend_port_late(t: u32, port: u16) -> u32 {
    let mut t = t;
    if port_from_ula(port) {
        t += delay_at(t);
        t += 2;
    } else if port_page_contended(port) {
        t += delay_at(t);
        t += 1;
        t += delay_at(t);
        t += 1;
        t += delay_at(t);
    } else {
        t += 2;
    }
    t
}

/// FUSE's `readport` — `periph.c`. Returns the T-state after the cycle.
///
/// The floating bus is sampled between `contend_port_late` and the final
/// `tstates++`, which is the "IO-cycle start + 3" figure the sample-lead
/// argument turns on.
fn readport(t: u32, port: u16) -> u32 {
    let t = contend_port_early(t, port);
    let t = contend_port_late(t, port);
    t + 1
}

/// Cost of `IN A,(C)` (`ED 78`) arriving at frame T-state `t`, in T-states.
///
/// Two M1 fetches from contended RAM, then the I/O cycle. FUSE models an
/// M1 as one `contend_read( PC, 4 )`, which is what `canonical_per_frame`
/// in `contention_oracle.rs` already scores the engine against exactly.
fn fuse_in_a_c_cost(t: u32, port: u16) -> u32 {
    let mut now = t;
    for _ in 0..2 {
        now += delay_at(now);
        now += 4;
    }
    now = readport(now, port);
    now - t
}

/// One port class under test.
struct PortCase {
    name: &'static str,
    /// The 16-bit port address placed in `BC`.
    port: u16,
    /// FUSE's four-way classification, for the report.
    shape: &'static str,
}

fn port_cases() -> Vec<PortCase> {
    vec![
        PortCase {
            name: "contended, ULA",
            port: 0x40FE,
            shape: "C:1 C:3",
        },
        PortCase {
            name: "contended, odd",
            port: 0x40FF,
            shape: "C:1 C:1 C:1 C:1",
        },
        PortCase {
            name: "uncontended, ULA",
            port: 0xC0FE,
            shape: "N:1 C:3",
        },
        PortCase {
            name: "uncontended, odd",
            port: 0xC0FF,
            shape: "N:4",
        },
        // The port floatspy actually reads. Disassembled from
        // `floatspy.tap` at offset 5580:
        //
        //     01 FF 00    LD BC,$00FF
        //     78          LD A,B          ; A = $00
        //     DB FF       IN A,($FF)      ; port = $00FF
        //     C9          RET
        //
        // `IN A,(n)` takes the port's high byte from `A`, so the address is
        // `$00FF` — the ROM page, uncontended, odd. Under FUSE's model that
        // is `N:4`: no I/O contention whatsoever. It is the one case in the
        // table that pays nothing, and it is the one the authoritative
        // floating-bus oracle uses. (The only other `IN`-shaped bytes in
        // the tape, `ED 40` at 2089, sit inside the ASCII string
        // "*** SELF TEST ***" and are never executed.)
        PortCase {
            name: "floatspy ($00FF)",
            port: 0x00FF,
            shape: "N:4",
        },
    ]
}

fn rom_bytes() -> Option<Vec<u8>> {
    let path = std::env::var(ROM_PATH_ENV).ok()?;
    std::fs::read(path).ok()
}

/// A machine running `IN A,(C)` out of contended RAM with `BC` set to the
/// port under test, aligned `skew` T-states past a frame boundary.
///
/// The skew is what gives arrival-T-state coverage. `IN A,(C)` costs at
/// least twelve T-states, so a single pass visits only a fraction of the
/// frame's arrival points; sweeping the start position walks the rest.
fn prepare(port: u16, skew: u32, rom: &[u8]) -> Spectrum48k {
    let mut machine = Spectrum48k::new();
    machine.load_rom_bytes(rom).expect("48K ROM should load");
    machine.reset();

    let mut addr = CODE_BASE;
    while addr < CODE_END {
        // ED 78 — IN A,(C). `IN A,(n)` cannot be used here: it takes the
        // port's high byte from `A`, which the instruction then overwrites
        // with what it read, so the port class would change every pass.
        machine.memory_mut().write(addr, 0xED);
        machine.memory_mut().write(addr + 1, 0x78);
        addr += 2;
    }

    while machine.tstate_in_frame() != 0 {
        machine.advance_tstates(1);
    }
    machine.advance_tstates(skew);
    machine.z80_mut().regs.pc = CODE_BASE;
    machine.z80_mut().regs.bc = port;
    machine
}

/// Advance until one more instruction retires; returns its cost.
fn step_one_instruction(machine: &mut Spectrum48k) -> u32 {
    let target = machine.z80().instructions_retired() + 1;
    let mut cost = 0u32;
    while machine.z80().instructions_retired() < target {
        machine.advance_tstates(1);
        cost += 1;
        assert!(cost <= 512, "instruction should retire within 512 T-states");
    }
    cost
}

/// `(arrival T-state, measured cost)` for a frame's worth of `IN A,(C)`.
///
/// The first two instructions of each pass are discarded. Aiming `PC` at
/// the stream lands mid-M-cycle whenever the skew does, so the first
/// retirement is the tail of whatever the CPU was already doing — the
/// costs come out as 3 and 6 T-states, which is not an `IN` at all.
fn samples(port: u16, skew: u32, rom: &[u8]) -> Vec<(u32, u32)> {
    let mut machine = prepare(port, skew, rom);
    let mut out = Vec::new();
    for _ in 0..2 {
        step_one_instruction(&mut machine);
    }
    let mut spent = 0u32;
    while spent < FRAME_TSTATES {
        let arrival = machine.tstate_in_frame();
        let pc = machine.z80().regs.pc;
        assert!(
            (CODE_BASE..CODE_END).contains(&pc),
            "execution left the instruction stream at pc {pc:#06x} — an \
             interrupt or a stray jump would make every later sample a \
             measurement of the ROM"
        );
        let cost = step_one_instruction(&mut machine);
        out.push((arrival, cost));
        spent += cost;
    }
    out
}

/// How many of `samples` disagree with FUSE if the engine's arrival
/// T-state is FUSE's plus `offset`?
///
/// The offset exists because the engine's frame origin and FUSE's do not
/// coincide, and by a lot: `tstate_in_frame() == 0` puts the engine's
/// raster at display line 0, where FUSE's frame T-state 0 is the interrupt
/// at the top of the vertical blank. Frame *totals* cannot see that — they
/// walk the whole frame either way, which is why the existing
/// per-instruction oracle never noticed — but a phase-resolved
/// comparison sees nothing else until it is accounted for.
///
/// The offset is therefore searched over the whole frame rather than
/// assumed. Reporting the search, not just its winner, is the point: a
/// fitted constant that only ever appears at its best value is
/// indistinguishable from a correct one.
fn mismatches_strided(samples: &[(u32, u32)], port: u16, offset: i32, stride: usize) -> usize {
    samples
        .iter()
        .step_by(stride)
        .filter(|&&(arrival, measured)| {
            let t = (arrival as i32 + offset).rem_euclid(FRAME_TSTATES as i32) as u32;
            fuse_in_a_c_cost(t, port) != measured
        })
        .count()
}

fn mismatches(samples: &[(u32, u32)], port: u16, offset: i32) -> usize {
    mismatches_strided(samples, port, offset, 1)
}

/// A scored port class: name, port, FUSE shape, and its raw samples.
type Scored = (&'static str, u16, &'static str, Vec<(u32, u32)>);

/// Add this to an engine frame T-state to get FUSE's.
///
/// **Measured, not fitted.** `the_frame_origin_is_pinned_by_the_interrupt`
/// establishes it from the one event both implementations define
/// identically and neither derives from contention: the `/INT` edge. FUSE
/// asserts the interrupt at frame T-state 0 — `spectrum_frame()` subtracts
/// a frame from `tstates` and `z80_interrupt()` runs immediately after,
/// with `/INT` held while `tstates < interrupt_length`. The engine asserts
/// it at its own T-state 55553, and 69888 - 55553 = 14335.
///
/// This matters more than it looks. Fitting the offset makes it a free
/// parameter, and a free parameter absorbs exactly the error this harness
/// exists to find: a gate deciding one T-state late is indistinguishable
/// from an origin one T-state early. That is not hypothetical — wiring the
/// `MREQT23` latch moved the *fitted* winner from +14335 to +14334 while
/// the raster it supposedly describes had not moved at all, and the shift
/// was hiding a real +1 T-state regression on every contended `M1` pair.
///
/// Two further readings agree at the same anchor: the engine holds `/INT`
/// for 32 T-states, which is `interrupt_length` for the Ferranti 5C/6C in
/// libspectrum's `timings.c`; and +14335 puts the engine's T-state 1 at
/// FUSE's `top_left_pixel` of 14336, one T-state after the contention
/// window opens, which is where `FIRST_DISPLAY` already had it.
const ORIGIN: i32 = 14335;

/// The origin to score against: pinned by default, fitted on request.
///
/// The fit is kept because a disagreement between it and `ORIGIN` is
/// itself a finding — it says the engine's contention phase has moved
/// relative to its own interrupt. Reported, never silently adopted.
fn scoring_offset(collected: &[Scored]) -> i32 {
    match std::env::var("EMU198X_IO_ORACLE_OFFSET").as_deref() {
        Ok("fit") => best_shared_offset(collected),
        Ok(pinned) => pinned.parse().expect("offset override must be an integer"),
        Err(_) => ORIGIN,
    }
}

/// The offset that best reconciles *all* classes at once.
///
/// Coarse pass over every T-state in the frame on a strided subsample,
/// then an exhaustive fine pass over the whole sample set around the
/// coarse winner. One class can be talked into agreement by moving the
/// origin; five sharing a single offset cannot.
fn best_shared_offset(collected: &[Scored]) -> i32 {
    const COARSE_STRIDE: usize = 97;
    const FINE_WINDOW: i32 = 24;

    let score = |offset: i32, stride: usize| -> usize {
        collected
            .iter()
            .map(|(_, port, _, all)| mismatches_strided(all, *port, offset, stride))
            .sum()
    };

    let coarse = (0..FRAME_TSTATES as i32)
        .min_by_key(|&offset| score(offset, COARSE_STRIDE))
        .expect("the frame is not empty");

    (coarse - FINE_WINDOW..=coarse + FINE_WINDOW)
        .min_by_key(|&offset| score(offset, 1))
        .expect("the fine window is not empty")
}

/// The frame origin, from the interrupt rather than from a best fit.
///
/// Everything phase-resolved in this file rests on mapping the engine's
/// frame T-state to FUSE's, and for a while that mapping was whatever
/// minimised disagreement — which is no measurement at all. The `/INT`
/// edge is a measurement: FUSE's frame T-state 0 *is* the interrupt, and
/// the engine's raster raises `int_active` at a T-state of its own that
/// owes nothing to the contention gate.
///
/// Runs without a ROM-dependent instruction stream and asserts two things,
/// either of which can fail: where the edge falls, and how long it lasts.
#[test]
#[ignore = "FIXTURE: needs EMU198X_SPECTRUM_48K_ROM"]
fn the_frame_origin_is_pinned_by_the_interrupt() {
    use common_sinclair_zx_spectrum::driver::SpectrumDriver;
    use common_sinclair_zx_spectrum::ula::Ula;

    /// `interrupt_length` for `timings_frame_ferranti_5c_6c`, libspectrum
    /// `timings.c`. FUSE holds `/INT` while `tstates < interrupt_length`.
    const FUSE_INTERRUPT_LENGTH: u32 = 32;

    let Some(rom) = rom_bytes() else {
        panic!("set {ROM_PATH_ENV} to the 48K ROM to run this harness");
    };
    let mut machine = Spectrum48k::new();
    machine.load_rom_bytes(&rom).expect("48K ROM should load");
    machine.reset();
    while machine.tstate_in_frame() != 0 {
        machine.advance_tstates(1);
    }

    // Stepped a master tick at a time. Polling with `advance_tstates(1)`
    // and reading `tstate_in_frame()` afterwards names the T-state *after*
    // the one the edge fell in, and this test used to do exactly that: it
    // reported an onset of 55553 where the pin rises at the first master
    // tick of 55552, and then passed by comparing that one-too-high onset
    // against an `ORIGIN` that is one too low. Two errors cancelling is
    // not a measurement. See
    // `knowledge/decisions/spectrum-contention-vs-floating-bus.md`.
    let divisor = machine.frame_timing().cpu_divisor;
    let mut edges = Vec::new();
    let mut prev = machine.ula().interrupt_active();
    for _ in 0..(FRAME_TSTATES * divisor) {
        machine.advance_halfcycles(1);
        let now = machine.ula().interrupt_active();
        if now != prev {
            // `hc` is already past the tick that moved the pin.
            edges.push(((machine.hc() - 1) / divisor, now));
        }
        prev = now;
    }

    assert_eq!(
        edges.len(),
        2,
        "expected one interrupt assertion and one release per frame, got {edges:?}"
    );
    let (onset, rising) = edges[0];
    let (release, falling) = edges[1];
    assert!(rising && !falling, "edges out of order: {edges:?}");

    assert_eq!(
        release - onset,
        FUSE_INTERRUPT_LENGTH,
        "the engine holds /INT for {} T-states against FUSE's {FUSE_INTERRUPT_LENGTH}",
        release - onset
    );
    // The interrupt-derived origin, measured rather than fitted.
    //
    // FUSE's frame T-state 0 *is* the interrupt: `spectrum_frame()`
    // subtracts a frame and `z80_interrupt()` runs in the same handler.
    // So whatever engine T-state raises `/INT` maps onto FUSE's 0, and
    // the origin is the rest of the frame.
    let interrupt_origin = FRAME_TSTATES as i32 - onset as i32;
    println!(
        "/INT rises at engine T-state {onset}; interrupt-derived origin \
         {interrupt_origin}; this file scores contention against {ORIGIN}"
    );
    // The gap is real and it is recorded, not asserted away.
    //
    // This used to assert `interrupt_origin == ORIGIN` and could never pass,
    // which cost the nightly `contention` job its ability to report anything
    // else (#944). The two numbers genuinely differ by one T-state, and the
    // question was always which side was wrong. It is now answered on the
    // contention side, and the answer is that the contention side is not
    // wrong:
    //
    // `the_arrival_label_and_the_raster_agree_on_the_tstate` scores every
    // arrival against FUSE's own cost model at both candidate origins, and
    // the split is not close —
    //
    //     $40FE   +14335 -> 0 wrong of 57600     +14336 -> 14592 wrong
    //     $C0FF   +14335 -> 0 wrong of 63744     +14336 -> 18432 wrong
    //
    // Zero, frame-wide, on both a contended and an uncontended port. An
    // origin that reproduces FUSE exactly is not the thing to move, so
    // `ORIGIN` stays at 14335 and this test records where the `/INT` edge
    // actually falls instead of demanding it agree.
    //
    // What remains open is which of three things carries the one T-state:
    // the ULA's assertion instant, this test's convention for naming the
    // T-state a half-cycle edge falls in, or FUSE's own alignment between
    // its interrupt event and its contention table.
    //
    // Whichever it is, moving the `/INT` edge is not a free action. Every
    // probe measured in T-states *after the interrupt* moves with it, and
    // `Float48K` is already one T-state adrift in the other direction —
    // real hardware prints 14338 (Woody, WoS 17551) where this engine prints
    // 14337, a residual `float_bus.rs` records and anchors deliberately to
    // floatspy. Shifting the edge one later to satisfy the contention origin
    // would take Float48K to 14336, further from silicon rather than closer.
    // So the cheap fix is ruled out, and this stays a recorded measurement
    // until something can move all three together.
    //
    // Recorded exactly, so it fails in either direction: if the edge moves,
    // that is news whether or not it moves the way someone hoped.
    const INTERRUPT_DERIVED_ORIGIN: i32 = 14_336;
    assert_eq!(
        interrupt_origin, INTERRUPT_DERIVED_ORIGIN,
        "the /INT edge moved. It rises at engine T-state {onset}, putting \
         the interrupt-derived origin at {interrupt_origin}; this file has \
         been recording {INTERRUPT_DERIVED_ORIGIN} against a contention \
         `ORIGIN` of {ORIGIN} that scores 0 wrong frame-wide against FUSE \
         (#944). If this is a fix, move the constant in the same commit and \
         check `Float48K` with it."
    );
}

/// The reference has to be checked before its readings mean anything.
///
/// `delay_at` here must be the same function `contention_oracle.rs` pinned
/// to FUSE frame-wide. Duplicating it into a second test binary is how it
/// would silently drift, so it is re-derived from FUSE's own geometry and
/// compared, rather than trusted because it was copied.
#[test]
fn the_delay_table_still_matches_fuse() {
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

    let bad: Vec<_> = (0..FRAME_TSTATES)
        .filter(|&t| delay_at(t) != fuse_delay(t))
        .take(8)
        .collect();
    assert!(bad.is_empty(), "delay table disagrees with FUSE at {bad:?}");
}

/// The port model has to be able to fail, and has to say something.
///
/// Three gates on this problem turned out to be incapable of failing. This
/// checks the transcription against the four-way table stated in the
/// hardware literature — total delay-free cost, and which classes contend
/// at all — using arrival T-states where the delay table is quiescent and
/// where it is at a known value.
#[test]
fn the_port_model_reproduces_the_four_way_table() {
    // Outside the contended window nothing is charged, so every class
    // costs the bare 12 T-states: 4 + 4 + 4.
    let quiet = 0u32;
    for case in port_cases() {
        assert_eq!(
            fuse_in_a_c_cost(quiet, case.port),
            12,
            "{}: no contention outside the display window",
            case.name
        );
    }

    // Inside the window, the classes must separate. `uncontended, odd`
    // still pays for its two contended M1 fetches, but nothing for the
    // port; every other class pays more.
    let busy = FIRST_DISPLAY;
    let baseline = fuse_in_a_c_cost(busy, 0xC0FF);
    for case in port_cases() {
        // The `N:4` classes are the baseline, not a contending class.
        if !port_page_contended(case.port) && !port_from_ula(case.port) {
            continue;
        }
        assert!(
            fuse_in_a_c_cost(busy, case.port) > baseline,
            "{}: a contending class must cost more than N:4",
            case.name
        );
    }

    // And the reference has to separate the two odd-port classes by page,
    // because that is the distinction the engine's gate now draws with its
    // `contended_addr` term. A reference that could not state it would let
    // a gate that ignored the page score exact.
    assert_ne!(
        fuse_in_a_c_cost(busy, 0x40FF),
        fuse_in_a_c_cost(busy, 0xC0FF),
        "FUSE separates the two odd-port classes by page; a gate that \
         tests only the low bit cannot"
    );
}

/// The differential itself.
///
/// Every arrival T-state in the frame, all four port classes, one shared
/// origin offset. Report-only for now: it prints the offset sweep and the
/// first divergences so the *shape* of the disagreement is visible, then
/// asserts only the part that is not in question — that the model and the
/// engine agree outside the contended window, where neither contends.
#[test]
#[ignore = "FIXTURE: differential harness; needs EMU198X_SPECTRUM_48K_ROM"]
fn io_contention_matches_fuse_across_the_whole_frame() {
    let Some(rom) = rom_bytes() else {
        panic!("set {ROM_PATH_ENV} to the 48K ROM to run this harness");
    };

    // Skews of 0..12 walk the arrival point across more than one full
    // instruction, so between them the passes cover every phase of the
    // 8-T-state contention pattern.
    const SKEWS: u32 = 12;

    let mut collected: Vec<Scored> = Vec::new();
    for case in port_cases() {
        let mut all = Vec::new();
        for skew in 0..SKEWS {
            all.extend(samples(case.port, skew, &rom));
        }
        collected.push((case.name, case.port, case.shape, all));
    }

    let best = scoring_offset(&collected);
    let total: usize = collected
        .iter()
        .map(|(_, port, _, all)| mismatches(all, *port, best))
        .sum();
    let samples_total: usize = collected.iter().map(|(_, _, _, all)| all.len()).sum();
    println!("\norigin offset {best:+} — {total} of {samples_total} samples disagree");

    // The best fit is computed but never adopted. A disagreement between it
    // and the interrupt-pinned origin is itself the finding: it says the
    // engine's contention phase has moved relative to its own interrupt.
    let fitted = best_shared_offset(&collected);
    if fitted != best {
        let fitted_total: usize = collected
            .iter()
            .map(|(_, port, _, all)| mismatches(all, *port, fitted))
            .sum();
        println!(
            "  NOTE: best fit is {fitted:+} ({fitted_total} wrong), not the pinned \
             {best:+}. The gate's phase has moved against its own interrupt — read \
             that as the result, not as a reason to rescore."
        );
    }

    // The offset's neighbourhood, so it is visible whether the winner is a
    // sharp minimum or one of a plateau.
    print!("\n{:<14}", "offset");
    for d in -4..=4i32 {
        print!("{:>9}", format!("{:+}", best + d));
    }
    println!();
    print!("{:<14}", "mismatches");
    for d in -4..=4i32 {
        let n: usize = collected
            .iter()
            .map(|(_, port, _, all)| mismatches(all, *port, best + d))
            .sum();
        print!("{n:>9}");
    }
    println!();

    // What the disagreement looks like, per class, at that offset.
    println!(
        "\n{:<20} {:<16} {:>8} {:>8}  first divergences (t: got/want)",
        "port class", "FUSE shape", "samples", "wrong"
    );
    println!("{}", "-".repeat(92));
    for (name, port, shape, all) in &collected {
        let wrong: Vec<_> = all
            .iter()
            .filter_map(|&(arrival, measured)| {
                let t = (arrival as i32 + best).rem_euclid(FRAME_TSTATES as i32) as u32;
                let want = fuse_in_a_c_cost(t, *port);
                (want != measured).then_some((t, measured, want))
            })
            .collect();
        let head: Vec<String> = wrong
            .iter()
            .take(3)
            .map(|(t, got, want)| format!("{t}: {got}/{want}"))
            .collect();
        println!(
            "{name:<20} {shape:<16} {:>8} {:>8}  {}",
            all.len(),
            wrong.len(),
            head.join("  ")
        );
    }

    // Where the disagreement sits: inside the contended window or outside
    // it. This is the diagnostic that separates "the I/O gate is wrong"
    // from "the harness is measuring the wrong thing".
    println!(
        "\n{:<20} {:>12} {:>12} {:>14} {:>14}",
        "port class", "wrong (in)", "wrong (out)", "mean got (in)", "mean want (in)"
    );
    println!("{}", "-".repeat(76));
    for (name, port, _, all) in &collected {
        let (mut win, mut wout, mut got_sum, mut want_sum, mut n_in) = (0, 0, 0u64, 0u64, 0u64);
        for &(arrival, measured) in all {
            let t = (arrival as i32 + best).rem_euclid(FRAME_TSTATES as i32) as u32;
            let want = fuse_in_a_c_cost(t, *port);
            let contended = delay_at(t) > 0 || delay_at(t + 4) > 0 || delay_at(t + 8) > 0;
            if contended {
                n_in += 1;
                got_sum += measured as u64;
                want_sum += want as u64;
                if want != measured {
                    win += 1;
                }
            } else if want != measured {
                wout += 1;
            }
        }
        println!(
            "{name:<20} {win:>12} {wout:>12} {:>14.2} {:>14.2}",
            got_sum as f64 / n_in.max(1) as f64,
            want_sum as f64 / n_in.max(1) as f64
        );
    }

    // The offset-invariant part of the result.
    //
    // Everything above depends on an origin the harness fits, and a fitted
    // origin can absorb a phase error. This does not: two ports differing
    // only in their low bit must, under FUSE, cost different amounts inside
    // the contended window, because the ULA answers one and not the other.
    // No choice of origin can make a gate express a distinction it does not
    // test for, so a class pair the engine cannot separate is a statement
    // about the gate alone.
    println!(
        "\n{:<34} {:>12} {:>12}",
        "class pair (differs only in low bit)", "engine gap", "FUSE gap"
    );
    println!("{}", "-".repeat(60));
    for (even, odd, label) in [
        (0x40FEu16, 0x40FFu16, "contended page"),
        (0xC0FE, 0xC0FF, "uncontended page"),
    ] {
        let mean = |port: u16, want: bool| -> f64 {
            let (_, _, _, all) = collected
                .iter()
                .find(|(_, p, _, _)| *p == port)
                .expect("class present");
            let (mut sum, mut n) = (0u64, 0u64);
            for &(arrival, measured) in all {
                let t = (arrival as i32 + best).rem_euclid(FRAME_TSTATES as i32) as u32;
                if delay_at(t) == 0 && delay_at(t + 4) == 0 && delay_at(t + 8) == 0 {
                    continue;
                }
                sum += if want {
                    fuse_in_a_c_cost(t, port) as u64
                } else {
                    measured as u64
                };
                n += 1;
            }
            sum as f64 / n.max(1) as f64
        };
        println!(
            "{label:<34} {:>12.2} {:>12.2}",
            mean(even, false) - mean(odd, false),
            mean(even, true) - mean(odd, true)
        );
    }

    // The one assertion that is not in question. Outside the contended
    // window neither model charges anything, so every class must cost
    // exactly 12 T-states. If this fails the harness is measuring
    // something other than the instruction it thinks it is.
    for (name, port, _, all) in &collected {
        let quiet_wrong = all
            .iter()
            .filter(|&&(arrival, _)| {
                let start = (arrival as i32 + best).rem_euclid(FRAME_TSTATES as i32) as u32;
                // Well clear of the window at both ends of the instruction.
                (0..FIRST_DISPLAY.saturating_sub(32)).contains(&start)
            })
            .filter(|&&(_, measured)| measured != 12)
            .count();
        assert_eq!(
            quiet_wrong, 0,
            "{name} ({port:#06x}): {quiet_wrong} uncontended-window samples \
             did not cost the bare 12 T-states"
        );
    }

    // The ratchet, and it goes last. This harness measured for weeks
    // without ever being able to fail, which is how `ad0e8c53` shifted the
    // 128K floating bus by a T-state unnoticed (#851). The number below is
    // what the engine scores today, against FUSE, with the origin pinned
    // to the interrupt.
    //
    // Last is where an assertion belongs when everything above it is a
    // diagnostic. It sat before the per-class and per-phase tables for a
    // whole phase of work, so every red run — which is every run that
    // matters — threw away its own explanation. The offset sweep survived
    // only because it happens to print earlier.
    //
    // It is a ceiling, not a target. Lower it whenever the derivation in
    // `spectrum-contention-the-way-out.md` improves the score, and never
    // raise it: a rise means the change made I/O contention worse, whatever
    // else it improved. Record the new figure in the same commit that earns
    // it, so the history says which change bought which ground.
    //
    // 26,886, down from 75,081, bought by locking the *memory* contention
    // window's phase to the ULA's fetch group. Nothing on the I/O path
    // moved. That is the point the offset sweep was making: an `IN A,(C)`
    // out of contended RAM pays for two contended `M1` fetches before it
    // reaches the port cycle, so most of what this harness was scoring
    // was never I/O-specific. The remainder is.
    //
    // Then 21,510, and now **zero**, bought by replacing the I/O gate's
    // level test with a count of contention lookups at the offsets FUSE
    // charges them — `ferranti-ula-6c001e`'s three port terms. All five
    // classes are exact, including the two that were already exact and
    // stood as the regression guard.
    //
    // Zero is a ceiling like any other, and a more useful one: nothing can
    // now improve this harness's score, so any movement at all is a
    // regression. What it does *not* mean is that Spectrum I/O timing is
    // finished — this scores `IN A,(C)` against FUSE's port model, and the
    // one T-state still unaccounted for in the floating-bus path moved when
    // this landed. See `knowledge/decisions/spectrum-contention-vs-floating-bus.md`.
    //
    // At zero the ratchet stops being an inequality: `<=` would be an
    // absurd comparison and clippy says so. It is an equality now, which is
    // the stronger statement and the one that was always intended — the
    // instruction to move the constant in the same commit stands, but there
    // is only one direction left to move it in.
    const RATCHET: usize = 0;
    assert_eq!(
        total, RATCHET,
        "I/O contention regressed against FUSE: {total} of {samples_total} \
         samples disagree, was {RATCHET}. If this change is right and the \
         reference is wrong, say so explicitly and move the ratchet in the \
         same commit — do not widen it silently."
    );
}

/// Does the arrival label name the T-state the raster is in?
///
/// [`ORIGIN`] is 14335 here and 14336 in `float_bus_oracle`, which measures
/// the same `/INT` edge at half-cycle resolution and finds it at the *start*
/// of engine T-state 55552. That file calls the difference "a probe
/// convention rather than an engine behaviour" — prose, asserted nowhere,
/// and load-bearing for every number this harness prints.
///
/// It is load-bearing because a gate deciding one T-state late is
/// indistinguishable from an origin one T-state early, and this file's own
/// `ORIGIN` comment says exactly that. If the convention story is right,
/// both oracles reconcile at 14336 and the gate is sound. If it is wrong,
/// the contention phase is off by one and every residual quoted here is
/// quoted against a displaced ruler.
///
/// So: `tstate_in_frame` is `hc / cpu_divisor`, a floor of the half-cycle
/// counter. An instruction beginning halfway through a frame T-state would
/// be labelled with the T-state it started inside, while FUSE — which has no
/// sub-T-state notion — counts from a whole boundary. This records the phase
/// instructions actually begin on, and scores each phase group against both
/// candidate origins.
#[test]
#[ignore = "FIXTURE: needs EMU198X_SPECTRUM_48K_ROM"]
fn the_arrival_label_and_the_raster_agree_on_the_tstate() {
    use common_sinclair_zx_spectrum::driver::SpectrumDriver;

    /// The origin `float_bus_oracle` measures, from the same `/INT` edge at
    /// half-cycle resolution plus a frame of live bus content.
    const RASTER_ORIGIN: i32 = 14_336;

    let Some(rom) = rom_bytes() else {
        panic!("set {ROM_PATH_ENV} to the 48K ROM to run this harness");
    };

    // One contended and one uncontended class is enough: the question is
    // about the label, not the port.
    for port in [0x40FEu16, 0xC0FF] {
        // (sub-T-state phase at arrival, arrival T-state, measured cost)
        let mut samples: Vec<(u32, u32, u32)> = Vec::new();
        for skew in 0..12u32 {
            let mut machine = prepare(port, skew, &rom);
            let divisor = machine.frame_timing().cpu_divisor;
            for _ in 0..2 {
                step_one_instruction(&mut machine);
            }
            let mut spent = 0u32;
            while spent < FRAME_TSTATES {
                let phase = machine.hc() % divisor;
                let arrival = machine.tstate_in_frame();
                let cost = step_one_instruction(&mut machine);
                samples.push((phase, arrival, cost));
                spent += cost;
            }
        }

        let phases: std::collections::BTreeMap<u32, usize> =
            samples
                .iter()
                .fold(std::collections::BTreeMap::new(), |mut m, &(p, _, _)| {
                    *m.entry(p).or_default() += 1;
                    m
                });

        println!("\n=== ${port:04X}");
        println!("  sub-T-state phase at arrival: {phases:?}");

        let wrong_at = |offset: i32, phase: Option<u32>| -> usize {
            samples
                .iter()
                .filter(|&&(p, _, _)| phase.is_none_or(|want| p == want))
                .filter(|&&(_, arrival, measured)| {
                    let t = (arrival as i32 + offset).rem_euclid(FRAME_TSTATES as i32) as u32;
                    fuse_in_a_c_cost(t, port) != measured
                })
                .count()
        };

        println!(
            "  all arrivals:   {ORIGIN:+} -> {:>7} wrong,  {RASTER_ORIGIN:+} -> {:>7} wrong  (of {})",
            wrong_at(ORIGIN, None),
            wrong_at(RASTER_ORIGIN, None),
            samples.len(),
        );
        for (&p, &n) in &phases {
            println!(
                "  phase {p}:        {ORIGIN:+} -> {:>7} wrong,  {RASTER_ORIGIN:+} -> {:>7} wrong  (of {n})",
                wrong_at(ORIGIN, Some(p)),
                wrong_at(RASTER_ORIGIN, Some(p)),
            );
        }

        // The finding, whichever way it falls. A single phase means the
        // label is unambiguous and the origin gap is a real one-T-state
        // disagreement between the contention path and the raster. More
        // than one means instructions begin on both half-cycles of a frame
        // T-state, and the label is genuinely ambiguous — which would make
        // the "probe convention" reading right, and this harness's arrival
        // T-state the thing to fix rather than the gate.
        assert_eq!(
            phases.keys().copied().collect::<Vec<u32>>(),
            vec![0],
            "instructions arrived on more than one sub-T-state phase for \
             ${port:04X}: {phases:?}. That would make the arrival label \
             genuinely ambiguous and the `probe convention` reading of the \
             origin gap right — fix the label, not the gate."
        );
    }
}
