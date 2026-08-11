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

/// Ticks on which the gate array withholds the clock, over one frame.
///
/// Half-cycle resolution: the engine runs two ticks per T-state.
fn withheld_half_cycles() -> Vec<bool> {
    let mut gate = AmstradGateArray::new();
    let mut framebuffer = vec![0; SCREEN_WIDTH * SCREEN_HEIGHT];
    let frame_half_cycles = (gate.frame_timing().tstates_per_frame * 2) as usize;

    // Settle a whole frame first so the beam counters are where a running
    // machine would have them, rather than at their construction values.
    for _ in 0..frame_half_cycles {
        Ula::tick(
            &mut gate,
            &Contended,
            0x4000,
            true,
            false,
            false,
            &mut framebuffer,
        );
    }

    let mut withheld = Vec::with_capacity(frame_half_cycles);
    for _ in 0..frame_half_cycles {
        Ula::tick(
            &mut gate,
            &Contended,
            0x4000,
            true,
            false,
            false,
            &mut framebuffer,
        );
        withheld.push(!gate.cpu_clock_active());
    }
    withheld
}

/// The delay implied by the recorded map: the run of withheld half-cycles
/// from `t`, halved, as a T-state count.
fn engine_delay(withheld: &[bool], t: u32) -> u32 {
    let n = withheld.len();
    let start = (t as usize * 2) % n;
    let mut run = 0usize;
    while run < n && withheld[(start + run) % n] {
        run += 1;
    }
    (run / 2) as u32
}

/// **This harness is not yet validated, and its number must not be quoted.**
///
/// Driving with `/MREQ` asserted continuously reports the gate never
/// withholding the clock at all — max delay 0 across the frame. A separate
/// probe that winds in with `/MREQ` idle and *then* asserts it measures
/// non-zero stalls (3 half-cycles arriving at half-cycle 14). Both cannot
/// be right about the same gate.
///
/// The likely cause is that holding `/MREQ` permanently leaves
/// `track_z80_clock`'s latch and `z80_clock_high` in a state a real M-cycle
/// never reaches, so contention never arms. A real CPU asserts `/MREQ` for
/// part of an M-cycle, not forever.
///
/// Until the drive reproduces the probe's known-good stalls, this cannot
/// distinguish "the mask is wrong" (which #856 establishes by other means)
/// from "the harness is wrong". The FUSE side is sound — its constants come
/// from the vendored source and are quoted above — so what needs work is
/// the engine side alone.
#[test]
#[ignore = "UNVALIDATED HARNESS + KNOWN DIVERGENCE (#856): DELAY_TABLE_PLUS2A caps contention at \
            1 T-state where FUSE's pattern reaches 7 — the mask cannot \
            express its own documented sequence at any alignment"]
fn contention_matches_fuse_across_the_whole_frame() {
    let withheld = withheld_half_cycles();
    let frame = withheld.len() as u32 / 2;

    let mut disagreements = 0u32;
    let mut first: Vec<(u32, u32, u32)> = Vec::new();
    let mut engine_max = 0;
    let mut fuse_max = 0;

    for t in 0..frame {
        let ours = engine_delay(&withheld, t);
        let theirs = fuse_delay(t);
        engine_max = engine_max.max(ours);
        fuse_max = fuse_max.max(theirs);
        if ours != theirs {
            disagreements += 1;
            if first.len() < 8 {
                first.push((t, ours, theirs));
            }
        }
    }

    println!(
        "\n+2A vs FUSE: {disagreements} of {frame} T-states disagree \
         (engine max delay {engine_max}, FUSE max {fuse_max})"
    );
    for (t, ours, theirs) in &first {
        println!("  T={t}: ours {ours}, FUSE {theirs}");
    }

    // Self-check first: an instrument that cannot reproduce a known-good
    // measurement is not measuring the thing it claims to.
    assert!(
        engine_max > 0,
        "harness fault, not a finding: this drive reports the gate never \
         withholding the clock across a whole frame, while a wind-in probe \
         measures stalls at half-cycles 14/15/0. Fix the drive before \
         reading the disagreement count."
    );

    assert_eq!(
        disagreements, 0,
        "the +2A gate array disagrees with FUSE on {disagreements} of {frame} \
         T-states; engine tops out at {engine_max} T-states against FUSE's \
         {fuse_max}. See #856 — the mask is the suspect, not this oracle."
    );
}
