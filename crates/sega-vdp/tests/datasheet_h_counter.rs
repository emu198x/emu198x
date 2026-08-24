//! The H counter, port $7F.
//!
//! The counter free runs across the line and the CPU never sees it directly.
//! What port $7F returns is whatever a TH transition on a controller port
//! captured — which is how a Light Phaser reports where it was pointing, and
//! the only way to read a horizontal raster position on this machine.
//!
//! MAME states the value as `((hpos - 1 - 46) >> 1) & 0xFF` over a 342-pixel
//! raster whose active display starts at pixel 63. That is arithmetic rather
//! than a table, and the tests here check the structure it produces rather
//! than restating it: 171 values for 342 pixels, two pixels to a count, and
//! the run $00-$93 followed by $E9-$FF.

use sega_vdp::{DOTS_PER_LINE, SegaVdp, VdpRegion, VdpVariant};

fn vdp() -> SegaVdp {
    SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2)
}

/// Every H counter value across one whole line, sampled by latching at each
/// dot in turn.
fn line_of_counts() -> Vec<u8> {
    let mut vdp = vdp();
    (0..DOTS_PER_LINE)
        .map(|_| {
            vdp.latch_h_counter();
            let value = vdp.read_h_counter();
            vdp.tick();
            value
        })
        .collect()
}

/// 342 pixels, two to a count, so 171 distinct values — not 256. The chip has
/// fewer counts than a byte holds, which is why the sequence has a gap in it
/// rather than simply wrapping.
#[test]
fn the_counter_advances_once_every_two_pixels() {
    let counts = line_of_counts();
    assert_eq!(counts.len(), DOTS_PER_LINE as usize);

    let mut distinct: Vec<u8> = counts.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        DOTS_PER_LINE as usize / 2,
        "342 pixels at two per count should give 171 values"
    );

    for pair in counts.chunks(2) {
        if pair.len() == 2 {
            // Each count spans two pixels, though the line's odd start means
            // a pair may straddle the boundary between two of them.
            assert!(
                pair[0] == pair[1] || pair[1] == pair[0].wrapping_add(1),
                "a count should hold for two pixels, saw {pair:?}"
            );
        }
    }
}

/// The 171 values are $00-$93 and $E9-$FF. Nothing between $94 and $E8 is
/// ever produced — that band is what the counter skips.
#[test]
fn the_counter_runs_0x00_to_0x93_then_0xe9_to_0xff() {
    let mut seen: Vec<u8> = line_of_counts();
    seen.sort_unstable();
    seen.dedup();

    let expected: Vec<u8> = (0x00..=0x93u8).chain(0xE9..=0xFF).collect();
    assert_eq!(expected.len(), 171, "the two runs should account for 171");
    assert_eq!(seen, expected, "the counter's value set");
}

/// The counter is a position, so consecutive dots give consecutive counts —
/// once each, in order, with exactly one discontinuity where the band is
/// skipped.
#[test]
fn the_counter_climbs_with_the_beam_and_skips_once() {
    let counts = line_of_counts();
    let mut jumps = Vec::new();
    for pair in counts.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if b != a && b != a.wrapping_add(1) {
            jumps.push((a, b));
        }
    }
    assert_eq!(
        jumps,
        vec![(0x93, 0xE9)],
        "the only step that is not by one should be the skipped band"
    );
}

/// The active display is 256 pixels, so it spans 128 counts. It starts at $08
/// rather than $00: the counter's origin sits sixteen pixels earlier, in the
/// blanking before the left border.
#[test]
fn the_active_display_spans_128_counts_starting_at_0x08() {
    let counts = line_of_counts();
    assert_eq!(counts[0], 0x08, "the first active pixel");
    assert_eq!(counts[255], 0x87, "the last active pixel");

    let across: Vec<u8> = counts[..256].to_vec();
    let mut distinct = across;
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), 128, "256 active pixels at two per count");
}

/// Nothing reads the counter live. Port $7F returns what the last TH
/// transition captured, and it does not move until the next one — which is
/// what makes it usable as a position at all.
#[test]
fn the_port_returns_the_latched_value_and_not_the_live_one() {
    let mut vdp = vdp();
    assert_eq!(vdp.read_h_counter(), 0, "nothing latched yet");

    for _ in 0..100 {
        vdp.tick();
    }
    assert_eq!(
        vdp.read_h_counter(),
        0,
        "the beam moving must not move the port"
    );

    vdp.latch_h_counter();
    let captured = vdp.read_h_counter();
    assert_ne!(captured, 0, "a latch at dot 100 should not read as reset");

    for _ in 0..50 {
        vdp.tick();
    }
    assert_eq!(
        vdp.read_h_counter(),
        captured,
        "the port holds the captured value until the next transition"
    );

    vdp.latch_h_counter();
    assert_ne!(
        vdp.read_h_counter(),
        captured,
        "and the next transition replaces it"
    );
}

/// Two latches fifty pixels apart differ by twenty-five counts, because the
/// counter is a position and not a timestamp.
#[test]
fn the_distance_between_two_latches_is_the_distance_the_beam_moved() {
    let mut vdp = vdp();
    for _ in 0..20 {
        vdp.tick();
    }
    vdp.latch_h_counter();
    let first = vdp.read_h_counter();

    for _ in 0..50 {
        vdp.tick();
    }
    vdp.latch_h_counter();
    let second = vdp.read_h_counter();

    assert_eq!(
        u32::from(second) - u32::from(first),
        25,
        "fifty pixels is twenty-five counts"
    );
}

/// The counter is horizontal, so it repeats identically on every line.
#[test]
fn every_line_gives_the_same_sequence() {
    let mut vdp = vdp();
    let mut first_line = Vec::new();
    for _ in 0..DOTS_PER_LINE {
        vdp.latch_h_counter();
        first_line.push(vdp.read_h_counter());
        vdp.tick();
    }
    let mut second_line = Vec::new();
    for _ in 0..DOTS_PER_LINE {
        vdp.latch_h_counter();
        second_line.push(vdp.read_h_counter());
        vdp.tick();
    }
    assert_eq!(first_line, second_line);
}
