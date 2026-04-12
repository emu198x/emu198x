//! Timing constants for the ZX Spectrum 48K.
//!
//! Source references:
//! - `wiki/systems/spectrum/overview.md`
//! - `wiki/systems/spectrum/contention.md`
//! - Adapted from `/Users/stevehill/Projects/Emu198x-Older/crates/common-sinclair-zx-spectrum/src/timing.rs`
//!
//! This module holds the stable timing data for the 48K machine plus the
//! `FrameTiming` descriptor used by ULA implementations.

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

/// Standard Spectrum framebuffer width including border.
pub const SCREEN_WIDTH: usize = SCREEN_WIDTH_48K;

/// Standard Spectrum framebuffer height including border.
pub const SCREEN_HEIGHT: usize = SCREEN_HEIGHT_48K;

/// Hi-res framebuffer width for Timex-class modes.
pub const SCREEN_WIDTH_HIRES: usize = 704;

/// Per-8-T-state contention delay pattern for the Ferranti 6C001E ULA.
pub const CONTENTION_PATTERN_48K: [u8; 8] = [6, 5, 4, 3, 2, 1, 0, 0];

/// Frame timing constants for one Spectrum-family video configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameTiming {
    /// Master crystal frequency in Hz.
    pub master_hz: u64,
    /// Half-cycles per CPU T-state.
    pub cpu_divisor: u32,
    /// T-states per scanline.
    pub tstates_per_line: u32,
    /// Half-cycles per scanline.
    pub halfcycles_per_line: u32,
    /// Total scanlines per frame.
    pub lines_per_frame: u32,
    /// Total half-cycles per frame.
    pub halfcycles_per_frame: u32,
    /// Total T-states per frame.
    pub tstates_per_frame: u32,
    /// First visible border line.
    pub first_border_line: u32,
    /// First active display line.
    pub first_screen_line: u32,
    /// Last active display line plus one.
    pub last_screen_line: u32,
    /// Last visible border line plus one.
    pub last_border_line: u32,
    /// T-state offset where screen pixels begin.
    pub first_screen_tstate: u32,
    /// Number of screen pixels per line.
    pub screen_pixels_per_line: u32,
    /// T-state offset where the left border begins.
    pub left_border_tstate: u32,
    /// T-state offset where the right border ends.
    pub right_border_tstate: u32,
    /// T-state within the frame where contention starts.
    pub contention_start_tstate: u32,
    /// Eight-step contention delay pattern.
    pub contention_pattern: [u8; 8],
    /// Pattern phase offset.
    pub contention_phase: u32,
    /// Number of contended T-states per line.
    pub contention_tstates_per_line: u32,
    /// T-state where INT asserts.
    pub interrupt_start_tstate: u32,
    /// INT duration in T-states.
    pub interrupt_length_tstates: u32,
}

/// ZX Spectrum 48K (Ferranti 6C001E ULA, PAL).
pub const TIMING_48K: FrameTiming = FrameTiming {
    master_hz: MASTER_HZ_48K,
    cpu_divisor: 4,
    tstates_per_line: TSTATES_PER_LINE_48K,
    halfcycles_per_line: TSTATES_PER_LINE_48K * 4,
    lines_per_frame: LINES_PER_FRAME_48K,
    halfcycles_per_frame: TSTATES_PER_LINE_48K * 4 * LINES_PER_FRAME_48K,
    tstates_per_frame: TSTATES_PER_FRAME_48K,
    first_border_line: 8,
    first_screen_line: 64,
    last_screen_line: 256,
    last_border_line: 304,
    first_screen_tstate: 24,
    screen_pixels_per_line: 256,
    left_border_tstate: 0,
    right_border_tstate: 176,
    contention_start_tstate: 14_335,
    contention_pattern: CONTENTION_PATTERN_48K,
    contention_phase: 0,
    contention_tstates_per_line: 128,
    interrupt_start_tstate: 0,
    interrupt_length_tstates: 32,
};

impl FrameTiming {
    /// Converts a T-state count to half-cycles.
    #[must_use]
    pub const fn tstates_to_hc(&self, tstates: u32) -> u32 {
        tstates * self.cpu_divisor
    }

    /// Converts half-cycles to T-states, rounding down.
    #[must_use]
    pub const fn hc_to_tstates(&self, hc: u32) -> u32 {
        hc / self.cpu_divisor
    }

    /// Splits a frame T-state into scanline and line-local position.
    #[must_use]
    pub const fn tstate_to_line_pos(&self, tstate: u32) -> (u32, u32) {
        (
            tstate / self.tstates_per_line,
            tstate % self.tstates_per_line,
        )
    }
}

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
        assert_eq!(TIMING_48K.halfcycles_per_frame, 279_552);
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

    #[test]
    fn timing_conversion_helpers_match_reference_values() {
        assert_eq!(TIMING_48K.tstates_to_hc(224), 896);
        assert_eq!(TIMING_48K.hc_to_tstates(896), 224);
        assert_eq!(TIMING_48K.tstate_to_line_pos(224), (1, 0));
    }
}
