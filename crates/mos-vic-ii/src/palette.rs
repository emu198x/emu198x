//! C64 colour palette.
//!
//! 16 colours as ARGB32, using the VICE PAL palette values.

/// C64 palette: 16 colours indexed 0-15 in ARGB32 format.
pub const PALETTE: [u32; 16] = [
    0xFF00_0000, // 0: Black
    0xFFFF_FFFF, // 1: White
    0xFF88_3932, // 2: Red
    0xFF67_B6BD, // 3: Cyan
    0xFF8B_3F96, // 4: Purple
    0xFF55_A049, // 5: Green
    0xFF40_318D, // 6: Blue
    0xFFBF_CE72, // 7: Yellow
    0xFF8B_5429, // 8: Orange
    0xFF57_4200, // 9: Brown
    0xFFB8_6962, // 10: Light Red
    0xFF50_5050, // 11: Dark Grey
    0xFF78_7878, // 12: Medium Grey
    0xFF94_E089, // 13: Light Green
    0xFF78_68C0, // 14: Light Blue
    0xFF9F_9F9F, // 15: Light Grey
];
