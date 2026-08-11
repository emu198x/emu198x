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
    let rom = rom_bytes().expect("48K ROM should be provisioned");
    let mut m = Spectrum48k::new();
    m.load_rom_bytes(&rom).expect("48K ROM should load");
    m.reset();

    // `LD A,(HL)` repeated: an M1 fetch plus a memory read, both from
    // contended RAM, and nothing else to reason about.
    let mut addr = CODE_BASE;
    while addr < CODE_END {
        m.memory_mut().write(addr, 0x7E);
        addr += 1;
    }
    m.z80_mut().regs.hl = 0x5000;

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
