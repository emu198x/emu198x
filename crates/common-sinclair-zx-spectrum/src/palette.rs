//! Spectrum-family palette helpers.
//!
//! Source references:
//! - `docs/platforms/sinclair-zx-spectrum/language/ZX-SPECTRUM-MEMORY-AND-GRAPHICS-REFERENCE.md`
//! - Adapted from `/Users/stevehill/Projects/Emu198x-Older/crates/common-sinclair-zx-spectrum/src/palette.rs`

/// Standard ZX Spectrum 16-colour palette as RGBA (0xRRGGBBAA).
///
/// Indices 0-7: normal brightness. Indices 8-15: bright.
/// Index 0 and 8 are both black (bright black = normal black).
pub const SPECTRUM_PALETTE: [u32; 16] = [
    // Normal brightness
    0x000000FF, // 0: black
    0x0000CDFF, // 1: blue
    0xCD0000FF, // 2: red
    0xCD00CDFF, // 3: magenta
    0x00CD00FF, // 4: green
    0x00CDCDFF, // 5: cyan
    0xCDCD00FF, // 6: yellow
    0xCDCDCDFF, // 7: white
    // Bright
    0x000000FF, // 8: bright black (same as normal)
    0x0000FFFF, // 9: bright blue
    0xFF0000FF, // 10: bright red
    0xFF00FFFF, // 11: bright magenta
    0x00FF00FF, // 12: bright green
    0x00FFFFFF, // 13: bright cyan
    0xFFFF00FF, // 14: bright yellow
    0xFFFFFFFF, // 15: bright white
];

/// Convert a Spectrum attribute byte to ink and paper palette indices.
///
/// Attribute format: FBPPPIII
///   F = flash, B = bright, PPP = paper colour (0-7), III = ink colour (0-7)
///
/// Returns (ink_index, paper_index) into the 16-colour palette.
#[inline]
pub fn attr_to_indices(attr: u8) -> (u8, u8) {
    let bright = if attr & 0x40 != 0 { 8 } else { 0 };
    let ink = (attr & 0x07) | bright;
    let paper = ((attr >> 3) & 0x07) | bright;
    (ink, paper)
}

/// Check if an attribute has the FLASH bit set.
#[inline]
pub fn attr_flash(attr: u8) -> bool {
    attr & 0x80 != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attr_basic() {
        // Normal white ink on blue paper, no flash
        let (ink, paper) = attr_to_indices(0x38 | 0x07); // paper=7 (white), ink=7 (white)
        assert_eq!(ink, 7);
        assert_eq!(paper, 7);

        // Bright red ink on black paper
        let (ink, paper) = attr_to_indices(0x42); // bright=1, paper=0, ink=2
        assert_eq!(ink, 10); // bright red
        assert_eq!(paper, 8); // bright black
    }
}
