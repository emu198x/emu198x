//! Floating-bus differential: score the ULA's *live* data bus against
//! FUSE's model, at every T-state in the frame, on the real machine.
//!
//! Two floating-bus things had been measured and one had not. The *sample
//! instant* is derived — `IO_READ_DATA_LATCH_LEAD_TSTATES`, fixed M-cycle
//! geometry, shared by every variant — and the *model* our `IN` path reads
//! from, `floating_bus_byte`, is checked against FUSE's
//! `spectrum_unattached_port` frame-wide by
//! `fuse_floating_bus_differential::matches_fuse_across_the_whole_frame`.
//!
//! What had never been scored against anything is the bus the ULA actually
//! drives. `IDLE_TABLE` and `MEM_TABLE` decide, per pixel, when the ULA is
//! fetching and what it leaves on the bus; Seam 1 shifted both four pixels;
//! and the only test that touched them re-stated their contents.
//!
//! ## What it found
//!
//! **The tables are right, and the anchor is not.** The live bus is
//! byte-exact against FUSE — 0 of 69,888 T-states — at one offset and one
//! only: `+14336`, libspectrum's `top_left_pixel` for this ULA. Every
//! neighbouring offset scores at least 15,360. `IDLE_TABLE` and `MEM_TABLE`
//! are therefore not the floating-bus defect, and neither is the model.
//!
//! `io_contention_oracle` scores against `+14335`, one T-state away, and
//! calls it pinned by the `/INT` edge. Measured at half-cycle resolution
//! rather than T-state resolution, the `/INT` edge agrees with the bus:
//! it rises at the *start* of engine T-state 55552, which puts the origin
//! at 69888 - 55552 = **14336**.
//!
//! The `14335` comes from the probe, not the engine.
//! `the_frame_origin_is_pinned_by_the_interrupt` advances a whole T-state
//! and *then* reads `interrupt_active()`, so it labels an edge that fell
//! during T-state *k* as *k+1*. Two independent anchors — an interrupt
//! and a frame of screen bytes — now agree exactly, which is what a frame
//! origin should look like.
//!
//! ```sh
//! cargo test --release -p machine-sinclair-zx-spectrum-48k \
//!     --test float_bus_oracle -- --ignored --nocapture
//! ```
//!
//! ## Why this shape
//!
//! The same shape as the two differentials that worked
//! (`io_contention_oracle`, `contention_oracle`), for the same reasons:
//!
//! - The reference is FUSE's `spectrum_unattached_port` transcribed whole
//!   and re-checked against our own model, not lifted from it.
//! - The subject is the engine's own `Ula::floating_bus()`, read on a real
//!   machine tick by tick, not a second model of it.
//! - **Every** T-state in the frame is scored. Frame totals are blind to
//!   phase here as well: the fetch pattern is sixteen whole 8-T-state
//!   groups per line, so a bus running a whole group late carries the same
//!   number of data slots.
//! - The origin is measured twice over and its uniqueness asserted, not
//!   fitted. A fitted origin absorbs exactly the error this harness exists
//!   to find.
//! - The harness checks that it is driving what it claims to drive: the
//!   screen it scores against must be the screen the ULA read, and the CPU
//!   must not have touched it.

use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::ula::Ula;
use common_sinclair_zx_spectrum::ula_engine::{UlaEngine, floating_bus_byte};
use machine_sinclair_zx_spectrum_48k::Spectrum48k;

const ROM_PATH_ENV: &str = "EMU198X_SPECTRUM_48K_ROM";

/// T-states in a 48K frame.
const FRAME_TSTATES: u32 = 69_888;
/// T-states per scan line.
const PER_LINE: u32 = 224;
/// ULA ticks — pixels — per CPU T-state. The ULA runs at 7 MHz against the
/// CPU's 3.5 MHz.
const PIXELS_PER_TSTATE: u32 = 2;
/// Master-clock half-cycles per CPU T-state, `TIMING_48K.cpu_divisor`.
const HC_PER_TSTATE: u32 = 4;

/// Screen RAM: bitmap plus attributes.
const SCREEN_BASE: u16 = 0x4000;
const SCREEN_END: u16 = 0x5B00;

/// Where the CPU is parked for the walk, in uncontended upper RAM.
const PARK_ADDR: u16 = 0x8000;

/// Add this to an engine frame T-state to get FUSE's.
///
/// libspectrum's `timings_frame_ferranti_5c_6c.top_left_pixel`, and the
/// origin `SpectrumMachineCore::floating_bus_read` already maps through.
/// `floating_bus_matches_fuse_at_every_tstate` asserts it is the *only*
/// offset in the frame at which the live bus and FUSE agree everywhere,
/// and `the_interrupt_and_the_bus_agree_on_the_frame_origin` derives the
/// same number from the `/INT` edge.
const ORIGIN: i32 = 14_336;

/// The origin `io_contention_oracle` and `contention_oracle` score
/// against.
///
/// One T-state earlier than this file measures, and the difference is a
/// probe convention rather than an engine behaviour — see the module docs.
/// Kept here so the gap is a reading rather than an argument: the
/// contention differentials' residuals are quoted at an origin one T-state
/// from the one the raster keeps.
const CONTENTION_ORACLE_ORIGIN: i32 = 14_335;

/// The engine T-state at which `/INT` rises, and what the two anchors
/// imply. FUSE puts its own `/INT` at frame T-state 0.
const INT_ONSET_TSTATE: u32 = 55_552;

/// `interrupt_length` for `timings_frame_ferranti_5c_6c`, libspectrum
/// `timings.c`. FUSE holds `/INT` while `tstates < interrupt_length`.
const FUSE_INTERRUPT_LENGTH: u32 = 32;

/// Spectron's `FloatingBusStartTicks` for the 48K, the constant our own
/// model is parameterised by.
const FLOAT_START: u32 = 14_338;

/// A screen byte that identifies its own address and is never `0xFF`.
///
/// `0xFF` therefore means "the bus is idle" unambiguously, and any other
/// disagreement means the two models addressed different bytes rather than
/// disagreeing about whether there was a byte at all.
fn screen_pattern(addr: u16) -> u8 {
    (addr % 251) as u8
}

fn rom_bytes() -> Option<Vec<u8>> {
    let path = std::env::var(ROM_PATH_ENV).ok()?;
    std::fs::read(path).ok()
}

/// FUSE's `spectrum_unattached_port`, transcribed.
///
/// Constants from libspectrum's `timings_frame_ferranti_5c_6c`
/// (left_border 24, horizontal_screen 128, 224 T/line, top_left_pixel
/// 14336) and FUSE's `machine.c`, where
/// `line_times[0] = top_left_pixel - 24*224 - 16 = 8944`, so the first
/// screen line starts at `line_times[24] = 14320`.
fn fuse_unattached_port(t: u32, memory: &dyn MemoryBus) -> u8 {
    const FIRST_SCREEN_LINE_T: u32 = 14_320;
    const LEFT_BORDER: u32 = 24;
    const HORIZONTAL_SCREEN: u32 = 128;
    /// `left_border - DISPLAY_BORDER_WIDTH_COLS * 4` = 24 - 16.
    const THROUGH_LINE_ADJUST: u32 = 8;

    if t < FIRST_SCREEN_LINE_T {
        return 0xFF;
    }
    let line = (t - FIRST_SCREEN_LINE_T) / PER_LINE;
    if line >= 192 {
        return 0xFF;
    }
    let through_line = t - (FIRST_SCREEN_LINE_T + line * PER_LINE) + THROUGH_LINE_ADJUST;
    if !(LEFT_BORDER..LEFT_BORDER + HORIZONTAL_SCREEN).contains(&through_line) {
        return 0xFF;
    }
    let mut column = ((through_line - LEFT_BORDER) / 8) * 2;
    let y = line as u16;
    match through_line % 8 {
        5 => {
            column += 1;
            memory.read_screen(0x4000 | UlaEngine::compute_attr_addr(y).wrapping_add(column as u16))
        }
        3 => {
            memory.read_screen(0x4000 | UlaEngine::compute_attr_addr(y).wrapping_add(column as u16))
        }
        4 => {
            column += 1;
            memory.read_screen(0x4000 | UlaEngine::compute_data_addr(y).wrapping_add(column as u16))
        }
        2 => {
            memory.read_screen(0x4000 | UlaEngine::compute_data_addr(y).wrapping_add(column as u16))
        }
        _ => 0xFF,
    }
}

/// The reference has to be checked before its readings mean anything.
///
/// This transcription lives in a second test binary from the one in
/// `ula_engine.rs`, which is how it would silently drift. It is compared
/// against our own `floating_bus_byte` — the function the production `IN`
/// path reads — over the whole frame, so a divergence says which of the
/// two copies moved rather than leaving the differential to blame the
/// engine for a typo in its own oracle.
#[test]
fn the_reference_still_matches_our_floating_bus_model() {
    struct PatternScreen;
    impl MemoryBus for PatternScreen {
        fn read(&self, addr: u16) -> u8 {
            screen_pattern(addr)
        }
        fn write(&mut self, _addr: u16, _value: u8) {}
        fn is_contended(&self, _addr: u16) -> bool {
            true
        }
    }

    let mem = PatternScreen;
    let bad: Vec<_> = (0..FRAME_TSTATES)
        .filter(|&t| {
            floating_bus_byte(t, FLOAT_START, PER_LINE, &mem) != fuse_unattached_port(t, &mem)
        })
        .take(8)
        .collect();
    assert!(
        bad.is_empty(),
        "the FUSE transcription in this file disagrees with `floating_bus_byte` at {bad:?}"
    );
}

/// A machine with a known screen, its CPU parked where it cannot disturb
/// one, aligned to its own frame origin.
fn prepare(rom: &[u8]) -> Spectrum48k {
    let mut machine = Spectrum48k::new();
    machine.load_rom_bytes(rom).expect("48K ROM should load");
    machine.reset();

    // A two-byte `JR $-2` in uncontended upper RAM. The CPU spends the
    // whole walk fetching those two bytes and never writes anything.
    machine.memory_mut().write(PARK_ADDR, 0x18);
    machine.memory_mut().write(PARK_ADDR + 1, 0xFE);

    for addr in SCREEN_BASE..SCREEN_END {
        machine.memory_mut().write(addr, screen_pattern(addr));
    }

    machine.z80_mut().regs.pc = PARK_ADDR;
    // No interrupt: the ROM's handler would run out of the parked loop and
    // its scroll / FLASH bookkeeping is exactly the kind of write this
    // harness must not have happening underneath it.
    machine.z80_mut().regs.iff1 = false;
    machine.z80_mut().regs.iff2 = false;

    while machine.tstate_in_frame() != 0 {
        machine.advance_tstates(1);
    }
    machine
}

/// The frame origin, from the interrupt, read at the resolution the edge
/// actually has.
///
/// `io_contention_oracle`'s version of this test advances a whole T-state
/// and then samples `interrupt_active()`, so an edge falling during
/// T-state *k* is recorded as *k+1*. That is where its `ORIGIN = 14335`
/// comes from. Stepping in half-cycles puts the edge where it is, and the
/// answer agrees with the frame of screen bytes
/// `floating_bus_matches_fuse_at_every_tstate` scores — two anchors that
/// share nothing but the raster.
#[test]
#[ignore = "needs EMU198X_SPECTRUM_48K_ROM"]
fn the_interrupt_and_the_bus_agree_on_the_frame_origin() {
    let Some(rom) = rom_bytes() else {
        panic!("set {ROM_PATH_ENV} to the 48K ROM to run this harness");
    };
    let mut machine = prepare(&rom);

    let mut edges: Vec<(u32, bool)> = Vec::new();
    let mut prev = machine.ula().interrupt_active();
    for hc in 0..FRAME_TSTATES * HC_PER_TSTATE {
        machine.advance_halfcycles(1);
        let now = machine.ula().interrupt_active();
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
    assert_eq!(onset, INT_ONSET_TSTATE, "the /INT edge moved");
    assert_eq!(
        FRAME_TSTATES as i32 - onset as i32,
        ORIGIN,
        "/INT rises at engine T-state {onset}, which puts the origin at {}, \
         not the {ORIGIN} the live floating bus is byte-exact at",
        FRAME_TSTATES as i32 - onset as i32
    );
}

/// The bus the ULA drove at every pixel of one frame, index 0 at the
/// engine's own frame T-state 0.
///
/// Sampled after each ULA tick rather than each T-state: the tables under
/// test are indexed by pixel, and reading only one pixel in two would hide
/// a half-T-state error in exactly the place Seam 1 moved things.
fn record_frame(machine: &mut Spectrum48k) -> Vec<u8> {
    let ula_tick_halfcycles = HC_PER_TSTATE / PIXELS_PER_TSTATE;
    let mut bus = Vec::with_capacity((FRAME_TSTATES * PIXELS_PER_TSTATE) as usize);
    for _ in 0..FRAME_TSTATES * PIXELS_PER_TSTATE {
        machine.advance_halfcycles(ula_tick_halfcycles);
        bus.push(machine.ula().floating_bus());
    }
    bus
}

/// How many T-states disagree with FUSE if the engine's T-state is FUSE's
/// plus `offset`?
fn mismatches_strided(bus: &[u8], machine: &Spectrum48k, offset: i32, stride: usize) -> usize {
    (0..FRAME_TSTATES as usize)
        .step_by(stride)
        .filter(|&t| {
            let ours = bus[t * PIXELS_PER_TSTATE as usize];
            let fuse_t = (t as i32 + offset).rem_euclid(FRAME_TSTATES as i32) as u32;
            ours != fuse_unattached_port(fuse_t, machine)
        })
        .count()
}

fn mismatches(bus: &[u8], machine: &Spectrum48k, offset: i32) -> usize {
    mismatches_strided(bus, machine, offset, 1)
}

/// The bus is one half of the `IN`. This is the other.
///
/// `floating_bus_matches_fuse_at_every_tstate` proves the ULA leaves the
/// right byte on the bus at the right T-state. It says nothing about
/// *when the CPU reads it*, which is `floating_bus_read`'s
/// `ORIGIN + IO_READ_DATA_LATCH_LEAD_TSTATES` — and that is the only part
/// of the path still capable of putting `$40` where Spectron has `$00`.
///
/// So: run the instruction floatspy runs, at every arrival T-state in the
/// frame, and score the byte it comes back with — not its cost — against
/// the byte FUSE's `readport` would have sampled.
///
/// Out of *uncontended* RAM and on FUSE's `N:4` port, deliberately. The
/// `IN` costs a flat twelve T-states with nothing charged at either end,
/// so the sample instant is fixed geometry from the arrival T-state and
/// this measures the read phase alone. A contended arrangement would
/// re-measure the contention gate and call it a floating-bus result.
#[test]
#[ignore = "differential harness; needs EMU198X_SPECTRUM_48K_ROM"]
fn the_in_path_samples_the_bus_where_fuse_does() {
    /// `IN A,(C)` out of uncontended RAM: `M1`, `M1`, then the I/O cycle.
    /// FUSE models the two `M1`s as four T-states each, so the I/O
    /// M-cycle opens eight T-states after the instruction arrives.
    const IO_MCYCLE_OFFSET: u32 = 8;
    /// FUSE samples the unattached port at the I/O cycle's start plus
    /// three: `ula_contend_port_early` adds one and `_late` two more
    /// before `readport_internal` runs (`periph.c`). For `$00FF` — an odd
    /// port in an uncontended page, FUSE's `N:4` class — neither adds any
    /// delay, so the three are the bare M-cycle geometry.
    const FUSE_SAMPLE_OFFSET: u32 = 3;
    /// The instruction's uncontended cost: 4 + 4 + 4.
    const BARE_COST: u32 = 12;
    /// The port floatspy reads, and the one class that pays nothing.
    const PORT: u16 = 0x00FF;
    /// Uncontended upper RAM, so no `M1` fetch is charged either.
    const CODE_BASE: u16 = 0x8000;
    const CODE_END: u16 = 0xC000;

    let Some(rom) = rom_bytes() else {
        panic!("set {ROM_PATH_ENV} to the 48K ROM to run this harness");
    };

    let mut samples: Vec<(u32, u8)> = Vec::new();
    // Twelve skews walk the arrival point across more than one whole
    // instruction, so between them the passes cover every phase of the
    // 8-T-state fetch pattern.
    for skew in 0..12u32 {
        let mut machine = Spectrum48k::new();
        machine.load_rom_bytes(&rom).expect("48K ROM should load");
        machine.reset();
        for addr in SCREEN_BASE..SCREEN_END {
            machine.memory_mut().write(addr, screen_pattern(addr));
        }
        let mut addr = CODE_BASE;
        while addr < CODE_END {
            machine.memory_mut().write(addr, 0xED);
            machine.memory_mut().write(addr + 1, 0x78);
            addr += 2;
        }
        while machine.tstate_in_frame() != 0 {
            machine.advance_tstates(1);
        }
        machine.advance_tstates(skew);
        machine.z80_mut().regs.pc = CODE_BASE;
        machine.z80_mut().regs.bc = PORT;
        machine.z80_mut().regs.iff1 = false;
        machine.z80_mut().regs.iff2 = false;

        // Aiming `PC` at the stream lands mid-M-cycle whenever the skew
        // does, so the first retirement is the tail of whatever the CPU
        // was already doing.
        for _ in 0..2 {
            step_one_instruction(&mut machine);
        }
        let mut spent = 0u32;
        while spent < FRAME_TSTATES {
            let arrival = machine.tstate_in_frame();
            assert!(
                (CODE_BASE..CODE_END).contains(&machine.z80().regs.pc),
                "execution left the instruction stream, so every later sample \
                 is a measurement of the ROM"
            );
            let cost = step_one_instruction(&mut machine);
            assert_eq!(
                cost, BARE_COST,
                "an `IN A,(C)` on an uncontended page cost {cost} T-states, \
                 not {BARE_COST} — something is charging this instruction and \
                 the sample instant is no longer fixed geometry"
            );
            samples.push((arrival, (machine.z80().regs.af >> 8) as u8));
            spent += cost;
        }
    }

    // The harness has to be reading the floating bus at all. A port some
    // peripheral claims, or a `read_fe` misroute, would return a constant
    // and agree with nothing.
    let distinct: std::collections::BTreeSet<u8> = samples.iter().map(|&(_, b)| b).collect();
    assert!(
        distinct.len() > 16,
        "the `IN` returned only {} distinct bytes across the frame, so it is \
         not reading the floating bus",
        distinct.len()
    );

    let screen = {
        let mut machine = Spectrum48k::new();
        machine.load_rom_bytes(&rom).expect("48K ROM should load");
        machine.reset();
        for addr in SCREEN_BASE..SCREEN_END {
            machine.memory_mut().write(addr, screen_pattern(addr));
        }
        machine
    };

    let wrong = |lead: i64| -> usize {
        samples
            .iter()
            .filter(|&&(arrival, got)| {
                let t = (arrival as i64
                    + ORIGIN as i64
                    + IO_MCYCLE_OFFSET as i64
                    + FUSE_SAMPLE_OFFSET as i64
                    + lead)
                    .rem_euclid(FRAME_TSTATES as i64) as u32;
                got != fuse_unattached_port(t, &screen)
            })
            .count()
    };

    println!("\n{:<10} {:>10}", "lead delta", "wrong");
    for delta in -4..=4i64 {
        println!("{delta:<+10} {:>10}", wrong(delta));
    }

    let total = wrong(0);
    let first: Vec<String> = samples
        .iter()
        .filter_map(|&(arrival, got)| {
            let t =
                (arrival + ORIGIN as u32 + IO_MCYCLE_OFFSET + FUSE_SAMPLE_OFFSET) % FRAME_TSTATES;
            let want = fuse_unattached_port(t, &screen);
            (got != want).then(|| format!("arrival {arrival} (fuse {t}): got {got}, want {want}"))
        })
        .take(6)
        .collect();
    for line in &first {
        println!("  {line}");
    }

    assert_eq!(
        total,
        0,
        "the `IN` path returned the wrong floating-bus byte at {total} of {} \
         arrival T-states. The bus itself is byte-exact against FUSE \
         (`floating_bus_matches_fuse_at_every_tstate`), so a non-zero count \
         here is the read phase — `ORIGIN` or \
         `IO_READ_DATA_LATCH_LEAD_TSTATES` in `floating_bus_read`.",
        samples.len()
    );
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

/// The differential itself.
#[test]
#[ignore = "differential harness; needs EMU198X_SPECTRUM_48K_ROM"]
fn floating_bus_matches_fuse_at_every_tstate() {
    let Some(rom) = rom_bytes() else {
        panic!("set {ROM_PATH_ENV} to the 48K ROM to run this harness");
    };
    let mut machine = prepare(&rom);
    let bus = record_frame(&mut machine);

    // === The harness checks it drove what it claims to have driven ===
    //
    // `contention_arming` reported a finding for a week that was its own
    // harness setting `BC` before the machine settled. The checks below are
    // the counterpart: the screen the ULA read has to be the screen this
    // test scores against, and the CPU has to have stayed out of it.
    assert_eq!(
        machine.z80().regs.pc,
        PARK_ADDR,
        "the CPU left the parked loop, so it was executing something that \
         could have written to the screen"
    );
    assert!(
        !(0x40..=0x7F).contains(&machine.z80().regs.i),
        "I = {:#04x} puts the refresh address in screen RAM, which arms snow \
         and redirects the very fetches this harness is scoring",
        machine.z80().regs.i
    );
    let disturbed: Vec<u16> = (SCREEN_BASE..SCREEN_END)
        .filter(|&addr| machine.read_screen(addr) != screen_pattern(addr))
        .take(4)
        .collect();
    assert!(
        disturbed.is_empty(),
        "screen RAM changed during the walk at {disturbed:x?} — the reference \
         is being computed over bytes the ULA did not read"
    );

    // The bus has to have carried something. A run that reads idle
    // everywhere agrees with FUSE outside the display and is otherwise
    // measuring the border.
    let data_slots = bus.iter().filter(|&&b| b != 0xFF).count();
    let expected_slots = 192 * 64 * PIXELS_PER_TSTATE as usize;
    assert_eq!(
        data_slots, expected_slots,
        "the ULA drove data on {data_slots} pixels; the 48K display is \
         192 lines x 64 data T-states = {expected_slots} pixels"
    );

    // Both pixels of a T-state must carry the same byte. The ULA fetches on
    // even pixels only, so this is the CPU's T-state grid and the ULA's
    // pixel counter sharing an origin — the same claim `contention_tables`
    // reproduces at alignment 0, read here off the data bus instead of off
    // the delay table.
    let split = (0..FRAME_TSTATES as usize)
        .filter(|&t| bus[t * 2] != bus[t * 2 + 1])
        .count();
    assert_eq!(
        split, 0,
        "{split} T-states carried a different byte in each of their two \
         pixels, so the ULA's fetch pattern is not aligned to the CPU's \
         T-state grid"
    );

    // === The score ===
    let pinned = mismatches(&bus, &machine, ORIGIN);
    let contention_anchor = mismatches(&bus, &machine, CONTENTION_ORACLE_ORIGIN);
    println!("\norigin {ORIGIN:+} (top_left_pixel, /INT) — {pinned} of {FRAME_TSTATES} disagree");
    println!(
        "origin {CONTENTION_ORACLE_ORIGIN:+} (contention oracles) — {contention_anchor} of {FRAME_TSTATES} disagree"
    );

    print!("\n{:<14}", "offset");
    for d in -4..=4i32 {
        print!("{:>9}", format!("{:+}", ORIGIN + d));
    }
    println!();
    print!("{:<14}", "mismatches");
    for d in -4..=4i32 {
        print!("{:>9}", mismatches(&bus, &machine, ORIGIN + d));
    }
    println!();

    assert_eq!(
        pinned, 0,
        "the ULA's live floating bus disagrees with FUSE at {pinned} of \
         {FRAME_TSTATES} T-states at the frame origin both anchors give"
    );

    // Uniqueness, which is what makes the zero above a measurement rather
    // than a coincidence. An offset sweep that reports its winner without
    // its neighbourhood cannot distinguish a sharp minimum from a plateau,
    // and a plateau is what a table with the wrong *shape* produces.
    //
    // Strided, because an exhaustive frame-by-frame sweep is 4.9e9
    // comparisons. A wrong offset that survives 720 samples spread across
    // the frame is not a wrong offset this harness needs to worry about.
    const STRIDE: usize = 97;
    let rivals: Vec<i32> = (0..FRAME_TSTATES as i32)
        .filter(|&offset| offset != ORIGIN)
        .filter(|&offset| mismatches_strided(&bus, &machine, offset, STRIDE) == 0)
        .take(4)
        .collect();
    assert!(
        rivals.is_empty(),
        "the live bus also matches FUSE at {rivals:?}, so {ORIGIN} is one of \
         several origins rather than the frame's"
    );
}
