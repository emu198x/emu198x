//! HDL against FUSE at a *fixed* alignment, with no rotation search.
//!
//! Every previous comparison between these two searched sixteen rotations
//! for one that reconciled them, found `(1,1)`, and reported exact. A
//! sixteen-way search absorbs a uniform one-T-state offset without trace —
//! and one is exactly what the engine shows against FUSE once its gate is
//! made to match the HDL. That is the third time a fitted alignment has
//! hidden a real error here, after `SAMPLE_LEAD` and the oracle's
//! `delay_at` origin, so this file removes the freedom.
//!
//! Both sides anchor to the same event, and neither anchor is fitted:
//!
//! - FUSE's frame T-state 0 **is** the interrupt — `spectrum_frame()`
//!   subtracts a frame from `tstates` and `z80_interrupt()` runs
//!   immediately after.
//! - The HDL asserts `msk_int_n` at `vc == 248, hc == 0`
//!   (`fpga_version/rtl/ula.v`).
//!
//! From `vc = 248` to `vc = 0` is `(312 - 248) * 448 = 28672` `clk7`
//! cycles — **14336 T-states**, which is exactly libspectrum's
//! `top_left_pixel` for the Ferranti 5C/6C. The two geometries agree
//! about where the display starts relative to the interrupt, so the
//! mapping below is forced rather than chosen.

/// `clk7` cycles per line, and lines per frame, from the HDL.
const HC_PER_LINE: u32 = 448;
const VC_PER_FRAME: u32 = 312;
/// The line the HDL raises the interrupt on.
const INT_VC: u32 = 248;
/// T-states in a 48K frame.
const FRAME_TSTATES: u32 = 69888;

/// The HDL's `(vc, hc)` at FUSE frame T-state `t`.
///
/// `hc` counts `clk7`, which is two per T-state, and the interrupt puts
/// `hc = 0, vc = 248` at `t = 0`.
fn hdl_position(t: u32) -> (u32, u32) {
    let clk7 = 2 * (t % FRAME_TSTATES);
    let hc = clk7 % HC_PER_LINE;
    let vc = (INT_VC + clk7 / HC_PER_LINE) % VC_PER_FRAME;
    (vc, hc)
}

/// The HDL's `Border_n`: `(vc[7] & vc[6]) | vc[8] | hc[8]` inverted, so
/// display is `hc < 256 && vc < 192`.
fn hdl_border_n(vc: u32, hc: u32) -> bool {
    !((vc & 0x80 != 0 && vc & 0x40 != 0) || vc & 0x100 != 0 || hc & 0x100 != 0)
}

/// Does the HDL's gate permit contention at FUSE T-state `t`?
///
/// `(hc[2] | hc[3]) & /Border`, the two terms of `Nor1`/`Nor2` that depend
/// on nothing but the raster.
fn hdl_window(t: u32) -> bool {
    let (vc, hc) = hdl_position(t);
    hdl_border_n(vc, hc) && ((hc & 0x04 != 0) || (hc & 0x08 != 0))
}

// FUSE's side, transcribed in `machine-sinclair-zx-spectrum-48k`'s
// `io_contention_oracle.rs` and pinned there against
// `spectrum_contend_delay_65432100` frame-wide.
const FIRST_DISPLAY: u32 = 14335;
const PER_LINE: u32 = 224;
const DISPLAY_LINES: u32 = 192;
const CONTENDED_PER_LINE: u32 = 128;
const PATTERN: [u32; 8] = [6, 5, 4, 3, 2, 1, 0, 0];

fn fuse_delay(t: u32) -> u32 {
    if t < FIRST_DISPLAY {
        return 0;
    }
    let into = t - FIRST_DISPLAY;
    if into / PER_LINE >= DISPLAY_LINES {
        return 0;
    }
    let in_line = into % PER_LINE;
    if in_line >= CONTENDED_PER_LINE {
        return 0;
    }
    PATTERN[(in_line % 8) as usize]
}

/// Where FUSE charges a delay, does the HDL permit contention?
///
/// Both are "is this T-state inside the contended window", anchored to the
/// same interrupt. Any offset between them is a real disagreement about
/// when the ULA contends, not a convention.
#[test]
fn the_hdl_window_and_fuse_agree_on_where_contention_starts() {
    let fuse_contends = |t: u32| fuse_delay(t) > 0;

    // Score the natural alignment, then a small sweep purely to *report*
    // what offset would reconcile them. The sweep is diagnostic; the
    // assertion is on offset zero.
    println!("\noffset  mismatched T-states (of {FRAME_TSTATES})");
    let mut by_offset = Vec::new();
    for offset in -4i32..=4 {
        let n = (0..FRAME_TSTATES)
            .filter(|&t| {
                let shifted = (t as i32 + offset).rem_euclid(FRAME_TSTATES as i32) as u32;
                hdl_window(shifted) != fuse_contends(t)
            })
            .count();
        println!("{offset:>+6}  {n}");
        by_offset.push((offset, n));
    }

    let best = by_offset.iter().min_by_key(|(_, n)| *n).expect("non-empty");
    println!("\nbest offset {:+} with {} mismatches", best.0, best.1);

    // Where the first disagreements fall, at the natural alignment.
    let first: Vec<u32> = (0..FRAME_TSTATES)
        .filter(|&t| hdl_window(t) != fuse_contends(t))
        .take(6)
        .collect();
    println!("first mismatches at offset 0: {first:?}");
    for t in first.iter().take(3) {
        let (vc, hc) = hdl_position(*t);
        println!(
            "  t={t}: HDL vc={vc} hc={hc} border_n={} window={} | FUSE delay={}",
            hdl_border_n(vc, hc),
            hdl_window(*t),
            fuse_delay(*t)
        );
    }

    // Locked at the measured value, not at zero. The two describe the
    // *same* window — identical shape, identical duty — displaced by
    // exactly three T-states, with FUSE the earlier. Asserting the
    // displacement rather than agreement is what makes this a gate: it
    // fires if either side moves, including if someone "fixes" one to
    // match the other without recording why.
    //
    // Three is not a new number. The retracted window section at the top
    // of the decision record derived it by hand (14335 against 14338) and
    // was reverted for making the timing survey worse; `sinclair-ula-7k010e`
    // carries the same gap as a comment, noting the 128K contends from
    // T=14361 while the first fetch is at T=14364. It was right about the
    // gap and wrong about what to do with it.
    const MEASURED_OFFSET: i32 = 3;
    let at_measured = by_offset
        .iter()
        .find(|(o, _)| *o == MEASURED_OFFSET)
        .expect("measured offset scored")
        .1;
    assert_eq!(
        at_measured, 0,
        "the HDL window and FUSE's no longer reconcile at {MEASURED_OFFSET:+}          T-states. Best offset is now {:+} with {} mismatches.",
        best.0, best.1
    );
    let at_zero = by_offset
        .iter()
        .find(|(o, _)| *o == 0)
        .expect("offset 0 scored")
        .1;
    assert_ne!(
        at_zero, 0,
        "the HDL and FUSE now agree with no displacement, which would make          the three-T-state finding obsolete — a good outcome, but it must          be recorded rather than silently absorbed."
    );
}
