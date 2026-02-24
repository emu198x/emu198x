//! Commodore Denise OCS — video output, bitplane shifter, and sprite engine.
//!
//! Denise receives bitplane data from Agnus DMA and shifts it out pixel by
//! pixel, combining with the colour palette to produce the final framebuffer.

pub const FB_WIDTH: u32 = 320;
pub const FB_HEIGHT: u32 = 256;

pub struct DeniseOcs {
    pub palette: [u16; 32],
    pub framebuffer: Vec<u32>,
    pub bpl_data: [u16; 6],  // Holding latches: written by DMA
    pub bpl_shift: [u16; 6], // Shift registers: loaded from latches on BPL1DAT write
    pub shift_count: u8,     // Pixels remaining in shift register (0 -> output COLOR00)
    pub bplcon0: u16,
    pub bplcon1: u16,
    pub bplcon2: u16,
    pub clxcon: u16,
    pub clxdat: u16,
    pub spr_pos: [u16; 8],
    pub spr_ctl: [u16; 8],
    pub spr_data: [u16; 8],
    pub spr_datb: [u16; 8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SpritePixel {
    palette_idx: usize,
    sprite_group: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlayfieldId {
    Pf1,
    Pf2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PlayfieldPixel {
    visible_color_idx: usize,
    front_playfield: Option<PlayfieldId>,
}

impl DeniseOcs {
    pub fn new() -> Self {
        Self {
            palette: [0; 32],
            framebuffer: vec![0xFF000000; (FB_WIDTH * FB_HEIGHT) as usize],
            bpl_data: [0; 6],
            bpl_shift: [0; 6],
            shift_count: 0,
            bplcon0: 0,
            bplcon1: 0,
            bplcon2: 0,
            clxcon: 0,
            clxdat: 0,
            spr_pos: [0; 8],
            spr_ctl: [0; 8],
            spr_data: [0; 8],
            spr_datb: [0; 8],
        }
    }

    pub fn set_palette(&mut self, idx: usize, val: u16) {
        if idx < 32 {
            self.palette[idx] = val & 0x0FFF;
        }
    }

    pub fn load_bitplane(&mut self, idx: usize, val: u16) {
        if idx < 6 {
            self.bpl_data[idx] = val;
        }
    }

    pub fn read_clxdat(&mut self) -> u16 {
        let value = self.clxdat;
        self.clxdat = 0;
        value
    }

    fn sprite_hstart(pos: u16, ctl: u16) -> u16 {
        ((pos & 0x00FF) << 1) | (ctl & 0x0001)
    }

    fn sprite_vstart(pos: u16, ctl: u16) -> u16 {
        (((ctl >> 2) & 0x0001) << 8) | ((pos >> 8) & 0x00FF)
    }

    fn sprite_vstop(_pos: u16, ctl: u16) -> u16 {
        (((ctl >> 1) & 0x0001) << 8) | ((ctl >> 8) & 0x00FF)
    }

    fn sprite_code_at(&self, sprite: usize, beam_x: u32, beam_y: u32) -> Option<u8> {
        if sprite >= 8 {
            return None;
        }
        let pos = self.spr_pos[sprite];
        let ctl = self.spr_ctl[sprite];
        let hstart = u32::from(Self::sprite_hstart(pos, ctl));
        let vstart = u32::from(Self::sprite_vstart(pos, ctl));
        let vstop = u32::from(Self::sprite_vstop(pos, ctl));
        if vstop <= vstart {
            return None;
        }
        if beam_y < vstart || beam_y >= vstop {
            return None;
        }
        if beam_x < hstart || beam_x >= hstart + 16 {
            return None;
        }

        let bit = 15 - (beam_x - hstart) as u16;
        let lo = (self.spr_data[sprite] >> bit) & 1;
        let hi = (self.spr_datb[sprite] >> bit) & 1;
        Some((lo | (hi << 1)) as u8)
    }

    fn sprite_pixel(&self, beam_x: u32, beam_y: u32) -> Option<SpritePixel> {
        // Minimal OCS sprite overlay:
        // - attached pairs (1->0, 3->2, 5->4, 7->6) produce 4-bit colors from
        //   the full sprite palette range (COLOR17..COLOR31, 0 => transparent)
        // - collision detection is handled separately from display priority
        // - lower sprite number wins on overlap (pair priority by lower sprite)
        for sprite in 0..8usize {
            if sprite & 1 == 1 {
                // Odd sprite is handled by the preceding even sprite when its
                // ATTACH bit is set.
                if (self.spr_ctl[sprite] & 0x0080) != 0 {
                    continue;
                }
            }

            let pair = sprite & !1;
            let odd = pair + 1;
            let odd_attached = odd < 8 && (self.spr_ctl[odd] & 0x0080) != 0;
            if sprite == pair && odd_attached {
                let even_code = self.sprite_code_at(pair, beam_x, beam_y).unwrap_or(0);
                let odd_code = self.sprite_code_at(odd, beam_x, beam_y).unwrap_or(0);
                let code = ((odd_code as usize) << 2) | (even_code as usize);
                if code == 0 {
                    continue;
                }
                return Some(SpritePixel {
                    palette_idx: 16 + code,
                    sprite_group: pair / 2,
                });
            }

            let Some(code) = self.sprite_code_at(sprite, beam_x, beam_y) else {
                continue;
            };
            if code == 0 {
                continue;
            }
            let base = 16 + (sprite / 2) * 4;
            return Some(SpritePixel {
                palette_idx: base + usize::from(code),
                sprite_group: sprite / 2,
            });
        }
        None
    }

    fn collision_group_mask(&self, beam_x: u32, beam_y: u32) -> u8 {
        let mut mask = 0u8;
        for sprite in 0..8usize {
            let Some(code) = self.sprite_code_at(sprite, beam_x, beam_y) else {
                continue;
            };
            if code == 0 {
                continue;
            }
            let group = sprite / 2;
            if (sprite & 1) == 0 || self.clxcon_odd_sprite_enabled(sprite) {
                mask |= 1u8 << group;
            }
        }
        mask
    }

    fn clxcon_odd_sprite_enabled(&self, sprite: usize) -> bool {
        match sprite {
            1 => (self.clxcon & 0x1000) != 0, // ENSP1
            3 => (self.clxcon & 0x2000) != 0, // ENSP3
            5 => (self.clxcon & 0x4000) != 0, // ENSP5
            7 => (self.clxcon & 0x8000) != 0, // ENSP7
            _ => true,
        }
    }

    fn latch_collisions(
        &mut self,
        odd_bitplanes_active: bool,
        even_bitplanes_active: bool,
        sprite_groups: u8,
    ) {
        let mut bits = 0u16;
        if odd_bitplanes_active && even_bitplanes_active {
            bits |= 1 << 0;
        }

        for group in 0..4u8 {
            if (sprite_groups & (1u8 << group)) == 0 {
                continue;
            }
            if odd_bitplanes_active {
                bits |= 1u16 << (1 + group);
            }
            if even_bitplanes_active {
                bits |= 1u16 << (5 + group);
            }
        }

        // Sprite pair-group collisions: SP01/SP23/SP45/SP67
        if (sprite_groups & 0b0011) == 0b0011 {
            bits |= 1 << 9;
        }
        if (sprite_groups & 0b0101) == 0b0101 {
            bits |= 1 << 10;
        }
        if (sprite_groups & 0b1001) == 0b1001 {
            bits |= 1 << 11;
        }
        if (sprite_groups & 0b0110) == 0b0110 {
            bits |= 1 << 12;
        }
        if (sprite_groups & 0b1010) == 0b1010 {
            bits |= 1 << 13;
        }
        if (sprite_groups & 0b1100) == 0b1100 {
            bits |= 1 << 14;
        }

        self.clxdat |= bits;
    }

    fn sprite_has_priority_over_playfield(
        &self,
        sprite_group: usize,
        playfield: PlayfieldId,
    ) -> bool {
        // PFxP2..PFxP0 select playfield placement among the four sprite
        // priority groups. Values >4 are invalid; clamp.
        let pf_pos = match playfield {
            PlayfieldId::Pf1 => usize::from(self.bplcon2 & 0x0007),
            PlayfieldId::Pf2 => usize::from((self.bplcon2 >> 3) & 0x0007),
        }
        .min(4);
        sprite_group < pf_pos
    }

    fn compose_playfield_pixel(
        &self,
        raw_color_idx: usize,
        pf1_code: u8,
        pf2_code: u8,
    ) -> PlayfieldPixel {
        let dual_playfield = (self.bplcon0 & 0x0400) != 0; // DBLPF
        if !dual_playfield {
            return PlayfieldPixel {
                visible_color_idx: raw_color_idx,
                front_playfield: if raw_color_idx != 0 {
                    Some(PlayfieldId::Pf1)
                } else {
                    None
                },
            };
        }

        let pf1_nonzero = pf1_code != 0;
        let pf2_nonzero = pf2_code != 0;
        match (pf1_nonzero, pf2_nonzero) {
            (false, false) => PlayfieldPixel {
                visible_color_idx: 0,
                front_playfield: None,
            },
            (true, false) => PlayfieldPixel {
                visible_color_idx: usize::from(pf1_code),
                front_playfield: Some(PlayfieldId::Pf1),
            },
            (false, true) => PlayfieldPixel {
                visible_color_idx: 8 + usize::from(pf2_code),
                front_playfield: Some(PlayfieldId::Pf2),
            },
            (true, true) => {
                let pf2_front = (self.bplcon2 & 0x0040) != 0; // PF2PRI
                if pf2_front {
                    PlayfieldPixel {
                        visible_color_idx: 8 + usize::from(pf2_code),
                        front_playfield: Some(PlayfieldId::Pf2),
                    }
                } else {
                    PlayfieldPixel {
                        visible_color_idx: usize::from(pf1_code),
                        front_playfield: Some(PlayfieldId::Pf1),
                    }
                }
            }
        }
    }

    /// Copy all bitplane holding latches into the shift registers.
    /// On real hardware this happens when BPL1DAT (plane 0) is written,
    /// which is always the last plane fetched in each 8-CCK DMA group.
    pub fn trigger_shift_load(&mut self) {
        for i in 0..6 {
            self.bpl_shift[i] = self.bpl_data[i];
        }
        self.shift_count = 16;
    }

    fn rgb12_to_argb32(rgb12: u16) -> u32 {
        let r = ((rgb12 >> 8) & 0xF) as u8;
        let g = ((rgb12 >> 4) & 0xF) as u8;
        let b = (rgb12 & 0xF) as u8;
        let r8 = (r << 4) | r;
        let g8 = (g << 4) | g;
        let b8 = (b << 4) | b;
        0xFF000000 | (u32::from(r8) << 16) | (u32::from(g8) << 8) | u32::from(b8)
    }

    pub fn output_pixel(&mut self, x: u32, y: u32) {
        self.output_pixel_with_beam(x, y, x, y);
    }

    pub fn output_pixel_with_beam(&mut self, x: u32, y: u32, beam_x: u32, beam_y: u32) {
        if x < FB_WIDTH && y < FB_HEIGHT {
            let mut raw_color_idx = 0usize;
            let mut pf1_code = 0u8;
            let mut pf2_code = 0u8;
            if self.shift_count > 0 {
                // Compute color index from shifter bits (MSB first)
                for plane in 0..6 {
                    let bit_set = (self.bpl_shift[plane] & 0x8000) != 0;
                    if bit_set {
                        raw_color_idx |= 1usize << plane;
                        if plane & 1 == 0 {
                            pf1_code |= 1u8 << (plane / 2);
                        } else {
                            pf2_code |= 1u8 << (plane / 2);
                        }
                    }
                    self.bpl_shift[plane] <<= 1;
                }
                self.shift_count -= 1;
            }

            let playfield = self.compose_playfield_pixel(raw_color_idx, pf1_code, pf2_code);
            let sprite_group_mask = self.collision_group_mask(beam_x, beam_y);
            self.latch_collisions(pf1_code != 0, pf2_code != 0, sprite_group_mask);
            let mut color_idx = playfield.visible_color_idx;
            if let Some(sprite_pixel) = self.sprite_pixel(beam_x, beam_y) {
                if let Some(front_pf) = playfield.front_playfield {
                    if self.sprite_has_priority_over_playfield(sprite_pixel.sprite_group, front_pf)
                    {
                        color_idx = sprite_pixel.palette_idx;
                    }
                } else {
                    // Background/COLOR00 only; sprite is visible.
                    color_idx = sprite_pixel.palette_idx;
                }
            }

            let argb32 = Self::rgb12_to_argb32(self.palette[color_idx]);
            self.framebuffer[(y * FB_WIDTH + x) as usize] = argb32;
        }
    }
}

impl Default for DeniseOcs {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_sprite_pos_ctl(x: u16, vstart: u16, vstop: u16) -> (u16, u16) {
        let pos = ((vstart & 0x00FF) << 8) | ((x >> 1) & 0x00FF);
        let ctl = ((vstop & 0x00FF) << 8)
            | (((vstart >> 8) & 1) << 2)
            | (((vstop >> 8) & 1) << 1)
            | (x & 1);
        (pos, ctl)
    }

    #[test]
    fn sprite_pixel_overrides_bitplane_pixel() {
        let mut denise = DeniseOcs::new();
        denise.set_palette(0, 0x000);
        denise.set_palette(1, 0x00F);
        denise.set_palette(17, 0xF00); // sprite 0/1 pair, color 1
        denise.bplcon2 = 0x0001; // PF1P=1 => sprite group 0 in front of PF1

        denise.bpl_shift[0] = 0x8000; // playfield color index 1
        denise.shift_count = 1;

        let (pos, ctl) = encode_sprite_pos_ctl(20, 10, 11);
        denise.spr_pos[0] = pos;
        denise.spr_ctl[0] = ctl;
        denise.spr_data[0] = 0x8000; // leftmost pixel = color code 1
        denise.spr_datb[0] = 0x0000;

        denise.output_pixel(20, 10);

        assert_eq!(
            denise.framebuffer[(10 * FB_WIDTH + 20) as usize],
            DeniseOcs::rgb12_to_argb32(0xF00)
        );
    }

    #[test]
    fn transparent_sprite_pixel_leaves_playfield_visible() {
        let mut denise = DeniseOcs::new();
        denise.set_palette(0, 0x000);
        denise.set_palette(1, 0x0F0);
        denise.set_palette(17, 0xF00);

        denise.bpl_shift[0] = 0x8000; // playfield color index 1
        denise.shift_count = 1;

        let (pos, ctl) = encode_sprite_pos_ctl(24, 12, 13);
        denise.spr_pos[0] = pos;
        denise.spr_ctl[0] = ctl;
        denise.spr_data[0] = 0x0000;
        denise.spr_datb[0] = 0x0000; // transparent

        denise.output_pixel(24, 12);

        assert_eq!(
            denise.framebuffer[(12 * FB_WIDTH + 24) as usize],
            DeniseOcs::rgb12_to_argb32(0x0F0)
        );
    }

    #[test]
    fn lower_numbered_sprite_has_priority_on_overlap() {
        let mut denise = DeniseOcs::new();
        denise.set_palette(0, 0x000);
        denise.set_palette(17, 0xF00); // sprite 0 pair color 1
        denise.set_palette(21, 0x0FF); // sprite 2 pair color 1

        let (pos0, ctl0) = encode_sprite_pos_ctl(30, 8, 9);
        denise.spr_pos[0] = pos0;
        denise.spr_ctl[0] = ctl0;
        denise.spr_data[0] = 0x8000;
        denise.spr_datb[0] = 0x0000;

        let (pos2, ctl2) = encode_sprite_pos_ctl(30, 8, 9);
        denise.spr_pos[2] = pos2;
        denise.spr_ctl[2] = ctl2;
        denise.spr_data[2] = 0x8000;
        denise.spr_datb[2] = 0x0000;

        denise.output_pixel(30, 8);

        assert_eq!(
            denise.framebuffer[(8 * FB_WIDTH + 30) as usize],
            DeniseOcs::rgb12_to_argb32(0xF00),
            "sprite 0 should appear in front of sprite 2"
        );
    }

    #[test]
    fn attached_sprite_pair_uses_full_sprite_palette_range() {
        let mut denise = DeniseOcs::new();
        denise.set_palette(0, 0x000);
        denise.set_palette(25, 0x0F0); // attached color value 1001 => COLOR25

        let (pos, ctl) = encode_sprite_pos_ctl(32, 14, 15);
        denise.spr_pos[0] = pos;
        denise.spr_ctl[0] = ctl;
        denise.spr_data[0] = 0x8000; // even sprite code = 01
        denise.spr_datb[0] = 0x0000;

        denise.spr_pos[1] = pos;
        denise.spr_ctl[1] = ctl | 0x0080; // ATTACH on odd sprite
        denise.spr_data[1] = 0x0000;
        denise.spr_datb[1] = 0x8000; // odd sprite code = 10 (high two bits)

        denise.output_pixel(32, 14);

        assert_eq!(
            denise.framebuffer[(14 * FB_WIDTH + 32) as usize],
            DeniseOcs::rgb12_to_argb32(0x0F0)
        );
    }

    #[test]
    fn bplcon2_pf1_priority_can_hide_sprite_group_0() {
        let mut denise = DeniseOcs::new();
        denise.set_palette(0, 0x000);
        denise.set_palette(1, 0x00F); // playfield color
        denise.set_palette(17, 0xF00); // sprite 0 color
        denise.bplcon2 = 0x0000; // PF1P = 0 => PF1 in front of all sprite groups

        denise.bpl_shift[0] = 0x8000; // playfield color index 1
        denise.shift_count = 1;

        let (pos, ctl) = encode_sprite_pos_ctl(18, 6, 7);
        denise.spr_pos[0] = pos;
        denise.spr_ctl[0] = ctl;
        denise.spr_data[0] = 0x8000;
        denise.spr_datb[0] = 0x0000;

        denise.output_pixel(18, 6);

        assert_eq!(
            denise.framebuffer[(6 * FB_WIDTH + 18) as usize],
            DeniseOcs::rgb12_to_argb32(0x00F),
            "PF1 priority should place sprite 0 behind a nonzero playfield pixel"
        );
    }

    #[test]
    fn bplcon2_pf1_priority_can_place_sprite_group_0_in_front() {
        let mut denise = DeniseOcs::new();
        denise.set_palette(0, 0x000);
        denise.set_palette(1, 0x00F); // playfield color
        denise.set_palette(17, 0xF00); // sprite 0 color
        denise.bplcon2 = 0x0001; // PF1P = 1 => SP01 in front of PF1

        denise.bpl_shift[0] = 0x8000; // playfield color index 1
        denise.shift_count = 1;

        let (pos, ctl) = encode_sprite_pos_ctl(19, 7, 8);
        denise.spr_pos[0] = pos;
        denise.spr_ctl[0] = ctl;
        denise.spr_data[0] = 0x8000;
        denise.spr_datb[0] = 0x0000;

        denise.output_pixel(19, 7);

        assert_eq!(
            denise.framebuffer[(7 * FB_WIDTH + 19) as usize],
            DeniseOcs::rgb12_to_argb32(0xF00),
            "PF1 priority should allow sprite 0 in front when PF1P=1"
        );
    }

    #[test]
    fn dual_playfield_pf2pri_and_pf2p_can_hide_or_show_sprite() {
        let mut denise = DeniseOcs::new();
        denise.bplcon0 = 0x0400; // DBLPF
        denise.set_palette(1, 0x00F); // PF1 color
        denise.set_palette(9, 0x0F0); // PF2 color
        denise.set_palette(17, 0xF00); // sprite 0 color

        let (pos, ctl) = encode_sprite_pos_ctl(22, 9, 10);
        denise.spr_pos[0] = pos;
        denise.spr_ctl[0] = ctl;
        denise.spr_data[0] = 0x8000;
        denise.spr_datb[0] = 0x0000;

        // Both playfields active on this pixel: PF1 code=1 (plane 1), PF2 code=1 (plane 2).
        // PF2PRI=1 puts PF2 in front of PF1.
        denise.bpl_shift[0] = 0x8000;
        denise.bpl_shift[1] = 0x8000;
        denise.shift_count = 1;
        denise.bplcon2 = 0x0044; // PF2PRI=1, PF1P=4 (sprite beats PF1), PF2P=0 (PF2 beats sprite)
        denise.output_pixel(22, 9);
        assert_eq!(
            denise.framebuffer[(9 * FB_WIDTH + 22) as usize],
            DeniseOcs::rgb12_to_argb32(0x0F0),
            "front PF2 should hide sprite when PF2P places PF2 ahead of SP01"
        );

        denise.bpl_shift[0] = 0x8000;
        denise.bpl_shift[1] = 0x8000;
        denise.shift_count = 1;
        denise.bplcon2 = 0x004C; // PF2PRI=1, PF2P=1 => SP01 in front of PF2
        denise.output_pixel(22, 9);
        assert_eq!(
            denise.framebuffer[(9 * FB_WIDTH + 22) as usize],
            DeniseOcs::rgb12_to_argb32(0xF00),
            "sprite should appear when PF2P places SP01 ahead of front PF2"
        );
    }
}
