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
/// ## What it says, recorded 2026-08-11
///
/// | port | gate | FUSE | withheld runs begin on |
/// |---|---|---|---|
/// | `$40FE` | 2 | 2 | `T1Fall`, `T3Fall` |
/// | `$00FE` | 1 | 1 | `T3Rise` |
/// | `$40FF` | **2** | **4** | `T1Fall`, `T3Fall` |
/// | `$00FF` | 0 | 0 | — |
///
/// Three classes of four already agree, which is a narrower result than
/// the frame-wide differential's "the engine cannot separate `$40FE` from
/// `$40FF`" and contradicts part of it. The gate *does* separate `$00FE`
/// from `$40FE`. What it cannot do is charge a contended-page odd port
/// four times: `$40FF` and `$40FE` come out identical because neither of
/// their runs is the ULA-answers decode at all. `ula_io` is false for an
/// odd port, so the I/O term never fires on `$40FF`, and both runs are the
/// *memory* gate — `contended_addr && !cpu_mreq`, which holds for every
/// half-cycle of an I/O M-cycle because `/MREQ` is never asserted in one.
///
/// That the memory gate is what implements `contend_port_early` is not
/// obviously wrong — FUSE's early charge is conditioned on the port page,
/// exactly what the memory gate tests. What is missing is the rest of
/// `contend_port_late`'s odd-port branch.
#[test]
#[ignore = "KNOWN DIVERGENCE: $40FF gets two withheld runs per I/O M-cycle \
            where FUSE charges four; the other three classes agree. See the \
            table on this test. Also needs EMU198X_SPECTRUM_48K_ROM"]
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
