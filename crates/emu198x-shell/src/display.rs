//! Television geometry: turning a core's raster into a pixel aspect ratio.
//!
//! A framebuffer is an array of samples, not a picture. What shape those
//! samples are on screen depends on how fast the machine emitted them and how
//! many of them the television spreads across its 4:3 face — never on how much
//! of the signal we chose to keep. Two emulators can crop the same machine
//! very differently and still owe the viewer the same proportions.
//!
//! That is the whole reason this module exists. Deriving the stretch from the
//! framebuffer's own dimensions ties the picture's shape to the crop, so
//! trimming a few lines of border silently changes the geometry.
//!
//! # The derivation
//!
//! A set displays its active picture across a 4:3 face. If the machine emits
//! `N_h` framebuffer pixels in one active line time and `N_v` framebuffer
//! lines fill the active height, then one pixel is `4/N_h` wide and `3/N_v`
//! tall, so
//!
//! ```text
//! PAR = (4/3) × N_v / N_h
//! ```
//!
//! `N_h` comes from the pixel clock; `N_v` is the caller's to state, because
//! only the core knows whether its framebuffer holds one field or two.

use crate::machine::Region;

/// Width ÷ height of the face a domestic set presents. Both 625- and
/// 525-line broadcast standards are 4:3; widescreen is a later story.
const FRAME_ASPECT: f64 = 4.0 / 3.0;

/// Active picture time in one 64 µs line. The rest is sync, burst and porch,
/// which the set never shows, and a domestic set overscans a little more.
///
/// Published PAL ratios back-calculate to 52.02 µs for the C64 and 51.97 µs
/// for the Atari 2600, so 52.0 sits between them and reproduces both inside a
/// tenth of a percent. See [`NTSC_ACTIVE_LINE_SECONDS`] for the same exercise
/// on the other standard, where the agreement is tighter still.
const PAL_ACTIVE_LINE_SECONDS: f64 = 52.0e-6;

/// Active picture time in one 63.55 µs line.
///
/// Broadcast documents give about 52.6 µs here, but that is the whole active
/// video interval, and a domestic set overscans some of it. 52.148 µs is what
/// four independently published pixel aspect ratios agree on, from four chips
/// with four different clocks:
///
/// | Machine | Published | Implies |
/// |---|---|---|
/// | C64 NTSC | 0.7500 | 52.148 µs |
/// | NES NTSC | 8:7 | 52.148 µs |
/// | TMS9918 | 8:7 | 52.148 µs |
/// | Atari 2600 NTSC | 12:7 | 52.148 µs |
///
/// Four chips converging on one figure to three decimals is the measurement.
/// Taking 52.6 instead puts every NTSC machine about 0.9% out.
const NTSC_ACTIVE_LINE_SECONDS: f64 = 52.148e-6;

/// Lines of a 625-line signal a set displays: 312.5 per field, less the
/// vertical interval. Pass this as `lines_per_tv_height` for a progressive
/// PAL core, and twice it for one whose framebuffer holds both fields.
pub const PAL_ACTIVE_LINES: f64 = 288.0;

/// The same for a 525-line signal.
pub const NTSC_ACTIVE_LINES: f64 = 240.0;

/// Active lines for a region, or `None` when the region is not a television.
///
/// Prefer this to writing the number at a call site: passing a frame's *total*
/// line count where the *active* count belongs is the easy mistake here, and
/// it is silent — the picture is merely the wrong shape.
#[must_use]
pub fn active_lines(region: Region) -> Option<f64> {
    match region {
        Region::Pal => Some(PAL_ACTIVE_LINES),
        Region::Ntsc => Some(NTSC_ACTIVE_LINES),
        _ => None,
    }
}

/// Returns the pixel aspect ratio for a core's framebuffer, or `None` when
/// the region does not describe a television.
///
/// `pixel_clock_hz` is the rate at which the core emits *framebuffer* pixels
/// along a scanline — not the CPU clock, and not the dot clock of whichever
/// video mode happens to be selected. A core that renders every mode into one
/// fixed-width buffer has one pixel clock, whatever the mode.
///
/// `lines_per_tv_height` is how many framebuffer lines the set spreads over
/// its full height: the active line count for a progressive core, twice that
/// for one whose framebuffer holds both interlaced fields.
///
/// Neither argument mentions the framebuffer's dimensions, and that is the
/// point — see the module documentation.
///
/// # Examples
///
/// A ZX80 emits two pixels per 3.25 MHz T-state and fills PAL's 288 active
/// lines once, so its pixels are wider than they are tall:
///
/// ```
/// use emu198x_shell::display::pixel_aspect_ratio;
/// use emu198x_shell::machine::Region;
///
/// let par = pixel_aspect_ratio(Region::Pal, 6_500_000.0, 288.0).expect("PAL");
/// assert!((par - 1.136).abs() < 0.001);
/// ```
#[must_use]
pub fn pixel_aspect_ratio(
    region: Region,
    pixel_clock_hz: f64,
    lines_per_tv_height: f64,
) -> Option<f32> {
    let active_line_seconds = match region {
        Region::Pal => PAL_ACTIVE_LINE_SECONDS,
        Region::Ntsc => NTSC_ACTIVE_LINE_SECONDS,
        // Not a television. A handheld's LCD has square pixels because its
        // pixels are square, not because a standard says so.
        _ => return None,
    };
    if pixel_clock_hz <= 0.0 || lines_per_tv_height <= 0.0 {
        return None;
    }
    let pixels_across = pixel_clock_hz * active_line_seconds;
    Some((FRAME_ASPECT * lines_per_tv_height / pixels_across) as f32)
}

/// What a machine's video output actually reached.
///
/// The distinction matters because the three kinds derive their geometry in
/// genuinely different ways, and picking the wrong one is not a small error.
///
/// A **television** overscans: it shows a fixed slice of each line and a fixed
/// number of lines, whatever the machine sends. So its geometry comes from the
/// raster — a clock and a line count — and is completely independent of how
/// much of the signal we chose to keep in a framebuffer.
///
/// A **monitor** has no such convention. It displays the raster it is handed,
/// so the framebuffer *is* the picture and its dimensions decide the shape.
/// That is the one case where deriving from the framebuffer is right rather
/// than a mistake.
///
/// An **LCD** has square pixels because its pixels are square.
///
/// [`Region`] cannot stand in for any of this. It describes the signal a
/// machine generates, and machines exist whose region and display disagree —
/// a Game Gear reports `Region::Ntsc` and drives a panel; an Amiga running a
/// display card drives a monitor from a chipset that also feeds a set.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Display {
    /// A domestic television, which overscans.
    Television {
        /// Broadcast standard, which fixes how much of each line is shown.
        region: Region,
        /// Framebuffer pixels emitted per second along a scanline.
        pixel_clock_hz: f64,
        /// Framebuffer lines spanning the set's height: the active line count
        /// for a progressive core, twice it for an interlaced framebuffer.
        lines_per_tv_height: f64,
    },
    /// A flat panel. Square pixels, and no broadcast convention applies.
    Lcd,
    /// A dedicated monitor, which shows the whole raster and fills this shape.
    Monitor {
        /// Picture width ÷ height of the tube, typically `4.0 / 3.0`.
        aspect: f32,
    },
}

impl Display {
    /// A television whose clock differs between standards — the common case,
    /// since most chips of the era were built twice.
    ///
    /// Returns `None` when the region is not a broadcast standard, which is
    /// the caller's cue that this machine wants a different variant.
    #[must_use]
    pub fn television_for_region(region: Region, pal_hz: f64, ntsc_hz: f64) -> Option<Self> {
        let (pixel_clock_hz, lines_per_tv_height) = match region {
            Region::Pal => (pal_hz, PAL_ACTIVE_LINES),
            Region::Ntsc => (ntsc_hz, NTSC_ACTIVE_LINES),
            _ => return None,
        };
        Some(Self::Television {
            region,
            pixel_clock_hz,
            lines_per_tv_height,
        })
    }

    /// Pixel aspect ratio for this display.
    ///
    /// The framebuffer dimensions are used by [`Self::Monitor`] and ignored by
    /// the others, which is the asymmetry this type exists to express: a
    /// monitor shows the whole buffer, so its shape decides the geometry,
    /// while a television's crop cannot.
    #[must_use]
    pub fn pixel_aspect_ratio(&self, fb_width: u32, fb_height: u32) -> f32 {
        match *self {
            Self::Television {
                region,
                pixel_clock_hz,
                lines_per_tv_height,
            } => pixel_aspect_ratio(region, pixel_clock_hz, lines_per_tv_height).unwrap_or(1.0),
            Self::Lcd => 1.0,
            Self::Monitor { aspect } if fb_width > 0 && fb_height > 0 => {
                aspect * fb_height as f32 / fb_width as f32
            }
            Self::Monitor { .. } => 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Measured against MAME 0.289: the ZX80's character cells render 1.52:1
    /// on a set, and 1.33:1 if the 320×240 framebuffer is shown square.
    #[test]
    fn zx80_pixels_are_wider_than_they_are_tall() {
        let par = pixel_aspect_ratio(Region::Pal, 6_500_000.0, 288.0).expect("PAL");
        assert!(
            (par - 1.136).abs() < 0.001,
            "expected ~1.136, got {par}: 2 px per 3.25 MHz T-state over PAL's 288 active lines"
        );
    }

    /// The defect this replaces. Deriving the stretch from the framebuffer's
    /// own dimensions gives two different answers for two crops of one
    /// machine — here our ZX80 window and MAME 0.289's, over the identical
    /// raster. The new derivation takes no crop argument at all, so it cannot
    /// express the bug; that is structural, and there is nothing to assert.
    #[test]
    fn deriving_from_the_framebuffer_ties_geometry_to_the_crop() {
        let fill_four_thirds = |w: f64, h: f64| FRAME_ASPECT * h / w;

        let ours = fill_four_thirds(320.0, 240.0);
        let mames = fill_four_thirds(384.0, 311.0);

        assert!(
            (ours - mames).abs() > 0.07,
            "same machine, same raster, {ours} vs {mames} — the crop should not decide this"
        );
    }

    /// An interlaced framebuffer holds two fields, so twice as many lines
    /// span the same height — and the pixels come out taller.
    #[test]
    fn interlace_doubles_the_line_count_and_halves_the_pixel_height() {
        let progressive = pixel_aspect_ratio(Region::Pal, 14_187_500.0, 288.0).expect("PAL");
        let interlaced = pixel_aspect_ratio(Region::Pal, 14_187_500.0, 576.0).expect("PAL");
        assert!((interlaced / progressive - 2.0).abs() < 0.001);
    }

    /// NTSC's active line is longer, so the same pixel clock lays down more
    /// pixels across the same face, and each one is narrower.
    #[test]
    fn ntsc_pixels_are_narrower_than_pal_at_the_same_clock() {
        let pal = pixel_aspect_ratio(Region::Pal, 6_500_000.0, 288.0).expect("PAL");
        let ntsc = pixel_aspect_ratio(Region::Ntsc, 6_500_000.0, 288.0).expect("NTSC");
        assert!(ntsc < pal, "NTSC {ntsc} should be narrower than PAL {pal}");
    }

    /// Corroboration from outside this repository. The VIC-II's pixel aspect
    /// is widely published — 0.9365 for PAL, 0.7500 for NTSC — and derived by
    /// other people from the same hardware, so reproducing it is evidence the
    /// formula is right rather than merely self-consistent. Neither machine is
    /// migrated yet; these are the numbers to check against when they are.
    ///
    /// Both land inside a tenth of a percent, which is what calibrating
    /// [`NTSC_ACTIVE_LINE_SECONDS`] against four published ratios bought — it
    /// was a percent out before, and every NTSC machine would have inherited
    /// that.
    #[test]
    fn the_formula_reproduces_published_vic_ii_ratios() {
        let pal = pixel_aspect_ratio(Region::Pal, 7_881_984.0, PAL_ACTIVE_LINES).expect("PAL");
        assert!(
            (pal - 0.9365).abs() < 0.001,
            "C64 PAL should be ~0.9365, got {pal}"
        );

        let ntsc = pixel_aspect_ratio(Region::Ntsc, 8_181_816.0, NTSC_ACTIVE_LINES).expect("NTSC");
        assert!(
            (ntsc - 0.7500).abs() < 0.001,
            "C64 NTSC should be 0.75, got {ntsc}"
        );
    }

    /// The four ratios that calibrate `NTSC_ACTIVE_LINE_SECONDS`. They come
    /// from four chips at four clocks and were published by different people,
    /// so agreement here is agreement between independent sources rather than
    /// one number restated. A change to that constant should have to explain
    /// which of these it is prepared to break.
    #[test]
    fn the_published_ntsc_ratios_all_land() {
        for (name, clock, want) in [
            ("C64", 8_181_816.0, 0.7500),
            ("NES", 5_369_318.0, 8.0 / 7.0),
            ("TMS9918", 5_369_318.0, 8.0 / 7.0),
            ("Atari 2600", 3_579_545.0, 12.0 / 7.0),
        ] {
            let got = pixel_aspect_ratio(Region::Ntsc, clock, NTSC_ACTIVE_LINES).expect("NTSC");
            assert!(
                (f64::from(got) - want).abs() < 0.002,
                "{name}: published {want}, derived {got}"
            );
        }
    }

    /// The asymmetry the `Display` type exists to express: a television's
    /// geometry cannot depend on the crop, and a monitor's must.
    #[test]
    fn only_a_monitor_reads_the_framebuffer() {
        let tv = Display::Television {
            region: Region::Pal,
            pixel_clock_hz: 6_500_000.0,
            lines_per_tv_height: PAL_ACTIVE_LINES,
        };
        assert_eq!(
            tv.pixel_aspect_ratio(320, 240),
            tv.pixel_aspect_ratio(384, 311),
            "a set overscans; what we kept cannot change the pixel's shape"
        );

        let monitor = Display::Monitor { aspect: 4.0 / 3.0 };
        assert_ne!(
            monitor.pixel_aspect_ratio(320, 240),
            monitor.pixel_aspect_ratio(384, 311),
            "a monitor shows the whole raster, so the buffer is the picture"
        );
        assert!((monitor.pixel_aspect_ratio(320, 240) - 1.0).abs() < 1e-6);

        assert_eq!(Display::Lcd.pixel_aspect_ratio(160, 144), 1.0);
    }

    /// A machine on neither broadcast standard is not a television, and the
    /// constructor says so instead of inventing a clock.
    #[test]
    fn television_for_region_declines_a_non_broadcast_region() {
        assert_eq!(
            Display::television_for_region(Region::Other, 6_500_000.0, 6_500_000.0),
            None
        );
    }

    #[test]
    fn a_machine_that_never_drove_a_television_has_no_answer() {
        assert_eq!(pixel_aspect_ratio(Region::Other, 6_500_000.0, 288.0), None);
    }

    #[test]
    fn nonsense_inputs_are_declined_rather_than_producing_infinity() {
        assert_eq!(pixel_aspect_ratio(Region::Pal, 0.0, 288.0), None);
        assert_eq!(pixel_aspect_ratio(Region::Pal, 6_500_000.0, 0.0), None);
    }
}
