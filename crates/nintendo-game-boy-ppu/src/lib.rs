//! Game Boy DMG PPU.
//!
//! Pixel-FIFO renderer ticked at the master clock rate (one call per
//! T-cycle / dot). Mode 3 is modelled dot by dot (see [`mode3`]): the
//! background/window fetcher, the object fetch stalls, the SCX
//! fine-scroll discard and the shifter each read the PPU registers on
//! the dot the hardware does, so mid-scanline register writes land on
//! the right pixel. OAM scan happens once at the mode-2 → mode-3
//! transition, limited to 10 objects per scanline.
//!
//! Framebuffer holds post-palette 2-bit shade values (0 = lightest,
//! 3 = darkest); the runtime layer maps those to RGBA via the
//! [`common-nintendo-game-boy::palette`] helpers (or a custom green-LCD
//! palette).
//!
//! Ported from `~/Projects/Emu198x-Zig/src/ppu.zig`. The pin contract
//! with the machine is two one-shot pulses — `vblank_irq_latched`
//! and `stat_irq_latched` — that the machine consumes via
//! [`Ppu::consume_vblank_irq`] / [`Ppu::consume_stat_irq`] and OR's
//! into `IF` bits 0 / 1.

mod fifo;
mod mode3;

use common_nintendo_game_boy::{SCREEN_HEIGHT, SCREEN_WIDTH};
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::mode3::{Mode3, POSITION_START};

/// MMIO addresses for the PPU register block.
pub const REG_LCDC: u16 = 0xFF40;
pub const REG_STAT: u16 = 0xFF41;
pub const REG_SCY: u16 = 0xFF42;
pub const REG_SCX: u16 = 0xFF43;
pub const REG_LY: u16 = 0xFF44;
pub const REG_LYC: u16 = 0xFF45;
pub const REG_BGP: u16 = 0xFF47;
pub const REG_OBP0: u16 = 0xFF48;
pub const REG_OBP1: u16 = 0xFF49;
pub const REG_WY: u16 = 0xFF4A;
pub const REG_WX: u16 = 0xFF4B;

/// `IF` bit positions latched by the PPU.
pub const IF_VBLANK_BIT: u8 = 0;
pub const IF_STAT_BIT: u8 = 1;

const DOTS_PER_LINE: u16 = 456;
const LINES_PER_FRAME: u8 = 154;
pub(crate) const VBLANK_START: u8 = 144;
const OAM_END: u16 = 80;
const LCD_ENABLE_MODE0_DOTS: u16 = 80;
/// Dot at which the mode-3 pixel pipeline starts. SameBoy starts its
/// pipeline five dots after STAT reports mode 3; this value places our
/// pipeline against the CPU so that mid-line writes in the Mealybug
/// Tearoom tests land on the pixels the DMG reference photos show.
const MODE3_START: u16 = 85;

const FRAMEBUFFER_LEN: usize = (SCREEN_WIDTH * SCREEN_HEIGHT) as usize;

const fn default_lyc_match() -> bool {
    true
}

/// LCDC bit positions. The fetcher reaches into LCDC via raw bit
/// constants so it doesn't need to share this module; the names
/// here document the full bit field for the lib-side dispatch.
#[allow(dead_code)]
pub(crate) mod lcdc {
    pub const ENABLE: u8 = 0x80;
    pub const WINDOW_TILE_MAP: u8 = 0x40;
    pub const WINDOW_ENABLE: u8 = 0x20;
    pub const BG_TILE_DATA_UNSIGNED: u8 = 0x10;
    pub const BG_TILE_MAP: u8 = 0x08;
    pub const SPRITE_HEIGHT_16: u8 = 0x04;
    pub const SPRITES_ENABLE: u8 = 0x02;
    pub const BG_ENABLE: u8 = 0x01;
}

/// The LCDC bits only the background and object fetchers read. A CPU
/// write lands them a dot before the bits the shifter and window
/// trigger read (see [`Ppu::stage_cpu_write`]).
const LCDC_FETCHER_BITS: u8 = lcdc::WINDOW_TILE_MAP
    | lcdc::BG_TILE_DATA_UNSIGNED
    | lcdc::BG_TILE_MAP
    | lcdc::SPRITE_HEIGHT_16;

/// STAT bit positions for the writable interrupt-enable bits.
mod stat {
    pub const LYC_ENABLE: u8 = 0x40;
    pub const MODE2_ENABLE: u8 = 0x20;
    pub const MODE1_ENABLE: u8 = 0x10;
    pub const MODE0_ENABLE: u8 = 0x08;
    /// Mask covering the writable bits 3-6.
    pub const WRITABLE_MASK: u8 = 0x78;
}

/// A CPU register write in flight within one M-cycle.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct StagedWrite {
    addr: u16,
    value: u8,
    /// Register value before the write.
    old: u8,
    /// Dots of the M-cycle run so far.
    dots: u8,
}

/// PPU state.
#[derive(Clone, Serialize, Deserialize)]
pub struct Ppu {
    /// Dot counter within the current scanline (0..=455).
    pub dot: u16,
    /// Current scanline (0..=153).
    pub ly: u8,

    /// X coordinate of the next pixel to emit (0..=160).
    pub lcd_x: u8,
    /// SameBoy's `position_in_line`: a `u8` shift cursor for mode 3.
    /// 0..159 are on-screen (drawn); 240..255 (= −16..−1) are the
    /// off-screen warm-up / SCX fine-discard zone (shifted but not
    /// drawn). Drives both the draw gate and the fetcher's tile column.
    position_in_line: u8,

    /// Mode-3 pixel pipeline.
    m3: Mode3,
    /// A CPU write to a PPU register that lands part-way through the
    /// current M-cycle (see [`Ppu::stage_cpu_write`]).
    staged_write: Option<StagedWrite>,

    pub lcdc: u8,
    /// STAT's three writable interrupt-enable bits + the LYC-enable
    /// bit. Mode bits are computed on read; LYC coincidence is latched
    /// because LCD-off behaviour does not simply mirror `LY == LYC`.
    stat: u8,
    #[serde(default = "default_lyc_match")]
    lyc_match: bool,
    #[serde(default)]
    lcd_enable_mode0_dots: u16,
    pub scy: u8,
    pub scx: u8,
    pub lyc: u8,
    pub bgp: u8,
    pub obp0: u8,
    pub obp1: u8,
    pub wx: u8,
    pub wy: u8,

    #[serde(with = "BigArray")]
    framebuffer: [u8; FRAMEBUFFER_LEN],
    /// Set when the PPU enters VBlank; the runtime consumes this to
    /// know a frame is ready to present.
    pub frame_ready: bool,

    /// One-shot VBlank IRQ pulse, cleared by
    /// [`consume_vblank_irq`](Ppu::consume_vblank_irq).
    vblank_irq_latched: bool,
    /// One-shot STAT IRQ pulse, cleared by
    /// [`consume_stat_irq`](Ppu::consume_stat_irq). Asserted on the
    /// rising edge of the OR of the four STAT enable sources.
    stat_irq_latched: bool,
    /// Last-known STAT-line state; used for rising-edge detection.
    stat_line_prev: bool,
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}

impl Ppu {
    /// Creates a PPU at the documented post-boot-ROM register state
    /// for the DMG (LCD on, BG enabled, BGP = $FC, OBP0/1 = $FF).
    #[must_use]
    pub fn new() -> Self {
        Self::new_post_bootrom_with_dot(0)
    }

    /// Creates a PPU at the post-boot register state with a
    /// model-specific LCD dot phase.
    #[must_use]
    pub fn new_post_bootrom_with_dot(dot: u16) -> Self {
        Self::new_post_bootrom_with_phase(0, dot)
    }

    /// Creates a PPU at the post-boot register state with a
    /// model-specific LCD scanline and dot phase.
    #[must_use]
    pub fn new_post_bootrom_with_phase(ly: u8, dot: u16) -> Self {
        Self {
            dot: dot % DOTS_PER_LINE,
            ly: ly % LINES_PER_FRAME,
            lcd_x: 0,
            position_in_line: POSITION_START,
            m3: Mode3::new(),
            staged_write: None,
            lcdc: 0x91,
            stat: 0,
            lyc_match: true,
            lcd_enable_mode0_dots: 0,
            scy: 0,
            scx: 0,
            lyc: 0,
            bgp: 0xFC,
            obp0: 0xFF,
            obp1: 0xFF,
            wx: 0,
            wy: 0,
            framebuffer: [0; FRAMEBUFFER_LEN],
            frame_ready: false,
            vblank_irq_latched: false,
            stat_irq_latched: false,
            stat_line_prev: false,
        }
    }

    /// Returns the current PPU mode.
    ///
    /// | Mode | Meaning            |
    /// |------|--------------------|
    /// | 0    | HBlank             |
    /// | 1    | VBlank             |
    /// | 2    | OAM scan           |
    /// | 3    | Pixel transfer     |
    #[must_use]
    pub fn mode(&self) -> u8 {
        if (self.lcdc & lcdc::ENABLE) == 0 || self.lcd_enable_mode0_dots != 0 {
            0
        } else if self.ly >= VBLANK_START {
            1
        } else if self.dot < OAM_END {
            2
        } else if self.dot < self.mode3_end_dot() {
            // Mode 3 length is the canonical formula (mooneye-verified).
            // The pixel shifter may finish a few dots later — it keeps
            // drawing under its own `lcd_x < 160` guard, invisibly to the
            // STAT mode reported here.
            3
        } else {
            0
        }
    }

    /// Current dot within the scanline.
    #[must_use]
    pub const fn dot(&self) -> u16 {
        self.dot
    }

    fn mode3_end_dot(&self) -> u16 {
        OAM_END + 172 + u16::from(self.scx & 7) + self.obj_mode3_penalty()
    }

    fn obj_mode3_penalty(&self) -> u16 {
        if (self.lcdc & lcdc::SPRITES_ENABLE) == 0 {
            return 0;
        }

        // DMG OBJ fetch timing depends on sprite X phase, offscreen-left
        // sprites, and whether multiple sprites contend for the same BG
        // fetch tile. These bucketed penalties match mooneye's mode-0
        // sprite interrupt timing cases while keeping CPU-visible STAT
        // reads aligned to the machine's end-of-M-cycle bus sample.
        let mut bg_tile_sprite_counts = [0u8; 32];
        let mut offscreen_left_sprites = 0u16;
        for sprite in self.m3.scanned_in_fetch_order() {
            if sprite.x == 0 {
                offscreen_left_sprites += 1;
                continue;
            }

            let screen_x = i16::from(sprite.x) - 8;
            if screen_x >= SCREEN_WIDTH as i16 {
                continue;
            }

            let bg_x = (i16::from(self.scx) + screen_x).rem_euclid(256) as u16;
            let tile = usize::from((bg_x / 8) & 0x1F);
            bg_tile_sprite_counts[tile] = bg_tile_sprite_counts[tile].saturating_add(1);
        }

        let visible_sprite_count = bg_tile_sprite_counts
            .iter()
            .map(|&count| u16::from(count))
            .sum::<u16>();
        let visible_bg_tile_count = bg_tile_sprite_counts
            .iter()
            .filter(|&&count| count != 0)
            .count();
        let mut seen_bg_tiles = [false; 32];
        let mut penalty = 0u16;
        let mut multi_unique_phase_2_tiles = 0u16;
        let mut multi_unique_phase_4_tiles = 0u16;
        let mut multi_sprite_phase_2_or_3_tiles = 0u16;
        for sprite in self.m3.scanned_in_fetch_order() {
            if sprite.x == 0 {
                continue;
            }

            let screen_x = i16::from(sprite.x) - 8;
            if screen_x >= SCREEN_WIDTH as i16 {
                continue;
            }

            let bg_x = (i16::from(self.scx) + screen_x).rem_euclid(256) as u16;
            let tile = usize::from((bg_x / 8) & 0x1F);
            let phase = bg_x & 7;
            if !seen_bg_tiles[tile] {
                seen_bg_tiles[tile] = true;
                let single_sprite_tile = bg_tile_sprite_counts[tile] == 1;
                let repeated_sprite_tile = bg_tile_sprite_counts[tile] > 1;
                let early_fetch_phase = matches!(phase, 0 | 1);
                if (single_sprite_tile && visible_sprite_count > 1)
                    || (single_sprite_tile && offscreen_left_sprites != 0 && early_fetch_phase)
                    || (repeated_sprite_tile && early_fetch_phase)
                {
                    if single_sprite_tile && phase == 2 {
                        multi_unique_phase_2_tiles += 1;
                    } else if single_sprite_tile && phase == 4 {
                        multi_unique_phase_4_tiles += 1;
                    }
                    let fine_penalty = if single_sprite_tile {
                        match phase {
                            0 | 1 => 4,
                            2 | 3 => 2,
                            _ => 0,
                        }
                    } else {
                        let pixels_to_right = 7 - phase;
                        pixels_to_right.saturating_sub(2).min(4)
                    };
                    penalty += fine_penalty;
                } else if matches!(phase, 2 | 3) {
                    multi_sprite_phase_2_or_3_tiles += 1;
                }
            }

            let fetch_penalty = if visible_sprite_count == 1 && phase >= 4 {
                2
            } else {
                6
            };
            penalty += fetch_penalty;
        }

        if offscreen_left_sprites != 0 {
            penalty += offscreen_left_sprites * 6 + 1;
            if visible_bg_tile_count > 1 {
                penalty += 8;
            }
        }
        if multi_unique_phase_2_tiles > 2 {
            penalty += 8;
        }
        if multi_unique_phase_4_tiles > 2 {
            penalty += 8;
        }
        if multi_sprite_phase_2_or_3_tiles > 1 {
            penalty += 1;
        }

        penalty
    }

    /// Reads STAT ($FF41). Composes the writable bits with the live
    /// mode and LYC coincidence flag.
    #[must_use]
    pub fn read_stat(&self) -> u8 {
        let mut s = self.stat & stat::WRITABLE_MASK;
        s |= self.mode();
        if self.effective_lyc_match() {
            s |= 0x04;
        }
        s
    }

    /// Reads LY ($FF44). Near the end of a visible scanline, the CPU
    /// observes the next line before the internal dot counter wraps.
    #[must_use]
    pub fn read_ly(&self) -> u8 {
        if self.cpu_visible_next_ly() {
            self.ly.wrapping_add(1)
        } else {
            self.ly
        }
    }

    /// Returns whether CPU reads from VRAM are blocked by the PPU.
    #[must_use]
    pub fn cpu_blocks_vram_read(&self) -> bool {
        (self.lcdc & lcdc::ENABLE) != 0
            && self.ly < VBLANK_START
            && self.lcd_enable_mode0_dots == 0
            && self.dot >= OAM_END - 4
            && self.dot < self.mode3_end_dot()
    }

    /// Returns whether CPU writes to VRAM are blocked by the PPU.
    #[must_use]
    pub fn cpu_blocks_vram_write(&self) -> bool {
        (self.lcdc & lcdc::ENABLE) != 0
            && self.ly < VBLANK_START
            && self.dot >= OAM_END
            && self.dot < self.mode3_end_dot()
    }

    /// Returns whether CPU reads from OAM are blocked by the PPU.
    #[must_use]
    pub fn cpu_blocks_oam_read(&self) -> bool {
        matches!(self.mode(), 2 | 3) || self.cpu_visible_next_ly()
    }

    /// Returns whether CPU writes to OAM are blocked by the PPU.
    #[must_use]
    pub fn cpu_blocks_oam_write(&self) -> bool {
        (self.lcdc & lcdc::ENABLE) != 0
            && self.ly < VBLANK_START
            && ((self.lcd_enable_mode0_dots == 0 && self.dot < OAM_END - 4)
                || (self.dot >= OAM_END && self.dot < self.mode3_end_dot()))
    }

    fn effective_lyc_match(&self) -> bool {
        if self.cpu_visible_next_ly() {
            false
        } else {
            self.lyc_match
        }
    }

    fn cpu_visible_next_ly(&self) -> bool {
        (self.lcdc & lcdc::ENABLE) != 0 && self.ly < VBLANK_START && self.dot >= 452
    }

    /// Writes STAT ($FF41). Only bits 3-6 are writable.
    pub fn write_stat(&mut self, value: u8) {
        self.stat = value & stat::WRITABLE_MASK;
        // A write that newly enables a STAT source can immediately
        // raise the line — re-evaluate edge detection.
        self.update_stat_line();
    }

    /// Writes LYC ($FF45) and re-evaluates the coincidence STAT
    /// source against the current LY.
    pub fn write_lyc(&mut self, value: u8) {
        self.lyc = value;
        if (self.lcdc & lcdc::ENABLE) != 0 {
            self.lyc_match = self.ly == self.lyc;
        }
        self.update_stat_line();
    }

    /// Writes LCDC ($FF40). Turning the LCD off freezes timing,
    /// resets the line counter, and blanks the framebuffer (real
    /// hardware shows white). Turning it back on resumes from
    /// `dot = 0, ly = 0`.
    pub fn write_lcdc(&mut self, value: u8) {
        let was_on = (self.lcdc & lcdc::ENABLE) != 0;
        let now_on = (value & lcdc::ENABLE) != 0;
        // Clearing OBJ_EN while an object is being fetched abandons the
        // fetch on the next dot.
        if (self.lcdc & lcdc::SPRITES_ENABLE) != 0
            && (value & lcdc::SPRITES_ENABLE) == 0
            && self.m3.during_object_fetch
        {
            self.m3.object_fetch_aborted = true;
            self.m3.wait = self.m3.wait.min(1);
        }
        self.lcdc = value;
        if was_on && !now_on {
            self.dot = 0;
            self.ly = 0;
            self.lcd_x = 0;
            self.position_in_line = POSITION_START;
            self.m3 = Mode3::new();
            self.staged_write = None;
            self.framebuffer.fill(0);
            self.stat_line_prev = false;
        } else if !was_on && now_on {
            self.lcd_enable_mode0_dots = LCD_ENABLE_MODE0_DOTS;
            self.lyc_match = self.ly == self.lyc;
        }
        self.wy_check(self.ly);
        self.update_stat_line();
    }

    /// Writes WY ($FF4A) and re-arms the window's `WY = LY` latch.
    pub fn write_wy(&mut self, value: u8) {
        self.wy = value;
        self.wy_check(self.ly);
    }

    /// Writes WX ($FF4B). On the DMG a write suppresses the one-pixel
    /// early window trigger (`WX = position + 6`) on the following dot.
    pub fn write_wx(&mut self, value: u8) {
        self.wx = value;
        self.m3.wx_just_changed = true;
    }

    /// Offers a CPU write the machine is about to perform at the end of
    /// the coming M-cycle. Returns `true` if the PPU takes it over, in
    /// which case the machine must not also write the register.
    ///
    /// On the DMG some PPU registers latch a CPU write before the end of
    /// the M-cycle, and the palettes and LCDC pass through an
    /// intermediate value for one dot. The base is SameBoy's DMG
    /// access-conflict map (`dmg_conflict_map` and `cycle_write` in
    /// `Core/sm83_cpu.c`): BGP/OBP0/OBP1 `old | new` two dots early,
    /// then `new` one dot early; SCX two dots early.
    ///
    /// What the background and object fetchers read lands two dots
    /// early too: SCY, and LCDC's tile-map, tile-data, window-map and
    /// object-size bits ([`LCDC_FETCHER_BITS`]). SameBoy has them one
    /// dot early. GateBoy, a gate-level DMG model, latches SCX, SCY and
    /// LCDC on one shared write strobe, so SCY and those LCDC bits land
    /// with SCX. Our fetcher then reads them on the same dot, relative
    /// to the write, as GateBoy's does, with or without an object
    /// fetch stalling it. That timing renders
    /// seven Mealybug Tearoom ROMs exactly as the DMG reference images
    /// do (`m3_lcdc_bg_map_change`, `m3_lcdc_tile_sel_change`,
    /// `m3_lcdc_tile_sel_win_change`, `m3_lcdc_win_map_change`,
    /// `m3_lcdc_obj_size_change`, `m3_lcdc_obj_size_change_scx`,
    /// `m3_scy_change`); GateBoy renders them identically.
    ///
    /// The bits the shifter and window trigger read keep SameBoy's
    /// timing: BG_EN joins as `old | new` two dots early, and OBJ_EN
    /// and WIN_EN land one dot early. GateBoy lands them with the rest
    /// and then misplaces its output by one pixel on
    /// `m3_lcdc_bg_en_change`, `m3_lcdc_obj_en_change` and
    /// `m3_bgp_change` (its own notes need a one-pixel delay there);
    /// SameBoy's timing matches the references for those bits.
    ///
    /// WIN_EN has a fetcher reader too: a tile fetch that starts while
    /// WIN_EN is clear fetches background, not window. That check sees
    /// WIN_EN when the fetcher bits land ([`Ppu::fetcher_window_enabled`]).
    ///
    /// A write that turns the LCD on or off is left to the machine so
    /// LCD-enable timing is unchanged.
    pub fn stage_cpu_write(&mut self, addr: u16, value: u8) -> bool {
        if (self.lcdc & lcdc::ENABLE) == 0 {
            return false;
        }
        let old = match addr {
            REG_LCDC if (value & lcdc::ENABLE) != 0 => self.lcdc,
            REG_SCY => self.scy,
            REG_SCX => self.scx,
            REG_BGP => self.bgp,
            REG_OBP0 => self.obp0,
            REG_OBP1 => self.obp1,
            _ => return false,
        };
        self.staged_write = Some(StagedWrite {
            addr,
            value,
            old,
            dots: 0,
        });
        true
    }

    /// LCDC's WIN_EN as the background fetcher sees it.
    ///
    /// When WIN_EN goes low, the fetch that starts next is a background
    /// fetch. GateBoy shows why: the write resets the window-mode latch
    /// (`XOFO` clears `PYNU`) on the shared write strobe, the same edge
    /// that latches the other LCDC bits. So the fetcher sees a staged
    /// WIN_EN two dots before the M-cycle ends, with
    /// [`LCDC_FETCHER_BITS`]. The window trigger and the blank-pixel
    /// glitch keep reading `lcdc`, where WIN_EN lands a dot later.
    ///
    /// The result: `m3_lcdc_win_en_change_multiple_wx` renders exactly
    /// as the DMG-CPU B reference image, and as GateBoy, a DMG-CPU B
    /// model, renders it. It is 3 px off the DMG-blob image; the two
    /// references differ in exactly those 3 px.
    pub(crate) fn fetcher_window_enabled(&self) -> bool {
        let lcdc = match self.staged_write {
            Some(StagedWrite {
                addr: REG_LCDC,
                value,
                dots,
                ..
            }) if dots >= 2 => value,
            _ => self.lcdc,
        };
        (lcdc & lcdc::WINDOW_ENABLE) != 0
    }

    /// Applies the part of a staged write due after the dot just run.
    fn apply_staged_write(&mut self) {
        let Some(mut staged) = self.staged_write else {
            return;
        };
        staged.dots += 1;
        let StagedWrite {
            addr,
            value,
            old,
            dots,
        } = staged;
        let done = match (addr, dots) {
            (REG_SCX, 2) => {
                self.scx = value;
                true
            }
            (REG_SCY, 2) => {
                self.scy = value;
                true
            }
            (REG_BGP | REG_OBP0 | REG_OBP1, 2 | 3) => {
                let latched = if dots == 2 { old | value } else { value };
                match addr {
                    REG_BGP => self.bgp = latched,
                    REG_OBP0 => self.obp0 = latched,
                    _ => self.obp1 = latched,
                }
                dots == 3
            }
            (REG_LCDC, 2) => {
                let mut held = (value & LCDC_FETCHER_BITS) | (self.lcdc & !LCDC_FETCHER_BITS);
                if (value & lcdc::SPRITES_ENABLE) == 0
                    && (self.position_in_line == 0 || self.m3.during_object_fetch)
                {
                    held &= !lcdc::SPRITES_ENABLE;
                }
                self.write_lcdc(held | (value & lcdc::BG_ENABLE));
                false
            }
            (REG_LCDC, 3) => {
                let window_switched_off =
                    (old & lcdc::WINDOW_ENABLE) != 0 && (value & lcdc::WINDOW_ENABLE) == 0;
                self.write_lcdc(value);
                if window_switched_off && self.m3.window_is_being_fetched {
                    self.m3.disable_window_pixel_insertion_glitch = true;
                }
                true
            }
            _ => dots >= 4,
        };
        self.staged_write = if done { None } else { Some(staged) };
    }

    /// Reads the framebuffer as a flat `width * height` slice of 2-bit
    /// shades (0 = lightest, 3 = darkest). The runtime maps to RGBA.
    #[must_use]
    pub fn framebuffer(&self) -> &[u8] {
        &self.framebuffer
    }

    /// Consumes the frame-ready latch: returns `true` and clears the
    /// flag if the PPU has entered VBlank since the last call.
    pub fn consume_frame_ready(&mut self) -> bool {
        let was = self.frame_ready;
        self.frame_ready = false;
        was
    }

    /// Consumes the VBlank IRQ pulse. The machine OR's the result
    /// into `IF` bit 0.
    pub fn consume_vblank_irq(&mut self) -> bool {
        let was = self.vblank_irq_latched;
        self.vblank_irq_latched = false;
        was
    }

    /// Consumes the STAT IRQ pulse. The machine OR's the result into
    /// `IF` bit 1.
    pub fn consume_stat_irq(&mut self) -> bool {
        let was = self.stat_irq_latched;
        self.stat_irq_latched = false;
        was
    }

    /// Advance the PPU by one T-cycle (one dot).
    ///
    /// `vram` must be the 8 KiB DMG VRAM block (i.e. CPU `$8000`
    /// is `vram[0]`). `oam` must be the 160-byte OAM block (CPU
    /// `$FE00` = `oam[0]`).
    pub fn tick(&mut self, vram: &[u8], oam: &[u8]) {
        if (self.lcdc & lcdc::ENABLE) == 0 {
            // LCD off: timing frozen.
            return;
        }

        if self.ly >= VBLANK_START {
            // Mode 1: VBlank. The single-shot frame-ready + vblank
            // IRQ latch fire on the very first dot of line 144.
            if self.ly == VBLANK_START && self.dot == 0 {
                self.frame_ready = true;
                self.vblank_irq_latched = true;
            }
        } else if self.dot < OAM_END {
            // Mode 2: OAM scan. We do the scan once at the
            // mode-2 → mode-3 transition (after the CPU has had a
            // chance to handle STAT interrupts that might change
            // LCDC).
        } else {
            if self.dot == OAM_END {
                self.scan_oam(oam);
            }
            if self.dot == MODE3_START {
                self.start_mode3(vram, oam);
            } else if self.dot > MODE3_START {
                self.mode3_dot(vram, oam);
            }
        }

        let wx_was_just_changed = self.m3.wx_just_changed;
        self.advance_timing();
        self.update_stat_line();
        if wx_was_just_changed {
            self.m3.wx_just_changed = false;
        }
        self.apply_staged_write();
    }

    /// Tick four times — one CPU m-cycle.
    pub fn tick_m(&mut self, vram: &[u8], oam: &[u8]) {
        for _ in 0..4 {
            self.tick(vram, oam);
        }
    }

    fn advance_timing(&mut self) {
        self.lcd_enable_mode0_dots = self.lcd_enable_mode0_dots.saturating_sub(1);
        self.dot += 1;
        if self.dot >= DOTS_PER_LINE {
            self.dot = 0;
            if self.m3.resume != mode3::Resume::Idle {
                // The pipeline overran the line (SameBoy's mode-3
                // abort): fill what is left and settle the window.
                self.finish_mode3();
            }
            let previous_line = self.ly;
            self.ly = self.ly.wrapping_add(1);
            if self.ly >= LINES_PER_FRAME {
                self.ly = 0;
                self.m3.wy_triggered = false;
                self.m3.window_y = 0xFF;
            }
            self.lyc_match = self.ly == self.lyc;
            if self.ly < VBLANK_START {
                self.lcd_x = 0;
                // The DMG compares WY at the top of each line against
                // the line just finished, then against the new LY as
                // mode 2 starts (SameBoy `wy_check`, `ly_for_comparison`).
                let comparison = if self.ly == 0 { 0 } else { previous_line };
                self.wy_check(comparison);
                self.wy_check(self.ly);
            }
        }
    }

    /// Composite the current STAT IRQ source line and latch a pulse
    /// on its rising edge.
    fn update_stat_line(&mut self) {
        let mode = self.mode();
        let line = ((self.stat & stat::LYC_ENABLE) != 0 && self.effective_lyc_match())
            || ((self.stat & stat::MODE2_ENABLE) != 0 && self.mode2_stat_active(mode))
            || ((self.stat & stat::MODE1_ENABLE) != 0
                && mode == 1
                && !(self.ly == VBLANK_START && self.dot == 0))
            || ((self.stat & stat::MODE0_ENABLE) != 0 && mode == 0);
        if line && !self.stat_line_prev {
            self.stat_irq_latched = true;
        }
        self.stat_line_prev = line;
    }

    fn mode2_stat_active(&self, mode: u8) -> bool {
        // On every line but 0 the OAM STAT source rises a dot before
        // STAT's mode bits change; on line 0 it rises with them, a dot
        // later (SameBoy `GB_display_run`, "The OAM STAT interrupt
        // occurs 1 T-cycle before STAT actually changes, except on line
        // 0"; the Mealybug Tearoom tests compensate for the resulting
        // 4-cycle later dispatch on line 0).
        (mode == 2 && !(self.ly == 0 && self.dot == 0))
            || (mode == 1 && self.ly == VBLANK_START && self.dot != 0)
    }
}

/// Apply a 2-bit BGP/OBP palette to a 2-bit pixel index.
#[inline]
pub fn apply_palette(palette: u8, index: u8) -> u8 {
    (palette >> ((index & 0b11) * 2)) & 0b11
}

#[cfg(test)]
mod tests;
