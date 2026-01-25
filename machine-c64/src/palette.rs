//! C64 color palettes for different VIC-II revisions.
//!
//! Each VIC-II revision produced slightly different colors due to
//! manufacturing variations. This module provides accurate palettes
//! for each revision based on measurements from real hardware.

use crate::config::VicRevision;

/// RGB color value.
#[derive(Clone, Copy, Debug)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Convert to RGBA bytes (for framebuffer).
    pub const fn to_rgba(self) -> [u8; 4] {
        [self.r, self.g, self.b, 255]
    }

    /// Convert to u32 (RGBA format).
    pub const fn to_u32(self) -> u32 {
        (self.r as u32) << 24 | (self.g as u32) << 16 | (self.b as u32) << 8 | 255
    }
}

/// A 16-color C64 palette.
pub type Palette = [Color; 16];

/// Get the color palette for a specific VIC revision.
pub const fn palette_for_revision(revision: VicRevision) -> &'static Palette {
    match revision {
        VicRevision::Vic6567R56A => &PALETTE_6567R56A,
        VicRevision::Vic6567R8 => &PALETTE_6567R8,
        VicRevision::Vic6569R1 => &PALETTE_6569R1,
        VicRevision::Vic6569R3 => &PALETTE_6569R3,
        VicRevision::Vic8562 => &PALETTE_8562,
        VicRevision::Vic8565 => &PALETTE_8565,
    }
}

/// Color names for reference.
pub const COLOR_NAMES: [&str; 16] = [
    "Black",
    "White",
    "Red",
    "Cyan",
    "Purple",
    "Green",
    "Blue",
    "Yellow",
    "Orange",
    "Brown",
    "Light Red",
    "Dark Grey",
    "Grey",
    "Light Green",
    "Light Blue",
    "Light Grey",
];

// Palettes based on VICE and community measurements.
// These try to capture the character of each VIC revision.

/// VIC-II 6567 R56A (early NTSC) - slightly washed out
pub const PALETTE_6567R56A: Palette = [
    Color::new(0x00, 0x00, 0x00), // Black
    Color::new(0xFF, 0xFF, 0xFF), // White
    Color::new(0xAF, 0x2A, 0x29), // Red
    Color::new(0x62, 0xD8, 0xCC), // Cyan
    Color::new(0xB0, 0x3F, 0xB6), // Purple
    Color::new(0x4A, 0xC6, 0x4A), // Green
    Color::new(0x37, 0x39, 0xC4), // Blue
    Color::new(0xE4, 0xED, 0x4E), // Yellow
    Color::new(0xB6, 0x59, 0x1C), // Orange
    Color::new(0x68, 0x38, 0x08), // Brown
    Color::new(0xEA, 0x74, 0x6C), // Light Red
    Color::new(0x4D, 0x4D, 0x4D), // Dark Grey
    Color::new(0x84, 0x84, 0x84), // Grey
    Color::new(0xA6, 0xFA, 0x9E), // Light Green
    Color::new(0x70, 0x7C, 0xE6), // Light Blue
    Color::new(0xB6, 0xB6, 0xB6), // Light Grey
];

/// VIC-II 6567 R8 (common NTSC) - vivid, saturated colors
pub const PALETTE_6567R8: Palette = [
    Color::new(0x00, 0x00, 0x00), // Black
    Color::new(0xFF, 0xFF, 0xFF), // White
    Color::new(0x9F, 0x4E, 0x44), // Red
    Color::new(0x6A, 0xBF, 0xC6), // Cyan
    Color::new(0xA0, 0x57, 0xA3), // Purple
    Color::new(0x5C, 0xAB, 0x5E), // Green
    Color::new(0x50, 0x45, 0x9B), // Blue
    Color::new(0xC9, 0xD4, 0x87), // Yellow
    Color::new(0xA1, 0x68, 0x3C), // Orange
    Color::new(0x6D, 0x54, 0x12), // Brown
    Color::new(0xCB, 0x7E, 0x75), // Light Red
    Color::new(0x62, 0x62, 0x62), // Dark Grey
    Color::new(0x89, 0x89, 0x89), // Grey
    Color::new(0x9A, 0xE2, 0x9B), // Light Green
    Color::new(0x88, 0x7E, 0xCB), // Light Blue
    Color::new(0xAD, 0xAD, 0xAD), // Light Grey
];

/// VIC-II 6569 R1 (early PAL) - warmer tones
pub const PALETTE_6569R1: Palette = [
    Color::new(0x00, 0x00, 0x00), // Black
    Color::new(0xFF, 0xFF, 0xFF), // White
    Color::new(0xB5, 0x2B, 0x20), // Red
    Color::new(0x6E, 0xE2, 0xD3), // Cyan
    Color::new(0xBA, 0x3C, 0xB5), // Purple
    Color::new(0x43, 0xD1, 0x50), // Green
    Color::new(0x3C, 0x31, 0xBA), // Blue
    Color::new(0xED, 0xF1, 0x71), // Yellow
    Color::new(0xB8, 0x5F, 0x1B), // Orange
    Color::new(0x6B, 0x41, 0x00), // Brown
    Color::new(0xEB, 0x6F, 0x66), // Light Red
    Color::new(0x50, 0x50, 0x50), // Dark Grey
    Color::new(0x80, 0x80, 0x80), // Grey
    Color::new(0xA4, 0xF8, 0xA2), // Light Green
    Color::new(0x7C, 0x70, 0xEB), // Light Blue
    Color::new(0xB0, 0xB0, 0xB0), // Light Grey
];

/// VIC-II 6569 R3 (common PAL) - the "classic" C64 look
pub const PALETTE_6569R3: Palette = [
    Color::new(0x00, 0x00, 0x00), // Black
    Color::new(0xFF, 0xFF, 0xFF), // White
    Color::new(0x68, 0x37, 0x2B), // Red
    Color::new(0x70, 0xA4, 0xB2), // Cyan
    Color::new(0x6F, 0x3D, 0x86), // Purple
    Color::new(0x58, 0x8D, 0x43), // Green
    Color::new(0x35, 0x28, 0x79), // Blue
    Color::new(0xB8, 0xC7, 0x6F), // Yellow
    Color::new(0x6F, 0x4F, 0x25), // Orange
    Color::new(0x43, 0x39, 0x00), // Brown
    Color::new(0x9A, 0x67, 0x59), // Light Red
    Color::new(0x44, 0x44, 0x44), // Dark Grey
    Color::new(0x6C, 0x6C, 0x6C), // Grey
    Color::new(0x9A, 0xD2, 0x84), // Light Green
    Color::new(0x6C, 0x5E, 0xB5), // Light Blue
    Color::new(0x95, 0x95, 0x95), // Light Grey
];

/// VIC-II 8562 (late NTSC / C64C) - slightly cooler tones
pub const PALETTE_8562: Palette = [
    Color::new(0x00, 0x00, 0x00), // Black
    Color::new(0xFF, 0xFF, 0xFF), // White
    Color::new(0x88, 0x39, 0x32), // Red
    Color::new(0x67, 0xB6, 0xBD), // Cyan
    Color::new(0x8B, 0x3F, 0x96), // Purple
    Color::new(0x55, 0xA0, 0x49), // Green
    Color::new(0x40, 0x31, 0x8D), // Blue
    Color::new(0xBF, 0xCE, 0x72), // Yellow
    Color::new(0x8B, 0x54, 0x29), // Orange
    Color::new(0x57, 0x42, 0x00), // Brown
    Color::new(0xB8, 0x69, 0x62), // Light Red
    Color::new(0x50, 0x50, 0x50), // Dark Grey
    Color::new(0x78, 0x78, 0x78), // Grey
    Color::new(0x94, 0xE0, 0x89), // Light Green
    Color::new(0x78, 0x69, 0xC4), // Light Blue
    Color::new(0x9F, 0x9F, 0x9F), // Light Grey
];

/// VIC-II 8565 (late PAL / C64C) - cleaner, more neutral
pub const PALETTE_8565: Palette = [
    Color::new(0x00, 0x00, 0x00), // Black
    Color::new(0xFF, 0xFF, 0xFF), // White
    Color::new(0x88, 0x39, 0x32), // Red
    Color::new(0x67, 0xB6, 0xBD), // Cyan
    Color::new(0x8B, 0x3F, 0x96), // Purple
    Color::new(0x55, 0xA0, 0x49), // Green
    Color::new(0x40, 0x31, 0x8D), // Blue
    Color::new(0xBF, 0xCE, 0x72), // Yellow
    Color::new(0x8B, 0x54, 0x29), // Orange
    Color::new(0x57, 0x42, 0x00), // Brown
    Color::new(0xB8, 0x69, 0x62), // Light Red
    Color::new(0x50, 0x50, 0x50), // Dark Grey
    Color::new(0x78, 0x78, 0x78), // Grey
    Color::new(0x94, 0xE0, 0x89), // Light Green
    Color::new(0x78, 0x69, 0xC4), // Light Blue
    Color::new(0x9F, 0x9F, 0x9F), // Light Grey
];

/// VICE default palette (for compatibility)
pub const PALETTE_VICE: Palette = [
    Color::new(0x00, 0x00, 0x00), // Black
    Color::new(0xFD, 0xFE, 0xFC), // White
    Color::new(0xBE, 0x1A, 0x24), // Red
    Color::new(0x30, 0xE6, 0xC6), // Cyan
    Color::new(0xB4, 0x1A, 0xE2), // Purple
    Color::new(0x1F, 0xD2, 0x1E), // Green
    Color::new(0x21, 0x1B, 0xAE), // Blue
    Color::new(0xDF, 0xF6, 0x0A), // Yellow
    Color::new(0xB8, 0x41, 0x04), // Orange
    Color::new(0x6A, 0x33, 0x04), // Brown
    Color::new(0xFE, 0x4A, 0x57), // Light Red
    Color::new(0x42, 0x45, 0x40), // Dark Grey
    Color::new(0x70, 0x74, 0x6F), // Grey
    Color::new(0x59, 0xFE, 0x59), // Light Green
    Color::new(0x5F, 0x53, 0xFE), // Light Blue
    Color::new(0xA4, 0xA7, 0xA2), // Light Grey
];

/// Pepto's palette (popular community palette)
pub const PALETTE_PEPTO: Palette = [
    Color::new(0x00, 0x00, 0x00), // Black
    Color::new(0xFF, 0xFF, 0xFF), // White
    Color::new(0x68, 0x37, 0x2B), // Red
    Color::new(0x70, 0xA4, 0xB2), // Cyan
    Color::new(0x6F, 0x3D, 0x86), // Purple
    Color::new(0x58, 0x8D, 0x43), // Green
    Color::new(0x35, 0x28, 0x79), // Blue
    Color::new(0xB8, 0xC7, 0x6F), // Yellow
    Color::new(0x6F, 0x4F, 0x25), // Orange
    Color::new(0x43, 0x39, 0x00), // Brown
    Color::new(0x9A, 0x67, 0x59), // Light Red
    Color::new(0x44, 0x44, 0x44), // Dark Grey
    Color::new(0x6C, 0x6C, 0x6C), // Grey
    Color::new(0x9A, 0xD2, 0x84), // Light Green
    Color::new(0x6C, 0x5E, 0xB5), // Light Blue
    Color::new(0x95, 0x95, 0x95), // Light Grey
];
