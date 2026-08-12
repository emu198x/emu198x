//! Where, inside an M-cycle, does the gate arm?
//!
//! The Sinclair ULA stalls the CPU *before* it commits to a contended
//! access: it sees a contended address with `/MREQ` still inactive, and
//! withholds the clock edge that would drop `/MREQ`. Smith describes the
//! circuit as detecting `T1` of a contended cycle by waiting for A14 high
//! with A15 and `MREQT23` low.
//!
//! Now that the Z80's strobes are the length Zilog draws them, a memory
//! read presents `/MREQ` for five of its six half-cycles. So the gate has
//! exactly **one** half-cycle per M-cycle in which it can arm — the one
//! where the CPU is about to run `T1Fall`. Everything therefore turns on
//! whether the gate's other term, `z80_clock_high`, is true on that
//! half-cycle. Nothing in the tree says which parity that is: it is a
//! free-running toggle seeded at `true` and frozen during stalls, and
//! before the pin fixes the gate had four loose half-cycles per M-cycle,
//! so either parity caught *something* and the question never had to be
//! answered.
//!
//! This measures it rather than arguing it.
//!
//! ## Why it is trustworthy
//!
//! Two earlier harnesses drove `FerrantiUla::tick` with synthetic pins and
//! both got the driver's tick order wrong, in opposite directions, and
//! both produced confident wrong verdicts. This one synthesises nothing:
//! it runs the real machine and reads the ULA's own recording, taken from
//! inside `tick`.
//!
//! The alignment between that recording and the CPU phase — which is the
//! only thing observed from outside — is **asserted, not assumed**. The
//! recording carries the `addr` and `/MREQ` the ULA was handed; the
//! external record carries what the CPU was driving at the half-cycle the
//! harness believes produced that entry. If the harness's idea of when
//! the ULA ticks is off by even one master cycle, those disagree and the
//! test fails as a harness fault, with that word in the message.
//!
//! ```sh
//! EMU198X_SPECTRUM_48K_ROM=... cargo test --release \
//!     -p machine-sinclair-zx-spectrum-48k --test contention_arming \
//!     -- --ignored --nocapture
//! ```

use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::ula_engine::DELAY_TABLE_48K;
use machine_sinclair_zx_spectrum_48k::Spectrum48k;

const ROM_PATH_ENV: &str = "EMU198X_SPECTRUM_48K_ROM";
const CODE_BASE: u16 = 0x4000;
const CODE_END: u16 = 0x8000;

fn rom_bytes() -> Option<Vec<u8>> {
    std::fs::read(std::env::var(ROM_PATH_ENV).ok()?).ok()
}

/// One observed half-cycle: what the CPU was about to do, and what the ULA
/// was given and decided.
struct Observed {
    /// The Z80 phase that ran during this half-cycle — recorded before the
    /// tick, so it names the half-cycle rather than the next one.
    phase: String,
    addr: u16,
    mreq: bool,
    iorq: bool,
    pixel: u16,
    video: bool,
    clock_high: bool,
    table: bool,
    /// `UlaEngine::mcycle_fall` as the contention decision saw it.
    mcycle_fall: u8,
    /// False when the ULA withheld this half-cycle's CPU clock edge.
    cpu_clock: bool,
}

/// Run `LD A,(HL)` out of contended RAM, reading contended RAM, and record
/// every ULA tick over `halfcycles` master cycles.
fn observe(halfcycles: u32) -> Vec<Observed> {
    // `LD A,(HL)` repeated: an M1 fetch plus a memory read, both from
    // contended RAM, and nothing else to reason about.
    observe_program(&[0x7E], |m| m.z80_mut().regs.hl = 0x5000, halfcycles)
}

/// The same, for `IN A,(C)` on a given port.
fn observe_io(port: u16, halfcycles: u32) -> Vec<Observed> {
    observe_program(
        &[0xED, 0x78],
        move |m| m.z80_mut().regs.bc = port,
        halfcycles,
    )
}

fn observe_program(
    program: &[u8],
    setup: impl FnOnce(&mut Spectrum48k),
    halfcycles: u32,
) -> Vec<Observed> {
    let rom = rom_bytes().expect("48K ROM should be provisioned");
    let mut m = Spectrum48k::new();
    m.load_rom_bytes(&rom).expect("48K ROM should load");
    m.reset();

    let mut addr = CODE_BASE;
    while addr < CODE_END {
        m.memory_mut()
            .write(addr, program[((addr - CODE_BASE) as usize) % program.len()]);
        addr += 1;
    }
    // Settle well inside the display window, where the table is live, then
    // run on to an instruction boundary before touching the registers.
    //
    // Order matters twice over. Settling runs ROM code, which rewrites
    // `BC` — so the registers must be set afterwards, not before. And the
    // settle stops mid-instruction, so a ROM instruction still in flight
    // would retire over the top of them. Getting either wrong measured
    // four port classes that were all secretly the same port.
    while m.tstate_in_frame() < 20_000 {
        m.advance_tstates(1);
    }
    let boundary = m.z80().instructions_retired() + 1;
    while m.z80().instructions_retired() < boundary {
        m.advance_tstates(1);
    }
    m.z80_mut().regs.pc = CODE_BASE;
    setup(&mut m);

    let divisor = m.frame_timing().cpu_divisor;
    let second = divisor / 2;

    let mut external = Vec::new();
    m.ula_mut().debug_trace_start();
    for _ in 0..halfcycles {
        let hc_phase = m.hc() % divisor;
        if hc_phase == 0 || hc_phase == second {
            // The ULA ticks on this master cycle, before the CPU does.
            external.push((
                format!("{:?}", m.z80().phase)
                    .replace("InternalPhase { remaining: ", "")
                    .replace(" }", ""),
                m.z80().addr,
                m.z80().mreq,
            ));
        }
        m.advance_halfcycles(1);
    }
    let trace = m.ula_mut().debug_trace_take();

    assert_eq!(
        trace.len(),
        external.len(),
        "harness fault, not a finding: the ULA ticked {} times where this \
         harness expected {}. Its idea of when the driver ticks the ULA is \
         wrong, and every alignment below would be wrong with it.",
        trace.len(),
        external.len(),
    );

    let mut out = Vec::new();
    for (i, (t, (phase, addr, mreq))) in trace.iter().zip(external).enumerate() {
        assert_eq!(
            (t.addr, t.mreq),
            (addr, mreq),
            "harness fault, not a finding: at entry {i} the ULA was handed \
             addr={:04X} mreq={} while the CPU was observed driving \
             addr={addr:04X} mreq={mreq}. The external record is skewed \
             against the recording; do not read any verdict from this run.",
            t.addr,
            t.mreq,
        );
        out.push(Observed {
            phase,
            addr,
            mreq,
            iorq: t.iorq,
            pixel: t.pixel,
            video: t.video,
            clock_high: t.clock_high_before,
            table: DELAY_TABLE_48K[(t.pixel as usize) & 0x0F],
            mcycle_fall: t.mcycle_fall,
            cpu_clock: t.cpu_clock_after,
        });
    }
    out
}

fn is_contended(addr: u16) -> bool {
    (0x4000..0x8000).contains(&addr)
}

#[test]
#[ignore = "needs EMU198X_SPECTRUM_48K_ROM"]
fn the_gate_arms_on_the_half_cycle_that_precedes_mreq() {
    let observed = observe(400);

    println!(
        "\n{:<22} {:>5} {:>5} {:>6} {:>6} {:>6} {:>6} {:>6}",
        "phase", "addr", "MREQ", "pixel", "video", "clkhi", "table", "clock"
    );
    println!("{}", "-".repeat(70));
    for o in observed.iter().take(64) {
        println!(
            "{:<22} {:>5X} {:>5} {:>6} {:>6} {:>6} {:>6} {:>6}",
            o.phase,
            o.addr,
            if o.mreq { "L" } else { "." },
            o.pixel,
            o.video,
            o.clock_high,
            o.table,
            if o.cpu_clock { "run" } else { "STALL" },
        );
    }

    // Self-check first, in the shape the +2A differential learned to use:
    // an instrument that measures no contention anywhere cannot be read as
    // evidence about where contention arms.
    let stalls = observed.iter().filter(|o| !o.cpu_clock).count();
    let display = observed.iter().filter(|o| o.video).count();
    println!("\nhalf-cycles recorded: {}", observed.len());
    println!("inside the display window: {display}");
    println!("stalled: {stalls}");

    assert!(
        display > 0,
        "harness fault, not a finding: none of the recorded half-cycles \
         are in the display window, so the delay table was never live."
    );

    // What the gate is actually offered. Every half-cycle where a
    // contended address sits on the bus with /MREQ inactive and the table
    // open is an arming opportunity; `z80_clock_high` is the only term
    // left to decide it.
    let offered: Vec<&Observed> = observed
        .iter()
        .filter(|o| o.video && is_contended(o.addr) && !o.mreq && o.table)
        .collect();
    let armed = offered.iter().filter(|o| o.clock_high).count();

    println!(
        "arming opportunities (contended addr, /MREQ idle, table open): {}",
        offered.len()
    );
    println!("  of those, with z80_clock_high true: {armed}");
    println!(
        "  of those, with z80_clock_high false: {}",
        offered.len() - armed
    );

    let phases: std::collections::BTreeMap<&str, usize> =
        offered
            .iter()
            .fold(std::collections::BTreeMap::new(), |mut m, o| {
                *m.entry(o.phase.as_str()).or_default() += 1;
                m
            });
    println!("  the half-cycles they fall on:");
    for (phase, n) in &phases {
        println!("    {phase:<22} {n}");
    }

    assert!(
        !offered.is_empty(),
        "harness fault, not a finding: no half-cycle in this window put a \
         contended address on the bus with /MREQ inactive and the table \
         open, so there was nothing for the gate to decide."
    );

    // The finding. The gate arms only where `z80_clock_high` is true, so
    // if every opportunity falls on the false parity the engine cannot
    // contend a memory access at all.
    assert!(
        stalls > 0,
        "the gate never stalled the CPU across {} half-cycles, with {} \
         arming opportunities offered. Every one of them fell on \
         z80_clock_high = {}. The parity of that term is the defect, not \
         a constant.",
        observed.len(),
        offered.len(),
        armed > 0,
    );
}

/// One stall episode: a run of consecutive withheld half-cycles.
fn stall_episodes(observed: &[Observed]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut run = 0usize;
    for o in observed {
        if o.cpu_clock {
            if run > 0 {
                out.push(run);
                run = 0;
            }
        } else {
            run += 1;
        }
    }
    if run > 0 {
        out.push(run);
    }
    out
}

/// One I/O read M-cycle, as the gate treated it.
///
/// `episodes` is the count that maps onto FUSE. A *charge point* there is
/// one consultation of the delay table; here it is one maximal run of
/// withheld half-cycles. Two charges landing on adjacent half-cycles would
/// merge into one run, so this is a lower bound on charge points — which
/// means a shortfall below is a real shortfall and an excess would be a
/// harness question.
struct IoCycle {
    stalled: usize,
    episodes: usize,
    /// The `IoRead` phase each withheld run began on.
    starts: Vec<String>,
    /// Was the delay table open anywhere in this M-cycle? A class that
    /// never meets an open table cannot contend whatever the gate says,
    /// so a zero reading from one of those means nothing.
    table_open: bool,
}

/// Split an observation into its I/O read M-cycles.
///
/// A stalled half-cycle does not advance the CPU, so the phase repeats;
/// the M-cycle is therefore the maximal run of consecutive `IoRead`
/// entries, not a fixed eight.
fn io_cycles(observed: &[Observed]) -> Vec<IoCycle> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < observed.len() {
        if !observed[i].phase.starts_with("IoRead") {
            i += 1;
            continue;
        }
        let start = i;
        while i < observed.len() && observed[i].phase.starts_with("IoRead") {
            i += 1;
        }
        let run = &observed[start..i];

        let mut episodes = 0usize;
        let mut starts = Vec::new();
        let mut prev_stalled = false;
        for o in run {
            let stalled = !o.cpu_clock;
            if stalled && !prev_stalled {
                episodes += 1;
                starts.push(o.phase.clone());
            }
            prev_stalled = stalled;
        }

        out.push(IoCycle {
            stalled: run.iter().filter(|o| !o.cpu_clock).count(),
            episodes,
            starts,
            table_open: run.iter().any(|o| o.video && o.table),
        });
    }
    out
}

/// FUSE's contention charge points for one `IN`, counted from
/// `ula_contend_port_early` and `ula_contend_port_late` in
/// `fuse-emulator-fuse/peripherals/ula.c`.
///
/// `early` consults `ula_contention_no_mreq` once when the port address
/// lies in a contended page. `late` consults it once when the ULA answers
/// the port, three times when it does not and the page is contended, and
/// not at all otherwise. That is the four-way table — `C:1, C:3` /
/// `N:1, C:3` / `C:1, C:1, C:1, C:1` / `N:4` — read as a count of the
/// points at which the raster is given the chance to stall the CPU.
///
/// Counting charge points rather than T-states is deliberate. A T-state
/// total depends on the arrival phase and on a frame origin, and this
/// project has had three separate errors hidden by a fitted origin. How
/// many times the gate can fire inside one I/O M-cycle depends on
/// neither: it is a property of the gate's terms alone, and no choice of
/// origin can give a gate a distinction it does not test for.
fn fuse_charge_points(port: u16) -> usize {
    let page_contended = (0x4000..0x8000).contains(&port);
    let answered_by_ula = port & 1 == 0;
    let early = usize::from(page_contended);
    let late = if answered_by_ula {
        1
    } else if page_contended {
        3
    } else {
        0
    };
    early + late
}

fn report_io(port: u16, label: &str) -> Vec<IoCycle> {
    // 2400 half-cycles is 600 T-states — nearly three scan lines, so the
    // I/O M-cycles land on every phase of the 8-T-state pattern rather
    // than on whichever few a short window happened to catch.
    let observed = observe_io(port, 2400);

    // Self-check: the port under test has to actually reach the bus. It is
    // not enough that the recording aligns with the CPU — the harness must
    // also be driving the thing it says it is. The first version of this
    // set `BC` before settling the machine, so ROM code overwrote it and
    // all four classes below ran on `$FFFF` and reported identical,
    // quotable, wrong results.
    let on_bus = observed.iter().filter(|o| o.iorq && o.addr == port).count();
    assert!(
        on_bus > 0,
        "harness fault, not a finding: no half-cycle put ${port:04X} on the \
         address bus with /IORQ asserted, so this run measured some other \
         port. Observed I/O addresses: {:?}",
        observed
            .iter()
            .filter(|o| o.iorq)
            .map(|o| format!("{:04X}", o.addr))
            .collect::<std::collections::BTreeSet<_>>(),
    );

    println!("\n=== IN A,(C) on ${port:04X} — {label}");
    println!(
        "{:<22} {:>5} {:>5} {:>5} {:>6} {:>6} {:>6} {:>6}",
        "phase", "addr", "MREQ", "IORQ", "pixel", "clkhi", "table", "clock"
    );
    println!("{}", "-".repeat(70));
    for o in observed
        .iter()
        .filter(|o| o.phase.starts_with("IoRead"))
        .take(24)
    {
        println!(
            "{:<22} {:>5X} {:>5} {:>5} {:>6} {:>6} {:>6} {:>6}",
            o.phase,
            o.addr,
            if o.mreq { "L" } else { "." },
            if o.iorq { "L" } else { "." },
            o.pixel,
            o.clock_high,
            o.table,
            if o.cpu_clock { "run" } else { "STALL" },
        );
    }

    let episodes = stall_episodes(&observed);
    let io_halfcycles = observed.iter().filter(|o| o.iorq).count();
    let cycles = io_cycles(&observed);

    let in_io_cycle: usize = cycles.iter().map(|c| c.stalled).sum();
    let with_table_open = cycles.iter().filter(|c| c.table_open).count();
    let max_episodes = cycles.iter().map(|c| c.episodes).max().unwrap_or(0);

    println!(
        "\nhalf-cycles with /IORQ asserted: {io_halfcycles}\n\
         stall episodes over the whole window: {episodes:?}\n\
         total stalled half-cycles: {}\n\
         I/O M-cycles observed: {}\n\
         of those, meeting an open delay table: {with_table_open}\n\
         stalled half-cycles inside them: {in_io_cycle}\n\
         most withheld runs in any one I/O M-cycle: {max_episodes} \
         (FUSE charges {} here)",
        episodes.iter().sum::<usize>(),
        cycles.len(),
        fuse_charge_points(port),
    );

    println!("\n  per I/O M-cycle — table, withheld half-cycles, runs, where they began");
    for (i, c) in cycles.iter().enumerate().take(16) {
        println!(
            "    {i:>2}  {:<6} {:>2} withheld  {:>1} run(s)  {:?}",
            if c.table_open { "open" } else { "shut" },
            c.stalled,
            c.episodes,
            c.starts,
        );
    }

    // Self-check, the second half of the pair this file paid for twice.
    // The first is in `observe_program`: the recording has to line up with
    // the CPU. This one is the other half — the run has to have met an
    // open delay table, or a class reading zero is reporting the border,
    // not the gate.
    assert!(
        with_table_open > 0,
        "harness fault, not a finding: none of the {} I/O M-cycles observed \
         for ${port:04X} met an open delay table, so this class could not \
         have contended whatever the gate does.",
        cycles.len(),
    );

    cycles
}

/// The four FUSE port classes, asserted rather than printed.
///
/// The memory side of this problem only became tractable once an
/// instrument named a half-cycle. The I/O side has had one 300,000-sample
/// aggregate in `io_contention_oracle` that can say a number moved and
/// nothing about *where*. This is the half-cycle-resolved counterpart: for
/// each of FUSE's four port classes it drives the real machine, records
/// from inside the ULA, and counts how many separate times the gate is
/// given the chance to stall the CPU inside one I/O M-cycle.
///
/// The reference is `ula_contend_port_early` / `_late` transcribed as
/// `fuse_charge_points`, not a constant lifted out of them.
///
/// ## What it said, recorded 2026-08-11, against the old level gate
///
/// | port | gate | FUSE | withheld runs begin on |
/// |---|---|---|---|
/// | `$40FE` | 2 | 2 | `T1Fall`, `T3Fall` |
/// | `$00FE` | 1 | 1 | `T3Rise` |
/// | `$40FF` | **2** | **4** | `T1Fall`, `T3Fall` |
/// | `$00FF` | 0 | 0 | — |
///
/// Three classes of four agreed, which was a narrower result than the
/// frame-wide differential's "the engine cannot separate `$40FE` from
/// `$40FF`" and contradicted part of it. What the gate could not do was
/// charge a contended-page odd port four times: `$40FF` and `$40FE` came
/// out identical because neither of their runs was the ULA-answers decode
/// at all. `ula_io` is false for an odd port, so the I/O term never fired
/// on `$40FF`, and both runs were the *memory* gate —
/// `contended_addr && !cpu_mreq`, which holds for every half-cycle of an
/// I/O M-cycle because `/MREQ` is never asserted in one.
///
/// That the memory gate is what implements `contend_port_early` turned out
/// to be right: FUSE's early charge is conditioned on the port page,
/// exactly what the memory gate tests, and the gate now leans on that
/// deliberately.
///
/// ## What it says now, and why it still cannot be the score
///
/// | port | gate | FUSE | charges the gate actually makes |
/// |---|---|---|---|
/// | `$40FE` | 1 | 2 | 2 — `T1Fall`, `T2Fall` |
/// | `$00FE` | 1 | 1 | 1 — `T2Fall` |
/// | `$40FF` | 2 | 4 | 4 — `T1Fall`..`T4Fall` |
/// | `$00FF` | 0 | 0 | 0 |
///
/// The charge counts are now FUSE's in every class —
/// `io_contention_oracle` scores 0 of 297,222 samples, per class, which is
/// the instrument that can see them. This one still reads low, and by
/// exactly the factor its own doc comment predicts: **a run merges
/// adjacent charges**. Two lookups one T-state apart withhold a contiguous
/// stretch of half-cycles and count as one run, so `$40FE`'s two charges
/// read 1 and `$40FF`'s four read 2.
///
/// That is not a defect to fix here. It is the reason
/// `io-contention-is-a-count-not-a-level.md` forbids scoring gate changes
/// against this file: three terms were tuned against a run count and died.
/// The `assert_ne!` at the end is the part of this test that survives the
/// merging, because it compares two classes rather than a class against a
/// number.
#[test]
#[ignore = "KNOWN LIMITATION: a withheld *run* merges charges landing one \
            T-state apart, so this reports 1 where the gate charges 2 and 2 \
            where it charges 4. Score I/O contention with \
            io_contention_oracle, which is at zero. See the tables on this \
            test. Also needs EMU198X_SPECTRUM_48K_ROM"]
fn io_contention_matches_the_four_fuse_port_classes() {
    let cases: [(u16, &str); 4] = [
        (0x40FE, "ULA port, contended page — FUSE: C:1, C:3"),
        (0x00FE, "ULA port, uncontended page — FUSE: N:1, C:3"),
        (
            0x40FF,
            "odd port, contended page — FUSE: C:1, C:1, C:1, C:1",
        ),
        (0x00FF, "odd port, uncontended page — FUSE: N:4"),
    ];

    let measured: Vec<(u16, Vec<IoCycle>)> = cases
        .iter()
        .map(|&(port, label)| (port, report_io(port, label)))
        .collect();

    let peak = |port: u16| -> usize {
        measured
            .iter()
            .find(|(p, _)| *p == port)
            .expect("class measured")
            .1
            .iter()
            .map(|c| c.episodes)
            .max()
            .unwrap_or(0)
    };

    println!("\n{:<8} {:>10} {:>10}", "port", "engine", "FUSE");
    println!("{}", "-".repeat(30));
    for &(port, _) in &cases {
        println!(
            "${port:04X}  {:>10} {:>10}",
            peak(port),
            fuse_charge_points(port)
        );
    }

    // Every class at once, rather than failing on the first. Which
    // classes agree is the diagnosis: three of four agreeing points at one
    // missing term, where four of four disagreeing would point at the
    // whole model.
    let wrong: Vec<String> = cases
        .iter()
        .filter(|&&(port, _)| peak(port) != fuse_charge_points(port))
        .map(|&(port, label)| {
            format!(
                "${port:04X} ({label}): gate {}, FUSE {}",
                peak(port),
                fuse_charge_points(port)
            )
        })
        .collect();
    assert!(
        wrong.is_empty(),
        "the gate and FUSE disagree on how many times the raster may stall \
         an I/O M-cycle:\n  {}\nThe per-M-cycle tables above say which \
         half-cycle each withheld run began on.",
        wrong.join("\n  "),
    );

    // The offset-invariant statement, kept separate because it is the one
    // no origin and no delay table can reach. `$40FE` and `$40FF` differ
    // only in the bit that decides whether the ULA answers the port, and
    // FUSE charges them two points and four. A gate that reaches them both
    // through a term testing something else must treat them alike.
    assert_ne!(
        peak(0x40FE),
        peak(0x40FF),
        "the gate gives $40FE and $40FF the same number of withheld runs \
         ({}), so whatever produced them is not the ULA-answers decode. \
         FUSE charges {} and {}.",
        peak(0x40FE),
        fuse_charge_points(0x40FE),
        fuse_charge_points(0x40FF),
    );
}

/// Where inside the I/O M-cycle each of FUSE's four lookups falls.
///
/// The gate can only express a *count* of lookups if it can name the
/// half-cycle each one lands on, and `UlaEngine::mcycle_fall` is the counter
/// that names them. Getting its origin wrong shifts every entry of the table
/// in `knowledge/decisions/io-contention-is-a-count-not-a-level.md` by one
/// and looks exactly like a phase error, so it is pinned here before any gate
/// reads it.
///
/// ## The correspondence
///
/// FUSE enters `readport` with `tstates` at the first T-state of the I/O
/// M-cycle — `IN A,(C)` is two `contend_read( PC, 4 )` calls and nothing
/// else beforehand — so its offsets 0, 1, 2 and 3 are that M-cycle's four
/// T-states in order. This state machine names them `T1`, `T2`, `T3`, `T4`
/// where Zilog names them `T1`, `T2`, `TW`, `T3` (see `tick_io_read`), which
/// leaves the mapping to the half-cycle:
///
/// | FUSE offset | Zilog | `IoPhase` | half-cycle |
/// |---|---|---|---|
/// | 0 | `T1` | `T1` | `T1Fall` |
/// | 1 | `T2` | `T2` | `T2Fall` |
/// | 2 | `TW` | `T3` | `T3Fall` |
/// | 3 | `T3` | `T4` | `T4Fall` |
///
/// The *falling* half-cycle of each, because that is the only one the gate
/// arms on — `the_gate_arms_on_the_half_cycle_that_precedes_mreq` above
/// measures that, and `UlaEngine::gate_arms_this_halfcycle` carries it.
///
/// That last column is not merely asserted. `io_contention_oracle` scores
/// the contended-odd class — FUSE's `C:1 C:1 C:1 C:1`, a lookup at every one
/// of the four offsets — at **zero wrong of 54,531 samples**, and the engine
/// reaches that class through a gate that arms on exactly these four falling
/// half-cycles and nothing else. Four lookups mapped onto four consecutive
/// T-states in order admit only the identity, so an exact class already
/// pins the origin; this test is what makes the pin fail loudly if the
/// M-cycle geometry underneath it ever moves.
///
/// ## What it checks
///
/// That `mcycle_fall` reads 1 on `T1Fall` and 2 on `T2Fall` — the two
/// lookups that happen before `/IORQ` is visible and which therefore have
/// nothing else to tell them apart — and that a reading of 2 happens *only*
/// there, so the counter cannot hand a spurious lookup to a memory cycle.
#[test]
#[ignore = "needs EMU198X_SPECTRUM_48K_ROM"]
fn the_io_lookup_offsets_are_pinned_to_the_falling_half_cycles() {
    /// The phase each FUSE offset's lookup must land on.
    const OFFSETS: [(&str, u8); 4] = [
        ("IoRead(T1Fall)", 1),
        ("IoRead(T2Fall)", 2),
        // Offsets 2 and 3 need no counter: `/IORQ` is visible by then, and
        // both carry the same rule, so the gate reads the pin directly.
        ("IoRead(T3Fall)", 0),
        ("IoRead(T4Fall)", 0),
    ];

    for port in [0x40FEu16, 0x00FE, 0x40FF, 0x00FF] {
        let observed = observe_io(port, 2400);

        // Self-check, the same one `report_io` pays for: the port has to
        // have reached the bus, or this is a measurement of some other port.
        let on_bus = observed.iter().filter(|o| o.iorq && o.addr == port).count();
        assert!(
            on_bus > 0,
            "harness fault, not a finding: no half-cycle put ${port:04X} on \
             the address bus with /IORQ asserted."
        );

        // What each phase of the M-cycle actually reads. Printed whole
        // because the assertions below only name four of them, and a
        // surprise anywhere else is the thing worth seeing.
        let mut census: std::collections::BTreeMap<String, std::collections::BTreeSet<(u8, bool)>> =
            std::collections::BTreeMap::new();
        for o in &observed {
            census
                .entry(o.phase.clone())
                .or_default()
                .insert((o.mcycle_fall, o.iorq));
        }
        println!("\n=== ${port:04X} — (mcycle_fall, IORQ) seen on each phase");
        for (phase, seen) in &census {
            println!("  {phase:<22} {seen:?}");
        }

        for (phase, want) in OFFSETS {
            let entries: Vec<&Observed> = observed.iter().filter(|o| o.phase == phase).collect();
            assert!(
                !entries.is_empty(),
                "${port:04X}: no half-cycle ran {phase}, so the offset it \
                 carries was never observed"
            );
            let wrong: Vec<u8> = entries
                .iter()
                .map(|o| o.mcycle_fall)
                .filter(|&got| got != want)
                .collect();
            assert!(
                wrong.is_empty(),
                "${port:04X}: {phase} read mcycle_fall {wrong:?} where the \
                 table needs {want}. The counter's origin is off, and every \
                 offset below it moves with it."
            );
            // The rule for offsets 2 and 3 is the `/IORQ` pin, so the pin
            // has to be up by then and down before then — one T-state of
            // slack either way would swap two lookups for two others.
            let iorq_wanted = want == 0;
            assert!(
                entries.iter().all(|o| o.iorq == iorq_wanted),
                "${port:04X}: {phase} disagrees about /IORQ — it must be \
                 {iorq_wanted} for the offsets the gate reads off the pin to \
                 be offsets 2 and 3 and no others"
            );
        }

        // The phase names above and the engine's own clock parity have to
        // agree about which half-cycles are falling ones, or the table's
        // right-hand column is naming something the gate cannot read.
        // `clock_high` false is what `gate_arms_this_halfcycle` returns true
        // for.
        let misnamed: Vec<&str> = observed
            .iter()
            .filter(|o| o.phase.ends_with("Fall)") == o.clock_high)
            .map(|o| o.phase.as_str())
            .collect();
        assert!(
            misnamed.is_empty(),
            "${port:04X}: {misnamed:?} disagree about their own half-cycle — \
             a phase named `Fall` must be one the gate arms on, and the \
             offsets are placed on falling half-cycles by name."
        );

        // Exclusivity, over the half-cycles the gate can act on. A count of 2
        // is the one reading nothing on the pins corroborates, so no other
        // arming half-cycle may reach it — an `Internal` cycle sitting on a
        // stale even address would otherwise be handed a lookup FUSE never
        // makes. The count is only advanced on falling half-cycles and so
        // still reads 2 on the rising one after `T2Fall`, where the gate's
        // own arming term discards it.
        let stray: std::collections::BTreeSet<&str> = observed
            .iter()
            .filter(|o| !o.clock_high && o.mcycle_fall >= 2 && o.phase != "IoRead(T2Fall)")
            .map(|o| o.phase.as_str())
            .collect();
        assert!(
            stray.is_empty(),
            "${port:04X}: mcycle_fall reached 2 or more on arming half-cycles \
             {stray:?}, not only on the I/O M-cycle's second T-state. Every \
             one of those is a lookup charged where FUSE charges none."
        );
    }
}
