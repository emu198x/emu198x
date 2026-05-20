//! Timing constants for baseline Commodore 64 breadbin machines.
//!
//! Source references:
//! - `docs/plans/2026-04-12-emulator-suite-coherent-development-plan.md`
//! - `wiki/decisions/archives-as-source.md`
//! - Adapted from `/Users/stevehill/Projects/198x/Emu198x-Older/crates/machine-commodore-c64/src/config.rs`
//! - Adapted from `/Users/stevehill/Projects/198x/Emu198x-Older/crates/mos-vic-ii/src/lib.rs`

/// Shared visible framebuffer width used by the archived VIC-II implementation.
pub const FRAMEBUFFER_WIDTH: u16 = 416;

/// Timing and raster facts for one baseline C64 hardware profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct C64Timing {
    /// VIC-II variant for this timing profile.
    pub vic_model: &'static str,
    /// Nominal CPU φ2 clock in Hz.
    pub cpu_hz: u64,
    /// CIA TOD tick rate in Hz.
    pub cia_tod_hz: u32,
    /// CIA TOD divider in CPU cycles.
    pub cia_tod_divider: u32,
    /// Total raster lines per frame.
    pub lines_per_frame: u16,
    /// CPU φ2 cycles per raster line.
    pub cycles_per_line: u8,
    /// Total CPU φ2 cycles per frame.
    pub cycles_per_frame: u32,
    /// First visible raster line.
    pub first_visible_line: u16,
    /// Last visible raster line, exclusive.
    pub last_visible_line: u16,
    /// First visible cycle in a raster line.
    pub first_visible_cycle: u8,
    /// Last visible cycle in a raster line, exclusive.
    pub last_visible_cycle: u8,
    /// Captured framebuffer width in pixels.
    pub framebuffer_width: u16,
    /// Captured framebuffer height in pixels.
    pub framebuffer_height: u16,
}

impl C64Timing {
    /// Visible raster lines captured in the framebuffer.
    #[must_use]
    pub const fn visible_lines(self) -> u16 {
        self.last_visible_line - self.first_visible_line
    }

    /// Visible CPU cycles captured in each raster line.
    #[must_use]
    pub const fn visible_cycles(self) -> u8 {
        self.last_visible_cycle - self.first_visible_cycle
    }
}

/// Commodore 64 PAL breadbin timing (6569 VIC-II).
pub const TIMING_PAL_BREADBIN: C64Timing = C64Timing {
    vic_model: "mos-6569",
    cpu_hz: 985_248,
    cia_tod_hz: 50,
    cia_tod_divider: 19_705,
    lines_per_frame: 312,
    cycles_per_line: 63,
    cycles_per_frame: 19_656,
    first_visible_line: 0,
    last_visible_line: 312,
    first_visible_cycle: 10,
    last_visible_cycle: 62,
    framebuffer_width: FRAMEBUFFER_WIDTH,
    framebuffer_height: 312,
};

/// Commodore 64 NTSC breadbin timing (6567 VIC-II).
pub const TIMING_NTSC_BREADBIN: C64Timing = C64Timing {
    vic_model: "mos-6567",
    cpu_hz: 1_022_727,
    cia_tod_hz: 60,
    cia_tod_divider: 17_045,
    lines_per_frame: 263,
    cycles_per_line: 65,
    cycles_per_frame: 17_095,
    first_visible_line: 14,
    last_visible_line: 258,
    first_visible_cycle: 10,
    last_visible_cycle: 62,
    framebuffer_width: FRAMEBUFFER_WIDTH,
    framebuffer_height: 244,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pal_frame_geometry_matches_reference_values() {
        assert_eq!(TIMING_PAL_BREADBIN.cpu_hz, 985_248);
        assert_eq!(TIMING_PAL_BREADBIN.lines_per_frame, 312);
        assert_eq!(TIMING_PAL_BREADBIN.cycles_per_line, 63);
        assert_eq!(TIMING_PAL_BREADBIN.cycles_per_frame, 19_656);
        assert_eq!(TIMING_PAL_BREADBIN.framebuffer_width, 416);
        assert_eq!(TIMING_PAL_BREADBIN.framebuffer_height, 312);
    }

    #[test]
    fn ntsc_frame_geometry_matches_reference_values() {
        assert_eq!(TIMING_NTSC_BREADBIN.cpu_hz, 1_022_727);
        assert_eq!(TIMING_NTSC_BREADBIN.lines_per_frame, 263);
        assert_eq!(TIMING_NTSC_BREADBIN.cycles_per_line, 65);
        assert_eq!(TIMING_NTSC_BREADBIN.cycles_per_frame, 17_095);
        assert_eq!(TIMING_NTSC_BREADBIN.framebuffer_width, 416);
        assert_eq!(TIMING_NTSC_BREADBIN.framebuffer_height, 244);
    }

    #[test]
    fn cia_tod_dividers_match_documented_hardware_values() {
        assert_eq!(TIMING_PAL_BREADBIN.cia_tod_hz, 50);
        assert_eq!(TIMING_PAL_BREADBIN.cia_tod_divider, 19_705);
        assert_eq!(TIMING_NTSC_BREADBIN.cia_tod_hz, 60);
        assert_eq!(TIMING_NTSC_BREADBIN.cia_tod_divider, 17_045);
    }

    #[test]
    fn visible_window_dimensions_match_archived_vic_reference() {
        assert_eq!(TIMING_PAL_BREADBIN.visible_cycles(), 52);
        assert_eq!(TIMING_NTSC_BREADBIN.visible_cycles(), 52);
        assert_eq!(TIMING_PAL_BREADBIN.visible_lines(), 312);
        assert_eq!(TIMING_NTSC_BREADBIN.visible_lines(), 244);
    }
}
