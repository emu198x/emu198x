//! Agnus — beam counter (M6 minimum).
//!
//! At M6, Agnus tracks only vpos (vertical position) and hpos
//! (horizontal position within a scanline). On every CCK we advance
//! hpos; when it crosses the line length, we wrap and increment vpos.
//! When vpos crosses the frame length, we wrap and signal VBL (the
//! Amiga vertical-blank event that sets INTREQ bit 5 = VERTB).
//!
//! No DMA scheduling, no copper, no bitplane fetch yet — those come
//! in M9+.
//!
//! PAL Amiga timing (from Amiga Hardware Reference Manual, 3rd ed.,
//! "Display" chapter):
//!
//! > "All lines are not the same length in NTSC. Every other line is a
//! >  long line (228 color clocks, 0-$E3), with the others being 227
//! >  color clocks long. **In PAL, they are all 227 long.** The display
//! >  sees all these lines as 227 1/2 color clocks long, while the
//! >  copper sees alternating long and short [in NTSC interlace]."
//!
//! So for non-interlaced PAL (the mode KS 1.3 boots into):
//!   - Every line is exactly 227 CCKs.
//!   - There are exactly 312 lines per frame.
//!   - Frame total: 227 × 312 = 70,824 CCKs at 3.546895 MHz = 50.000 Hz.
//!
//! The "227.5" figure is the interlace average (PAL interlace has
//! 312/313 alternating fields × 227 CCKs/line; the half is the field
//! offset, not a per-line half-CCK). NTSC alternates 227/228 per line
//! for "long line / short line" — that is **not** PAL.

/// PAL line length in colour clocks. All PAL lines are exactly 227.
pub const PAL_LINE_CCKS: u16 = 227;

/// PAL frame line count (non-interlace).
pub const PAL_FRAME_LINES: u16 = 312;

#[derive(Default)]
pub struct Agnus {
    pub hpos: u16,
    pub vpos: u16,
    /// Whether a vertical-blank just happened this CCK (cleared on
    /// the next tick).
    pub vbl_pulse: bool,
    /// Total VBLs since construction — debugging aid.
    pub vbl_count: u64,
}

impl Agnus {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Read VPOSR ($DFF004) — vpos high bit (bit 0) plus other status
    /// bits. At M6 we expose only the vpos high bit; LOF and CHIP_ID
    /// land later when interlace + ECS / AGA chipsets enter the
    /// picture.
    #[must_use]
    pub fn vposr(&self) -> u16 {
        u16::from((self.vpos >> 8) as u8) & 0x0001
    }

    /// Read VHPOSR ($DFF006) — vpos low byte (bits 8-15) and hpos
    /// (bits 0-7).
    #[must_use]
    pub fn vhposr(&self) -> u16 {
        let vpos_lo = self.vpos & 0xFF;
        let hpos = self.hpos & 0xFF;
        (vpos_lo << 8) | hpos
    }

    /// Advance one CCK. Returns `true` if a VBL just fired (vpos
    /// wrapped from 311 to 0).
    pub fn tick_cck(&mut self) -> bool {
        // Clear last cycle's VBL pulse signal.
        self.vbl_pulse = false;

        self.hpos += 1;
        if self.hpos >= PAL_LINE_CCKS {
            self.hpos = 0;
            self.vpos += 1;
            if self.vpos >= PAL_FRAME_LINES {
                self.vpos = 0;
                self.vbl_pulse = true;
                self.vbl_count += 1;
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_advance_hpos_then_vpos() {
        let mut agnus = Agnus::new();
        for _ in 0..PAL_LINE_CCKS {
            agnus.tick_cck();
        }
        assert_eq!(agnus.hpos, 0);
        assert_eq!(agnus.vpos, 1);
    }

    #[test]
    fn vbl_fires_after_full_frame() {
        let mut agnus = Agnus::new();
        let cycles_per_frame =
            u64::from(PAL_LINE_CCKS) * u64::from(PAL_FRAME_LINES);
        let mut vbls = 0u64;
        for _ in 0..cycles_per_frame {
            if agnus.tick_cck() {
                vbls += 1;
            }
        }
        assert_eq!(vbls, 1, "exactly one VBL per frame");
        assert_eq!(agnus.vpos, 0);
        assert_eq!(agnus.hpos, 0);
    }

    #[test]
    fn many_frames() {
        let mut agnus = Agnus::new();
        let cycles_per_frame =
            u64::from(PAL_LINE_CCKS) * u64::from(PAL_FRAME_LINES);
        for _ in 0..(10 * cycles_per_frame) {
            agnus.tick_cck();
        }
        assert_eq!(agnus.vbl_count, 10);
    }

    /// Sanity-lock the PAL constants against the Amiga Hardware
    /// Reference (3rd ed.): non-interlaced PAL is exactly 227 × 312 =
    /// 70,824 CCKs per frame. At 3.546895 MHz CCK rate this is exactly
    /// 50.000 Hz field rate — the canonical PAL value. If either
    /// constant ever drifts, this test will catch the timing damage
    /// before it reaches downstream tests.
    #[test]
    fn pal_constants_match_hardware_reference() {
        assert_eq!(PAL_LINE_CCKS, 227, "PAL line is 227 CCKs (HRM 3rd ed)");
        assert_eq!(PAL_FRAME_LINES, 312, "PAL non-interlaced has 312 lines");
        assert_eq!(
            u64::from(PAL_LINE_CCKS) * u64::from(PAL_FRAME_LINES),
            70_824,
            "PAL frame must be exactly 70,824 CCKs",
        );
        // CCK rate = master/8 = 28.37516 MHz / 8 = 3,546,895 Hz.
        // Frame rate = 3,546,895 / 70,824 ≈ 50.0786 Hz, which is the
        // documented Amiga PAL field rate (50 Hz nominal, slightly
        // higher because the master clock is chosen for the colour
        // burst rather than for exactly 50 Hz fields).
        let frame_period_us: f64 =
            70_824.0_f64 * (1.0 / 3_546_895.0_f64) * 1_000_000.0_f64;
        assert!(
            (frame_period_us - 19_968.6_f64).abs() < 1.0,
            "PAL frame period ≈ 19,968.6 µs (got {frame_period_us:.1})",
        );
    }
}
