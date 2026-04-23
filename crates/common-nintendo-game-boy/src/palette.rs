//! DMG four-shade palette helpers.
//!
//! The DMG LCD displays four shades of grey-green per pixel,
//! addressed by 2-bit pixel values from the PPU. The CPU configures
//! three palette registers — `BGP` ($FF47), `OBP0` ($FF48), `OBP1`
//! ($FF49) — that map each 2-bit pixel index to one of four shade
//! slots (also 2-bit). Slot 0 is the lightest (sometimes
//! transparent for sprites), slot 3 the darkest.
//!
//! CGB's 15-bit-RGB palettes are not modelled here — they live with
//! the (future) CGB-specific machine code.

use serde::{Deserialize, Serialize};

/// The four DMG shade slots, in order from light (`Off`) to dark
/// (`Black`). Stored 2-bit when packed into a `BGP`/`OBP*` byte.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DmgShade {
    /// Lightest — typical "off" pixel for a green LCD.
    Off,
    Light,
    Dark,
    /// Darkest.
    Black,
}

impl DmgShade {
    /// Decode a 2-bit shade slot from a palette byte.
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        match bits & 0b11 {
            0 => Self::Off,
            1 => Self::Light,
            2 => Self::Dark,
            _ => Self::Black,
        }
    }

    /// 8-bit greyscale intensity for this shade (0xFF = white,
    /// 0x00 = black).
    #[must_use]
    pub const fn greyscale(self) -> u8 {
        match self {
            Self::Off => 0xFF,
            Self::Light => 0xAA,
            Self::Dark => 0x55,
            Self::Black => 0x00,
        }
    }
}

/// A DMG palette: four shade slots indexed by the 2-bit pixel value
/// the PPU produces. Constructed from a `BGP`/`OBP*` byte.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DmgPalette([DmgShade; 4]);

impl DmgPalette {
    /// Decodes a palette from a `BGP`/`OBP*` register byte. Bits
    /// 1-0 are slot 0, bits 3-2 slot 1, bits 5-4 slot 2, bits 7-6
    /// slot 3.
    #[must_use]
    pub const fn from_byte(byte: u8) -> Self {
        Self([
            DmgShade::from_bits(byte),
            DmgShade::from_bits(byte >> 2),
            DmgShade::from_bits(byte >> 4),
            DmgShade::from_bits(byte >> 6),
        ])
    }

    /// Looks up the shade for a 2-bit pixel value.
    #[must_use]
    pub const fn shade(&self, pixel: u8) -> DmgShade {
        self.0[(pixel & 0b11) as usize]
    }
}

/// Convenience: decode a palette byte and a pixel value to a 32-bit
/// RGBA colour. Useful in the runtime layer for pushing frames out
/// to a host framebuffer.
#[must_use]
pub fn dmg_pixel_rgba(palette_byte: u8, pixel: u8) -> u32 {
    let shade = DmgPalette::from_byte(palette_byte).shade(pixel);
    let g = shade.greyscale();
    u32::from_be_bytes([g, g, g, 0xFF])
}

/// Convenience constructor mirroring `dmg_pixel_rgba` but returning
/// the [`DmgPalette`] for callers that want the whole table.
#[must_use]
pub fn dmg_palette_from_byte(byte: u8) -> DmgPalette {
    DmgPalette::from_byte(byte)
}

/// Pre-computed RGBA colour table for the four DMG shades. Indexable
/// by [`DmgShade`] cast to `usize`. The runtime layer can swap this
/// for a green-LCD-tinted palette without touching the PPU.
pub const DMG_GREYSCALE_RGBA: [u32; 4] = [
    u32::from_be_bytes([0xFF, 0xFF, 0xFF, 0xFF]),
    u32::from_be_bytes([0xAA, 0xAA, 0xAA, 0xFF]),
    u32::from_be_bytes([0x55, 0x55, 0x55, 0xFF]),
    u32::from_be_bytes([0x00, 0x00, 0x00, 0xFF]),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_bits_decodes_each_shade() {
        assert_eq!(DmgShade::from_bits(0), DmgShade::Off);
        assert_eq!(DmgShade::from_bits(1), DmgShade::Light);
        assert_eq!(DmgShade::from_bits(2), DmgShade::Dark);
        assert_eq!(DmgShade::from_bits(3), DmgShade::Black);
        // Higher bits are ignored.
        assert_eq!(DmgShade::from_bits(0xF7), DmgShade::Black);
    }

    #[test]
    fn palette_from_byte_decodes_four_slots() {
        // 0b11_10_01_00 → slot0=Off, slot1=Light, slot2=Dark, slot3=Black
        let palette = DmgPalette::from_byte(0b1110_0100);
        assert_eq!(palette.shade(0), DmgShade::Off);
        assert_eq!(palette.shade(1), DmgShade::Light);
        assert_eq!(palette.shade(2), DmgShade::Dark);
        assert_eq!(palette.shade(3), DmgShade::Black);
    }

    #[test]
    fn pixel_to_rgba_matches_table() {
        assert_eq!(dmg_pixel_rgba(0b1110_0100, 0), DMG_GREYSCALE_RGBA[0]);
        assert_eq!(dmg_pixel_rgba(0b1110_0100, 3), DMG_GREYSCALE_RGBA[3]);
    }

    #[test]
    fn pixel_to_rgba_handles_remapping() {
        // BGP that inverts the palette: slot0=Black slot3=Off.
        let inverted = 0b0001_1011;
        assert_eq!(dmg_pixel_rgba(inverted, 0), DMG_GREYSCALE_RGBA[3]);
        assert_eq!(dmg_pixel_rgba(inverted, 3), DMG_GREYSCALE_RGBA[0]);
    }
}
