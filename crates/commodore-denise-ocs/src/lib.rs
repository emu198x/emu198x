//! Commodore Denise OCS — video output, bitplane shifter, and sprite engine.
//!
//! Denise receives bitplane data from Agnus DMA and shifts it out pixel by
//! pixel, combining with the colour palette to produce the final framebuffer.

use std::sync::OnceLock;

pub const FB_WIDTH: u32 = 320;
pub const FB_HEIGHT: u32 = 256;
pub const HIRES_FB_WIDTH: u32 = 640;
const HIRES_LINEBUF_LEN: usize = 2048;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DeniseHiresLinebufOffsets {
    pub sample_offset: i32,
    pub bpl1dat_trigger_offset: i32,
    pub hstart_offset: Option<i32>,
    pub hstop_offset: Option<i32>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DeniseSourcePixelDebug {
    pub raw_color_idx: u8,
    pub pf1_code: u8,
    pub pf2_code: u8,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DeniseOutputPixelDebug {
    pub called: bool,
    pub beam_x: u32,
    pub beam_y: u32,
    pub requested_x: u32,
    pub requested_y: u32,
    pub write_visible: bool,
    pub hires: bool,
    pub source_pixels_per_fb_pixel: u8,
    pub pair_samples: [DeniseSourcePixelDebug; 2],
    pub plane_bits_mask: u8,
    pub final_color_idx: u8,
    pub playfield_visible_gate: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DeniseShiftLoadPlaneDebug {
    pub raw: u16,
    pub prev: u16,
    pub scroll: u8,
    pub combined_hi: u16,
    pub combined_lo: u16,
    pub shift_loaded: u16,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DeniseShiftLoadDebug {
    pub hires: bool,
    pub odd_scroll: u8,
    pub even_scroll: u8,
    pub num_bitplanes: u8,
    pub planes: [DeniseShiftLoadPlaneDebug; 3],
}

pub struct DeniseOcs {
    pub palette: [u16; 32],
    pub framebuffer: Vec<u32>,
    pub framebuffer_hires: Vec<u32>,
    pub bpl_data: [u16; 6],  // Holding latches: written by DMA
    pub bpl_shift: [u16; 6], // Shift registers: loaded from latches on BPL1DAT write
    pub shift_count: u8,     // Pixels remaining in shift register (0 -> output COLOR00)
    bpl_shift_count: [u8; 6],
    bpl_shift_delay: [u8; 6],
    bpl_prev_data: [u16; 6],
    bpl_pending_data: [u16; 6],
    // Pending parallel-load flags for odd/even numbered bitplanes (BPL1/3/5 and BPL2/4/6).
    bpl_pending_copy_odd_planes: bool,
    bpl_pending_copy_even_planes: bool,
    bplcon1_phase_bias: u8,
    serial_phase_bias: u16,
    hires_linebuf_offsets: DeniseHiresLinebufOffsets,
    bpl_scroll_pending_line: bool,
    pub bplcon0: u16,
    pub bplcon1: u16,
    pub bplcon2: u16,
    pub bplcon3: u16,
    pub clxcon: u16,
    pub clxdat: u16,
    pub spr_pos: [u16; 8],
    pub spr_ctl: [u16; 8],
    pub spr_data: [u16; 8],
    pub spr_datb: [u16; 8],
    spr_armed: [bool; 8],
    spr_shift_data: [u16; 8],
    spr_shift_datb: [u16; 8],
    spr_shift_count: [u8; 8],
    spr_current_code: [u8; 8],
    sprite_runtime_line_valid: bool,
    sprite_runtime_beam_x: u32,
    sprite_runtime_beam_y: u32,
    last_shift_load_debug: DeniseShiftLoadDebug,
    deferred_shift_load_after_source_pixels: Option<u8>,
    hires_cached_sample: (usize, u8, u8, u8),
    hires_cached_sample_valid: bool,
    hires_sample_pair_phase: u8,
    hires_linebuf_color_idx: Vec<u8>,
    hires_linebuf_beam_y: u32,
    hires_linebuf_line_valid: bool,
    hires_linebuf_serial_cursor: u32,
    hires_linebuf_serial_cursor_seeded: bool,
    hires_linebuf_serial_beam_origin: u32,
    hires_linebuf_bpl1dat_trigger_recorded: bool,
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
    fn num_bitplanes(&self) -> usize {
        (((self.bplcon0 >> 12) & 0x7) as usize).min(6)
    }

    pub fn new() -> Self {
        Self {
            palette: [0; 32],
            framebuffer: vec![0xFF000000; (FB_WIDTH * FB_HEIGHT) as usize],
            framebuffer_hires: vec![0xFF000000; (HIRES_FB_WIDTH * FB_HEIGHT) as usize],
            bpl_data: [0; 6],
            bpl_shift: [0; 6],
            shift_count: 0,
            bpl_shift_count: [0; 6],
            bpl_shift_delay: [0; 6],
            bpl_prev_data: [0; 6],
            bpl_pending_data: [0; 6],
            bpl_pending_copy_odd_planes: false,
            bpl_pending_copy_even_planes: false,
            bplcon1_phase_bias: 0,
            serial_phase_bias: 0,
            hires_linebuf_offsets: DeniseHiresLinebufOffsets::default(),
            bpl_scroll_pending_line: true,
            bplcon0: 0,
            bplcon1: 0,
            bplcon2: 0,
            bplcon3: 0,
            clxcon: 0,
            clxdat: 0,
            spr_pos: [0; 8],
            spr_ctl: [0; 8],
            spr_data: [0; 8],
            spr_datb: [0; 8],
            // Start armed for compatibility with existing direct-field tests.
            // Precise arm/disarm semantics are applied when register writes go
            // through the `write_sprite_*` helpers used by machine-amiga.
            spr_armed: [true; 8],
            spr_shift_data: [0; 8],
            spr_shift_datb: [0; 8],
            spr_shift_count: [0; 8],
            spr_current_code: [0; 8],
            sprite_runtime_line_valid: false,
            sprite_runtime_beam_x: 0,
            sprite_runtime_beam_y: 0,
            last_shift_load_debug: DeniseShiftLoadDebug::default(),
            deferred_shift_load_after_source_pixels: None,
            hires_cached_sample: (0, 0, 0, 0),
            hires_cached_sample_valid: false,
            hires_sample_pair_phase: 0,
            hires_linebuf_color_idx: vec![0xFF; HIRES_LINEBUF_LEN],
            hires_linebuf_beam_y: 0,
            hires_linebuf_line_valid: false,
            hires_linebuf_serial_cursor: 0,
            hires_linebuf_serial_cursor_seeded: false,
            hires_linebuf_serial_beam_origin: 0,
            hires_linebuf_bpl1dat_trigger_recorded: false,
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

    pub fn set_bplcon1_phase_bias(&mut self, phase_bias: u8) {
        self.bplcon1_phase_bias = phase_bias;
    }

    pub fn set_serial_phase_bias(&mut self, phase_bias: u16) {
        self.serial_phase_bias = phase_bias;
    }

    pub fn set_hires_linebuf_display_offset(&mut self, offset: i32) {
        self.hires_linebuf_offsets.sample_offset = offset;
    }

    pub fn set_hires_linebuf_offsets(&mut self, offsets: DeniseHiresLinebufOffsets) {
        self.hires_linebuf_offsets = offsets;
        self.hires_linebuf_bpl1dat_trigger_recorded = false;
    }

    /// Record the line-buffer output-domain offset where the first `BPL1DAT`
    /// trigger occurs on the current line.
    ///
    /// This mirrors WinUAE's per-line `bpl1dat_trigger_offset` capture and
    /// allows the hires preview line flush to use the actual DMA trigger timing
    /// instead of a heuristic estimate.
    pub fn note_hires_linebuf_bpl1dat_trigger_offset(&mut self, output_pos: i32) {
        if self.hires_linebuf_bpl1dat_trigger_recorded {
            return;
        }
        self.hires_linebuf_offsets.bpl1dat_trigger_offset = output_pos;
        self.hires_linebuf_bpl1dat_trigger_recorded = true;
    }

    pub fn read_clxdat(&mut self) -> u16 {
        let value = self.clxdat;
        self.clxdat = 0;
        value
    }

    pub fn write_sprite_pos(&mut self, sprite: usize, val: u16) {
        if sprite < 8 {
            self.spr_pos[sprite] = val;
        }
    }

    pub fn write_sprite_ctl(&mut self, sprite: usize, val: u16) {
        if sprite < 8 {
            self.spr_ctl[sprite] = val;
            // Writing SPRxCTL disables the horizontal comparator (HRM Fig. 4-13).
            self.spr_armed[sprite] = false;
            self.spr_shift_count[sprite] = 0;
            self.spr_current_code[sprite] = 0;
        }
    }

    pub fn write_sprite_data(&mut self, sprite: usize, val: u16) {
        if sprite < 8 {
            self.spr_data[sprite] = val;
            // Writing SPRxDATA arms the sprite comparator (manual mode) and is
            // also how DMA refreshes sprite line data before display.
            self.spr_armed[sprite] = true;
        }
    }

    pub fn write_sprite_datb(&mut self, sprite: usize, val: u16) {
        if sprite < 8 {
            self.spr_datb[sprite] = val;
        }
    }

    /// Mark the start of a new beam line for bitplane fine-scroll timing.
    ///
    /// In this simplified Denise model, `BPLCON1` horizontal scroll delay is
    /// applied to the first bitplane shift-load on each line.
    pub fn begin_beam_line(&mut self) {
        self.flush_hires_line_buffer_to_preview();
        self.bpl_scroll_pending_line = true;
        self.bpl_prev_data = [0; 6];
        self.hires_cached_sample_valid = false;
        self.hires_sample_pair_phase = 0;
        self.hires_linebuf_line_valid = false;
        self.hires_linebuf_serial_cursor = 0;
        self.hires_linebuf_serial_cursor_seeded = false;
        self.hires_linebuf_serial_beam_origin = 0;
        self.hires_linebuf_bpl1dat_trigger_recorded = false;
    }

    #[must_use]
    pub fn last_shift_load_debug(&self) -> DeniseShiftLoadDebug {
        self.last_shift_load_debug
    }

    /// Defer the next bitplane parallel shift-load until after `count`
    /// serialized source pixels have been consumed.
    ///
    /// This is a debug/bring-up hook for exploring sub-CCK load phase in hires
    /// modes without rewriting the caller's render pipeline ordering.
    pub fn defer_shift_load_after_source_pixels(&mut self, count: u8) {
        if count == 0 {
            self.trigger_shift_load();
        } else {
            self.deferred_shift_load_after_source_pixels = Some(count);
        }
    }

    /// Queue a BPL1DAT-triggered parallel load. The actual copy into the
    /// serial shift registers happens later when Denise's horizontal comparator
    /// matches `BPLCON1`, mirroring real hardware behavior more closely.
    pub fn queue_shift_load_from_bpl1dat(&mut self) {
        self.bpl_pending_data = self.bpl_data;
        self.bpl_pending_copy_odd_planes = true;
        self.bpl_pending_copy_even_planes = true;
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

    fn sprite_line_active(beam_y: u32, vstart: u32, vstop: u32) -> bool {
        if vstart == vstop {
            return false;
        }
        if vstart < vstop {
            beam_y >= vstart && beam_y < vstop
        } else {
            beam_y >= vstart || beam_y < vstop
        }
    }

    fn reset_sprite_line_runtime(&mut self, beam_y: u32) {
        self.spr_shift_count = [0; 8];
        self.spr_current_code = [0; 8];
        self.sprite_runtime_line_valid = true;
        self.sprite_runtime_beam_x = 0;
        self.sprite_runtime_beam_y = beam_y;
    }

    fn step_sprite_runtime_one_pixel(&mut self, beam_x: u32, beam_y: u32) {
        self.spr_current_code = [0; 8];
        for sprite in 0..8usize {
            if !self.spr_armed[sprite] {
                self.spr_shift_count[sprite] = 0;
                continue;
            }

            let pos = self.spr_pos[sprite];
            let ctl = self.spr_ctl[sprite];
            let hstart = u32::from(Self::sprite_hstart(pos, ctl));
            let vstart = u32::from(Self::sprite_vstart(pos, ctl));
            let vstop = u32::from(Self::sprite_vstop(pos, ctl));

            // Comparator phase: detect the horizontal match for this pixel.
            let load_pulse = Self::sprite_line_active(beam_y, vstart, vstop) && beam_x == hstart;

            // Load phase: copy sprite data regs into the serial shifters.
            if load_pulse {
                self.spr_shift_data[sprite] = self.spr_data[sprite];
                self.spr_shift_datb[sprite] = self.spr_datb[sprite];
                self.spr_shift_count[sprite] = 16;
            }

            // Shift/output phase: emit one low-res sprite pixel.
            if self.spr_shift_count[sprite] == 0 {
                continue;
            }

            let lo = (self.spr_shift_data[sprite] >> 15) & 1;
            let hi = (self.spr_shift_datb[sprite] >> 15) & 1;
            self.spr_current_code[sprite] = (lo | (hi << 1)) as u8;
            self.spr_shift_data[sprite] <<= 1;
            self.spr_shift_datb[sprite] <<= 1;
            self.spr_shift_count[sprite] -= 1;
        }
    }

    fn sync_sprite_runtime_to_beam(&mut self, beam_x: u32, beam_y: u32) {
        if !self.sprite_runtime_line_valid || self.sprite_runtime_beam_y != beam_y {
            self.reset_sprite_line_runtime(beam_y);
            // Fast-forward from the line start to the requested beam pixel.
            for x in 0..=beam_x {
                self.step_sprite_runtime_one_pixel(x, beam_y);
            }
            self.sprite_runtime_beam_x = beam_x;
            return;
        }

        if beam_x <= self.sprite_runtime_beam_x {
            self.reset_sprite_line_runtime(beam_y);
            for x in 0..=beam_x {
                self.step_sprite_runtime_one_pixel(x, beam_y);
            }
            self.sprite_runtime_beam_x = beam_x;
            return;
        }

        for x in (self.sprite_runtime_beam_x + 1)..=beam_x {
            self.step_sprite_runtime_one_pixel(x, beam_y);
        }
        self.sprite_runtime_beam_x = beam_x;
    }

    fn sprite_pixel(&self, _beam_x: u32, _beam_y: u32) -> Option<SpritePixel> {
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
                // Intentionally combine independently-evaluated odd/even sprite
                // codes at this beam position. This matches the HRM behavior
                // where attached pairs can move independently and, when
                // misaligned, pixels "revert" to shifted color subsets.
                let even_code = self.spr_current_code[pair];
                let odd_code = self.spr_current_code[odd];
                let code = ((odd_code as usize) << 2) | (even_code as usize);
                if code == 0 {
                    continue;
                }
                return Some(SpritePixel {
                    palette_idx: 16 + code,
                    sprite_group: pair / 2,
                });
            }

            let code = self.spr_current_code[sprite];
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

    fn collision_group_mask(&self, _beam_x: u32, _beam_y: u32) -> u8 {
        let mut mask = 0u8;
        for sprite in 0..8usize {
            let code = self.spr_current_code[sprite];
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

    fn clxcon_bitplane_match(&self, plane_bits_mask: u8, even_planes: bool) -> bool {
        // CLXCON bit layout:
        //   ENBP1..ENBP6 = bits 6..11
        //   MVBP1..MVBP6 = bits 0..5
        //
        // Plane numbering is 1-based in the docs, while `plane_bits_mask`
        // stores bitplane 1 in bit 0, bitplane 6 in bit 5.
        let plane_indices: [u8; 3] = if even_planes { [1, 3, 5] } else { [0, 2, 4] };
        for plane_idx in plane_indices {
            let enabled = (self.clxcon & (1u16 << (6 + plane_idx))) != 0;
            if !enabled {
                continue;
            }
            let expected = (self.clxcon & (1u16 << plane_idx)) != 0;
            let actual = (plane_bits_mask & (1u8 << plane_idx)) != 0;
            if actual != expected {
                return false;
            }
        }
        true
    }

    fn latch_collisions(&mut self, plane_bits_mask: u8, sprite_groups: u8) {
        let odd_bitplanes_match = self.clxcon_bitplane_match(plane_bits_mask, false);
        let even_bitplanes_match = self.clxcon_bitplane_match(plane_bits_mask, true);
        let mut bits = 0u16;
        if odd_bitplanes_match && even_bitplanes_match {
            bits |= 1 << 0;
        }

        for group in 0..4u8 {
            if (sprite_groups & (1u8 << group)) == 0 {
                continue;
            }
            if odd_bitplanes_match {
                bits |= 1u16 << (1 + group);
            }
            if even_bitplanes_match {
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
        self.deferred_shift_load_after_source_pixels = None;
        self.bpl_pending_copy_odd_planes = false;
        self.bpl_pending_copy_even_planes = false;
        self.hires_cached_sample_valid = false;
        // BPLCON1 fine-scroll is a continuous barrel shift across fetched
        // bitplane words. Model this by combining the previous and current
        // DMA words per plane when loading the serial shift registers.
        let hires = (self.bplcon0 & 0x8000) != 0;
        // In hires modes, KS2.x output matches better when BPLCON1 is modeled
        // as a line-start delay rather than a per-fetch-group barrel carry.
        // Keep the old model available for lowres experimentation only.
        let line_delay_only_scroll = hires || denise_experiment_bplcon1_line_delay_only();
        let first_shift_load_this_line = self.bpl_scroll_pending_line;
        let mut odd_scroll = ((self.bplcon1 >> 4) & 0x000F) as u8;
        let mut even_scroll = (self.bplcon1 & 0x000F) as u8;
        if denise_experiment_ignore_bplcon1() {
            odd_scroll = 0;
            even_scroll = 0;
        }
        if hires {
            // HRM: in hires mode horizontal scrolling is in 2-pixel increments.
            // Model this as ignoring the low bit of each delay nibble.
            odd_scroll &= !1;
            even_scroll &= !1;
        }
        self.bpl_scroll_pending_line = false;
        let num_bpl = self.num_bitplanes();
        let mut shift_dbg = DeniseShiftLoadDebug {
            hires,
            odd_scroll,
            even_scroll,
            num_bitplanes: num_bpl as u8,
            planes: [DeniseShiftLoadPlaneDebug::default(); 3],
        };
        for i in 0..6 {
            if i >= num_bpl {
                self.bpl_shift[i] = 0;
                self.bpl_shift_count[i] = 0;
                self.bpl_shift_delay[i] = 0;
                self.bpl_prev_data[i] = 0;
                continue;
            }
            let raw = self.bpl_data[i];
            let prev = self.bpl_prev_data[i];
            let scroll = if i & 1 == 0 { odd_scroll } else { even_scroll };
            let combined = if denise_experiment_reverse_bplcon1_carry() {
                (u32::from(raw) << 16) | u32::from(prev)
            } else {
                (u32::from(prev) << 16) | u32::from(raw)
            };
            if line_delay_only_scroll {
                self.bpl_shift[i] = raw;
            } else {
                self.bpl_shift[i] = if scroll == 0 {
                    raw
                } else {
                    (combined >> scroll) as u16
                };
            }
            if i < 3 {
                shift_dbg.planes[i] = DeniseShiftLoadPlaneDebug {
                    raw,
                    prev,
                    scroll,
                    combined_hi: (combined >> 16) as u16,
                    combined_lo: combined as u16,
                    shift_loaded: self.bpl_shift[i],
                };
            }
            self.bpl_shift_count[i] = 16;
            self.bpl_shift_delay[i] = if line_delay_only_scroll && first_shift_load_this_line {
                scroll
            } else {
                0
            };
            self.bpl_prev_data[i] = raw;
        }
        self.last_shift_load_debug = shift_dbg;
        self.shift_count = 16;
    }

    fn bplcon1_scrolls_for_current_mode(&self) -> (u8, u8, bool) {
        let hires = (self.bplcon0 & 0x8000) != 0;
        let mut odd_scroll = ((self.bplcon1 >> 4) & 0x000F) as u8;
        let mut even_scroll = (self.bplcon1 & 0x000F) as u8;
        if denise_experiment_ignore_bplcon1() {
            odd_scroll = 0;
            even_scroll = 0;
        }
        if hires {
            // HRM: hires fine scroll is in 2-pixel increments.
            odd_scroll &= !1;
            even_scroll &= !1;
        }
        (odd_scroll, even_scroll, hires)
    }

    fn commit_pending_shift_load_group(&mut self, odd_planes: bool) {
        let num_bpl = self.num_bitplanes();
        for plane in 0..num_bpl {
            let plane_is_odd_numbered = plane % 2 == 0; // plane 0 => BPL1
            if plane_is_odd_numbered != odd_planes {
                continue;
            }
            self.bpl_shift[plane] = self.bpl_pending_data[plane];
            self.bpl_shift_count[plane] = 16;
            self.bpl_shift_delay[plane] = 0;
        }
        self.hires_cached_sample_valid = false;
    }

    fn update_shift_count_from_planes(&mut self) {
        self.shift_count = self
            .bpl_shift_count
            .iter()
            .zip(self.bpl_shift_delay.iter())
            .map(|(&count, &delay)| count.saturating_add(delay))
            .max()
            .unwrap_or(0);
    }

    fn apply_pending_shift_load_if_due(&mut self, phase_counter: u16) {
        if !self.bpl_pending_copy_odd_planes && !self.bpl_pending_copy_even_planes {
            return;
        }
        let (odd_scroll, even_scroll, hires) = self.bplcon1_scrolls_for_current_mode();
        let phase_mask = if hires { 0x07 } else { 0x0F };
        let phase = ((phase_counter as u8).wrapping_add(self.bplcon1_phase_bias)) & phase_mask;

        if self.bpl_pending_copy_odd_planes && phase == odd_scroll {
            self.commit_pending_shift_load_group(true);
            self.bpl_pending_copy_odd_planes = false;
        }
        if self.bpl_pending_copy_even_planes && phase == even_scroll {
            self.commit_pending_shift_load_group(false);
            self.bpl_pending_copy_even_planes = false;
        }

        // Keep legacy debug payload populated with a snapshot of the raw latches
        // when a pending load commits.
        if (!self.bpl_pending_copy_odd_planes || phase == odd_scroll)
            && (!self.bpl_pending_copy_even_planes || phase == even_scroll)
        {
            let num_bpl = self.num_bitplanes();
            let mut dbg = DeniseShiftLoadDebug {
                hires,
                odd_scroll,
                even_scroll,
                num_bitplanes: num_bpl as u8,
                planes: [DeniseShiftLoadPlaneDebug::default(); 3],
            };
            for i in 0..num_bpl.min(3) {
                dbg.planes[i] = DeniseShiftLoadPlaneDebug {
                    raw: self.bpl_pending_data[i],
                    prev: self.bpl_prev_data[i],
                    scroll: if i & 1 == 0 { odd_scroll } else { even_scroll },
                    combined_hi: self.bpl_prev_data[i],
                    combined_lo: self.bpl_pending_data[i],
                    shift_loaded: self.bpl_shift[i],
                };
            }
            self.last_shift_load_debug = dbg;
        }

        self.update_shift_count_from_planes();
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

    fn ensure_legacy_shift_state_compat(&mut self) {
        // Older unit tests directly set `shift_count`/`bpl_shift` without using
        // `trigger_shift_load()`. Lazily mirror that into the per-plane state.
        if self.shift_count == 0 {
            return;
        }
        if self.bpl_shift_count.iter().any(|&c| c != 0) || self.bpl_shift_delay.iter().any(|&d| d != 0) {
            return;
        }
        self.bpl_shift_count = [self.shift_count; 6];
    }

    fn ensure_hires_line_buffer_line(&mut self, beam_y: u32) {
        if self.hires_linebuf_line_valid && self.hires_linebuf_beam_y == beam_y {
            return;
        }
        self.hires_linebuf_color_idx.fill(0xFF);
        self.hires_linebuf_beam_y = beam_y;
        self.hires_linebuf_line_valid = true;
        self.hires_linebuf_serial_cursor = 0;
        self.hires_linebuf_serial_cursor_seeded = false;
        self.hires_linebuf_serial_beam_origin = 0;
        self.hires_linebuf_bpl1dat_trigger_recorded = false;
    }

    fn flush_hires_line_buffer_to_preview(&mut self) {
        if !self.hires_linebuf_line_valid {
            return;
        }
        let y = self.hires_linebuf_beam_y;
        if y >= FB_HEIGHT {
            return;
        }
        let row_base = (y * HIRES_FB_WIDTH) as usize;
        for x in 0..HIRES_FB_WIDTH {
            let color_idx = self.sample_hires_line_buffer_color_for_window(x, 0, true) as usize;
            let argb = Self::rgb12_to_argb32(self.palette[color_idx]);
            let idx = row_base + x as usize;
            if idx < self.framebuffer_hires.len() {
                self.framebuffer_hires[idx] = argb;
            }
        }
    }

    fn record_hires_line_buffer_pair(&mut self, beam_x: u32, beam_y: u32, color_pair: [u8; 2]) {
        self.ensure_hires_line_buffer_line(beam_y);
        if !self.hires_linebuf_serial_cursor_seeded {
            self.hires_linebuf_serial_beam_origin = beam_x.saturating_mul(2);
            self.hires_linebuf_serial_cursor = 0;
            self.hires_linebuf_serial_cursor_seeded = true;
        }
        let base = self.hires_linebuf_serial_cursor;
        for (sub, color_idx) in color_pair.into_iter().enumerate() {
            let idx = base.saturating_add(sub as u32) as usize;
            if idx < self.hires_linebuf_color_idx.len() {
                self.hires_linebuf_color_idx[idx] = color_idx;
            }
        }
        self.hires_linebuf_serial_cursor = self.hires_linebuf_serial_cursor.saturating_add(2);
    }

    fn sample_hires_line_buffer_color(&self, linebuf_x: u32, fallback: u8) -> u8 {
        let sample_offset =
            self.hires_linebuf_offsets.sample_offset + denise_experiment_hires_linebuf_sample_offset();
        let serial_x = linebuf_x as i32 + sample_offset;
        if serial_x < 0 {
            return fallback;
        }
        if serial_x < self.hires_linebuf_offsets.bpl1dat_trigger_offset {
            return 0;
        }
        if let Some(hstart) = self.hires_linebuf_offsets.hstart_offset
            && serial_x < hstart
        {
            return 0;
        }
        if let Some(hstop) = self.hires_linebuf_offsets.hstop_offset
            && serial_x >= hstop
        {
            return 0;
        }
        let idx = serial_x as usize;
        self.hires_linebuf_color_idx
            .get(idx)
            .copied()
            .filter(|&v| v != 0xFF)
            .unwrap_or(fallback)
    }

    fn sample_hires_line_buffer_color_for_window(
        &self,
        linebuf_x: u32,
        fallback: u8,
        playfield_visible_gate: bool,
    ) -> u8 {
        let sample_offset =
            self.hires_linebuf_offsets.sample_offset + denise_experiment_hires_linebuf_sample_offset();
        // `linebuf_x` is the output-pixel counter in the hires preview
        // framebuffer (cnt-like domain). `sample_offset` maps that output
        // counter to the serialized playfield sample buffer (cp-like domain).
        // HDIW/BPL1DAT trigger offsets are output-domain gates and must not be
        // folded into source sampling.
        let output_pos = linebuf_x as i32 + sample_offset;
        let serial_x = output_pos;

        if !playfield_visible_gate {
            return 0;
        }
        if serial_x < 0 {
            return fallback;
        }
        if output_pos < self.hires_linebuf_offsets.bpl1dat_trigger_offset {
            return 0;
        }
        if let Some(hstart) = self.hires_linebuf_offsets.hstart_offset
            && output_pos < hstart
        {
            return 0;
        }
        if let Some(hstop) = self.hires_linebuf_offsets.hstop_offset
            && output_pos >= hstop
        {
            return 0;
        }

        let idx = serial_x as usize;
        self.hires_linebuf_color_idx
            .get(idx)
            .copied()
            .filter(|&v| v != 0xFF)
            .unwrap_or(fallback)
    }

    fn shift_one_playfield_source_pixel(&mut self) -> (usize, u8, u8, u8) {
        self.ensure_legacy_shift_state_compat();

        let mut raw_color_idx = 0usize;
        let mut pf1_code = 0u8;
        let mut pf2_code = 0u8;
        let mut plane_bits_mask = 0u8;

        if self.shift_count > 0 {
            // Compute color index from per-plane shifter bits (MSB first),
            // honoring BPLCON1 odd/even horizontal delay.
            let mut num_bpl = self.num_bitplanes();
            if num_bpl == 0 {
                // Legacy unit tests may seed shift registers directly without
                // programming BPLCON0. Infer a minimal active plane span from
                // the mirrored legacy shift state in that case.
                num_bpl = self
                    .bpl_shift_count
                    .iter()
                    .rposition(|&c| c != 0)
                    .map(|idx| idx + 1)
                    .or_else(|| self.bpl_shift.iter().rposition(|&w| w != 0).map(|idx| idx + 1))
                    .unwrap_or(0);
            }
            for plane in 0..num_bpl {
                if self.bpl_shift_delay[plane] > 0 {
                    self.bpl_shift_delay[plane] -= 1;
                    continue;
                }
                if self.bpl_shift_count[plane] == 0 {
                    continue;
                }
                let bit_set = (self.bpl_shift[plane] & 0x8000) != 0;
                if bit_set {
                    raw_color_idx |= 1usize << plane;
                    plane_bits_mask |= 1u8 << plane;
                    if plane & 1 == 0 {
                        pf1_code |= 1u8 << (plane / 2);
                    } else {
                        pf2_code |= 1u8 << (plane / 2);
                    }
                }
                self.bpl_shift[plane] <<= 1;
                self.bpl_shift_count[plane] -= 1;
            }
            self.shift_count = self
                .bpl_shift_count
                .iter()
                .zip(self.bpl_shift_delay.iter())
                .map(|(&count, &delay)| count.saturating_add(delay))
                .max()
                .unwrap_or(0);
        }

        if let Some(remaining) = self.deferred_shift_load_after_source_pixels {
            if remaining <= 1 {
                self.deferred_shift_load_after_source_pixels = None;
                self.trigger_shift_load();
            } else {
                self.deferred_shift_load_after_source_pixels = Some(remaining - 1);
            }
        }

        (raw_color_idx, pf1_code, pf2_code, plane_bits_mask)
    }

    fn shift_one_playfield_render_sample(&mut self, hires: bool) -> (usize, u8, u8, u8) {
        if !hires {
            return self.shift_one_playfield_source_pixel();
        }

        if denise_experiment_hires_full_rate_shift() {
            return self.shift_one_playfield_source_pixel();
        }

        if !self.hires_cached_sample_valid {
            self.hires_cached_sample = self.shift_one_playfield_source_pixel();
            self.hires_cached_sample_valid = true;
            self.hires_sample_pair_phase = 0;
        }

        let out = self.hires_cached_sample;
        self.hires_sample_pair_phase = (self.hires_sample_pair_phase + 1) & 1;
        if self.hires_sample_pair_phase == 0 {
            self.hires_cached_sample = self.shift_one_playfield_source_pixel();
            self.hires_cached_sample_valid = true;
        }
        out
    }

    pub fn output_pixel(&mut self, x: u32, y: u32) {
        self.output_pixel_with_beam(x, y, x, y);
    }

    fn output_pixel_with_beam_n_source_samples(
        &mut self,
        x: u32,
        y: u32,
        beam_x: u32,
        beam_y: u32,
        source_pixels_per_output_call: u8,
        playfield_visible_gate: bool,
    ) -> DeniseOutputPixelDebug {
        let write_visible = x < FB_WIDTH && y < FB_HEIGHT;
        self.sync_sprite_runtime_to_beam(beam_x, beam_y);
        let hires = (self.bplcon0 & 0x8000) != 0;
        let source_pixels_per_fb_pixel = source_pixels_per_output_call.clamp(1, 2);
        let mut pair_samples = [(0usize, 0u8, 0u8); 2];
        let mut pair_samples_debug = [DeniseSourcePixelDebug::default(); 2];
        let mut raw_color_idx = 0usize;
        let mut pf1_code = 0u8;
        let mut pf2_code = 0u8;
        let mut pair_playfield_colors = [0u8; 2];
        let mut plane_bits_mask = 0u8;
        // Denise's horizontal comparator (`denise_hcounter_cmp` in WinUAE) is
        // a CCK-rate counter, not a half-CCK/subpixel counter. In this model
        // `beam_x` advances twice per CCK, so use `beam_x >> 1` for BPLCON1
        // pending-load comparator phase.
        let comparator_phase = if hires {
            ((beam_x >> 1) as u16).wrapping_sub(self.serial_phase_bias)
        } else {
            beam_x as u16
        };

        let copy_before_shift = hires && denise_experiment_hires_copy_before_shift();
        let comparator_tick_this_call = !hires || (beam_x & 1) != 0;
        if copy_before_shift && comparator_tick_this_call {
            self.apply_pending_shift_load_if_due(comparator_phase);
        }

        for sample_idx in 0..source_pixels_per_fb_pixel {
            let (raw, pf1, pf2, mask) = self.shift_one_playfield_render_sample(hires);
            if sample_idx < 2 {
                pair_samples[sample_idx as usize] = (raw, pf1, pf2);
                pair_samples_debug[sample_idx as usize] = DeniseSourcePixelDebug {
                    raw_color_idx: raw as u8,
                    pf1_code: pf1,
                    pf2_code: pf2,
                };
            }
            // For the 640->320 hires downsample path, use the later source
            // pixel in the pair as the displayed color and merge collision
            // visibility from both source pixels.
            raw_color_idx = raw;
            pf1_code = pf1;
            pf2_code = pf2;
            plane_bits_mask |= mask;
            if hires && sample_idx < 2 {
                let playfield_sub = self.compose_playfield_pixel(raw, pf1, pf2);
                pair_playfield_colors[sample_idx as usize] = playfield_sub.visible_color_idx as u8;
            }
        }

        // Denise's BPLCON1 scroll comparator uses a horizontal counter phase,
        // not a "source pixels shifted so far on this line" counter. Queue
        // BPL1DAT-triggered loads and commit them on the comparator match using
        // the absolute beam phase of this output step.
        if !copy_before_shift && comparator_tick_this_call {
            self.apply_pending_shift_load_if_due(comparator_phase);
        }

        let playfield = if playfield_visible_gate {
            self.compose_playfield_pixel(raw_color_idx, pf1_code, pf2_code)
        } else {
            PlayfieldPixel {
                visible_color_idx: 0,
                front_playfield: None,
            }
        };
        let sprite_group_mask = self.collision_group_mask(beam_x, beam_y);
        if write_visible {
            self.latch_collisions(
                if playfield_visible_gate {
                    plane_bits_mask
                } else {
                    0
                },
                sprite_group_mask,
            );
        }
        let mut color_idx = playfield.visible_color_idx;
        if let Some(sprite_pixel) = self.sprite_pixel(beam_x, beam_y) {
            if let Some(front_pf) = playfield.front_playfield {
                if self.sprite_has_priority_over_playfield(sprite_pixel.sprite_group, front_pf) {
                    color_idx = sprite_pixel.palette_idx;
                }
            } else {
                // Background/COLOR00 only; sprite is visible.
                color_idx = sprite_pixel.palette_idx;
            }
        }

        if hires {
            // Stage playfield serial samples into a per-line hires buffer even
            // when the current host pixel is clipped. This preserves hidden
            // pre-window scroll pixels and decouples Denise serial generation
            // from host framebuffer visibility.
            self.record_hires_line_buffer_pair(beam_x, beam_y, pair_playfield_colors);
        }

        if write_visible {
            let argb32 = Self::rgb12_to_argb32(self.palette[color_idx]);
            self.framebuffer[(y * FB_WIDTH + x) as usize] = argb32;

            // Maintain a hires-resolution playfield preview buffer for ECS/KS2.x
            // debugging and screenshots. This intentionally omits per-subpixel
            // sprite composition; lowres pixels are replicated.
            let hx = x * 2;
            let hy = y;
            if hires {
                for sub in 0..2usize {
                    let linebuf_x = hx.saturating_add(sub as u32);
                    let color_idx_sub = self.sample_hires_line_buffer_color_for_window(
                        linebuf_x,
                        pair_playfield_colors[sub],
                        playfield_visible_gate,
                    ) as usize;
                    let argb_sub = Self::rgb12_to_argb32(self.palette[color_idx_sub]);
                    let hidx = (hy * HIRES_FB_WIDTH + hx + sub as u32) as usize;
                    if hidx < self.framebuffer_hires.len() {
                        self.framebuffer_hires[hidx] = argb_sub;
                    }
                }
            } else {
                let hidx = (hy * HIRES_FB_WIDTH + hx) as usize;
                if hidx + 1 < self.framebuffer_hires.len() {
                    self.framebuffer_hires[hidx] = argb32;
                    self.framebuffer_hires[hidx + 1] = argb32;
                }
            }
        }

        DeniseOutputPixelDebug {
            called: true,
            beam_x,
            beam_y,
            requested_x: x,
            requested_y: y,
            write_visible,
            hires,
            source_pixels_per_fb_pixel,
            pair_samples: pair_samples_debug,
            plane_bits_mask,
            final_color_idx: color_idx as u8,
            playfield_visible_gate,
        }
    }

    pub fn output_pixel_with_beam_and_playfield_gate(
        &mut self,
        x: u32,
        y: u32,
        beam_x: u32,
        beam_y: u32,
        playfield_visible_gate: bool,
    ) -> DeniseOutputPixelDebug {
        let hires = (self.bplcon0 & 0x8000) != 0;
        let source_pixels_per_output_call = if hires { 2 } else { 1 };
        self.output_pixel_with_beam_n_source_samples(
            x,
            y,
            beam_x,
            beam_y,
            source_pixels_per_output_call,
            playfield_visible_gate,
        )
    }

    pub fn output_pixel_with_beam(
        &mut self,
        x: u32,
        y: u32,
        beam_x: u32,
        beam_y: u32,
    ) -> DeniseOutputPixelDebug {
        self.output_pixel_with_beam_and_playfield_gate(x, y, beam_x, beam_y, true)
    }
}

fn denise_experiment_reverse_bplcon1_carry() -> bool {
    static REVERSE: OnceLock<bool> = OnceLock::new();
    *REVERSE.get_or_init(|| std::env::var_os("AMIGA_EXPERIMENT_REVERSE_BPLCON1_CARRY").is_some())
}

fn denise_experiment_ignore_bplcon1() -> bool {
    static IGNORE: OnceLock<bool> = OnceLock::new();
    *IGNORE.get_or_init(|| std::env::var_os("AMIGA_EXPERIMENT_IGNORE_BPLCON1").is_some())
}

fn denise_experiment_bplcon1_line_delay_only() -> bool {
    static LINE_DELAY_ONLY: OnceLock<bool> = OnceLock::new();
    *LINE_DELAY_ONLY
        .get_or_init(|| std::env::var_os("AMIGA_EXPERIMENT_BPLCON1_LINE_DELAY_ONLY").is_some())
}

fn denise_experiment_hires_copy_before_shift() -> bool {
    static COPY_BEFORE: OnceLock<bool> = OnceLock::new();
    *COPY_BEFORE.get_or_init(|| std::env::var_os("AMIGA_EXPERIMENT_HIRES_COPY_BEFORE_SHIFT").is_some())
}

fn denise_experiment_hires_full_rate_shift() -> bool {
    static FULL_RATE: OnceLock<bool> = OnceLock::new();
    *FULL_RATE.get_or_init(|| std::env::var_os("AMIGA_EXPERIMENT_HIRES_FULL_RATE_SHIFT").is_some())
}

fn denise_experiment_hires_linebuf_sample_offset() -> i32 {
    static OFFSET: OnceLock<i32> = OnceLock::new();
    *OFFSET.get_or_init(|| {
        let Some(raw) = std::env::var_os("AMIGA_EXPERIMENT_HIRES_LINEBUF_SAMPLE_OFFSET") else {
            return 0;
        };
        let s = raw.to_string_lossy();
        s.parse::<i32>().unwrap_or(0)
    })
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

    fn collect_raw_source_pixels(denise: &mut DeniseOcs, count: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            let (raw, _, _, _) = denise.shift_one_playfield_source_pixel();
            out.push(raw as u8);
        }
        out
    }

    fn invisible_output_pixel(denise: &mut DeniseOcs, beam_x: u32) -> DeniseOutputPixelDebug {
        denise.output_pixel_with_beam(u32::MAX, u32::MAX, beam_x, 0)
    }

    #[test]
    fn hires_bplcon1_delay_applies_only_to_first_shift_load_of_line() {
        let mut denise = DeniseOcs::new();
        denise.bplcon0 = 0x9000; // HIRES + 1 bitplane
        denise.bplcon1 = 0x0040; // odd planes scroll by 4 hires pixels
        denise.begin_beam_line();

        denise.bpl_data[0] = 0x8000;
        denise.trigger_shift_load();
        assert_eq!(
            collect_raw_source_pixels(&mut denise, 16),
            vec![0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            "first hires shift load of the line should honor BPLCON1 delay"
        );

        denise.bpl_data[0] = 0x8000;
        denise.trigger_shift_load();
        assert_eq!(
            collect_raw_source_pixels(&mut denise, 16),
            vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            "subsequent hires shift loads on the same line should not reapply the line-start delay"
        );
    }

    #[test]
    fn hires_bplcon1_ignores_low_scroll_bit() {
        let mut denise = DeniseOcs::new();
        denise.bplcon0 = 0x9000; // HIRES + 1 bitplane
        denise.bplcon1 = 0x0050; // odd nibble = 5 -> should behave as 4 in hires
        denise.begin_beam_line();

        denise.bpl_data[0] = 0x8000;
        denise.trigger_shift_load();

        let first_six = collect_raw_source_pixels(&mut denise, 6);
        assert_eq!(
            first_six,
            vec![0, 0, 0, 0, 1, 0],
            "hires scroll should use 2-pixel increments (ignore low nibble bit)"
        );
        assert_eq!(denise.last_shift_load_debug().odd_scroll, 4);
    }

    #[test]
    fn lowres_bplcon1_uses_previous_word_carry_on_later_shift_loads() {
        let mut denise = DeniseOcs::new();
        denise.bplcon0 = 0x1000; // 1 bitplane, lowres
        denise.bplcon1 = 0x0010; // odd planes scroll by 1 pixel
        denise.begin_beam_line();

        denise.bpl_data[0] = 0x0001;
        denise.trigger_shift_load();
        let _ = collect_raw_source_pixels(&mut denise, 16);

        denise.bpl_data[0] = 0x0000;
        denise.trigger_shift_load();

        assert_eq!(
            denise.last_shift_load_debug().planes[0].shift_loaded,
            0x8000,
            "lowres BPLCON1 should barrel-shift across the previous/current fetched words"
        );
        let first_four = collect_raw_source_pixels(&mut denise, 4);
        assert_eq!(first_four, vec![1, 0, 0, 0]);
    }

    #[test]
    fn lowres_output_pixel_with_beam_consumes_one_source_pixel_per_call() {
        let mut denise = DeniseOcs::new();
        denise.bplcon0 = 0x1000; // 1 bitplane, lowres
        denise.begin_beam_line();
        denise.bpl_data[0] = 0xA000; // bits: 1,0,1,0,...
        denise.trigger_shift_load();

        let dbg = invisible_output_pixel(&mut denise, 0);

        assert!(dbg.called);
        assert!(!dbg.hires);
        assert_eq!(dbg.source_pixels_per_fb_pixel, 1);
        assert_eq!(dbg.pair_samples[0].raw_color_idx, 1);
        assert_eq!(
            dbg.pair_samples[1],
            DeniseSourcePixelDebug::default(),
            "lowres path should not consume a second source pixel in the same call"
        );
        assert_eq!(denise.shift_count, 15);
    }

    #[test]
    fn hires_output_pixel_with_beam_consumes_two_source_pixels_per_call() {
        let mut denise = DeniseOcs::new();
        denise.bplcon0 = 0x9000; // HIRES + 1 bitplane
        denise.begin_beam_line();
        denise.bpl_data[0] = 0xC000; // bits: 1,1,0,0,...
        denise.trigger_shift_load();

        let dbg = invisible_output_pixel(&mut denise, 0);

        assert!(dbg.called);
        assert!(dbg.hires);
        assert_eq!(dbg.source_pixels_per_fb_pixel, 2);
        assert_eq!(dbg.pair_samples[0].raw_color_idx, 1);
        assert_eq!(dbg.pair_samples[1].raw_color_idx, 1);
        assert_eq!(denise.shift_count, 14);
    }

    #[test]
    fn two_hires_output_calls_advance_four_source_pixels_total() {
        let mut denise = DeniseOcs::new();
        denise.bplcon0 = 0x9000; // HIRES + 1 bitplane
        denise.begin_beam_line();
        denise.bpl_data[0] = 0xA000; // bits: 1,0,1,0,...
        denise.trigger_shift_load();

        let dbg0 = invisible_output_pixel(&mut denise, 0);
        let dbg1 = invisible_output_pixel(&mut denise, 1);

        assert_eq!(
            [dbg0.pair_samples[0].raw_color_idx, dbg0.pair_samples[1].raw_color_idx],
            [1, 1]
        );
        assert_eq!(
            [dbg1.pair_samples[0].raw_color_idx, dbg1.pair_samples[1].raw_color_idx],
            [0, 0],
            "with CCK-rate hires shifting, the second output call reuses the second half of the same CCK serializer phase"
        );
        assert_eq!(denise.shift_count, 13);
    }

    #[test]
    fn deferred_shift_load_can_land_between_two_hires_samples_in_one_call() {
        let mut denise = DeniseOcs::new();
        denise.bplcon0 = 0x9000; // HIRES + 1 bitplane
        denise.begin_beam_line();
        denise.bpl_data[0] = 0x0000;
        denise.trigger_shift_load();

        denise.bpl_data[0] = 0x8000; // next fetched word
        denise.defer_shift_load_after_source_pixels(1);

        let dbg = invisible_output_pixel(&mut denise, 0);
        assert_eq!(
            [dbg.pair_samples[0].raw_color_idx, dbg.pair_samples[1].raw_color_idx],
            [0, 0],
            "with CCK-rate hires shifting, deferred parallel load no longer lands between the two samples of one output call"
        );
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
    fn sprite_ctl_disarms_and_sprite_data_rearms_comparator() {
        let mut denise = DeniseOcs::new();
        denise.set_palette(0, 0x000);
        denise.set_palette(17, 0xF00);

        let (pos, ctl) = encode_sprite_pos_ctl(26, 10, 11);
        denise.write_sprite_pos(0, pos);
        denise.write_sprite_ctl(0, ctl); // disarm
        denise.write_sprite_datb(0, 0x0000);
        denise.write_sprite_data(0, 0x8000); // arm

        denise.output_pixel(26, 10);
        assert_eq!(
            denise.framebuffer[(10 * FB_WIDTH + 26) as usize],
            DeniseOcs::rgb12_to_argb32(0xF00)
        );

        denise.write_sprite_ctl(0, ctl); // disarm again
        denise.output_pixel(26, 10);
        assert_eq!(
            denise.framebuffer[(10 * FB_WIDTH + 26) as usize],
            DeniseOcs::rgb12_to_argb32(0x000),
            "writing SPRxCTL should disable sprite output until re-armed"
        );

        denise.write_sprite_datb(0, 0x0000); // DATB alone must not arm
        denise.output_pixel(26, 10);
        assert_eq!(
            denise.framebuffer[(10 * FB_WIDTH + 26) as usize],
            DeniseOcs::rgb12_to_argb32(0x000),
            "writing SPRxDATB alone should not arm the comparator"
        );

        denise.write_sprite_data(0, 0x8000); // DATA arms
        denise.output_pixel(26, 10);
        assert_eq!(
            denise.framebuffer[(10 * FB_WIDTH + 26) as usize],
            DeniseOcs::rgb12_to_argb32(0xF00),
            "writing SPRxDATA should arm the comparator"
        );
    }

    #[test]
    fn sprite_pos_write_moves_armed_sprite_horizontally() {
        let mut denise = DeniseOcs::new();
        denise.set_palette(0, 0x000);
        denise.set_palette(17, 0x0F0);

        let (pos_a, ctl) = encode_sprite_pos_ctl(40, 12, 13);
        let (pos_b, _) = encode_sprite_pos_ctl(42, 12, 13);
        denise.write_sprite_pos(0, pos_a);
        denise.write_sprite_ctl(0, ctl);
        denise.write_sprite_datb(0, 0x0000);
        denise.write_sprite_data(0, 0x8000); // arm

        denise.write_sprite_pos(0, pos_b); // move while armed

        denise.output_pixel(40, 12);
        denise.output_pixel(42, 12);

        assert_eq!(
            denise.framebuffer[(12 * FB_WIDTH + 40) as usize],
            DeniseOcs::rgb12_to_argb32(0x000),
            "sprite should no longer appear at the old horizontal position"
        );
        assert_eq!(
            denise.framebuffer[(12 * FB_WIDTH + 42) as usize],
            DeniseOcs::rgb12_to_argb32(0x0F0),
            "writing SPRxPOS should move an armed sprite horizontally"
        );
    }

    #[test]
    fn mid_line_sprite_data_write_affects_next_line_not_current_line() {
        let mut denise = DeniseOcs::new();
        denise.set_palette(0, 0x000);
        denise.set_palette(17, 0xF00);

        let (pos, ctl) = encode_sprite_pos_ctl(20, 10, 12); // active on lines 10 and 11
        denise.write_sprite_pos(0, pos);
        denise.write_sprite_ctl(0, ctl);
        denise.write_sprite_datb(0, 0x0000);
        denise.write_sprite_data(0, 0xC000); // first two pixels set

        // First pixel of line 10 loads and begins shifting.
        denise.output_pixel(20, 10);
        assert_eq!(
            denise.framebuffer[(10 * FB_WIDTH + 20) as usize],
            DeniseOcs::rgb12_to_argb32(0xF00)
        );

        // Mid-line data rewrite should not affect the already-loaded serial data
        // for this line, but should be visible on the next line.
        denise.write_sprite_data(0, 0x0000);
        denise.output_pixel(21, 10);
        assert_eq!(
            denise.framebuffer[(10 * FB_WIDTH + 21) as usize],
            DeniseOcs::rgb12_to_argb32(0xF00),
            "mid-line SPRxDATA write must not alter the current line after load"
        );

        denise.output_pixel(20, 11);
        assert_eq!(
            denise.framebuffer[(11 * FB_WIDTH + 20) as usize],
            DeniseOcs::rgb12_to_argb32(0x000),
            "next line should use the newly written sprite data"
        );
    }

    #[test]
    fn mid_line_sprite_pos_write_before_hstart_moves_same_line_trigger() {
        let mut denise = DeniseOcs::new();
        denise.set_palette(0, 0x000);
        denise.set_palette(17, 0x0FF);

        let (pos_a, ctl) = encode_sprite_pos_ctl(26, 9, 10);
        let (pos_b, _) = encode_sprite_pos_ctl(24, 9, 10);
        denise.write_sprite_pos(0, pos_a);
        denise.write_sprite_ctl(0, ctl);
        denise.write_sprite_datb(0, 0x0000);
        denise.write_sprite_data(0, 0x8000);

        denise.output_pixel(23, 9); // before either HSTART
        denise.write_sprite_pos(0, pos_b); // move before comparator hit
        denise.output_pixel(24, 9);
        denise.output_pixel(26, 9);

        assert_eq!(
            denise.framebuffer[(9 * FB_WIDTH + 24) as usize],
            DeniseOcs::rgb12_to_argb32(0x0FF),
            "SPRxPOS write before HSTART should affect the current line comparator hit"
        );
        assert_eq!(
            denise.framebuffer[(9 * FB_WIDTH + 26) as usize],
            DeniseOcs::rgb12_to_argb32(0x000),
            "sprite should not also trigger again at the old HSTART"
        );
    }

    #[test]
    fn spritedata_rearm_after_hstart_waits_until_next_line() {
        let mut denise = DeniseOcs::new();
        denise.set_palette(0, 0x000);
        denise.set_palette(17, 0xF0F);

        let (pos, ctl) = encode_sprite_pos_ctl(28, 11, 13); // active on lines 11 and 12
        denise.write_sprite_pos(0, pos);
        denise.write_sprite_ctl(0, ctl); // disarm
        denise.write_sprite_datb(0, 0x0000);

        denise.output_pixel(29, 11); // HSTART already passed on line 11
        denise.write_sprite_data(0, 0x8000); // arm after HSTART
        denise.output_pixel(30, 11);
        assert_eq!(
            denise.framebuffer[(11 * FB_WIDTH + 30) as usize],
            DeniseOcs::rgb12_to_argb32(0x000),
            "arming after HSTART should wait for the next line's comparison"
        );

        denise.output_pixel(28, 12);
        assert_eq!(
            denise.framebuffer[(12 * FB_WIDTH + 28) as usize],
            DeniseOcs::rgb12_to_argb32(0xF0F),
            "next line should trigger output after late-line SPRxDATA arm"
        );
    }

    #[test]
    fn clxdat_follows_loaded_sprite_serial_data_under_mid_line_data_write() {
        let mut denise = DeniseOcs::new();
        let (pos, ctl) = encode_sprite_pos_ctl(20, 10, 12); // active on lines 10 and 11
        denise.write_sprite_pos(0, pos);
        denise.write_sprite_ctl(0, ctl);
        denise.write_sprite_datb(0, 0x0000);
        denise.write_sprite_data(0, 0xC000); // two sprite pixels on each active line

        // First pixel on line 10 collides with odd bitplane.
        denise.bpl_shift[0] = 0x8000;
        denise.shift_count = 1;
        denise.output_pixel(20, 10);
        assert_eq!(denise.read_clxdat() & (1 << 1), 1 << 1);

        // Mid-line data rewrite should not affect the already-loaded serial data
        // for line 10, so the second pixel still collides.
        denise.write_sprite_data(0, 0x0000);
        denise.bpl_shift[0] = 0x8000;
        denise.shift_count = 1;
        denise.output_pixel(21, 10);
        assert_eq!(denise.read_clxdat() & (1 << 1), 1 << 1);

        // Next line uses the rewritten data, so no collision occurs.
        denise.bpl_shift[0] = 0x8000;
        denise.shift_count = 1;
        denise.output_pixel(20, 11);
        assert_eq!(denise.read_clxdat() & (1 << 1), 0);
    }

    #[test]
    fn clxdat_stops_latching_after_mid_line_ctl_disarm() {
        let mut denise = DeniseOcs::new();
        let (pos, ctl) = encode_sprite_pos_ctl(24, 8, 9);
        denise.write_sprite_pos(0, pos);
        denise.write_sprite_ctl(0, ctl);
        denise.write_sprite_datb(0, 0x0000);
        denise.write_sprite_data(0, 0xC000); // two sprite pixels

        denise.bpl_shift[0] = 0x8000;
        denise.shift_count = 1;
        denise.output_pixel(24, 8);
        assert_eq!(denise.read_clxdat() & (1 << 1), 1 << 1);

        // Disarm mid-line before the second sprite pixel.
        denise.write_sprite_ctl(0, ctl);
        denise.bpl_shift[0] = 0x8000;
        denise.shift_count = 1;
        denise.output_pixel(25, 8);
        assert_eq!(
            denise.read_clxdat() & (1 << 1),
            0,
            "SPRxCTL disarm should stop further same-line sprite collisions"
        );
    }

    #[test]
    fn clxdat_pos_write_before_hstart_moves_same_line_collision_point() {
        let mut denise = DeniseOcs::new();
        let (pos_a, ctl) = encode_sprite_pos_ctl(26, 9, 10);
        let (pos_b, _) = encode_sprite_pos_ctl(24, 9, 10);
        denise.write_sprite_pos(0, pos_a);
        denise.write_sprite_ctl(0, ctl);
        denise.write_sprite_datb(0, 0x0000);
        denise.write_sprite_data(0, 0x8000);

        denise.output_pixel(23, 9); // establish runtime before comparator hit
        let _ = denise.read_clxdat();

        denise.write_sprite_pos(0, pos_b); // move before HSTART

        denise.bpl_shift[0] = 0x8000;
        denise.shift_count = 1;
        denise.output_pixel(24, 9);
        assert_eq!(denise.read_clxdat() & (1 << 1), 1 << 1);

        denise.bpl_shift[0] = 0x8000;
        denise.shift_count = 1;
        denise.output_pixel(26, 9);
        assert_eq!(
            denise.read_clxdat() & (1 << 1),
            0,
            "collision should not also occur at the old HSTART after a pre-hit SPRxPOS move"
        );
    }

    #[test]
    fn clxdat_arm_after_hstart_waits_until_next_line() {
        let mut denise = DeniseOcs::new();
        let (pos, ctl) = encode_sprite_pos_ctl(28, 11, 13); // active on lines 11 and 12
        denise.write_sprite_pos(0, pos);
        denise.write_sprite_ctl(0, ctl); // disarm
        denise.write_sprite_datb(0, 0x0000);

        denise.output_pixel(29, 11); // HSTART has passed on line 11
        denise.write_sprite_data(0, 0x8000); // arm late

        denise.bpl_shift[0] = 0x8000;
        denise.shift_count = 1;
        denise.output_pixel(30, 11);
        assert_eq!(
            denise.read_clxdat() & (1 << 1),
            0,
            "late-line arm must not cause a same-line collision after HSTART has passed"
        );

        denise.bpl_shift[0] = 0x8000;
        denise.shift_count = 1;
        denise.output_pixel(28, 12);
        assert_eq!(
            denise.read_clxdat() & (1 << 1),
            1 << 1,
            "next line should latch collision after late-line SPRxDATA arm"
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
    fn misaligned_attached_pair_reverts_to_shifted_color_subsets() {
        let mut denise = DeniseOcs::new();
        denise.set_palette(0, 0x000);
        denise.set_palette(17, 0xF00); // even-only attached fallback color (code 0001)
        denise.set_palette(20, 0x0F0); // odd-only attached fallback color (code 0100)

        let (pos0, ctl0) = encode_sprite_pos_ctl(40, 10, 11);
        denise.spr_pos[0] = pos0;
        denise.spr_ctl[0] = ctl0;
        denise.spr_data[0] = 0x8000; // pixel at x=40 only
        denise.spr_datb[0] = 0x0000;

        let (pos1, ctl1) = encode_sprite_pos_ctl(41, 10, 11); // shifted right by 1 pixel
        denise.spr_pos[1] = pos1;
        denise.spr_ctl[1] = ctl1 | 0x0080; // ATTACH on odd sprite
        denise.spr_data[1] = 0x8000; // odd-only pixel at x=41
        denise.spr_datb[1] = 0x0000;

        denise.output_pixel(40, 10);
        denise.output_pixel(41, 10);

        assert_eq!(
            denise.framebuffer[(10 * FB_WIDTH + 40) as usize],
            DeniseOcs::rgb12_to_argb32(0xF00),
            "even-only pixel in misaligned attached pair should use COLOR17..19 subset"
        );
        assert_eq!(
            denise.framebuffer[(10 * FB_WIDTH + 41) as usize],
            DeniseOcs::rgb12_to_argb32(0x0F0),
            "odd-only pixel in misaligned attached pair should use shifted COLOR20/24/28 subset"
        );
    }

    #[test]
    fn attach_bit_on_even_sprite_is_ignored() {
        let mut denise = DeniseOcs::new();
        denise.set_palette(0, 0x000);
        denise.set_palette(17, 0xF00); // would appear if sprite 2 were incorrectly treated as attached
        denise.set_palette(21, 0x00F); // normal sprite-2 color code 1 (group 1 base)

        let (pos, ctl) = encode_sprite_pos_ctl(44, 12, 13);
        denise.spr_pos[2] = pos;
        denise.spr_ctl[2] = ctl | 0x0080; // ATTACH bit on even sprite must be ignored
        denise.spr_data[2] = 0x8000;
        denise.spr_datb[2] = 0x0000;

        denise.output_pixel(44, 12);

        assert_eq!(
            denise.framebuffer[(12 * FB_WIDTH + 44) as usize],
            DeniseOcs::rgb12_to_argb32(0x00F),
            "ATTACH is only valid on odd sprites; even sprite 2 should render as normal group-1 sprite"
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
