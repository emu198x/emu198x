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
    setup(&mut m);

    // Settle well inside the display window, where the table is live.
    while m.tstate_in_frame() < 20_000 {
        m.advance_tstates(1);
    }
    m.z80_mut().regs.pc = CODE_BASE;

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

fn report_io(port: u16, label: &str) {
    let observed = observe_io(port, 600);

    println!("\n=== IN A,(C) on ${port:04X} — {label}");
    println!(
        "{:<22} {:>5} {:>5} {:>5} {:>6} {:>6} {:>6} {:>6}",
        "phase", "addr", "MREQ", "IORQ", "pixel", "clkhi", "table", "clock"
    );
    println!("{}", "-".repeat(70));
    for o in observed.iter().skip(24).take(40) {
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
    println!(
        "\nhalf-cycles with /IORQ asserted: {io_halfcycles}\n\
         stall episodes (length in half-cycles): {episodes:?}\n\
         total stalled half-cycles: {}",
        episodes.iter().sum::<usize>(),
    );
}

#[test]
#[ignore = "needs EMU198X_SPECTRUM_48K_ROM"]
fn report_where_io_contention_arms() {
    // FUSE's four-way table, from `ula_contend_port_early` /
    // `ula_contend_port_late` in `peripherals/ula.c`, distinguishes these
    // by the port's page and whether the ULA answers the port at all.
    report_io(0x40FE, "ULA port, contended page — FUSE: C:1, C:3");
    report_io(0x00FE, "ULA port, uncontended page — FUSE: N:1, C:3");
    report_io(
        0x40FF,
        "odd port, contended page — FUSE: C:1, C:1, C:1, C:1",
    );
    report_io(0x00FF, "odd port, uncontended page — FUSE: N:4");
}
