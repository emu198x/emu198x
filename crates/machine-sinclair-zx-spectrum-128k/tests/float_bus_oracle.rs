//! Frame-wide comparison of physical ULA bus bytes with FUSE's pattern.
//!
//! FUSE's first data timestamp is 14364; physical counter C8 is T4.
//! Thus the pattern coordinate offset is 14360. This is not the IRQ
//! origin and is never used by the production I/O read path. CPU reads
//! consume the live bus at their latch edge. Float128K and Floatspy
//! independently validate the integrated interrupt/read timing.

use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::timing::TIMING_128K;
use common_sinclair_zx_spectrum::ula::Ula;
use common_sinclair_zx_spectrum::ula_engine::{UlaEngine, floating_bus_byte};
use machine_sinclair_zx_spectrum_128k::{Memory128K, Spectrum128K};

const ROM0_PATH_ENV: &str = "EMU198X_SPECTRUM_128K_ROM0";
const ROM1_PATH_ENV: &str = "EMU198X_SPECTRUM_128K_ROM1";

/// T-states in a 128K frame.
const FRAME_TSTATES: u32 = 70_908;
/// T-states per scan line.
const PER_LINE: u32 = 228;
/// Master-clock half-cycles per CPU T-state, `TIMING_128K.cpu_divisor`.
const HC_PER_TSTATE: u32 = 5;

/// Screen RAM: bitmap plus attributes, in bank 5 at `$4000`.
const SCREEN_BASE: u16 = 0x4000;
const SCREEN_END: u16 = 0x5B00;

/// Where the CPU is parked for the walk: bank 2 at `$8000`, uncontended,
/// and not the bank the ULA displays from.
const PARK_ADDR: u16 = 0x8000;

/// FUSE first data timestamp minus physical first data counter (C8 / 2).
const PATTERN_ORIGIN: i32 = 14_360;
/// FUSE's first data timestamp, not an offset from counter zero.
const FIRST_DATA_TIMESTAMP: i32 = 14_364;
const FLOAT_START: u32 = 14_364;

/// A screen byte that identifies its own address and is never `0xFF`.
fn screen_pattern(addr: u16) -> u8 {
    (addr % 251) as u8
}

fn roms() -> Option<(Vec<u8>, Vec<u8>)> {
    let rom0 = std::fs::read(std::env::var(ROM0_PATH_ENV).ok()?).ok()?;
    let rom1 = std::fs::read(std::env::var(ROM1_PATH_ENV).ok()?).ok()?;
    Some((rom0, rom1))
}

fn tstate_in_frame(machine: &Spectrum128K) -> u32 {
    machine.hc() / TIMING_128K.cpu_divisor
}

/// FUSE's `spectrum_unattached_port`, transcribed for this ULA.
///
/// `machine.c` sets `line_times[0] = top_left_pixel - 24*228 - 16 = 8874`,
/// so the first screen line starts at `line_times[24] = 14346`.
fn fuse_unattached_port(t: u32, memory: &dyn MemoryBus) -> u8 {
    const FIRST_SCREEN_LINE_T: u32 = 14_346;
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
/// This transcription lives in a different test binary from the 48K's and
/// from `ula_engine.rs`'s, which is how it would silently drift. It is
/// compared against our own `floating_bus_byte` — the function `io_read`
/// reads — over the whole frame.
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
fn prepare(roms: &(Vec<u8>, Vec<u8>)) -> Spectrum128K {
    let mut machine = Spectrum128K::new();
    machine.memory.load_roms(&roms.0, &roms.1);
    machine.reset();

    // A two-byte `JR $-2` in bank 2. The CPU spends the whole walk
    // fetching those two bytes and never writes anything.
    machine.memory.write(PARK_ADDR, 0x18);
    machine.memory.write(PARK_ADDR + 1, 0xFE);

    for addr in SCREEN_BASE..SCREEN_END {
        machine.memory.write(addr, screen_pattern(addr));
    }

    machine.z80.regs.pc = PARK_ADDR;
    // No interrupt: the ROM's handler would run out of the parked loop and
    // its bookkeeping is exactly the kind of write this harness must not
    // have happening underneath it.
    machine.z80.regs.iff1 = false;
    machine.z80.regs.iff2 = false;

    while tstate_in_frame(&machine) != 0 {
        machine.advance_tstates(1);
    }
    machine
}

/// The bus the ULA drove at every pixel of one frame, indexed by T-state.
///
/// Returns `(first pixel, second pixel)` per T-state.
///
/// **The 48K's loop does not port.** There, `cpu_divisor` is 4 and the two
/// ULA ticks fall on half-cycles 0 and 2 of every four, so advancing two
/// half-cycles at a time steps exactly one tick. Here the divisor is 5:
/// `SpectrumDriver::tick_one_halfcycle` ticks the ULA on phase 0 and on
/// `divisor / 2` = 2, so the gaps run 2, 3, 2, 3 and a fixed stride lands
/// on a half-cycle where nothing happened. Stepping one half-cycle at a
/// time and reading the two that carry a tick is the only version of this
/// that is right on both machines.
fn record_frame(machine: &mut Spectrum128K) -> Vec<(u8, u8)> {
    let mut bus = Vec::with_capacity(FRAME_TSTATES as usize);
    for _ in 0..FRAME_TSTATES {
        let mut pixels = (0u8, 0u8);
        for phase in 0..HC_PER_TSTATE {
            machine.advance_halfcycles(1);
            if phase == 0 {
                pixels.0 = machine.ula.floating_bus();
            } else if phase == HC_PER_TSTATE / 2 {
                pixels.1 = machine.ula.floating_bus();
            }
        }
        bus.push(pixels);
    }
    bus
}

/// How many T-states disagree with FUSE if the engine's T-state is FUSE's
/// plus `offset`?
fn mismatches_strided(bus: &[(u8, u8)], memory: &Memory128K, offset: i32, stride: usize) -> usize {
    (0..FRAME_TSTATES as usize)
        .step_by(stride)
        .filter(|&t| {
            let fuse_t = (t as i32 + offset).rem_euclid(FRAME_TSTATES as i32) as u32;
            bus[t].0 != fuse_unattached_port(fuse_t, memory)
        })
        .count()
}

fn mismatches(bus: &[(u8, u8)], memory: &Memory128K, offset: i32) -> usize {
    mismatches_strided(bus, memory, offset, 1)
}

/// The differential itself, and the second anchor.
#[test]
#[ignore = "FIXTURE: differential harness; needs EMU198X_SPECTRUM_128K_ROM0 / ROM1"]
fn floating_bus_matches_fuse_at_every_tstate() {
    let Some(roms) = roms() else {
        panic!("set {ROM0_PATH_ENV} and {ROM1_PATH_ENV} to run this harness");
    };
    let mut machine = prepare(&roms);
    let bus = record_frame(&mut machine);

    // === The harness checks it drove what it claims to have driven ===
    assert_eq!(
        machine.z80.regs.pc, PARK_ADDR,
        "the CPU left the parked loop, so it was executing something that \
         could have written to the screen"
    );
    assert!(
        !(0x40..=0x7F).contains(&machine.z80.regs.i),
        "I = {:#04x} puts the refresh address in screen RAM, which arms snow \
         and redirects the very fetches this harness is scoring",
        machine.z80.regs.i
    );
    assert_eq!(
        machine.memory.screen_bank(),
        5,
        "the ULA is displaying from bank {}, not the bank this harness wrote \
         its pattern into",
        machine.memory.screen_bank()
    );
    let disturbed: Vec<u16> = (SCREEN_BASE..SCREEN_END)
        .filter(|&addr| machine.memory.read_screen(addr) != screen_pattern(addr))
        .take(4)
        .collect();
    assert!(
        disturbed.is_empty(),
        "screen RAM changed during the walk at {disturbed:x?} — the reference \
         is being computed over bytes the ULA did not read"
    );

    // The bus has to have carried something.
    let data_slots = bus.iter().filter(|&&(a, _)| a != 0xFF).count();
    let expected_slots = 192 * 64;
    assert_eq!(
        data_slots, expected_slots,
        "the ULA drove data on {data_slots} T-states; the display is 192 \
         lines x 64 data T-states = {expected_slots}"
    );

    // Both pixels of a T-state must carry the same byte: the CPU's T-state
    // grid and the ULA's pixel counter sharing an origin.
    let split = bus.iter().filter(|&&(a, b)| a != b).count();
    assert_eq!(
        split, 0,
        "{split} T-states carried a different byte in each of their two \
         pixels, so the ULA's fetch pattern is not aligned to the CPU's \
         T-state grid"
    );

    // === The two anchors ===
    let at_top_left = mismatches(&bus, &machine.memory, PATTERN_ORIGIN);
    let at_int = mismatches(&bus, &machine.memory, FIRST_DATA_TIMESTAMP);
    println!(
        "\norigin {PATTERN_ORIGIN:+} (pattern coordinate) — {at_top_left} of {FRAME_TSTATES} disagree"
    );
    println!(
        "origin {FIRST_DATA_TIMESTAMP:+} (first-data timestamp)                       — {at_int} of {FRAME_TSTATES} disagree"
    );

    print!("\n{:<14}", "offset");
    for d in -4..=4i32 {
        print!("{:>9}", format!("{:+}", PATTERN_ORIGIN + d));
    }
    println!();
    print!("{:<14}", "mismatches");
    for d in -4..=4i32 {
        print!(
            "{:>9}",
            mismatches(&bus, &machine.memory, PATTERN_ORIGIN + d)
        );
    }
    println!();

    assert_eq!(
        at_top_left, 0,
        "the ULA's live floating bus disagrees with FUSE at {at_top_left} of \
         {FRAME_TSTATES} T-states after mapping physical counters to FUSE timestamps"
    );

    // Uniqueness, which is what makes the zero a measurement rather than a
    // coincidence. Strided, because an exhaustive sweep is 5e9
    // comparisons; a wrong offset surviving 730 samples spread across the
    // frame is not one this harness needs to worry about.
    const STRIDE: usize = 97;
    let rivals: Vec<i32> = (0..FRAME_TSTATES as i32)
        .filter(|&offset| offset != PATTERN_ORIGIN)
        .filter(|&offset| mismatches_strided(&bus, &machine.memory, offset, STRIDE) == 0)
        .take(4)
        .collect();
    assert!(
        rivals.is_empty(),
        "the live bus also matches FUSE at {rivals:?}, so \
         {PATTERN_ORIGIN} is one of several origins rather than the \
         frame's"
    );
}

/// A timestamp naming the first data slot cannot also name counter zero.
#[test]
#[ignore = "FIXTURE: differential harness; needs EMU198X_SPECTRUM_128K_ROM0 / ROM1"]
fn first_data_timestamp_is_not_the_counter_origin() {
    let Some(roms) = roms() else {
        panic!("set {ROM0_PATH_ENV} and {ROM1_PATH_ENV} to run this harness");
    };
    let mut machine = prepare(&roms);
    let bus = record_frame(&mut machine);
    assert_ne!(mismatches(&bus, &machine.memory, FIRST_DATA_TIMESTAMP), 0);
    assert_eq!(mismatches(&bus, &machine.memory, PATTERN_ORIGIN), 0);
}
