//! The palette, checked against the chip's own data manual.
//!
//! `PALETTE` shipped as fifteen hex constants with no citation. This derives
//! them instead, from Table 2-3 of the TMS9918A/9928A/9929A data manual
//! (November 1982), held at
//! `reference/by-topic/vdp-tms9918/tms9918a-9928a-9929a-vdp-data-manual-nov82.txt`.
//!
//! That matters more here than in most crates. Eight machines share this chip
//! — the ColecoVision, MSX, SG-1000, Sord M5, SVI-328, Memotech MTX, Tatung
//! Einstein and Master System — so a wrong entry is wrong in every frame all
//! eight of them draw, and nothing else in the repository was checking it.
//!
//! # How the table becomes RGB
//!
//! Table 2-3 gives the TMS9928A/9929A colours as `Y`, `R-Y` and `B-Y`. The
//! colour-difference values carry an offset, which the same table states by
//! listing `BLACK LEVEL` as `Y = 0.00` with `R-Y = B-Y = .47`: so the signed
//! difference is the tabulated value less `.47`.
//!
//! From there `R = Y + (R-Y)`, `B = Y + (B-Y)`, and `G` falls out of the
//! luminance definition. Rec. 601 coefficients reproduce the shipped table to
//! the byte, which is itself the evidence that they are the ones it was built
//! with.

use ti_tms9918::PALETTE;

/// `(Y, R-Y, B-Y)` per Table 2-3, TMS9928A/9929A columns, index 1-15.
/// Index 0 is transparent and has no entry.
const TABLE_2_3: [(f64, f64, f64); 15] = [
    (0.00, 0.47, 0.47), // 1  black
    (0.53, 0.07, 0.20), // 2  medium green
    (0.67, 0.17, 0.27), // 3  light green
    (0.40, 0.40, 1.00), // 4  dark blue
    (0.53, 0.43, 0.93), // 5  light blue
    (0.47, 0.83, 0.30), // 6  dark red
    (0.73, 0.00, 0.70), // 7  cyan
    (0.53, 0.93, 0.27), // 8  medium red
    (0.67, 0.93, 0.27), // 9  light red
    (0.73, 0.57, 0.07), // A  dark yellow
    (0.80, 0.57, 0.17), // B  light yellow
    (0.47, 0.13, 0.23), // C  dark green
    (0.53, 0.73, 0.67), // D  magenta
    (0.80, 0.47, 0.47), // E  gray
    (1.00, 0.47, 0.47), // F  white
];

/// The offset Table 2-3 states by giving `BLACK LEVEL` an `R-Y` and `B-Y` of
/// `.47` at `Y = 0.00`.
const COLOUR_DIFFERENCE_OFFSET: f64 = 0.47;

fn channel(value: f64) -> u8 {
    // Clamped, and the clamp is load-bearing: light red's `R-Y` puts R at 1.13,
    // so its realised luminance is lower than the 0.67 the table nominates.
    // That looks like a bad entry when the table is read back off the RGB and
    // is the chip being asked for more red than the signal can carry.
    let scaled = (value * 255.0).round();
    if scaled < 0.0 {
        0
    } else if scaled > 255.0 {
        255
    } else {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            scaled as u8
        }
    }
}

fn derive(index: usize) -> (u8, u8, u8) {
    let (y, r_y, b_y) = TABLE_2_3[index - 1];
    let r = y + (r_y - COLOUR_DIFFERENCE_OFFSET);
    let b = y + (b_y - COLOUR_DIFFERENCE_OFFSET);
    let g = (y - 0.299 * r - 0.114 * b) / 0.587;
    (channel(r), channel(g), channel(b))
}

/// Every colour the chip can show is the one its data manual specifies.
#[test]
fn the_palette_is_table_2_3() {
    for (index, shipped) in PALETTE.iter().enumerate().skip(1) {
        let (r, g, b) = derive(index);
        let (sr, sg, sb) = (
            ((shipped >> 16) & 0xFF) as u8,
            ((shipped >> 8) & 0xFF) as u8,
            (shipped & 0xFF) as u8,
        );
        // One count of slack per channel, for the rounding: the manual gives
        // two decimal places, which is finer than a byte in some places and
        // coarser in others.
        for (name, derived, ours) in [("R", r, sr), ("G", g, sg), ("B", b, sb)] {
            let delta = i32::from(derived) - i32::from(ours);
            assert!(
                delta.abs() <= 1,
                "colour {index:X}, channel {name}: Table 2-3 derives {derived}, \
                 the palette ships {ours}"
            );
        }
    }
}

/// Transparent is transparent, and the manual says what happens when nothing
/// covers it.
///
/// Table 2-3 gives colour 0 no luminance or chrominance at all, and the note
/// above it says: *"Whenever all planes are of the transparent color at a
/// given point, and external video is not selected, the color shown at that
/// point will be black."*
#[test]
fn transparent_carries_no_colour() {
    assert_eq!(
        PALETTE[0] >> 24,
        0,
        "colour 0 is transparent, so it should carry no alpha"
    );
}

/// Which chip this is.
///
/// The manual notes that *"the gray levels differ slightly for the TMS9918A
/// when compared to the TMS9928A/9929A"*, and Table 2-3 carries both columns.
/// Two colours separate them: cyan is luminance `.67` on the TMS9918A against
/// `Y = .73` on the TMS9928A/9929A, and dark green `.46` against `.47`.
///
/// The shipped palette is the **TMS9928A/9929A** column — the component-output
/// part. That was not written down anywhere before this test, and it is a
/// choice rather than an accident: every machine using this crate gets the
/// component colours whether its board carried a composite TMS9918A or not.
/// Worth revisiting per machine; asserted here so that it cannot drift
/// silently in the meantime.
#[test]
fn the_palette_is_the_component_variant() {
    let luminance = |index: usize| {
        let c = PALETTE[index];
        let (r, g, b) = (
            f64::from((c >> 16) & 0xFF),
            f64::from((c >> 8) & 0xFF),
            f64::from(c & 0xFF),
        );
        (0.299 * r + 0.587 * g + 0.114 * b) / 255.0
    };

    let cyan = luminance(0x7);
    assert!(
        (cyan - 0.73).abs() < 0.01,
        "cyan should be the TMS9928A/9929A's Y of .73, not the TMS9918A's \
         luminance of .67; it is {cyan:.3}"
    );

    let dark_green = luminance(0xC);
    assert!(
        (dark_green - 0.47).abs() < 0.01,
        "dark green should be the TMS9928A/9929A's .47 rather than the \
         TMS9918A's .46; it is {dark_green:.3}"
    );
}

/// And the rule that decides what an uncovered transparent pixel looks like.
///
/// The note above Table 2-3: *"Whenever all planes are of the transparent
/// color at a given point, and external video is not selected, the color shown
/// at that point will be black."*
///
/// So transparent is not a colour that reaches the screen. With a transparent
/// pattern colour over a transparent backdrop and nothing behind it, the pixel
/// is black — `PALETTE[1]` — rather than `PALETTE[0]`, which carries no alpha
/// and would composite as a hole.
///
/// The code does this by falling through to the backdrop and mapping a
/// transparent backdrop to black, in two places a fix could later touch
/// separately. Hence a test on the outcome rather than on either step.
#[test]
fn an_uncovered_transparent_pixel_is_black() {
    use ti_tms9918::{Tms9918, VdpRegion};

    let mut vdp = Tms9918::new(VdpRegion::Ntsc);
    // Graphics I, display enabled: R1 bit 6 turns the screen on.
    vdp.write_control(0x00);
    vdp.write_control(0x80);
    vdp.write_control(0x40);
    vdp.write_control(0x81);
    // Name table at $0000, colour at $2000, patterns at $0800.
    for (value, reg) in [(0x00, 0x82), (0x80, 0x83), (0x01, 0x84)] {
        vdp.write_control(value);
        vdp.write_control(reg);
    }
    // Backdrop transparent.
    vdp.write_control(0x00);
    vdp.write_control(0x87);

    // Tile 0 everywhere, a solid pattern, and both its colours transparent.
    for row in 0..8u16 {
        vdp.write_vram(0x0800 + row, 0xFF);
    }
    vdp.write_vram(0x2000, 0x00);

    for _ in 0..262 {
        vdp.tick_scanline();
    }

    let width = vdp.framebuffer_width() as usize;
    let centre = (vdp.framebuffer_height() as usize / 2) * width + width / 2;
    assert_eq!(
        vdp.framebuffer()[centre],
        PALETTE[1],
        "an all-transparent pixel should show black, not {:#010X}",
        vdp.framebuffer()[centre]
    );
}
