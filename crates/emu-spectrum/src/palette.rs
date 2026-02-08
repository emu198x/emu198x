//! ZX Spectrum 16-colour palette.
//!
//! The Spectrum ULA outputs 15 unique colours (black appears twice) using a
//! 3-bit RGB scheme with a BRIGHT modifier. Non-bright colours use a lower
//! intensity (0xCD) while bright colours use full intensity (0xFF).

/// ARGB32 palette: 16 entries (8 normal + 8 bright).
///
/// Index layout: `bright_bit << 3 | ink_3bit`
///
/// Colours: black, blue, red, magenta, green, cyan, yellow, white.
pub const PALETTE: [u32; 16] = [
    // Normal (bright = 0)
    0xFF00_0000, // 0: Black
    0xFF00_00CD, // 1: Blue
    0xFFCD_0000, // 2: Red
    0xFFCD_00CD, // 3: Magenta
    0xFF00_CD00, // 4: Green
    0xFF00_CDCD, // 5: Cyan
    0xFFCD_CD00, // 6: Yellow
    0xFFCD_CDCD, // 7: White
    // Bright (bright = 1)
    0xFF00_0000, // 8: Black (same as normal)
    0xFF00_00FF, // 9: Bright Blue
    0xFFFF_0000, // 10: Bright Red
    0xFFFF_00FF, // 11: Bright Magenta
    0xFF00_FF00, // 12: Bright Green
    0xFF00_FFFF, // 13: Bright Cyan
    0xFFFF_FF00, // 14: Bright Yellow
    0xFFFF_FFFF, // 15: Bright White
];
