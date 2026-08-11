//! The +2A/+3 gate array's contention, scored against FUSE across a frame.
//!
//! The 48K has had a frame-wide differential since the contention work
//! started; the +2A/+3 family has had nothing but a boot test. That matters
//! because five contention configs route through the shared `UlaEngine` and
//! only one of them could report a regression — which is how `ad0e8c53`
//! moved the 128K floating bus by a T-state unnoticed (#851).
//!
//! This measures the **engine**, not a model of it. The gate array is driven
//! across a whole frame with a contended address and `/MREQ` asserted, and
//! the ticks on which it withholds the CPU clock are recorded. The delay a
//! CPU would suffer arriving at a given T-state is then the length of the
//! withheld run starting there. Nothing here consults
//! `CONTENTION_PATTERN_PLUS2A`, which has no runtime consumer and is
//! documentation rather than behaviour (#856).
//!
//! FUSE is the reference, per
//! `knowledge/decisions/fuse-governs-the-contended-window.md` and RULES #32.
//! Its numbers are taken from the vendored source at
//! `198x/emulators/zx-spectrum/`, not from memory:
//!
//! - `fuse-1.7.0/spectrum.c` — `contention_pattern_76543210 = {5,4,3,2,1,0,7,6}`,
//!   used via `contend_delay_common(time, pattern, 4)`. Both
//!   `machines/specplus2a.c` and `machines/specplus3e.c` select it.
//! - `libspectrum-1.6.0/timings.c` — `timings_frame_amstrad_asic`:
//!   24/128/24/52 horizontal (228 per line), 48/192/48/23 vertical (311
//!   lines), `top_left_pixel = 14365`.

use amstrad_ula_40077::AmstradGateArray;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH};
use common_sinclair_zx_spectrum::ula::Ula;

const PER_LINE: u32 = 228;
const DISPLAY_LINES: u32 = 192;
const BORDER_HEIGHT: u32 = 48;
const LEFT_BORDER: u32 = 24;
const HORIZONTAL_SCREEN: u32 = 128;
const TOP_LEFT_PIXEL: u32 = 14_365;
const OFFSET: u32 = 4;
const FUSE_PATTERN: [u32; 8] = [5, 4, 3, 2, 1, 0, 7, 6];

/// `machine.c` sets `line_times[0] = top_left_pixel - border*per_line - 16`.
const LINE_TIMES_0: u32 = TOP_LEFT_PIXEL - BORDER_HEIGHT * PER_LINE - 16;

struct Contended;

impl MemoryBus for Contended {
    fn read(&self, _addr: u16) -> u8 {
        0
    }

    fn write(&mut self, _addr: u16, _value: u8) {}

    fn is_contended(&self, _addr: u16) -> bool {
        true
    }
}

/// FUSE's `contend_delay_common` for the Amstrad ASIC.
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

/// Cost of one contended memory-read M-cycle, in T-states.
///
/// A real M-cycle is three T-states — six half-cycles — and asserts
/// `/MREQ` for part of that span, not throughout. Holding it permanently
/// is what left `track_z80_clock` in a state no M-cycle reaches, so
/// contention never armed and the first version of this harness measured
/// a gate that never stalls.
///
/// The CPU advances only on half-cycles where the gate leaves the clock
/// live, so the excess over six half-cycles is the contention delay.
fn mcycle_cost(gate: &mut AmstradGateArray, framebuffer: &mut [u8]) -> u32 {
    const HALF_CYCLES: usize = 6;
    let mut advanced = 0usize;
    let mut ticks = 0usize;

    while advanced < HALF_CYCLES && ticks < 128 {
        // `/MREQ` low from the back half of T1 to the back half of T3,
        // as a Z80 read cycle drives it.
        let mreq = (1..=4).contains(&advanced);
        Ula::tick(gate, &Contended, 0x4000, mreq, false, false, framebuffer);
        ticks += 1;
        if gate.cpu_clock_active() {
            advanced += 1;
        }
    }

    ((ticks - HALF_CYCLES) / 2) as u32
}

/// Costs of back-to-back contended M-cycles across one frame.
///
/// One continuous pass rather than a fresh wind-in per T-state: the
/// latter is quadratic, and the M-cycles land on a wide spread of phases
/// as the frame advances.
fn mcycle_costs() -> Vec<u32> {
    let mut gate = AmstradGateArray::new();
    let mut framebuffer = vec![0; SCREEN_WIDTH * SCREEN_HEIGHT];
    let frame_half_cycles = (gate.frame_timing().tstates_per_frame * 2) as usize;

    // Settle a frame so the beam counters sit where a running machine
    // would have them.
    for _ in 0..frame_half_cycles {
        Ula::tick(
            &mut gate,
            &Contended,
            0x4000,
            false,
            false,
            false,
            &mut framebuffer,
        );
    }

    let mut costs = Vec::new();
    let mut spent = 0usize;
    while spent < frame_half_cycles {
        let cost = mcycle_cost(&mut gate, &mut framebuffer);
        spent += 6 + (cost as usize) * 2;
        costs.push(cost);
    }
    costs
}

/// **Validated.** The first version of this drive held `/MREQ` asserted
/// throughout and measured a gate that never stalls — max delay 0 across
/// a whole frame — while the wind-in probe in #856 measured stalls of
/// three half-cycles. Holding `/MREQ` permanently leaves
/// `track_z80_clock` in a state no real M-cycle reaches, so contention
/// never armed.
///
/// Driving proper M-cycles instead, contention fires: 1,728 of 23,060
/// M-cycles in a frame are contended. The self-check below is what stands
/// between that fix and quietly reporting the broken version's confident
/// "21,504 of 70,908 T-states disagree".
///
/// What it now measures, against FUSE: the engine tops out at **1**
/// T-state of contention where FUSE's pattern reaches **7**. That
/// corroborates #856 by an independent route — a driven M-cycle scored
/// against the reference, rather than deriving the declared pattern from
/// the mask and finding no alignment fits.
#[test]
#[ignore = "KNOWN DIVERGENCE (#856): DELAY_TABLE_PLUS2A caps contention at \
            1 T-state where FUSE's pattern reaches 7 — the mask cannot \
            express its own documented sequence at any alignment"]
fn contention_matches_fuse_across_the_whole_frame() {
    let costs = mcycle_costs();
    let engine_max = costs.iter().copied().max().unwrap_or(0);
    let fuse_max = (0..70_908u32).map(fuse_delay).max().unwrap_or(0);
    let contended = costs.iter().filter(|&&c| c > 0).count();

    println!(
        "\n+2A M-cycles: {} measured, {contended} contended, \
         engine max delay {engine_max} T-states, FUSE max {fuse_max}",
        costs.len(),
    );

    // Self-check first: an instrument that cannot reproduce a known-good
    // measurement is not measuring the thing it claims to. The wind-in
    // probe in #856 sees stalls, so a drive that sees none is broken.
    assert!(
        engine_max > 0,
        "harness fault, not a finding: this drive measured no contention \
         anywhere in a frame, while a wind-in probe measures stalls at \
         half-cycles 14/15/0. Fix the drive before reading any comparison."
    );

    assert_eq!(
        engine_max, fuse_max,
        "the +2A gate array tops out at {engine_max} T-states of contention \
         against FUSE's {fuse_max}. See #856 — the mask is the suspect, not \
         this oracle."
    );
}
