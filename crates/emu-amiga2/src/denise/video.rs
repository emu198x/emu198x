//! Video output utilities.

/// Framebuffer width (320 active + border margins).
pub const FB_WIDTH: u32 = 360;
/// Framebuffer height (256 active + top/bottom border).
pub const FB_HEIGHT: u32 = 284;

/// Convert 12-bit RGB to ARGB32.
#[must_use]
pub fn rgb12_to_argb32(rgb12: u16) -> u32 {
    let r = u32::from((rgb12 >> 8) & 0xF);
    let g = u32::from((rgb12 >> 4) & 0xF);
    let b = u32::from(rgb12 & 0xF);
    0xFF00_0000 | ((r << 4 | r) << 16) | ((g << 4 | g) << 8) | (b << 4 | b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb12_black() {
        assert_eq!(rgb12_to_argb32(0x000), 0xFF00_0000);
    }

    #[test]
    fn rgb12_white() {
        assert_eq!(rgb12_to_argb32(0xFFF), 0xFFFF_FFFF);
    }

    #[test]
    fn rgb12_red() {
        assert_eq!(rgb12_to_argb32(0xF00), 0xFFFF_0000);
    }
}
