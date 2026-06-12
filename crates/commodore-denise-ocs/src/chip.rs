//! `DeniseOcs` — the OCS Denise chip core.
//!
//! Owns the bitplane shifter, sprite engine, palette resolution, and the
//! raster framebuffer. The two impl blocks below are split for readability
//! only: the first carries every register-write / shift / output entry
//! point, while the second carries the viewport-extraction bridge that
//! ties [`crate::viewport`] back to this chip's framebuffer.

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::PAL_RASTER_FB_HEIGHT;
use crate::RASTER_FB_WIDTH;
use crate::debug::{
    DeniseOutputPixelDebug, DeniseShiftLoadDebug, DeniseShiftLoadPlaneDebug, DeniseSourcePixelDebug,
};
use crate::viewport::{ViewportImage, ViewportPreset};

#[derive(Clone, Serialize, Deserialize)]
pub struct DeniseOcs {
    pub palette: [u16; 32],
    /// AGA 256-entry 24-bit palette (0x00RRGGBB).
    #[serde(with = "BigArray")]
    pub palette_24: [u32; 256],
    /// Full-raster framebuffer at superhires resolution, double-height for interlace.
    /// Indexed as `[vpos * 2 + field_row] * RASTER_FB_WIDTH + hpos * 8 + sub`.
    pub framebuffer_raster: Vec<u32>,
    pub raster_fb_width: u32,
    pub raster_fb_height: u32,
    /// Whether interlace mode (BPLCON0 LACE) is active.
    pub interlace_active: bool,
    /// Long frame flag — toggles each frame when interlace is active.
    pub lof: bool,
    /// Maximum bitplane count: 6 for OCS/ECS, 8 for AGA.
    ///
    /// Controls whether BPLCON0 bit 4 extends the BPU field to 4 bits (8 planes).
    /// Set at construction time by the outermost chipset wrapper.
    pub max_bitplanes: u8,
    pub bpl_data: [u16; 8],  // Holding latches: written by DMA
    pub bpl_shift: [u16; 8], // Shift registers: loaded from latches on BPL1DAT write
    pub shift_count: u8,     // Pixels remaining in shift register (0 -> output COLOR00)
    bpl_shift_count: [u8; 8],
    bpl_shift_delay: [u8; 8],
    bpl_prev_data: [u16; 8],
    bpl_pending_data: [u16; 8],
    // Pending parallel-load flags for odd/even numbered bitplanes (BPL1/3/5 and BPL2/4/6).
    bpl_pending_copy_odd_planes: bool,
    bpl_pending_copy_even_planes: bool,
    bpl_scroll_pending_line: bool,
    pub bplcon0: u16,
    pub bplcon1: u16,
    pub bplcon2: u16,
    pub bplcon4: u16,
    pub clxcon: u16,
    pub clxdat: u16,
    pub spr_pos: [u16; 8],
    /// Shadow sprite position for the display comparator: trails spr_pos
    /// by one pixel step to model hardware pipeline delay.
    spr_pos_display: [u16; 8],
    /// Flags indicating a position write happened since the last pixel step.
    spr_pos_dirty: [bool; 8],
    pub spr_ctl: [u16; 8],
    pub spr_data: [u64; 8],
    pub spr_datb: [u64; 8],
    pub spr_armed: [bool; 8],
    spr_shift_data: [u64; 8],
    spr_shift_datb: [u64; 8],
    spr_shift_count: [u8; 8],
    /// Sprite pixel width: 16 (OCS/ECS), 32, or 64 (AGA via FMODE bits 3-2).
    pub spr_width: u8,
    spr_current_code: [u8; 8],
    /// Cumulative count of output pixels each sprite has contributed to
    /// the composited display (a sprite group rendered a non-transparent
    /// pixel inside the display window). A query/diagnostic surface for
    /// confirming a sprite is actually drawing — see `sprite_pixels_rendered`.
    spr_pixels_rendered: [u64; 8],
    sprite_runtime_line_valid: bool,
    sprite_runtime_beam_x: u32,
    sprite_runtime_beam_y: u32,
    last_shift_load_debug: DeniseShiftLoadDebug,
    deferred_shift_load_after_source_pixels: Option<u8>,
    /// HAM mode: previous pixel's 12-bit RGB (for hold-and-modify).
    /// Reset to COLOR00 at the start of each scanline.
    ham_prev_rgb: u16,
    /// HAM8 mode: previous pixel's 24-bit RGB (0x00RRGGBB).
    /// Reset to palette_24[0] at the start of each scanline.
    pub ham_prev_rgb24: u32,
    /// AGA bitplane FIFO for wider FMODE fetches — the tail words of the
    /// *currently displayed* group. A 64-bit fetch is one word in the
    /// shift register plus up to three here, popped as the shift register
    /// drains. `bpl_fifo_len` tracks fill level.
    bpl_fifo: [[u16; 4]; 8],
    bpl_fifo_len: [u8; 8],
    /// Staging latch for a wide fetch's tail words (1..=3). Filled at
    /// fetch time by `push_bpl_fifo`, then moved into `bpl_fifo` atomically
    /// when the group's first word commits to the shift register. This
    /// keeps the FIFO synced to the shift-load: the next group's fetch
    /// overlaps the current group's *display*, so writing the live FIFO at
    /// fetch time would corrupt the still-draining group (whole-word colour
    /// errors). Staging defers the handoff to the commit boundary.
    bpl_fetch_tail: [[u16; 3]; 8],
    bpl_fetch_tail_len: [u8; 8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SpritePixel {
    palette_idx: usize,
    sprite_group: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum PlayfieldId {
    Pf1,
    Pf2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PlayfieldPixel {
    visible_color_idx: usize,
    front_playfield: Option<PlayfieldId>,
}

impl DeniseOcs {
    pub fn num_bitplanes(&self) -> usize {
        if self.max_bitplanes > 6 {
            // AGA: 4-bit BPU from bits 14-12 (3 bits) + bit 4 (extra high bit).
            let bpu_hi3 = ((self.bplcon0 >> 12) & 0x7) as usize;
            let bpu_bit3 = ((self.bplcon0 >> 4) & 0x1) as usize;
            let bpu = (bpu_bit3 << 3) | bpu_hi3;
            bpu.min(self.max_bitplanes as usize)
        } else {
            (((self.bplcon0 >> 12) & 0x7) as usize).min(6)
        }
    }

    /// Create a new Denise with PAL raster dimensions (default).
    pub fn new() -> Self {
        Self::new_with_raster_height(PAL_RASTER_FB_HEIGHT)
    }

    /// Create a new Denise with explicit raster buffer height.
    pub fn new_with_raster_height(raster_fb_height: u32) -> Self {
        Self::new_internal(raster_fb_height)
    }

    fn new_internal(raster_fb_height: u32) -> Self {
        Self {
            palette: [0; 32],
            palette_24: [0; 256],
            framebuffer_raster: vec![0xFF000000; (RASTER_FB_WIDTH * raster_fb_height) as usize],
            raster_fb_width: RASTER_FB_WIDTH,
            raster_fb_height,
            interlace_active: false,
            lof: true,
            max_bitplanes: 6,
            bpl_data: [0; 8],
            bpl_shift: [0; 8],
            shift_count: 0,
            bpl_shift_count: [0; 8],
            bpl_shift_delay: [0; 8],
            bpl_prev_data: [0; 8],
            bpl_pending_data: [0; 8],
            bpl_pending_copy_odd_planes: false,
            bpl_pending_copy_even_planes: false,
            bpl_scroll_pending_line: true,
            bplcon0: 0,
            bplcon1: 0,
            bplcon2: 0,
            bplcon4: 0,
            clxcon: 0,
            clxdat: 0,
            spr_pos: [0; 8],
            spr_pos_display: [0; 8],
            spr_pos_dirty: [false; 8],
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
            spr_width: 16,
            spr_current_code: [0; 8],
            spr_pixels_rendered: [0; 8],
            sprite_runtime_line_valid: false,
            sprite_runtime_beam_x: 0,
            sprite_runtime_beam_y: 0,
            last_shift_load_debug: DeniseShiftLoadDebug::default(),
            deferred_shift_load_after_source_pixels: None,
            ham_prev_rgb: 0,
            ham_prev_rgb24: 0,
            bpl_fifo: [[0; 4]; 8],
            bpl_fifo_len: [0; 8],
            bpl_fetch_tail: [[0; 3]; 8],
            bpl_fetch_tail_len: [0; 8],
        }
    }

    pub fn set_palette(&mut self, idx: usize, val: u16) {
        if idx < 32 {
            self.palette[idx] = val & 0x0FFF;
        }
    }

    /// CPU / copper write to a Denise-owned custom register.
    ///
    /// Unified dispatcher used by the live machine's custom-register
    /// bus and by Copper MOVEs. Handles the full Denise address slice:
    ///
    /// | Offset      | Register                                          |
    /// | ----------- | ------------------------------------------------- |
    /// | `$100`      | BPLCON0 (mirrored — Agnus owns the primary copy)  |
    /// | `$102`      | BPLCON1 (fine scroll, dual-playfield)             |
    /// | `$104`      | BPLCON2 (sprite/playfield priority)               |
    /// | `$098`      | CLXCON (collision match/enable mask)              |
    /// | `$10C`      | BPLCON4 (AGA sprite XOR — ignored on OCS)         |
    /// | `$110..$11C`| BPL1DAT..BPL6DAT (shift-load triggers on BPL1DAT) |
    /// | `$140..$17C`| SPRxPOS / SPRxCTL / SPRxDATA / SPRxDATB × 8       |
    /// | `$180..$1BE`| COLOR00..COLOR31                                  |
    ///
    /// Anything else is silently ignored; machine dispatch routes
    /// non-Denise registers elsewhere before reaching here.
    pub fn write_word(&mut self, offset: u16, val: u16) {
        match offset {
            0x098 => self.clxcon = val,
            0x100 => self.bplcon0 = val,
            0x102 => self.bplcon1 = val,
            0x104 => self.bplcon2 = val,
            0x10C => self.bplcon4 = val,
            0x110..=0x11C => {
                let plane = ((offset - 0x110) / 2) as usize;
                self.load_bitplane(plane, val);
                // BPL1DAT (plane 0) is always the last plane fetched
                // in each 8-CCK DMA group. Its arrival queues the
                // parallel shift-load that BPLCON1 will time via its
                // comparator.
                if plane == 0 {
                    self.queue_shift_load_from_bpl1dat();
                }
            }
            0x140..=0x17C => {
                let sprite = ((offset - 0x140) / 8) as usize;
                match (offset - 0x140) % 8 {
                    0 => self.write_sprite_pos(sprite, val),
                    2 => self.write_sprite_ctl(sprite, val),
                    4 => self.write_sprite_data(sprite, val),
                    6 => self.write_sprite_datb(sprite, val),
                    _ => {}
                }
            }
            0x180..=0x1BE => {
                let idx = ((offset - 0x180) / 2) as usize;
                self.set_palette(idx, val);
            }
            _ => {}
        }
    }

    /// Convert 12-bit RGB (Amiga OCS) to 24-bit RGB (0x00RRGGBB).
    #[must_use]
    pub fn rgb12_to_rgb24(rgb12: u16) -> u32 {
        let r = ((rgb12 >> 8) & 0xF) as u32;
        let g = ((rgb12 >> 4) & 0xF) as u32;
        let b = (rgb12 & 0xF) as u32;
        ((r << 4 | r) << 16) | ((g << 4 | g) << 8) | (b << 4 | b)
    }

    /// Convert 24-bit RGB (0x00RRGGBB) to ARGB32 (0xFFRRGGBB).
    #[must_use]
    pub fn rgb24_to_argb32(rgb24: u32) -> u32 {
        0xFF000000 | rgb24
    }

    pub fn load_bitplane(&mut self, idx: usize, val: u16) {
        if idx < 8 {
            self.bpl_data[idx] = val;
        }
    }

    /// Stage a tail word from a wide (FMODE > 0) bitplane fetch.
    ///
    /// The words are held in `bpl_fetch_tail` until the group's first word
    /// commits to the shift register (`load_fifo_tail`), then moved into
    /// `bpl_fifo` to be popped as the shift register drains. Staging — not
    /// writing `bpl_fifo` directly — is what keeps the FIFO synced to the
    /// shift-load: the next group's fetch overlaps this group's display, so
    /// a direct write would corrupt the still-draining group. Only the
    /// 1..=3 words after word 0 are queued, so the cap is 3.
    pub fn push_bpl_fifo(&mut self, idx: usize, val: u16) {
        if idx < 8 {
            let len = self.bpl_fetch_tail_len[idx] as usize;
            if len < 3 {
                self.bpl_fetch_tail[idx][len] = val;
                self.bpl_fetch_tail_len[idx] += 1;
            }
        }
    }

    /// Move a plane's staged wide-fetch tail into the active FIFO, replacing
    /// any prior contents, and clear the staging. Called when the group's
    /// first word commits to the shift register so the FIFO holds exactly
    /// the tail of the group now being displayed. A no-op for 16-bit
    /// fetches (staging stays empty → FIFO cleared), keeping OCS / ECS
    /// byte-identical.
    fn load_fifo_tail(&mut self, plane: usize) {
        if plane >= 8 {
            return;
        }
        let len = self.bpl_fetch_tail_len[plane];
        for i in 0..len as usize {
            self.bpl_fifo[plane][i] = self.bpl_fetch_tail[plane][i];
        }
        self.bpl_fifo_len[plane] = len;
        self.bpl_fetch_tail_len[plane] = 0;
    }

    /// Pop one word from the AGA bitplane FIFO, if available.
    fn pop_bpl_fifo(&mut self, idx: usize) -> Option<u16> {
        if idx >= 8 || self.bpl_fifo_len[idx] == 0 {
            return None;
        }
        let val = self.bpl_fifo[idx][0];
        let len = self.bpl_fifo_len[idx] as usize;
        for i in 1..len {
            self.bpl_fifo[idx][i - 1] = self.bpl_fifo[idx][i];
        }
        self.bpl_fifo_len[idx] -= 1;
        Some(val)
    }

    pub fn read_clxdat(&mut self) -> u16 {
        let value = self.clxdat;
        self.clxdat = 0;
        value
    }

    /// Non-destructive CLXDAT read for the debug / inspection bus
    /// (`&self`). The real CPU read clears on read via [`Self::read_clxdat`];
    /// a `memory_read` of `$DFF00E` must not destroy collision state.
    #[must_use]
    pub fn peek_clxdat(&self) -> u16 {
        self.clxdat
    }

    pub fn write_sprite_pos(&mut self, sprite: usize, val: u16) {
        if sprite < 8 {
            // Register updates immediately for Agnus DMA control (vstart/vstop).
            self.spr_pos[sprite] = val;
            // Display comparator sees the new value one pixel later.
            self.spr_pos_dirty[sprite] = true;
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
            self.spr_data[sprite] = u64::from(val);
            // Writing SPRxDATA arms the sprite comparator (manual mode) and is
            // also how DMA refreshes sprite line data before display.
            self.spr_armed[sprite] = true;
        }
    }

    pub fn write_sprite_datb(&mut self, sprite: usize, val: u16) {
        if sprite < 8 {
            self.spr_datb[sprite] = u64::from(val);
        }
    }

    /// Cumulative count of composited pixels a sprite has contributed to
    /// the display. Non-zero means the sprite is actually drawing — a
    /// diagnostic/query surface (used to confirm DMA-driven sprites
    /// render). Returns 0 for out-of-range indices.
    #[must_use]
    pub fn sprite_pixels_rendered(&self, sprite: usize) -> u64 {
        self.spr_pixels_rendered.get(sprite).copied().unwrap_or(0)
    }

    /// Reset per-line state for bitplane shift-load timing.
    ///
    /// Clears `bpl_prev_data` so the BPLCON1 barrel-shift carry does not
    /// leak across scanlines. Sets `bpl_scroll_pending_line` for the
    /// legacy `trigger_shift_load()` path used by unit tests.
    pub fn begin_beam_line(&mut self) {
        self.bpl_scroll_pending_line = true;
        self.bpl_prev_data = [0; 8];
        self.ham_prev_rgb = self.palette[0];
        self.ham_prev_rgb24 = self.palette_24[0];
        // Drop any wide-fetch words left over from the previous line so
        // they cannot leak into this one.
        self.bpl_fifo_len = [0; 8];
        self.bpl_fetch_tail_len = [0; 8];
    }

    /// Write a pixel to the full-raster framebuffer.
    ///
    /// Coordinates map directly from beam position:
    /// - `hpos`: Agnus horizontal position (CCK, 0..226)
    /// - `vpos`: Agnus vertical position (line, 0..311 PAL)
    /// - `sub`: sub-CCK hires pixel offset (0..3)
    /// - `argb32`: pre-composed ARGB32 color
    ///
    /// Non-interlaced mode writes the same pixel to both rows of the
    /// double-height pair. Interlaced mode writes to one row per field.
    pub fn write_raster_pixel(&mut self, hpos: u16, vpos: u16, sub: u8, argb32: u32) {
        let fb_x = u32::from(hpos) * 8 + u32::from(sub);
        if fb_x >= self.raster_fb_width {
            return;
        }
        let row_base = u32::from(vpos) * 2;
        if self.interlace_active {
            let fb_y = if self.lof { row_base } else { row_base + 1 };
            if fb_y >= self.raster_fb_height {
                return;
            }
            let idx = (fb_y * self.raster_fb_width + fb_x) as usize;
            if idx < self.framebuffer_raster.len() {
                self.framebuffer_raster[idx] = argb32;
            }
        } else {
            // Non-interlaced: write both rows of the double-height pair.
            for offset in 0..2u32 {
                let fb_y = row_base + offset;
                if fb_y >= self.raster_fb_height {
                    break;
                }
                let idx = (fb_y * self.raster_fb_width + fb_x) as usize;
                if idx < self.framebuffer_raster.len() {
                    self.framebuffer_raster[idx] = argb32;
                }
            }
        }
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
        // Latch current positions into the display shadow at line start.
        self.spr_pos_display = self.spr_pos;
        self.spr_pos_dirty = [false; 8];
        self.sprite_runtime_line_valid = true;
        self.sprite_runtime_beam_x = 0;
        self.sprite_runtime_beam_y = beam_y;
    }

    fn step_sprite_runtime_one_pixel(&mut self, beam_x: u32, beam_y: u32) {
        // Propagate position writes with a 1-pixel pipeline delay:
        // the display comparator uses spr_pos_display which trails spr_pos.
        for sprite in 0..8usize {
            if self.spr_pos_dirty[sprite] {
                self.spr_pos_display[sprite] = self.spr_pos[sprite];
                self.spr_pos_dirty[sprite] = false;
            }
        }

        // OCS sprites run at lores resolution: one sprite pixel per lores
        // pixel. `beam_x` is in lores units (one step per lores pixel, two
        // per colour clock), so the serial shifter advances on *every*
        // beam_x step. HSTART (SPRxPOS H8-H1 + SPRxCTL H0) is the lores
        // position the comparator matches against. An earlier model shifted
        // only once per two beam_x steps ("one pixel per CCK"), which made
        // every sprite pixel two lores pixels wide — invisible until the
        // first DMA-driven sprite (the Workbench pointer) rendered and
        // showed up double-width. gap #162.
        self.spr_current_code = [0; 8];
        for sprite in 0..8usize {
            if !self.spr_armed[sprite] {
                self.spr_shift_count[sprite] = 0;
                continue;
            }

            let pos = self.spr_pos_display[sprite];
            let ctl = self.spr_ctl[sprite];
            let hstart = u32::from(Self::sprite_hstart(pos, ctl));
            let vstart = u32::from(Self::sprite_vstart(pos, ctl));
            let vstop = u32::from(Self::sprite_vstop(pos, ctl));

            // Comparator: detect the horizontal match at lores resolution.
            let load_pulse = Self::sprite_line_active(beam_y, vstart, vstop) && beam_x == hstart;

            // Load phase: copy sprite data regs into the serial shifters.
            if load_pulse {
                self.spr_shift_data[sprite] = self.spr_data[sprite];
                self.spr_shift_datb[sprite] = self.spr_datb[sprite];
                self.spr_shift_count[sprite] = self.spr_width;
            }

            if self.spr_shift_count[sprite] == 0 {
                // Shifter exhausted: emit nothing. No code clear is needed
                // here — `spr_current_code` was already zeroed for every
                // sprite at the top of this step (the `= [0; 8]` above), so an
                // exhausted sprite contributes no collision code for the rest
                // of the line. (This is why the "stale right-edge collision
                // trail" of issue #459 cannot occur; locked in by the
                // `exhausted_sprite_leaves_no_collision_trail*` regression.)
                continue;
            }

            // Output the current MSB, then advance one lores pixel.
            let msb = u32::from(self.spr_width) - 1;
            let lo = (self.spr_shift_data[sprite] >> msb) & 1;
            let hi = (self.spr_shift_datb[sprite] >> msb) & 1;
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
                // BPLCON4 ESPRM (bits 3-0) XOR against upper nybble of the
                // colour index for even sprites (attached pair uses even base).
                let esprm = (self.bplcon4 & 0x0F) as usize;
                let idx = (16 + code) ^ (esprm << 4);
                return Some(SpritePixel {
                    palette_idx: idx,
                    sprite_group: pair / 2,
                });
            }

            let code = self.spr_current_code[sprite];
            if code == 0 {
                continue;
            }
            let base = 16 + (sprite / 2) * 4;
            // BPLCON4 OSPRM (bits 7-4) for odd sprites, ESPRM (bits 3-0) for
            // even sprites. Each XORs against the upper nybble (bank select)
            // of the sprite colour index.
            let sprm = if sprite & 1 == 1 {
                ((self.bplcon4 >> 4) & 0x0F) as usize
            } else {
                (self.bplcon4 & 0x0F) as usize
            };
            let idx = (base + usize::from(code)) ^ (sprm << 4);
            return Some(SpritePixel {
                palette_idx: idx,
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

    fn compose_playfield_pixel(
        &self,
        raw_color_idx: usize,
        pf1_code: u8,
        pf2_code: u8,
    ) -> PlayfieldPixel {
        let dual_playfield = (self.bplcon0 & 0x0400) != 0; // DBLPF
        let mut pf = if !dual_playfield {
            PlayfieldPixel {
                visible_color_idx: raw_color_idx,
                front_playfield: if raw_color_idx != 0 {
                    Some(PlayfieldId::Pf1)
                } else {
                    None
                },
            }
        } else {
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
        };

        // BPLCON4 BPLAM (bits 15-8): XOR the playfield colour index just
        // before the palette lookup, matching WinUAE
        // (`pix ^ bplcon4_denise_xor_val`). AGA-only — `bplcon4` is 0 on
        // OCS/ECS, so this is a no-op there. It affects the *colour* only:
        // playfield priority (`front_playfield`) and collision both key off
        // the raw plane bits, not this index (#96).
        pf.visible_color_idx ^= ((self.bplcon4 >> 8) & 0xFF) as usize;
        pf
    }

    /// Copy all bitplane holding latches into the shift registers.
    /// On real hardware this happens when BPL1DAT (plane 0) is written,
    /// which is always the last plane fetched in each 8-CCK DMA group.
    pub fn trigger_shift_load(&mut self) {
        self.deferred_shift_load_after_source_pixels = None;
        self.bpl_pending_copy_odd_planes = false;
        self.bpl_pending_copy_even_planes = false;

        // BPLCON1 fine-scroll is a continuous barrel shift across fetched
        // bitplane words. Model this by combining the previous and current
        // DMA words per plane when loading the serial shift registers.
        let hires = (self.bplcon0 & 0x8000) != 0;
        // BPLCON1 scroll is implemented as a barrel-shift across consecutive
        // BPL DMA words. The combined (prev << 16 | raw) >> scroll window
        // works identically for lowres and hires — only the scroll value
        // range differs (lowres 0-15, hires 0-14 even).
        let mut odd_scroll = ((self.bplcon1 >> 4) & 0x000F) as u8;
        let mut even_scroll = (self.bplcon1 & 0x000F) as u8;
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
        for i in 0..8 {
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
            let combined = (u32::from(prev) << 16) | u32::from(raw);
            self.bpl_shift[i] = if scroll == 0 {
                raw
            } else {
                (combined >> scroll) as u16
            };
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
            self.bpl_shift_delay[i] = 0;
            self.bpl_prev_data[i] = raw;
            // Hand this group's wide-fetch tail to the FIFO in lockstep
            // with the word-0 load (no-op for 16-bit fetches).
            self.load_fifo_tail(i);
        }
        self.last_shift_load_debug = shift_dbg;
        self.shift_count = 16;
    }

    fn bplcon1_scrolls_for_current_mode(&self) -> (u8, u8, bool) {
        let hires = (self.bplcon0 & 0x8000) != 0;
        let mut odd_scroll = ((self.bplcon1 >> 4) & 0x000F) as u8;
        let mut even_scroll = (self.bplcon1 & 0x000F) as u8;
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
            // Sync this plane's wide-fetch tail to the committed word 0.
            self.load_fifo_tail(plane);
        }
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
        let phase = (phase_counter as u8) & phase_mask;

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

    pub fn rgb12_to_argb32(rgb12: u16) -> u32 {
        let r = ((rgb12 >> 8) & 0xF) as u8;
        let g = ((rgb12 >> 4) & 0xF) as u8;
        let b = (rgb12 & 0xF) as u8;
        let r8 = (r << 4) | r;
        let g8 = (g << 4) | g;
        let b8 = (b << 4) | b;
        0xFF000000 | (u32::from(r8) << 16) | (u32::from(g8) << 8) | u32::from(b8)
    }

    /// Resolve a playfield colour index to 12-bit RGB, accounting for the
    /// current display mode (normal, EHB, or HAM).
    ///
    /// - **Normal** (≤5 planes, or DBLPF): index 0-31 → palette lookup.
    /// - **EHB** (6 planes, no HAM, no DBLPF): index 0-31 from palette;
    ///   index 32-63 halves the RGB of the base colour (index & 31).
    /// - **HAM** (HOMOD set, 6 planes, no DBLPF): bits 5-4 select mode:
    ///   00 = palette[bits 3-0], 01 = modify blue, 10 = modify red,
    ///   11 = modify green of the previous pixel's colour.
    ///
    /// Updates `ham_prev_rgb` for HAM mode continuity across pixels.
    pub fn resolve_color_rgb12(&mut self, color_idx: u8) -> u16 {
        let ham = (self.bplcon0 & 0x0800) != 0;
        let dual_playfield = (self.bplcon0 & 0x0400) != 0;
        let num_planes = self.num_bitplanes();

        if ham && !dual_playfield && num_planes >= 5 {
            // HAM mode: 6-bit value, top 2 bits = control
            let control = (color_idx >> 4) & 0x03;
            let data = u16::from(color_idx & 0x0F);
            let rgb = match control {
                0b00 => self.palette[data as usize],
                0b01 => (self.ham_prev_rgb & 0xFF0) | data, // Modify blue
                0b10 => (self.ham_prev_rgb & 0x0FF) | (data << 8), // Modify red
                0b11 => (self.ham_prev_rgb & 0xF0F) | (data << 4), // Modify green
                _ => unreachable!(),
            };
            self.ham_prev_rgb = rgb;
            rgb
        } else if !ham && !dual_playfield && num_planes == 6 {
            // EHB mode: 6-bit index, bit 5 = half-brite flag
            let base_idx = (color_idx & 0x1F) as usize;
            if color_idx & 0x20 != 0 {
                // Half-brite: halve each RGB nibble
                let base = self.palette[base_idx];
                let r = ((base >> 8) & 0xF) >> 1;
                let g = ((base >> 4) & 0xF) >> 1;
                let b = (base & 0xF) >> 1;
                (r << 8) | (g << 4) | b
            } else {
                self.palette[base_idx]
            }
        } else {
            // Normal mode: direct palette lookup
            self.palette[(color_idx as usize) & 0x1F]
        }
    }

    fn ensure_legacy_shift_state_compat(&mut self) {
        // Older unit tests directly set `shift_count`/`bpl_shift` without using
        // `trigger_shift_load()`. Lazily mirror that into the per-plane state.
        if self.shift_count == 0 {
            return;
        }
        if self.bpl_shift_count.iter().any(|&c| c != 0)
            || self.bpl_shift_delay.iter().any(|&d| d != 0)
        {
            return;
        }
        self.bpl_shift_count = [self.shift_count; 8];
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
            //
            // BPLCON0 BPU=0 means "no bitplanes displayed" (Hardware
            // Reference Manual). Real Denise outputs COLOR00 only — see
            // WinUAE `drawing.cpp::getlinetype()`, which classifies the
            // line as `LINETYPE_BORDER` when `GET_PLANES(bplcon0) == 0`.
            // The fallback below only fires when BPLCON0 has never been
            // written (still the default `0`), preserving compatibility
            // with legacy unit tests that seed `bpl_shift[]` directly
            // and don't bother to program BPLCON0. Any program that
            // explicitly writes BPLCON0 — including BPLCON0 = $0000 —
            // takes the spec-correct path and BPU=0 blanks the playfield.
            //
            // See `knowledge/decisions/amiga-denise-bpu-zero-rendering.md`.
            let mut num_bpl = self.num_bitplanes();
            if num_bpl == 0 && self.bplcon0 == 0 {
                // Legacy unit-test compatibility: BPLCON0 has never been
                // touched, so treat any seeded shift state as the active
                // plane span. Real Amiga code always sets BPLCON0 (e.g.
                // bit 9 COLOR enable) so this branch never fires for
                // ROM-driven traffic.
                num_bpl = self
                    .bpl_shift_count
                    .iter()
                    .rposition(|&c| c != 0)
                    .map(|idx| idx + 1)
                    .or_else(|| {
                        self.bpl_shift
                            .iter()
                            .rposition(|&w| w != 0)
                            .map(|idx| idx + 1)
                    })
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
                // AGA FIFO auto-reload: when a plane's shift register drains
                // and there are queued wider-fetch words, pop the next one.
                if self.bpl_shift_count[plane] == 0
                    && self.bpl_fifo_len[plane] > 0
                    && let Some(word) = self.pop_bpl_fifo(plane)
                {
                    self.bpl_shift[plane] = word;
                    self.bpl_shift_count[plane] = 16;
                }
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

    fn shift_one_playfield_render_sample(&mut self, _hires: bool) -> (usize, u8, u8, u8) {
        // Hires outputs 4 source pixels per CCK (2 per output call × 2 calls).
        // Each call shifts one actual source pixel — no caching. The 16-bit
        // shift register drains in 4 CCK, matching the BPL1DAT fetch rate.
        // The 640→320 downsample uses the later pixel of each pair (handled
        // by the caller's loop overwriting raw_color_idx).
        self.shift_one_playfield_source_pixel()
    }

    pub fn output_pixel(&mut self, x: u32, y: u32) {
        self.output_pixel_with_beam(x, y, x, y);
    }

    /// Output a pixel and return its ARGB32 color.
    ///
    /// Convenience wrapper for unit tests: calls the output pipeline and
    /// returns the composited color (playfield + sprites + priority).
    pub fn output_pixel_color(&mut self, x: u32, y: u32) -> u32 {
        let debug = self.output_pixel_with_beam(x, y, x, y);
        if debug.called {
            let rgb12 = self.resolve_color_rgb12(debug.final_color_idx);
            Self::rgb12_to_argb32(rgb12)
        } else {
            0xFF00_0000
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn output_pixel_with_beam_n_source_samples(
        &mut self,
        x: u32,
        y: u32,
        beam_x: u32,
        beam_y: u32,
        spr_beam_x: u32,
        spr_beam_y: u32,
        source_pixels_per_output_call: u8,
        playfield_visible_gate: bool,
    ) -> DeniseOutputPixelDebug {
        // The sprite comparator runs in *absolute* beam coordinates
        // (SPRxPOS/CTL decode to absolute raster line and lores HSTART),
        // independent of the bitplane pipeline's scroll-relative
        // `beam_x`/`beam_y`. The board passes the absolute beam position
        // here; the standalone Denise wrappers pass `beam_x`/`beam_y`
        // (in those call sites the two spaces coincide). gap #162.
        self.sync_sprite_runtime_to_beam(spr_beam_x, spr_beam_y);
        let hires = (self.bplcon0 & 0x8000) != 0;
        let source_pixels_per_fb_pixel = source_pixels_per_output_call.clamp(1, 4);
        let mut quad_samples = [(0usize, 0u8, 0u8); 4];
        let mut quad_samples_debug = [DeniseSourcePixelDebug::default(); 4];
        let mut raw_color_idx = 0usize;
        let mut pf1_code = 0u8;
        let mut pf2_code = 0u8;
        let mut plane_bits_mask = 0u8;
        // The BPLCON1 scroll comparator determines when pending BPL data is
        // copied into the shift register. In WinUAE, the copy check happens
        // AFTER pixel output within each half-CCK iteration, so newly copied
        // data appears one pixel later. Our model does the copy BEFORE pixel
        // output, so we compensate with a 1-pixel offset: beam_x - 1.
        // Verified by pixel-level comparison against FS-UAE: offset 0 → 2px
        // left, offset 2 → 2px right, offset 1 → exact match.
        let comparator_phase = (beam_x as u16).wrapping_sub(1);

        // Commit BPL1DAT-triggered pending loads BEFORE shifting pixels out,
        // matching real hardware where the parallel load replaces the shift
        // register contents before the next serial output.
        self.apply_pending_shift_load_if_due(comparator_phase);

        for sample_idx in 0..source_pixels_per_fb_pixel {
            let (raw, pf1, pf2, mask) = self.shift_one_playfield_render_sample(hires);
            if sample_idx < 4 {
                quad_samples[sample_idx as usize] = (raw, pf1, pf2);
                quad_samples_debug[sample_idx as usize] = DeniseSourcePixelDebug {
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
        }

        // Compose playfield pixel for the "last" sample (used for final_color_idx
        // and lores output). In hires mode we also compose the first sample
        // independently for the per-pixel quad_color_idx.
        let playfield = if playfield_visible_gate {
            self.compose_playfield_pixel(raw_color_idx, pf1_code, pf2_code)
        } else {
            PlayfieldPixel {
                visible_color_idx: 0,
                front_playfield: None,
            }
        };
        let sprite_group_mask = self.collision_group_mask(beam_x, beam_y);
        self.latch_collisions(
            if playfield_visible_gate {
                plane_bits_mask
            } else {
                0
            },
            sprite_group_mask,
        );

        // Sprite lookup (lores resolution — same sprite for both hires sub-pixels).
        // On real Denise, the display window (DIWSTRT/DIWSTOP) blanks both
        // playfields AND sprites — only COLOR00 is output outside the window.
        let sprite_pixel = if playfield_visible_gate {
            self.sprite_pixel(beam_x, beam_y)
        } else {
            None
        };
        if let Some(sp) = &sprite_pixel {
            // A sprite produced a non-transparent pixel inside the display
            // window. (Diagnostic counter; over an empty playfield the
            // sprite wins priority and this pixel reaches the framebuffer.)
            self.spr_pixels_rendered[sp.sprite_group & 7] =
                self.spr_pixels_rendered[sp.sprite_group & 7].saturating_add(1);
        }

        // Cache BPLCON2 priority positions for sprite resolution (avoids
        // re-borrowing &self through a closure while &mut self is live).
        let bplcon2 = self.bplcon2;

        let resolve_sprite_priority =
            |pf: &PlayfieldPixel, sp: &Option<SpritePixel>| -> (usize, bool) {
                let mut c = pf.visible_color_idx;
                let mut from_sprite = false;
                if let Some(s) = sp {
                    if let Some(front_pf) = pf.front_playfield {
                        let pf_pos = match front_pf {
                            PlayfieldId::Pf1 => usize::from(bplcon2 & 0x0007),
                            PlayfieldId::Pf2 => usize::from((bplcon2 >> 3) & 0x0007),
                        }
                        .min(4);
                        if s.sprite_group < pf_pos {
                            c = s.palette_idx;
                            from_sprite = true;
                        }
                    } else {
                        c = s.palette_idx;
                        from_sprite = true;
                    }
                }
                (c, from_sprite)
            };

        let (color_idx, is_sprite) = resolve_sprite_priority(&playfield, &sprite_pixel);

        // In hires/superhires, compose each source pixel independently for
        // full-res output. In lores all four entries are identical.
        let (quad_color_idx, quad_is_sprite) =
            if source_pixels_per_fb_pixel > 1 && playfield_visible_gate {
                let mut quad = [color_idx as u8; 4];
                let mut quad_sp = [is_sprite; 4];
                for i in 0..source_pixels_per_fb_pixel.min(4) as usize {
                    let (raw_i, pf1_i, pf2_i) = quad_samples[i];
                    let pf_i = self.compose_playfield_pixel(raw_i, pf1_i, pf2_i);
                    let (ci, sp) = resolve_sprite_priority(&pf_i, &sprite_pixel);
                    quad[i] = ci as u8;
                    quad_sp[i] = sp;
                }
                (quad, quad_sp)
            } else {
                ([color_idx as u8; 4], [is_sprite; 4])
            };

        DeniseOutputPixelDebug {
            called: true,
            beam_x,
            beam_y,
            requested_x: x,
            requested_y: y,
            hires,
            source_pixels_per_fb_pixel,
            quad_samples: quad_samples_debug,
            plane_bits_mask,
            final_color_idx: color_idx as u8,
            quad_color_idx,
            quad_is_sprite,
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
        let shres = (self.bplcon0 & 0x0040) != 0;
        let source_pixels_per_output_call = if shres {
            4
        } else if hires {
            2
        } else {
            1
        };
        self.output_pixel_with_beam_n_source_samples(
            x,
            y,
            beam_x,
            beam_y,
            beam_x,
            beam_y,
            source_pixels_per_output_call,
            playfield_visible_gate,
        )
    }

    /// Output one pixel with the sprite comparator driven by *absolute*
    /// beam coordinates (`spr_beam_x` in lores units, `spr_beam_y` the
    /// raster line), separate from the bitplane pipeline's scroll-relative
    /// `beam_x`/`beam_y`. The board uses this so DMA-driven sprites — whose
    /// SPRxPOS/CTL HSTART/VSTART are absolute — position correctly against
    /// the real beam rather than the data-fetch-relative pipeline. gap #162.
    #[allow(clippy::too_many_arguments)]
    pub fn output_pixel_with_beam_sprite_coords(
        &mut self,
        x: u32,
        y: u32,
        beam_x: u32,
        beam_y: u32,
        spr_beam_x: u32,
        spr_beam_y: u32,
        playfield_visible_gate: bool,
    ) -> DeniseOutputPixelDebug {
        let hires = (self.bplcon0 & 0x8000) != 0;
        let shres = (self.bplcon0 & 0x0040) != 0;
        let source_pixels_per_output_call = if shres {
            4
        } else if hires {
            2
        } else {
            1
        };
        self.output_pixel_with_beam_n_source_samples(
            x,
            y,
            beam_x,
            beam_y,
            spr_beam_x,
            spr_beam_y,
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

impl DeniseOcs {
    /// Extract a viewport from the raster framebuffer.
    ///
    /// Returns the cropped region at hires resolution. For non-interlaced
    /// content, adjacent row pairs are identical; pass `deinterlace=true`
    /// to take every other row (halving the height).
    pub fn extract_viewport(
        &self,
        preset: ViewportPreset,
        pal: bool,
        deinterlace: bool,
    ) -> ViewportImage {
        let bounds = if pal {
            preset.pal_bounds()
        } else {
            preset.ntsc_bounds()
        };

        let h_pixels = u32::from(bounds.h_end_cck - bounds.h_start_cck) * 8;
        let v_lines = u32::from(bounds.v_end_line - bounds.v_start_line);
        let raster_rows = v_lines * 2; // double-height buffer

        let out_height = if deinterlace { v_lines } else { raster_rows };
        let mut pixels = Vec::with_capacity((h_pixels * out_height) as usize);

        let row_step = if deinterlace { 2u32 } else { 1u32 };
        let fb_w = self.raster_fb_width;

        for row_idx in 0..out_height {
            let raster_row = u32::from(bounds.v_start_line) * 2 + row_idx * row_step;
            let raster_x_start = u32::from(bounds.h_start_cck) * 8;

            for px in 0..h_pixels {
                let fb_x = raster_x_start + px;
                let idx = (raster_row * fb_w + fb_x) as usize;
                let color = self
                    .framebuffer_raster
                    .get(idx)
                    .copied()
                    .unwrap_or(0xFF000000);
                pixels.push(color);
            }
        }

        ViewportImage {
            pixels,
            width: h_pixels,
            height: out_height,
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
    //! Inline tests covering symbols that are private to this module —
    //! the legacy `shift_one_playfield_source_pixel` raw-pixel iterator
    //! and the `ham_prev_rgb` private state machine. Tests that exercise
    //! only the public Denise API live under `tests/` as integration
    //! files.

    use super::*;

    fn collect_raw_source_pixels(denise: &mut DeniseOcs, count: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            let (raw, _, _, _) = denise.shift_one_playfield_source_pixel();
            out.push(raw as u8);
        }
        out
    }

    #[test]
    fn hires_bplcon1_barrel_shift_applies_on_every_load() {
        let mut denise = DeniseOcs::new();
        denise.bplcon0 = 0x9000; // HIRES + 1 bitplane
        denise.bplcon1 = 0x0040; // odd planes scroll by 4 hires pixels
        denise.begin_beam_line();

        // First load: prev=0, raw=0x8000, combined=(0<<16|0x8000)>>4 = 0x0800
        denise.bpl_data[0] = 0x8000;
        denise.trigger_shift_load();
        assert_eq!(
            collect_raw_source_pixels(&mut denise, 16),
            vec![0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            "first hires shift load of the line should honor BPLCON1 barrel-shift delay"
        );

        // Second load: prev=0x8000, raw=0x8000, combined=(0x8000<<16|0x8000)>>4 = 0x0800
        // Barrel shift carries the same scroll offset on every load for smooth scrolling.
        denise.bpl_data[0] = 0x8000;
        denise.trigger_shift_load();
        assert_eq!(
            collect_raw_source_pixels(&mut denise, 16),
            vec![0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            "subsequent hires shift loads apply the same barrel-shift scroll for smooth scrolling"
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
    fn ham_line_start_resets_to_color00() {
        let mut denise = DeniseOcs::new();
        denise.bplcon0 = 0x6800;
        denise.set_palette(0, 0x123);

        // Pollute ham_prev_rgb
        denise.ham_prev_rgb = 0xFFF;

        denise.begin_beam_line();
        // After line start, prev should be COLOR00
        let rgb = denise.resolve_color_rgb12(0x10); // control=01, data=0 → modify blue to 0
        assert_eq!(rgb, 0x120); // 0x123 with blue=0
    }
}
