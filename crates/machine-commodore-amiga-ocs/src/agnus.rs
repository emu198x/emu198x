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
//! PAL Amiga timing (from Hardware Reference Manual):
//!   - 227.5 CCKs per line; we approximate as 227 (an extra CCK lands
//!     in the long line every other frame; not modelled at M6).
//!   - 313 lines per long PAL frame (312 + 1 for interlace odd-field).
//!     We use 312 for non-interlace, which is what KS 1.3 boot
//!     produces.

/// PAL line length in CCKs (approximate, ignoring the half-CCK that
/// lives in long lines).
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
}
