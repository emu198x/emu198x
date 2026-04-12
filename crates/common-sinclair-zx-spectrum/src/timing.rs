//! Timing constants for the ZX Spectrum 48K.
//!
//! Source references:
//! - `wiki/systems/spectrum/overview.md`
//! - `wiki/systems/spectrum/contention.md`
//! - Adapted from `/Users/stevehill/Projects/Emu198x-Older/crates/common-sinclair-zx-spectrum/src/timing.rs`
//!
//! This module intentionally carries only the stable constants needed for the
//! first 48K implementation pass. ULA-specific contention start drift and
//! scanline-phase behavior belong in the ULA crate, not here.

/// Master crystal frequency in Hz for the 48K PAL machine.
pub const MASTER_HZ_48K: u64 = 14_000_000;

/// CPU clock frequency in Hz for the 48K PAL machine.
pub const CPU_HZ_48K: u64 = 3_500_000;

/// T-states per scanline.
pub const TSTATES_PER_LINE_48K: u32 = 224;

/// Total scanlines per frame.
pub const LINES_PER_FRAME_48K: u32 = 312;

/// Total T-states per frame.
pub const TSTATES_PER_FRAME_48K: u32 = TSTATES_PER_LINE_48K * LINES_PER_FRAME_48K;

/// Visible framebuffer width including border.
pub const SCREEN_WIDTH_48K: usize = 352;

/// Visible framebuffer height including border.
pub const SCREEN_HEIGHT_48K: usize = 296;

/// Per-8-T-state contention delay pattern for the Ferranti 6C001E ULA.
pub const CONTENTION_PATTERN_48K: [u8; 8] = [6, 5, 4, 3, 2, 1, 0, 0];

/// Returns `true` if the address lies in the 48K machine's contended RAM.
#[must_use]
pub const fn is_contended_address_48k(addr: u16) -> bool {
    addr >= 0x4000 && addr <= 0x7fff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_dimensions_match_documented_values() {
        assert_eq!(TSTATES_PER_FRAME_48K, 69_888);
        assert_eq!(SCREEN_WIDTH_48K, 352);
        assert_eq!(SCREEN_HEIGHT_48K, 296);
    }

    #[test]
    fn contended_range_is_screen_ram_bank() {
        assert!(!is_contended_address_48k(0x3fff));
        assert!(is_contended_address_48k(0x4000));
        assert!(is_contended_address_48k(0x7fff));
        assert!(!is_contended_address_48k(0x8000));
    }

    #[test]
    fn contention_pattern_matches_reference() {
        assert_eq!(CONTENTION_PATTERN_48K, [6, 5, 4, 3, 2, 1, 0, 0]);
    }
}
