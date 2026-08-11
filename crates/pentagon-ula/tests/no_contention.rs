//! The Pentagon never withholds the CPU clock.
//!
//! `PentagonUla::tick` assigns `cpu_clock = true` with the comment "No
//! contention", and `cpu_clock_active()` returns a hardcoded `true`. So
//! this pins a contract rather than hunting a bug: the clock must stay
//! live for a whole frame, and the declared contention pattern must stay
//! empty.
//!
//! **What it does not catch, stated plainly.** This was written expecting
//! to guard against contention leaking in from the shared `UlaEngine` —
//! the mechanism behind the 128K floating-bus regression (#851). It does
//! not. Removing the `cpu_clock = true` guard *and* making
//! `cpu_clock_active()` read the engine leaves this test green, because
//! the engine's clock never goes false for a config with no contention
//! pattern: `tick` never calls the contention path, so it is never told
//! the wrong answer.
//!
//! What it does catch is `cpu_clock_active()` becoming dynamic and
//! returning false at any point in a frame, which the crate's own unit
//! test cannot — that one only checks a freshly constructed ULA. A real
//! guard against engine-side leakage would have to drive a whole machine
//! and compare instruction timings, which belongs with the contention
//! oracles, not here.

use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH};
use common_sinclair_zx_spectrum::ula::Ula;
use pentagon_ula::PentagonUla;

/// Reports every address as contended, so a machine that consults
/// contention at all has every opportunity to withhold the clock.
struct EverythingContended;

impl MemoryBus for EverythingContended {
    fn read(&self, _addr: u16) -> u8 {
        0
    }

    fn write(&mut self, _addr: u16, _value: u8) {}

    fn is_contended(&self, _addr: u16) -> bool {
        true
    }
}

#[test]
fn the_cpu_clock_is_never_withheld_across_two_whole_frames() {
    let mut ula = PentagonUla::new();
    let mut framebuffer = vec![0; SCREEN_WIDTH * SCREEN_HEIGHT];
    let frame = ula.frame_timing().tstates_per_frame;

    // Two frames, so the check crosses a frame boundary rather than
    // stopping neatly at one.
    let ticks = frame * 2;
    let mut withheld = 0u32;
    let mut first_withheld = None;

    for t in 0..ticks {
        Ula::tick(
            &mut ula,
            &EverythingContended,
            0x4000, // squarely in the range a Sinclair ULA contends
            true,   // /MREQ asserted — a memory access is in progress
            false,
            false,
            &mut framebuffer,
        );
        if !ula.cpu_clock_active() {
            withheld += 1;
            first_withheld.get_or_insert(t);
        }
    }

    assert_eq!(
        withheld, 0,
        "the Pentagon withheld the CPU clock on {withheld} of {ticks} ticks \
         (first at {first_withheld:?}). It has no contention; if this fires, \
         contention has leaked in from the shared UlaEngine.",
    );
}

#[test]
fn the_frame_timing_declares_no_contention_pattern() {
    let ula = PentagonUla::new();
    assert_eq!(
        ula.frame_timing().contention_pattern,
        [0; 8],
        "the Pentagon's declared contention pattern must stay empty",
    );
}
