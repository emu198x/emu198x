//! `HALT`-to-interrupt differential: score the engine's interrupt
//! acknowledge against FUSE's, at every phase of the `HALT` refetch grid.
//!
//! ## Why this exists
//!
//! Float48K reads 14336 where FUSE and Woody's hardware read 14338, and
//! the two T-states are not in the floating-bus read. That path has been
//! eliminated at both ends:
//!
//! - the bus *pattern* matches FUSE at every T-state in the frame
//!   (`floating_bus_matches_fuse_at_every_tstate`);
//! - the *sample instant* matches FUSE for `IN A,(C)`
//!   (`the_in_path_samples_the_bus_where_fuse_does`, 0 wrong) and for
//!   `IN A,(n)` — the instruction Float48K actually uses — at every
//!   arrival T-state (`the_in_a_n_sample_instant_matches_fuse`).
//!
//! And the probe's timed window is provably free of contention: the
//! engine's cumulative contention-stall count is identical at the sync
//! interrupt and at the `IN`, every iteration. So what is left is the
//! **arrival** — where the `IN` lands relative to the interrupt — and
//! Float48K reaches it through `HALT` twice.
//!
//! Two `HALT` syncs, two T-states missing. That is a coincidence worth
//! measuring rather than believing, which is what this harness does.
//!
//! ## The reference
//!
//! FUSE 1.7.0, vendored at `198x/emulators/zx-spectrum/fuse-1.7.0` — the
//! build the hardware comparison was taken under. Three pieces:
//!
//! `opcodes_base.c:523` — `HALT` is an ordinary four-T-state `M1` that
//! refetches itself, with no special exit cost:
//!
//! ```c
//! case 0x76:              /* HALT */
//!   z80.halted=1;
//!   PC--;
//!   break;
//! ```
//!
//! `spectrum.c:91` — the frame event wraps the T-state counter and calls
//! `z80_interrupt()` immediately, and FUSE's event loop runs events only
//! at instruction boundaries. So the interrupt is accepted at the first
//! `HALT` refetch boundary at or after `/INT`, never inside one.
//!
//! `z80.c:202` — the acknowledge itself, and the line this harness is
//! really about:
//!
//! ```c
//! if( z80.halted ) { PC++; z80.halted = 0; }
//! IFF1=IFF2=0;
//! R++;
//! tstates += 7;                                   /* Longer than usual M1 */
//! writebyte( --SP, PCH ); writebyte( --SP, PCL ); /* 3 + 3 */
//! /* IM 2: */ PCL = readbyte(inttemp++); PCH = readbyte(inttemp); /* 3 + 3 */
//! ```
//!
//! **Leaving `HALT` costs nothing.** The flag is cleared, `PC` is stepped
//! past the opcode, and the acknowledge is 7 + 3 + 3 + 3 + 3 = **19**
//! T-states in IM 2 with everything uncontended. There is no phantom
//! fetch between `/INT` and the acknowledge.
//!
//! ## Why it is shaped like this
//!
//! Two things separate a wrong acceptance *instant* from a wrong
//! acknowledge *cost*, and only measuring both tells them apart. So the
//! harness records pin-level `M1` **rising edges** — `/M1` is asserted
//! across `T1`–`T2`, so a level test names whichever T-state the poll
//! landed on — and scores the two intervals independently:
//!
//! | event | pins |
//! |---|---|
//! | the refetch grid's origin | `M1` edge, `addr == HALT_ADDR` |
//! | each phantom refetch | `M1` edge with `halt` asserted |
//! | the handler's first fetch | `M1` edge, `addr == ISR_ADDR` |
//!
//! The acknowledge's start is taken as the last refetch plus four —
//! the instruction boundary — and deliberately **not** as the `/IORQ`
//! edge: `/IORQ` falls a T-state into the acknowledge `M1`, so timing
//! from it would understate the acknowledge by one and overstate the
//! wait by one, turning one error into two.
//!
//! Everything is stated **relative to the engine's own `/INT`
//! assertion**, which both implementations define and neither derives
//! from contention. That keeps the harness clear of `ORIGIN` — the
//! constant the floating-bus and contention paths still disagree about by
//! one T-state — so a verdict here cannot be an artefact of picking a
//! side in that argument.
//!
//! Every address in play is uncontended: `HALT` at `$9000`, the stack at
//! `$A000`, the IM 2 table at `$C000`, the handler at `$C1C1`. A
//! contention charge anywhere would confound the interval being measured,
//! and `the_measured_window_is_free_of_contention` asserts it does not
//! happen rather than assuming it.
//!
//! The `HALT` is reached by execution rather than by parking `PC` on it,
//! and its `M1` is *measured* rather than assumed, because setting `PC`
//! mid-M-cycle lands the CPU on whatever phase the skew leaves it on —
//! which is the whole quantity under test.
//!
//! ## What it found
//!
//! On first run the acknowledge **cost** was FUSE-exact — 19 T-states on
//! every phase — while the acceptance **instant** was four T-states late
//! on the one phase in four where `/INT` goes active exactly on a refetch
//! boundary. The engine required `/INT` asserted strictly *before* the
//! boundary; FUSE accepts one asserted *at* it.
//!
//! That defect is fixed. The Z80 now samples `/INT` at the boundary and
//! the Spectrum driver feeds the pin before ticking the CPU — two half-
//! T-state lags on the same signal, which is why correcting either alone
//! measured as no change at all. All four tests here pass, the
//! ZXSpectrum4.net timing survey went from eight failing cases to zero,
//! and Float48K moved onto its expected T-state. See
//! `spectrum-contention-vs-floating-bus.md` and, for the Zilog-versus-FUSE
//! question the fix settles by choosing FUSE,
//! `zilog-z80-samples-int-at-the-instruction-boundary.md`.
//!
//! ```sh
//! cargo test -p machine-sinclair-zx-spectrum-48k \
//!     --test halt_interrupt_oracle -- --nocapture
//! ```

use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::ula::Ula;
use machine_sinclair_zx_spectrum_48k::Spectrum48k;

/// Uncontended RAM holding the `NOP` run and the `HALT`.
const CODE_ADDR: u16 = 0x9000;
/// Where the `HALT` opcode sits, after the alignment `NOP`s.
const HALT_ADDR: u16 = 0x9010;
/// Uncontended stack, so the acknowledge's two pushes cost 3 each.
const STACK: u16 = 0xA000;
/// IM 2 vector table, filled so any data-bus value selects one handler.
const TABLE_BASE: u16 = 0xC000;
/// The byte the table is filled with, and therefore both halves of the
/// vector: the handler is at `$C1C1`.
const TABLE_FILL: u8 = 0xC1;
/// The interrupt handler, reached whatever the ULA leaves on the bus.
const ISR_ADDR: u16 = 0xC1C1;

/// FUSE's IM 2 acknowledge, uncontended: `tstates += 7`, then two
/// `writebyte`s and two `readbyte`s at 3 apiece (`z80.c:205-228`).
const FUSE_ACK_TSTATES: u32 = 19;

/// `HALT` refetches itself with a plain `M1`.
const HALT_REFETCH_TSTATES: u32 = 4;

const FRAME_TSTATES: u32 = 69_888;

/// One sample: what the pins said, in engine frame T-states.
///
/// Every field is an `M1` **rising edge** rather than a level. `/M1` is
/// asserted across `T1`–`T2`, so a level test names whichever T-state the
/// poll happened to land on; the edge names the M-cycle.
#[derive(Debug, Clone, Copy)]
struct Sample {
    /// Frame T-state at which `/INT` went active.
    int_assert: u32,
    /// `M1` of the `HALT` opcode itself. The refetch grid runs on
    /// four-T-state centres from here.
    halt_fetch: u32,
    /// `M1` of the last phantom refetch before the acknowledge.
    last_refetch: u32,
    /// How many phantom refetches ran, for the record.
    refetches: u32,
    /// `M1` of the handler's first opcode fetch.
    isr_start: u32,
    /// Contention stalls observed across the measured window.
    stalls: u32,
}

impl Sample {
    /// The instruction boundary the acknowledge started from: the
    /// refetch in progress runs to completion, then the CPU acknowledges.
    ///
    /// This is the engine's answer to the question FUSE answers with
    /// "the first event check after the current instruction", and it is
    /// deliberately *not* the `/IORQ` edge — `/IORQ` falls a T-state into
    /// the acknowledge `M1`, so timing from it would understate the
    /// acknowledge by one and overstate the wait by one.
    fn ack_boundary(&self) -> i64 {
        i64::from(self.last_refetch) + i64::from(HALT_REFETCH_TSTATES)
    }

    /// FUSE's acknowledge boundary: the first refetch grid point at or
    /// after `/INT`. `spectrum.c:91` schedules the interrupt at the frame
    /// wrap and FUSE runs events only between instructions, so the CPU
    /// finishes the refetch it is in and acknowledges from there.
    fn fuse_ack_boundary(&self) -> i64 {
        let grid = i64::from(self.halt_fetch);
        let int = i64::from(self.int_assert);
        let step = i64::from(HALT_REFETCH_TSTATES);
        // The grid runs `halt_fetch + 4k`; take the first at or after
        // `/INT`. `HALT`'s own `M1` counts as `k = 0`.
        grid + step
            * ((int - grid).div_euclid(step) + i64::from((int - grid).rem_euclid(step) != 0))
    }

    /// The phase of the refetch grid relative to `/INT`, which is the
    /// only thing FUSE's wait depends on.
    fn grid_phase(&self) -> i64 {
        (i64::from(self.halt_fetch) - i64::from(self.int_assert))
            .rem_euclid(i64::from(HALT_REFETCH_TSTATES))
    }

    /// The acknowledge's own cost: instruction boundary to the handler's
    /// first opcode fetch.
    fn ack_cost(&self) -> i64 {
        i64::from(self.isr_start) - self.ack_boundary()
    }
}

/// Build a machine parked in uncontended RAM with IM 2 armed.
///
/// No ROM: every address the CPU touches is written here, so the harness
/// runs in CI rather than only where a 48K image happens to be present.
///
/// `phase_fillers` seven-T-state `LD A,n`s lead the run-in. With every
/// address uncontended the CPU's `M1` lattice is a fixed four-T-state
/// grid across the whole frame, so *starting* the run-in at a different
/// T-state cannot move the `HALT`'s phase — the first version of this
/// harness swept eight skews and got one phase eight times. Seven is
/// three modulo four, so `k` fillers rotate the grid by `3k`, and
/// `k = 0..4` reaches every residue.
fn prepare(phase_fillers: u32) -> Spectrum48k {
    let mut machine = Spectrum48k::new();
    machine.reset();

    // A run-in into a `HALT`, so the CPU arrives at the `HALT` by
    // executing rather than by having `PC` planted on it.
    for addr in CODE_ADDR..HALT_ADDR {
        machine.memory_mut().write(addr, 0x00);
    }
    for i in 0..phase_fillers {
        let addr = CODE_ADDR + (i * 2) as u16;
        machine.memory_mut().write(addr, 0x3E); // LD A,n — 7 T-states
        machine.memory_mut().write(addr + 1, 0x00);
    }
    machine.memory_mut().write(HALT_ADDR, 0x76);

    // Fill the vector table with one byte so both halves of the vector
    // read the same, whatever the ULA leaves on the data bus.
    for addr in TABLE_BASE..=TABLE_BASE + 0x100 {
        machine.memory_mut().write(addr, TABLE_FILL);
    }
    // The handler: a `NOP`, then a `HALT` to park it again.
    machine.memory_mut().write(ISR_ADDR, 0x00);
    machine.memory_mut().write(ISR_ADDR + 1, 0x76);

    machine.z80_mut().regs.sp = STACK;
    machine.z80_mut().regs.i = (TABLE_BASE >> 8) as u8;
    machine.z80_mut().regs.im = 2;
    machine.z80_mut().regs.iff1 = true;
    machine.z80_mut().regs.iff2 = true;
    machine
}

/// The engine's frame T-state at which `/INT` goes active, found by
/// watching the pin rather than by quoting a constant.
///
/// The ULA raises `/INT` well before the frame counter wraps, so a
/// harness that parks relative to `FRAME_TSTATES` waits for an edge that
/// has already been and gone. Measuring it also keeps this file clear of
/// `ORIGIN`: every interval below is stated against this edge.
///
/// Measured at **half-cycle** resolution, and deliberately so. Polling
/// with `advance_tstates(1)` and reading `tstate_in_frame()` afterwards
/// reports the T-state *after* the one the edge fell in — the first
/// version of this harness did exactly that and put `/INT` at 55553
/// where the pin rises at the start of 55552, which moved every FUSE
/// boundary it computes.
fn int_assert_tstate() -> u32 {
    let mut machine = prepare(0);
    // Interrupts off, so the CPU cannot take the edge we are looking for.
    machine.z80_mut().regs.iff1 = false;
    machine.z80_mut().regs.iff2 = false;
    let divisor = machine.frame_timing().cpu_divisor;
    let mut prev = machine.ula().interrupt_active();
    for _ in 0..=(FRAME_TSTATES * divisor) {
        machine.advance_halfcycles(1);
        let now = machine.ula().interrupt_active();
        if now && !prev {
            // `hc` has already been incremented past the tick that
            // raised the pin, so name that tick rather than this one.
            return (machine.hc() - 1) / divisor;
        }
        prev = now;
    }
    panic!("`/INT` never went active across a whole frame");
}

/// Run one sample at the given skew, or `None` if the run did not reach
/// every event it needs (which the caller treats as a harness failure,
/// not a pass).
fn sample(phase_fillers: u32, run_in: u32) -> Option<Sample> {
    let mut machine = prepare(phase_fillers);

    // Park a few hundred T-states ahead of `/INT` — long enough to be
    // settled in `HALT`, short enough that the run stays cheap.
    let divisor = machine.frame_timing().cpu_divisor;
    let start = int_assert_tstate() - run_in;
    while machine.tstate_in_frame() != start {
        machine.advance_tstates(1);
    }
    machine.z80_mut().regs.pc = CODE_ADDR;

    let mut halt_fetch = None;
    let mut last_refetch = None;
    let mut refetches = 0u32;
    let mut int_assert = None;
    let mut isr_start = None;
    let mut stalls = 0u32;
    let mut prev_int = machine.ula().interrupt_active();
    let mut prev_m1 = machine.z80().m1;
    let mut prev_stalled = false;

    // Stepped a master tick at a time, not a T-state at a time. Reading
    // the pins once per T-state names the T-state *after* the edge —
    // `/INT` rises at the first master tick of 55552 and a per-T-state
    // poll reports 55553 — and while every interval here is a difference
    // and so survives a uniform shift, the printed T-states would not be
    // the machine's. This costs four times the iterations and buys
    // numbers that can be compared with a trace.
    for _ in 0..(4_000u32 * divisor) {
        machine.advance_halfcycles(1);
        // `hc` is already past the tick that produced this state.
        let t = (machine.hc().wrapping_sub(1)) / divisor;
        let z80 = machine.z80();
        let m1_edge = z80.m1 && !prev_m1;
        prev_m1 = z80.m1;

        if m1_edge && !z80.iorq {
            if z80.addr == HALT_ADDR && halt_fetch.is_none() {
                // The `HALT` opcode's own `M1`, which sets the grid.
                halt_fetch = Some(t);
            } else if halt_fetch.is_some() && z80.halt {
                // A phantom refetch. Our Z80 drives `HALT_ADDR + 1`
                // during these (`halt_phantom_fetch_reads_post_halt_-
                // address_and_ignores_data` in `zilog-z80`), so this
                // keys off the `HALT` pin rather than the address.
                last_refetch = Some(t);
                refetches += 1;
            } else if halt_fetch.is_some() && z80.addr == ISR_ADDR && isr_start.is_none() {
                isr_start = Some(t);
            }
        }

        let int_now = machine.ula().interrupt_active();
        if int_now && !prev_int {
            int_assert = Some(t);
        }
        prev_int = int_now;

        // Count stall *episodes*, not stalled master ticks: a single
        // contention event spans several.
        let stalled = !machine.ula().cpu_clock_active();
        if halt_fetch.is_some() && isr_start.is_none() && stalled && !prev_stalled {
            stalls += 1;
        }
        prev_stalled = stalled;

        if isr_start.is_some() {
            break;
        }
    }

    Some(Sample {
        int_assert: int_assert?,
        halt_fetch: halt_fetch?,
        last_refetch: last_refetch?,
        refetches,
        isr_start: isr_start?,
        stalls,
    })
}

/// Four grid phases, each at two run-in lengths — so a result that
/// depends on where in the frame the `HALT` was entered, rather than on
/// the grid's phase, shows up as a disagreement between the pairs.
const CASES: [(u32, u32); 8] = [
    (0, 500),
    (1, 500),
    (2, 500),
    (3, 500),
    (0, 733),
    (1, 733),
    (2, 733),
    (3, 733),
];

/// The sweep, with the count and the phase coverage asserted **here**
/// rather than in one test.
///
/// A `filter_map` that silently drops failed runs would leave every
/// assertion below quantifying over an empty vector and passing — which
/// is the shape `a-gate-nobody-runs-is-a-silent-gate.md` is about, and
/// which this harness did on its first run: three tests reported `ok`
/// against nothing at all.
fn samples() -> Vec<Sample> {
    let samples: Vec<Sample> = CASES
        .iter()
        .filter_map(|&(fillers, run_in)| sample(fillers, run_in))
        .collect();
    assert_eq!(
        samples.len(),
        CASES.len(),
        "only {} of {} cases reached every pin event the sample needs, \
         so the assertions below would be quantifying over a short \
         vector rather than the sweep",
        samples.len(),
        CASES.len(),
    );
    let phases: std::collections::BTreeSet<i64> = samples.iter().map(Sample::grid_phase).collect();
    assert_eq!(
        phases.len(),
        HALT_REFETCH_TSTATES as usize,
        "the sweep covered {:?} of the four refetch-grid phases. The \
         wait before an acknowledge is a function of that phase and \
         nothing else, so a sweep that misses phases cannot see a \
         phase-dependent error — the first version of this harness \
         swept eight start T-states and hit one phase eight times, \
         because an uncontended frame keeps the CPU's `M1` lattice \
         fixed.",
        phases,
    );
    samples
}

/// The harness has to be measuring an uncontended window, or the
/// intervals below are a statement about contention instead.
#[test]
fn the_measured_window_is_free_of_contention() {
    let samples = samples();
    for s in &samples {
        assert_eq!(
            s.stalls, 0,
            "the measured window took {} contention stalls, so the \
             acknowledge intervals are not a clean measurement: {s:?}",
            s.stalls
        );
    }
}

/// The acceptance instant: the acknowledge must begin at the first
/// `HALT` refetch boundary at or after `/INT`.
///
/// **Currently fails on one phase in four**, and is `#[ignore]`d rather
/// than deleted or weakened so that re-landing is a one-line change. The
/// engine requires `/INT` to be asserted strictly before the boundary
/// where FUSE accepts one asserted at it, so a `/INT` coinciding with a
/// refetch boundary costs a whole extra refetch. Weakening the assertion
/// to match would pin the divergence in; asserting the *current*
/// behaviour would make a wrong answer the gate.
///
/// Listed in `knowledge/decisions/spectrum-contention-vs-floating-bus.md`
/// so the ignore is not forgotten — this repository already carries
/// `a-gate-nobody-runs-is-a-silent-gate.md` on exactly that risk.
#[test]
fn the_acknowledge_begins_at_the_first_refetch_boundary() {
    let samples = samples();

    println!(
        "\n{:>5} {:>6} {:>11} {:>11} {:>9} {:>11} {:>11}",
        "case", "phase", "int_assert", "halt_fetch", "refetch", "boundary", "fuse"
    );
    for (i, s) in samples.iter().enumerate() {
        println!(
            "{i:>5} {:>6} {:>11} {:>11} {:>9} {:>11} {:>11}",
            s.grid_phase(),
            s.int_assert,
            s.halt_fetch,
            s.refetches,
            s.ack_boundary(),
            s.fuse_ack_boundary(),
        );
    }

    let wrong: Vec<&Sample> = samples
        .iter()
        .filter(|s| s.ack_boundary() != s.fuse_ack_boundary())
        .collect();
    assert!(
        wrong.is_empty(),
        "the acknowledge began at the wrong instant on {} of {} phases. \
         FUSE accepts at the first `HALT` refetch boundary at or after \
         `/INT`: `spectrum.c:91` wraps the frame and calls \
         `z80_interrupt()` in the same event handler, and FUSE runs \
         events only between instructions, so the CPU finishes the \
         refetch it is in and acknowledges from there. A surplus of 4 is \
         one extra phantom refetch taken after `/INT` was already \
         asserted. Offenders: {wrong:?}",
        wrong.len(),
        samples.len(),
    );
}

/// The acknowledge's cost: 19 T-states in IM 2, uncontended, with no
/// phantom fetch charged for leaving `HALT`.
#[test]
fn the_acknowledge_costs_what_fuse_charges() {
    let samples = samples();

    println!(
        "\n{:>5} {:>11} {:>11} {:>9} {:>9}",
        "case", "boundary", "isr_start", "cost", "fuse"
    );
    for (i, s) in samples.iter().enumerate() {
        println!(
            "{i:>5} {:>11} {:>11} {:>9} {:>9}",
            s.ack_boundary(),
            s.isr_start,
            s.ack_cost(),
            FUSE_ACK_TSTATES,
        );
    }

    let wrong: Vec<&Sample> = samples
        .iter()
        .filter(|s| s.ack_cost() != i64::from(FUSE_ACK_TSTATES))
        .collect();
    assert!(
        wrong.is_empty(),
        "the IM 2 acknowledge cost the wrong number of T-states on {} of \
         {} phases, against FUSE's {FUSE_ACK_TSTATES}. FUSE charges \
         `tstates += 7` for the long `M1`, then two pushes and two vector \
         reads at 3 each, and leaving `HALT` costs nothing at all — \
         `if( z80.halted ) {{ PC++; z80.halted = 0; }}` (`z80.c:202`). \
         Offenders: {wrong:?}",
        wrong.len(),
        samples.len(),
    );
}

/// The whole quantity Float48K depends on, in one number: `/INT` to the
/// handler's first opcode fetch. Stated separately because it is the
/// interval the probe actually inherits, and because a compensating pair
/// of errors in the two tests above would leave both failing and this
/// one passing — which would itself be a finding.
///
/// Fails on the same one phase in four, and for the same reason: the
/// acknowledge's cost is exact, so the whole error is the wait in front
/// of it. `#[ignore]`d alongside its cause.
#[test]
fn the_int_to_handler_latency_matches_fuse() {
    let samples = samples();

    println!(
        "\n{:>5} {:>6} {:>11} {:>11} {:>9} {:>9}",
        "case", "phase", "int_assert", "isr_start", "latency", "fuse"
    );
    let mut wrong = Vec::new();
    for (i, s) in samples.iter().enumerate() {
        let latency = i64::from(s.isr_start) - i64::from(s.int_assert);
        let want = s.fuse_ack_boundary() - i64::from(s.int_assert) + i64::from(FUSE_ACK_TSTATES);
        println!(
            "{i:>5} {:>6} {:>11} {:>11} {latency:>9} {want:>9}",
            s.grid_phase(),
            s.int_assert,
            s.isr_start,
        );
        if latency != want {
            wrong.push((i, latency, want));
        }
    }

    assert!(
        wrong.is_empty(),
        "`/INT` to the handler's first fetch disagreed with FUSE on {} of \
         {} phases (case, got, want): {wrong:?}. This is the interval \
         Float48K's two `HALT` syncs each inherit.",
        wrong.len(),
        samples.len(),
    );
}
