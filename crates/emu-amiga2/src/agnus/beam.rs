//! Beam counter utilities.
//!
//! The beam position is tracked directly in the Agnus struct (vpos, hpos).
//! This module provides helper functions for display-area calculations.

use super::{CCKS_PER_LINE, PAL_LINES_PER_FRAME, NTSC_LINES_PER_FRAME};
use crate::config::Region;

/// Crystal ticks per frame for the given region.
#[must_use]
pub const fn ticks_per_frame(region: Region) -> u64 {
    let lines = match region {
        Region::Pal => PAL_LINES_PER_FRAME as u64,
        Region::Ntsc => NTSC_LINES_PER_FRAME as u64,
    };
    lines * CCKS_PER_LINE as u64 * 8
}

/// Crystal frequency for the given region (Hz).
#[must_use]
pub const fn crystal_hz(region: Region) -> u64 {
    match region {
        Region::Pal => 28_375_160,
        Region::Ntsc => 28_636_360,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pal_ticks_per_frame() {
        // 312 lines * 227 CCKs * 8 = 566,592
        assert_eq!(ticks_per_frame(Region::Pal), 566_592);
    }

    #[test]
    fn ntsc_ticks_per_frame() {
        // 262 lines * 227 CCKs * 8 = 475,792
        assert_eq!(ticks_per_frame(Region::Ntsc), 475_792);
    }
}
