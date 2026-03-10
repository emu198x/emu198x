//! Commodore Lisa (AGA Denise) — wraps ECS Denise with AGA video extensions.
//!
//! Lisa adds 256-colour palette (24-bit per entry), HAM8, BPLCON4 bitplane
//! colour XOR, FMODE-based sprite widths, and wider sprite data writes.
//! All state lives in the inner OCS Denise; this crate provides the methods
//! that interpret that state in AGA mode.

use std::ops::{Deref, DerefMut};

pub use commodore_denise_ecs::DeniseEcs as InnerDeniseEcs;
pub use commodore_denise_ocs::DeniseOcs as InnerDeniseOcs;

/// AGA Lisa wrapper around the ECS Denise core.
pub struct DeniseAga {
    inner: InnerDeniseEcs,
}

impl DeniseAga {
    /// Create a new AGA Denise wrapper.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: InnerDeniseEcs::new(),
        }
    }

    /// Wrap an existing ECS Denise core.
    #[must_use]
    pub fn from_ecs(inner: InnerDeniseEcs) -> Self {
        Self { inner }
    }

    /// Borrow the wrapped ECS Denise core.
    #[must_use]
    pub const fn as_inner(&self) -> &InnerDeniseEcs {
        &self.inner
    }

    /// Mutably borrow the wrapped ECS Denise core.
    #[must_use]
    pub fn as_inner_mut(&mut self) -> &mut InnerDeniseEcs {
        &mut self.inner
    }

    /// Consume the wrapper and return the wrapped ECS Denise core.
    #[must_use]
    pub fn into_inner(self) -> InnerDeniseEcs {
        self.inner
    }

    /// AGA Lisa ID register value (low byte), as reported by DENISEID.
    #[must_use]
    pub const fn deniseid(&self) -> u16 {
        0x00F8
    }

    /// AGA palette write with BPLCON3 bank selection and LOCT support.
    ///
    /// `base_idx` is the 0-31 register offset from the COLOR register write.
    /// The full palette index is computed from BPLCON3 bits 15-13 (BANK).
    /// When LOCT (BPLCON3 bit 9) is clear, the high nibbles of R/G/B are
    /// written to bits 7-4 of each channel. When LOCT is set, the low
    /// nibbles are written to bits 3-0.
    pub fn set_palette_aga(&mut self, base_idx: usize, val: u16) {
        if base_idx >= 32 {
            return;
        }
        let bank = ((self.bplcon3 >> 13) & 7) as usize;
        let full_idx = bank * 32 + base_idx;
        let loct = (self.bplcon3 & 0x0200) != 0;

        let r4 = ((val >> 8) & 0xF) as u32;
        let g4 = ((val >> 4) & 0xF) as u32;
        let b4 = (val & 0xF) as u32;

        if loct {
            // Low nibbles: bits 3-0 of each channel.
            let existing = self.palette_24[full_idx];
            let r = (existing & 0x00F00000) | (r4 << 16);
            let g = (existing & 0x0000F000) | (g4 << 8);
            let b = (existing & 0x000000F0) | b4;
            self.palette_24[full_idx] = r | g | b;
        } else {
            // High nibbles: replicate to both nibble positions for OCS
            // backwards compatibility (e.g. $A → $AA). Matches real AGA
            // hardware and WinUAE's `cr = r + (r << 4)`.
            let r8 = (r4 << 4) | r4;
            let g8 = (g4 << 4) | g4;
            let b8 = (b4 << 4) | b4;
            self.palette_24[full_idx] = (r8 << 16) | (g8 << 8) | b8;
        }

        // Update OCS 12-bit palette for register readback compatibility.
        self.palette[base_idx] = val & 0x0FFF;
    }

    /// Resolve a colour index to 24-bit RGB (0x00RRGGBB) in AGA mode.
    ///
    /// Applies BPLCON4 bitplane colour XOR and palette lookup.
    /// HAM8 and EHB are handled by dedicated paths.
    ///
    /// BPLCON4 layout: bits 15-8 = BPLAM (bitplane colour XOR mask),
    /// bits 7-0 = ESPRM/OSPRM (sprite colour base — handled separately).
    pub fn resolve_color_rgb24(&mut self, color_idx: u8) -> u32 {
        let ham = (self.bplcon0 & 0x0800) != 0;
        let dual_playfield = (self.bplcon0 & 0x0400) != 0;
        let num_planes = self.num_bitplanes();
        let bplcon4_xor = ((self.bplcon4 >> 8) & 0xFF) as u8;

        if ham && !dual_playfield && num_planes >= 5 {
            if num_planes == 8 {
                // HAM8: 8-bit value, top 2 bits = control, bottom 6 = data.
                let control = (color_idx >> 6) & 0x03;
                let data6 = color_idx & 0x3F;
                // Expand 6-bit to 8-bit: replicate top 2 bits in low 2.
                let data8 = ((data6 as u32) << 2) | ((data6 as u32) >> 4);
                let rgb = match control {
                    0b00 => {
                        let idx = (data6 ^ bplcon4_xor) as usize & 0xFF;
                        self.palette_24[idx]
                    }
                    0b01 => {
                        // Modify blue
                        (self.ham_prev_rgb24 & 0x00FFFF00) | data8
                    }
                    0b10 => {
                        // Modify red
                        (self.ham_prev_rgb24 & 0x0000FFFF) | (data8 << 16)
                    }
                    0b11 => {
                        // Modify green
                        (self.ham_prev_rgb24 & 0x00FF00FF) | (data8 << 8)
                    }
                    _ => unreachable!(),
                };
                self.ham_prev_rgb24 = rgb;
                return rgb;
            }
            // HAM6 in AGA mode: use OCS HAM6 path, convert 12→24 bit.
            let rgb12 = self.resolve_color_rgb12(color_idx);
            return InnerDeniseOcs::rgb12_to_rgb24(rgb12);
        }

        if !ham && !dual_playfield && num_planes == 6 {
            // EHB: 6-bit index, bit 5 = half-brite flag.
            let effective = (color_idx ^ bplcon4_xor) as usize & 0xFF;
            if color_idx & 0x20 != 0 {
                let base = self.palette_24[effective & 0x1F];
                if self.inner.killehb_enabled() {
                    return base;
                }
                let r = ((base >> 16) & 0xFF) >> 1;
                let g = ((base >> 8) & 0xFF) >> 1;
                let b = (base & 0xFF) >> 1;
                return (r << 16) | (g << 8) | b;
            }
            return self.palette_24[effective];
        }

        // Normal mode: direct palette lookup with BPLCON4 XOR.
        let effective = (color_idx ^ bplcon4_xor) as usize & 0xFF;
        self.palette_24[effective]
    }

    /// Resolve a playfield colour index to 12-bit RGB through the ECS Denise
    /// compatibility layer that AGA builds on.
    pub fn resolve_color_rgb12(&mut self, color_idx: u8) -> u16 {
        self.inner.resolve_color_rgb12(color_idx)
    }

    /// Set sprite display width from FMODE register value.
    ///
    /// FMODE bits 3-2: 00 → 16 pixels, 01/10 → 32 pixels, 11 → 64 pixels.
    pub fn set_sprite_width_from_fmode(&mut self, fmode: u16) {
        self.spr_width = match (fmode >> 2) & 3 {
            0 => 16,
            1 | 2 => 32,
            3 => 64,
            _ => unreachable!(),
        };
    }

    /// Write 1-4 words (AGA wide fetch) into the sprite DATA holding latch.
    ///
    /// Words are packed MSB-first: word[0] occupies the highest bits. The
    /// number of words must match `spr_width / 16`.
    pub fn write_sprite_data_wide(&mut self, sprite: usize, words: &[u16]) {
        if sprite >= 8 || words.is_empty() {
            return;
        }
        let mut packed: u64 = 0;
        for &w in words {
            packed = (packed << 16) | u64::from(w);
        }
        self.spr_data[sprite] = packed;
        self.spr_armed[sprite] = true;
    }

    /// Write 1-4 words (AGA wide fetch) into the sprite DATB holding latch.
    pub fn write_sprite_datb_wide(&mut self, sprite: usize, words: &[u16]) {
        if sprite >= 8 || words.is_empty() {
            return;
        }
        let mut packed: u64 = 0;
        for &w in words {
            packed = (packed << 16) | u64::from(w);
        }
        self.spr_datb[sprite] = packed;
    }
}

impl Default for DeniseAga {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for DeniseAga {
    type Target = InnerDeniseEcs;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for DeniseAga {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl From<DeniseAga> for InnerDeniseEcs {
    fn from(denise: DeniseAga) -> Self {
        denise.into_inner()
    }
}

#[cfg(test)]
mod tests {
    use super::DeniseAga;

    fn make_aga_denise() -> DeniseAga {
        let mut denise = DeniseAga::new();
        denise.max_bitplanes = 8;
        denise
    }

    #[test]
    fn set_palette_aga_writes_high_nibbles_into_selected_bank() {
        let mut denise = make_aga_denise();
        denise.bplcon3 = 3 << 13; // bank 3, LOCT clear

        denise.set_palette_aga(5, 0x0ABC);

        // Nibbles are replicated: $A→$AA, $B→$BB, $C→$CC.
        assert_eq!(denise.palette_24[3 * 32 + 5], 0x00AABBCC);
        assert_eq!(denise.palette[5], 0x0ABC);
    }

    #[test]
    fn set_palette_aga_low_nibble_mode_merges_with_existing_high_nibbles() {
        let mut denise = make_aga_denise();
        denise.bplcon3 = 1 << 13; // bank 1
        denise.set_palette_aga(2, 0x0123);
        denise.bplcon3 = (1 << 13) | 0x0200; // same bank, LOCT set

        denise.set_palette_aga(2, 0x0456);

        assert_eq!(denise.palette_24[32 + 2], 0x00142536);
        assert_eq!(denise.palette[2], 0x0456);
    }

    #[test]
    fn resolve_color_rgb24_applies_bplcon4_xor_in_normal_mode() {
        let mut denise = make_aga_denise();
        denise.palette_24[0x34] = 0x00112233;
        // BPLCON4 high byte = bitplane XOR mask, low byte = sprite base.
        // XOR $30 on index $04 → effective index $34.
        denise.bplcon4 = 0x3000;

        let rgb = denise.resolve_color_rgb24(0x04);

        assert_eq!(rgb, 0x00112233);
    }

    #[test]
    fn resolve_color_rgb24_ham8_chains_palette_and_channel_modifications() {
        let mut denise = make_aga_denise();
        denise.bplcon0 = 0x0810; // HAM + 8 bitplanes
        denise.palette_24[5] = 0x00112233;

        let palette = denise.resolve_color_rgb24(0x05);
        assert_eq!(palette, 0x00112233);
        assert_eq!(denise.ham_prev_rgb24, 0x00112233);

        let red = denise.resolve_color_rgb24(0xAA); // control=10, data=0x2A -> 0xAA
        assert_eq!(red, 0x00AA2233);

        let green = denise.resolve_color_rgb24(0xEA); // control=11, data=0x2A -> 0xAA
        assert_eq!(green, 0x00AAAA33);

        let blue = denise.resolve_color_rgb24(0x6A); // control=01, data=0x2A -> 0xAA
        assert_eq!(blue, 0x00AAAAAA);
        assert_eq!(denise.ham_prev_rgb24, 0x00AAAAAA);
    }

    #[test]
    fn resolve_color_rgb24_honors_ecs_killehb_in_ehb_mode() {
        let mut denise = make_aga_denise();
        denise.bplcon0 = 0x6000; // 6 planes, EHB
        denise.palette_24[5] = 0x00112233;

        assert_eq!(denise.resolve_color_rgb24(0x25), 0x00081119);

        denise.bplcon3 = 0x0201; // KILLEHB + ENBPLCN3
        assert_eq!(denise.resolve_color_rgb24(0x25), 0x00112233);
    }

    #[test]
    fn deniseid_matches_aga_hrm_value() {
        let denise = make_aga_denise();
        assert_eq!(denise.deniseid(), 0x00F8);
    }

    #[test]
    fn set_sprite_width_from_fmode_decodes_aga_widths() {
        let mut denise = make_aga_denise();

        for (fmode, expected) in [(0x0000, 16), (0x0004, 32), (0x0008, 32), (0x000C, 64)] {
            denise.set_sprite_width_from_fmode(fmode);
            assert_eq!(denise.spr_width, expected, "FMODE={fmode:#06X}");
        }
    }

    #[test]
    fn wide_sprite_writes_pack_words_msb_first_and_arm_data_latch() {
        let mut denise = make_aga_denise();

        denise.write_sprite_data_wide(3, &[0x1122, 0x3344, 0x5566, 0x7788]);
        denise.write_sprite_datb_wide(3, &[0x99AA, 0xBBCC, 0xDDEE, 0xFF00]);

        assert_eq!(denise.spr_data[3], 0x1122_3344_5566_7788);
        assert_eq!(denise.spr_datb[3], 0x99AA_BBCC_DDEE_FF00);
        assert!(denise.spr_armed[3]);
    }
}
