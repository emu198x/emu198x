//! Game Boy DMG timing constants.
//!
//! All numbers verified against
//! [`wiki/systems/nintendo-game-boy/timing.md`](../../../wiki/systems/nintendo-game-boy/timing.md).
//! Everything derives from the 4.194304 MHz master clock; m-cycles
//! are master/4 (the [`sharp-lr35902`](crate) tick rate per
//! [`sm83-abstraction-level.md`](../../../wiki/decisions/sm83-abstraction-level.md)).
//!
//! CGB double-speed mode is not modelled here — it's a CPU-domain
//! knob that lives on the per-machine struct.

/// Master clock in Hz: 2²² (exact).
pub const DMG_MASTER_HZ: u32 = 4_194_304;

/// PPU dot clock in Hz — same as the master clock; one dot per
/// T-cycle.
pub const DOT_CLOCK_HZ: u32 = DMG_MASTER_HZ;

/// CPU m-cycle rate: master / 4.
pub const MCYCLE_HZ: u32 = DMG_MASTER_HZ / 4;

/// Display refresh in Hz, derived: master / dots_per_frame.
pub const DMG_REFRESH_HZ_FRAC: (u32, u32) = (DMG_MASTER_HZ, DOTS_PER_FRAME);

/// Display refresh expressed as a `f64` for convenience.
pub const DMG_REFRESH_HZ: f64 = DMG_MASTER_HZ as f64 / DOTS_PER_FRAME as f64;

/// PPU dot count per scanline. Constant across all PPU modes.
pub const DOTS_PER_SCANLINE: u32 = 456;

/// CPU m-cycle count per scanline (456 / 4).
pub const MCYCLES_PER_SCANLINE: u32 = DOTS_PER_SCANLINE / 4;

/// Total scanlines per frame (visible + VBlank).
pub const SCANLINES_PER_FRAME: u32 = 154;

/// Visible scanlines (LY = 0..143).
pub const VISIBLE_SCANLINES: u32 = 144;

/// VBlank scanlines (LY = 144..153).
pub const VBLANK_SCANLINES: u32 = SCANLINES_PER_FRAME - VISIBLE_SCANLINES;

/// PPU dots per frame: 456 × 154.
pub const DOTS_PER_FRAME: u32 = DOTS_PER_SCANLINE * SCANLINES_PER_FRAME;

/// CPU m-cycles per frame.
pub const MCYCLES_PER_FRAME: u32 = DOTS_PER_FRAME / 4;

/// Visible screen width in pixels.
pub const SCREEN_WIDTH: u32 = 160;

/// Visible screen height in pixels.
pub const SCREEN_HEIGHT: u32 = 144;

/// PPU mode 2 (OAM scan) length in dots — fixed across scanlines.
pub const PPU_MODE2_DOTS: u32 = 80;

/// Minimum PPU mode 3 (pixel transfer) length in dots; the actual
/// value grows with sprite count, window activation, and SCX
/// fine-scroll alignment.
pub const PPU_MODE3_MIN_DOTS: u32 = 172;

/// OAM DMA byte transfer count (and m-cycle count — one byte per
/// m-cycle for 160 m-cycles).
pub const OAM_DMA_M_CYCLES: u32 = 160;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_layout_is_self_consistent() {
        assert_eq!(DOTS_PER_FRAME, 70_224);
        assert_eq!(MCYCLES_PER_FRAME, 17_556);
        assert_eq!(MCYCLES_PER_SCANLINE * SCANLINES_PER_FRAME, MCYCLES_PER_FRAME);
        assert_eq!(VISIBLE_SCANLINES + VBLANK_SCANLINES, SCANLINES_PER_FRAME);
    }

    #[test]
    fn refresh_rate_matches_documented_value() {
        // Pan Docs cites 59.7275 Hz; floating-point compare with a
        // small tolerance to absorb division precision.
        assert!((DMG_REFRESH_HZ - 59.7275).abs() < 0.001);
    }

    #[test]
    fn screen_dimensions_match_dmg_lcd() {
        assert_eq!(SCREEN_WIDTH, 160);
        assert_eq!(SCREEN_HEIGHT, 144);
    }
}
