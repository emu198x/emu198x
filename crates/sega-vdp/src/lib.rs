//! Sega Master System / Game Gear VDP (315-5124 / 315-5246).
//!
//! Adapted from `Emu198x-Oldest/crates/sega-vdp` (port 2026-06-01) as
//! commit 1 of 3 unlocking Sega Master System. Self-contained port
//! with no external dependencies; first major new chip needed by SMS
//! beyond what ColecoVision + SG-1000 + MSX1 + Sord M5 already brought
//! in (TMS9918 + SN76489 + AY-3-8910 + 8255 PPI). The TMS9918 chip is
//! adjacent silicon — register I/O is mostly compatible at the
//! TMS9918-mode level — but the new Mode 4 tile pipeline, dual 16-colour
//! palettes, scroll registers, line-interrupt counter, and H/V counter
//! readback make this a substantial step beyond `ti-tms9918`.
//!
//! Extends the TMS9918A with Mode 4: 4bpp tiles with per-tile flip,
//! priority, and palette select; two 16-color palettes from 64 colors
//! (6-bit RGB); horizontal and vertical scrolling; 8 sprites per line;
//! and a line interrupt counter.
//!
//! All four TMS9918A legacy modes (Graphics I/II, Text, Multicolor) are
//! retained for SG-1000 backward compatibility.
//!
//! The Game Gear variant extends CRAM to 12-bit RGB (4096 colors) and
//! displays a 160×144 viewport from the 256×192 framebuffer.

#![allow(clippy::cast_possible_truncation)]

// ---------------------------------------------------------------------------
// Region and variant
// ---------------------------------------------------------------------------

/// VDP region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VdpRegion {
    Ntsc,
    Pal,
}

/// VDP variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VdpVariant {
    /// SMS1 (315-5124): no 224/240-line modes, sprite zoom bug.
    Sms1,
    /// SMS2 / Game Gear (315-5246): 224/240-line modes, fixed sprite zoom.
    Sms2,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Active display area dimensions (the pixels the SMS VDP actually
/// draws tiles + sprites into).
pub const ACTIVE_WIDTH: u32 = 256;
pub const ACTIVE_HEIGHT: u32 = 192;

/// Border thickness around the active area. Same canonical TV-visible
/// envelope as the TMS9918 family — close enough for screenshots,
/// generous enough that the backdrop register matters.
pub const BORDER_LEFT: u32 = 16;
pub const BORDER_RIGHT: u32 = 16;
pub const BORDER_TOP: u32 = 24;
pub const BORDER_BOTTOM: u32 = 24;

/// Framebuffer dimensions (active + border).
pub const FB_WIDTH: u32 = ACTIVE_WIDTH + BORDER_LEFT + BORDER_RIGHT;
pub const FB_HEIGHT: u32 = ACTIVE_HEIGHT + BORDER_TOP + BORDER_BOTTOM;

// ---------------------------------------------------------------------------
// VDP
// ---------------------------------------------------------------------------

/// Sega VDP.
pub struct SegaVdp {
    // VRAM: 16 KB
    vram: [u8; 16384],
    // CRAM: 32 bytes (SMS) or 64 bytes (GG)
    cram: [u8; 64],
    cram_latch: u8,
    is_game_gear: bool,

    // Registers (0-10)
    regs: [u8; 11],

    // Status register
    status: u8,

    // I/O state
    read_buffer: u8,
    address: u16,
    code: u8,
    latch_first: bool,
    latch_value: u8,

    // Counters
    v_counter: u16,
    h_counter: u8,
    line_counter: u8,
    line_irq_pending: bool,

    // Rendering
    scanline: u16,
    /// Current dot within the scanline (0-341), for per-dot rendering.
    dot: u16,
    region: VdpRegion,
    #[allow(dead_code)]
    variant: VdpVariant,
    framebuffer: Vec<u32>,
    /// Per-line sprite colour-index buffer (0 = no sprite pixel), evaluated at
    /// the start of each active line and overlaid per pixel. Transient — not
    /// part of the saved state.
    sprite_buf: [u8; 256],

    /// Interrupt output (directly drives Z80 INT).
    pub interrupt: bool,
    /// Frame counter.
    pub frame_count: u64,
}

impl SegaVdp {
    /// Create a new SMS VDP.
    #[must_use]
    pub fn new(region: VdpRegion, variant: VdpVariant) -> Self {
        Self::new_inner(region, variant, false)
    }

    /// Create a new Game Gear VDP.
    #[must_use]
    pub fn new_game_gear() -> Self {
        Self::new_inner(VdpRegion::Ntsc, VdpVariant::Sms2, true)
    }

    fn new_inner(region: VdpRegion, variant: VdpVariant, is_game_gear: bool) -> Self {
        Self {
            vram: [0; 16384],
            cram: [0; 64],
            cram_latch: 0,
            is_game_gear,
            regs: [0; 11],
            status: 0,
            read_buffer: 0,
            address: 0,
            code: 0,
            latch_first: true,
            latch_value: 0,
            v_counter: 0,
            h_counter: 0,
            line_counter: 0,
            line_irq_pending: false,
            scanline: 0,
            dot: 0,
            region,
            variant,
            framebuffer: vec![0; (FB_WIDTH * FB_HEIGHT) as usize],
            sprite_buf: [0; 256],
            interrupt: false,
            frame_count: 0,
        }
    }

    /// The current framebuffer (256×192 ARGB32).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    #[must_use]
    pub const fn framebuffer_width(&self) -> u32 {
        FB_WIDTH
    }
    #[must_use]
    pub const fn framebuffer_height(&self) -> u32 {
        FB_HEIGHT
    }

    fn lines_per_frame(&self) -> u16 {
        match self.region {
            VdpRegion::Ntsc => 262,
            VdpRegion::Pal => 313,
        }
    }

    fn mode4_active(&self) -> bool {
        self.regs[0] & 0x04 != 0
    }

    fn display_enabled(&self) -> bool {
        self.regs[1] & 0x40 != 0
    }

    fn backdrop_color(&self) -> u32 {
        // Backdrop from sprite palette (palette 1), entry from reg 7 low nibble
        let idx = (self.regs[7] & 0x0F) as usize + 16;
        self.cram_to_argb(idx)
    }

    /// Read-only access to CRAM (Colour RAM).
    ///
    /// SMS: 32 bytes (6-bit RGB), Game Gear: 64 bytes (12-bit RGB).
    pub fn cram(&self) -> &[u8] {
        if self.is_game_gear {
            &self.cram[..64]
        } else {
            &self.cram[..32]
        }
    }

    /// Whether this VDP is in Game Gear mode.
    pub fn is_game_gear(&self) -> bool {
        self.is_game_gear
    }

    fn cram_to_argb(&self, index: usize) -> u32 {
        if self.is_game_gear {
            // 12-bit RGB: low byte = xxxxGGGGRRRR, high byte = xxxxBBBB
            let lo = self.cram[(index * 2) & 0x3F] as u32;
            let hi = self.cram[(index * 2 + 1) & 0x3F] as u32;
            let r = (lo & 0x0F) * 17;
            let g = ((lo >> 4) & 0x0F) * 17;
            let b = (hi & 0x0F) * 17;
            0xFF00_0000 | (r << 16) | (g << 8) | b
        } else {
            // 6-bit RGB: %00BBGGRR
            let c = self.cram[index & 0x1F] as u32;
            let r = (c & 0x03) * 85;
            let g = ((c >> 2) & 0x03) * 85;
            let b = ((c >> 4) & 0x03) * 85;
            0xFF00_0000 | (r << 16) | (g << 8) | b
        }
    }

    // -----------------------------------------------------------------------
    // I/O
    // -----------------------------------------------------------------------

    /// Read VDP data port ($BE).
    pub fn read_data(&mut self) -> u8 {
        self.latch_first = true;
        let result = self.read_buffer;
        self.read_buffer = self.vram[self.address as usize & 0x3FFF];
        self.address = (self.address + 1) & 0x3FFF;
        result
    }

    /// Write VDP data port ($BE).
    pub fn write_data(&mut self, value: u8) {
        self.latch_first = true;

        match self.code {
            3 => {
                // CRAM write
                if self.is_game_gear {
                    let addr = self.address as usize & 0x3F;
                    if addr & 1 == 0 {
                        self.cram_latch = value;
                    } else {
                        self.cram[addr & 0xFE] = self.cram_latch;
                        self.cram[addr] = value;
                    }
                } else {
                    self.cram[self.address as usize & 0x1F] = value;
                }
            }
            _ => {
                // VRAM write
                self.vram[self.address as usize & 0x3FFF] = value;
            }
        }
        self.read_buffer = value;
        self.address = (self.address + 1) & 0x3FFF;
    }

    /// Read VDP control/status port ($BF).
    pub fn read_status(&mut self) -> u8 {
        self.latch_first = true;
        let result = self.status;
        self.status = 0;
        self.line_irq_pending = false;
        self.interrupt = false;
        result
    }

    /// Write VDP control port ($BF).
    pub fn write_control(&mut self, value: u8) {
        if self.latch_first {
            self.latch_value = value;
            self.latch_first = false;
            // Update address low byte immediately
            self.address = (self.address & 0x3F00) | u16::from(value);
            return;
        }

        self.latch_first = true;
        self.address = u16::from(self.latch_value) | (u16::from(value & 0x3F) << 8);
        self.code = (value >> 6) & 0x03;

        match self.code {
            0 => {
                // VRAM read setup — pre-fetch
                self.read_buffer = self.vram[self.address as usize & 0x3FFF];
                self.address = (self.address + 1) & 0x3FFF;
            }
            2 => {
                // Register write
                let reg = (value & 0x0F) as usize;
                if reg < self.regs.len() {
                    self.regs[reg] = self.latch_value;
                }
                self.update_interrupt();
            }
            _ => {} // Code 1 (VRAM write) or 3 (CRAM write) — just set code
        }
    }

    /// Read V counter ($7E).
    #[must_use]
    pub fn read_v_counter(&self) -> u8 {
        self.v_counter as u8
    }

    /// Read H counter ($7F).
    #[must_use]
    pub fn read_h_counter(&self) -> u8 {
        self.h_counter
    }

    /// Direct VRAM access.
    #[must_use]
    pub fn vram(&self) -> &[u8; 16384] {
        &self.vram
    }

    /// Direct VRAM write.
    pub fn write_vram(&mut self, addr: u16, value: u8) {
        self.vram[addr as usize & 0x3FFF] = value;
    }

    // -----------------------------------------------------------------------
    // Timing
    // -----------------------------------------------------------------------

    /// Fill the entire framebuffer with the current backdrop colour.
    /// Called at frame start so top + bottom border regions plus the
    /// left + right columns of each active row carry the border
    /// colour. Mid-frame backdrop changes affect the *next* frame —
    /// a v1 simplification, matches the TMS9918 family's treatment.
    fn fill_border(&mut self) {
        let backdrop = self.backdrop_color();
        self.framebuffer.fill(backdrop);
    }

    /// Tick one dot (pixel clock, ~5.37 MHz). Renders the active pixel scanned
    /// out at this dot, and processes the per-line events (line/frame interrupt,
    /// V counter) at line end. Returns true at frame end.
    ///
    /// Per dot, the line interrupt is flagged at the *end* of the line it
    /// belongs to, so a host that interleaves the CPU per dot sees it at the
    /// right scanline — the timing that makes Mode-4 raster splits land
    /// correctly. For a static frame the framebuffer is identical to the old
    /// scanline-batched render (both route every pixel through `bg_pixel`).
    pub fn tick(&mut self) -> bool {
        if self.scanline == 0 && self.dot == 0 {
            self.fill_border();
        }
        if self.scanline < 192 && self.dot == 0 {
            self.prepare_line_sprites(self.scanline as usize);
        }
        if self.scanline < 192 && self.dot < ACTIVE_WIDTH as u16 {
            self.render_pixel(self.scanline as usize, self.dot as usize);
        }
        self.dot += 1;
        if self.dot >= 342 {
            self.dot = 0;
            return self.advance_line();
        }
        false
    }

    /// Tick one whole scanline (batch render). Kept for tests and any per-line
    /// host; produces identical output to the per-dot path for a static frame.
    pub fn tick_scanline(&mut self) -> bool {
        if self.scanline == 0 {
            self.fill_border();
        }
        if self.scanline < 192 {
            self.render_scanline(self.scanline);
        }
        self.advance_line()
    }

    /// End-of-line events: line counter / frame interrupt, V counter, interrupt
    /// recompute, and the scanline advance. Returns true at frame end. Shared by
    /// [`tick`](Self::tick) and [`tick_scanline`](Self::tick_scanline).
    fn advance_line(&mut self) -> bool {
        let active_lines: u16 = 192;
        if self.scanline < active_lines {
            if self.line_counter == 0 {
                self.line_counter = self.regs[10];
                self.line_irq_pending = true;
            } else {
                self.line_counter -= 1;
            }
        } else if self.scanline == active_lines {
            self.status |= 0x80;
            self.line_counter = self.regs[10];
            self.frame_count += 1;
        } else {
            self.line_counter = self.regs[10];
        }

        self.v_counter = match self.region {
            VdpRegion::Ntsc => {
                if self.scanline <= 0xDA {
                    self.scanline
                } else {
                    self.scanline.wrapping_sub(6)
                }
            }
            VdpRegion::Pal => {
                if self.scanline <= 0xF2 {
                    self.scanline
                } else {
                    self.scanline.wrapping_sub(57)
                }
            }
        };

        self.update_interrupt();

        self.scanline += 1;
        if self.scanline >= self.lines_per_frame() {
            self.scanline = 0;
            return true;
        }
        false
    }

    fn update_interrupt(&mut self) {
        let frame_irq = self.status & 0x80 != 0 && self.regs[1] & 0x20 != 0;
        let line_irq = self.line_irq_pending && self.regs[0] & 0x10 != 0;
        self.interrupt = frame_irq || line_irq;
    }

    // -----------------------------------------------------------------------
    // Rendering
    // -----------------------------------------------------------------------

    fn active_offset(line: usize) -> usize {
        (BORDER_TOP as usize + line) * FB_WIDTH as usize + BORDER_LEFT as usize
    }

    fn render_scanline(&mut self, line: u16) {
        let line = line as usize;
        self.prepare_line_sprites(line);
        for x in 0..ACTIVE_WIDTH as usize {
            self.render_pixel(line, x);
        }
    }

    /// Draw the active pixel at column `x` of `line`. In Mode 4 the background
    /// and this line's sprite pixel are arbitrated by the SMS priority rule:
    /// the sprite is shown **unless** an *opaque* background pixel
    /// (`color_idx != 0`) belongs to a tile whose priority bit is set — then the
    /// foreground background tile occludes the sprite (status bars, HUD layers).
    /// Background `color_idx` 0 is transparent for this comparison, so sprites
    /// always show through it regardless of the priority bit.
    fn render_pixel(&mut self, line: usize, x: usize) {
        let sprite = self.sprite_buf[x];
        let argb = if self.display_enabled() && self.mode4_active() {
            let (bg_idx, bg_priority, palette) = self.mode4_bg_lookup(line, x);
            let bg_opaque = bg_idx != 0;
            if sprite != 0 && !(bg_priority && bg_opaque) {
                self.cram_to_argb(16 + sprite as usize)
            } else if bg_opaque || bg_priority {
                self.cram_to_argb(palette + bg_idx as usize)
            } else {
                self.backdrop_color()
            }
        } else if sprite != 0 {
            self.cram_to_argb(16 + sprite as usize)
        } else {
            self.bg_pixel(line, x)
        };
        self.framebuffer[Self::active_offset(line) + x] = argb;
    }

    /// Background colour when active Mode-4 rendering is not in effect (display
    /// blanked or a placeholder legacy TMS9918 mode) — both render as backdrop.
    fn bg_pixel(&self, _line: usize, _x: usize) -> u32 {
        self.backdrop_color()
    }

    /// Background colour-index, tile priority bit, and palette base at column
    /// `pixel_x` of `line` in Mode 4. `color_idx` 0 is transparent for sprite
    /// priority; `priority` is the tile's foreground bit. The colour itself is
    /// resolved by the caller so it can arbitrate against the sprite pixel.
    fn mode4_bg_lookup(&self, line: usize, pixel_x: usize) -> (u8, bool, usize) {
        // Column-0 blanking: the masked column reads as transparent backdrop.
        if self.regs[0] & 0x20 != 0 && pixel_x < 8 {
            return (0, false, 0);
        }

        let name_base = (self.regs[2] as usize & 0x0E) * 0x400;
        let scroll_x = self.regs[8] as usize;
        let scroll_y = self.regs[9] as usize;
        let hscroll_lock = self.regs[0] & 0x40 != 0;

        let effective_line = (line + scroll_y) % 224; // Name table wraps at 224 (28 rows)
        let tile_row = effective_line / 8;
        let fine_y = effective_line & 7;

        // Horizontal scroll (disabled for the top 2 rows if hscroll_lock).
        let scrolled_x = if hscroll_lock && line < 16 {
            pixel_x
        } else {
            (pixel_x + (256 - scroll_x)) & 0xFF
        };
        let tile_col = scrolled_x / 8;
        let fine_x = scrolled_x & 7;

        // Name table entry (2 bytes, little-endian).
        let nt_addr = name_base + (tile_row * 32 + tile_col) * 2;
        let nt_lo = self.vram[nt_addr & 0x3FFF] as u16;
        let nt_hi = self.vram[(nt_addr + 1) & 0x3FFF] as u16;
        let nt_entry = nt_lo | (nt_hi << 8);

        let pattern_idx = (nt_entry & 0x01FF) as usize;
        let h_flip = nt_entry & 0x0200 != 0;
        let v_flip = nt_entry & 0x0400 != 0;
        let palette = if nt_entry & 0x0800 != 0 { 16 } else { 0 };
        let priority = nt_entry & 0x1000 != 0;

        let row = if v_flip { 7 - fine_y } else { fine_y };
        let col = if h_flip { fine_x } else { 7 - fine_x };

        // 4bpp planar: 4 bytes per row, 32 bytes per tile.
        let pattern_addr = pattern_idx * 32 + row * 4;
        let b0 = self.vram[pattern_addr & 0x3FFF];
        let b1 = self.vram[(pattern_addr + 1) & 0x3FFF];
        let b2 = self.vram[(pattern_addr + 2) & 0x3FFF];
        let b3 = self.vram[(pattern_addr + 3) & 0x3FFF];

        let color_idx = ((b0 >> col) & 1)
            | (((b1 >> col) & 1) << 1)
            | (((b2 >> col) & 1) << 2)
            | (((b3 >> col) & 1) << 3);

        (color_idx, priority, palette)
    }

    /// Prepare this line's sprite overlay: clear `sprite_buf`, then (when the
    /// display is on and Mode 4 is active) evaluate the sprite table into it and
    /// set the overflow / collision status flags.
    fn prepare_line_sprites(&mut self, line: usize) {
        self.sprite_buf = [0u8; 256];
        if self.display_enabled() && self.mode4_active() {
            self.evaluate_sprites(line);
        }
    }

    fn evaluate_sprites(&mut self, line: usize) {
        let sat_base = (self.regs[5] as usize & 0x7E) * 0x80;
        let spg_base = if self.regs[6] & 0x04 != 0 {
            0x2000
        } else {
            0x0000
        };
        let tall_sprites = self.regs[1] & 0x02 != 0;
        let sprite_height: usize = if tall_sprites { 16 } else { 8 };
        let shift_left = self.regs[0] & 0x08 != 0;

        let mut sprite_buffer = [0u8; 256]; // Color index per pixel
        let mut sprites_on_line = 0u8;
        let mut collision = false;

        for sprite in 0..64 {
            let y_raw = self.vram[(sat_base + sprite) & 0x3FFF];

            // $D0 terminates in 192-line mode
            if y_raw == 0xD0 {
                break;
            }

            let y = y_raw as usize + 1;
            if line < y || line >= y + sprite_height {
                continue;
            }

            sprites_on_line += 1;
            if sprites_on_line > 8 {
                self.status |= 0x40;
                break;
            }

            // X and pattern from second half of SAT
            let x_addr = sat_base + 0x80 + sprite * 2;
            let mut x = self.vram[x_addr & 0x3FFF] as i16;
            let mut pattern = self.vram[(x_addr + 1) & 0x3FFF] as usize;

            if shift_left {
                x -= 8;
            }
            if tall_sprites {
                pattern &= 0xFE;
            }

            let sprite_row = line - y;
            let pattern_addr = spg_base + pattern * 32 + sprite_row * 4;

            let b0 = self.vram[(pattern_addr) & 0x3FFF];
            let b1 = self.vram[(pattern_addr + 1) & 0x3FFF];
            let b2 = self.vram[(pattern_addr + 2) & 0x3FFF];
            let b3 = self.vram[(pattern_addr + 3) & 0x3FFF];

            for bit in 0..8 {
                let px = x + bit as i16;
                if !(0..256).contains(&px) {
                    continue;
                }
                let px = px as usize;

                let col = 7 - bit;
                let color_idx = ((b0 >> col) & 1)
                    | (((b1 >> col) & 1) << 1)
                    | (((b2 >> col) & 1) << 2)
                    | (((b3 >> col) & 1) << 3);

                if color_idx == 0 {
                    continue;
                }

                if sprite_buffer[px] != 0 {
                    collision = true;
                } else {
                    sprite_buffer[px] = color_idx;
                }
            }
        }

        if collision {
            self.status |= 0x20;
        }

        // Publish the line's sprite pixels; `render_pixel` overlays them.
        self.sprite_buf = sprite_buffer;
    }

    // -----------------------------------------------------------------------
    // Save/load state
    // -----------------------------------------------------------------------

    /// Serialize VDP state to a byte vector.
    ///
    /// Layout: regs (11) + status (1) + read_buffer (1) + address (2) +
    /// code (1) + latch_first (1) + latch_value (1) + cram_latch (1) +
    /// v_counter (2) + h_counter (1) + line_counter (1) + line_irq_pending (1) +
    /// scanline (2) + interrupt (1) + frame_count (8) +
    /// vram (16384) + cram (64) = 16483 bytes.
    pub fn save_state(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.regs);
        out.push(self.status);
        out.push(self.read_buffer);
        out.extend_from_slice(&self.address.to_le_bytes());
        out.push(self.code);
        out.push(u8::from(self.latch_first));
        out.push(self.latch_value);
        out.push(self.cram_latch);
        out.extend_from_slice(&self.v_counter.to_le_bytes());
        out.push(self.h_counter);
        out.push(self.line_counter);
        out.push(u8::from(self.line_irq_pending));
        out.extend_from_slice(&self.scanline.to_le_bytes());
        out.push(u8::from(self.interrupt));
        out.extend_from_slice(&self.frame_count.to_le_bytes());
        out.extend_from_slice(&self.vram);
        out.extend_from_slice(&self.cram);
    }

    /// Restore VDP state from a byte slice. Returns bytes consumed or error.
    pub fn load_state(&mut self, data: &[u8]) -> Result<usize, String> {
        let needed = 11 + 1 + 1 + 2 + 1 + 1 + 1 + 1 + 2 + 1 + 1 + 1 + 2 + 1 + 8 + 16384 + 64;
        if data.len() < needed {
            return Err("SegaVdp state truncated".into());
        }
        let mut p = 0;
        self.regs.copy_from_slice(&data[p..p + 11]);
        p += 11;
        self.status = data[p];
        p += 1;
        self.read_buffer = data[p];
        p += 1;
        self.address = u16::from_le_bytes([data[p], data[p + 1]]);
        p += 2;
        self.code = data[p];
        p += 1;
        self.latch_first = data[p] != 0;
        p += 1;
        self.latch_value = data[p];
        p += 1;
        self.cram_latch = data[p];
        p += 1;
        self.v_counter = u16::from_le_bytes([data[p], data[p + 1]]);
        p += 2;
        self.h_counter = data[p];
        p += 1;
        self.line_counter = data[p];
        p += 1;
        self.line_irq_pending = data[p] != 0;
        p += 1;
        self.scanline = u16::from_le_bytes([data[p], data[p + 1]]);
        p += 2;
        self.interrupt = data[p] != 0;
        p += 1;
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&data[p..p + 8]);
        p += 8;
        self.frame_count = u64::from_le_bytes(bytes);
        self.vram.copy_from_slice(&data[p..p + 16384]);
        p += 16384;
        self.cram.copy_from_slice(&data[p..p + 64]);
        p += 64;
        Ok(p)
    }

    /// Read-only access to registers.
    #[must_use]
    pub fn registers(&self) -> &[u8; 11] {
        &self.regs
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_vdp_has_blank_framebuffer() {
        let vdp = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2);
        assert_eq!(vdp.framebuffer().len(), (FB_WIDTH * FB_HEIGHT) as usize);
    }

    #[test]
    fn control_port_register_write() {
        let mut vdp = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2);
        vdp.write_control(0x44); // value
        vdp.write_control(0x81); // register 1
        assert_eq!(vdp.regs[1], 0x44);
    }

    #[test]
    fn vram_write_and_read() {
        let mut vdp = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2);
        // Set write address $0000 (code 01)
        vdp.write_control(0x00);
        vdp.write_control(0x40);
        vdp.write_data(0xAB);
        vdp.write_data(0xCD);
        assert_eq!(vdp.vram[0], 0xAB);
        assert_eq!(vdp.vram[1], 0xCD);
    }

    #[test]
    fn cram_write_sms() {
        let mut vdp = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2);
        // Set CRAM write address $00 (code 11 = $C0)
        vdp.write_control(0x00);
        vdp.write_control(0xC0);
        vdp.write_data(0x3F); // White-ish (R=3, G=3, B=3)
        assert_eq!(vdp.cram[0], 0x3F);
    }

    #[test]
    fn cram_write_game_gear() {
        let mut vdp = SegaVdp::new_game_gear();
        // Set CRAM write address $00
        vdp.write_control(0x00);
        vdp.write_control(0xC0);
        vdp.write_data(0xF0); // Even byte: GG=F, RR=0
        vdp.write_data(0x0F); // Odd byte: BB=F
        // Should write to CRAM[0] and CRAM[1]
        assert_eq!(vdp.cram[0], 0xF0);
        assert_eq!(vdp.cram[1], 0x0F);
    }

    #[test]
    fn status_clears_on_read() {
        let mut vdp = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2);
        vdp.status = 0xE0; // All flags set
        let s = vdp.read_status();
        assert_eq!(s, 0xE0);
        assert_eq!(vdp.status, 0);
    }

    #[test]
    fn ntsc_frame_is_262_lines() {
        let mut vdp = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2);
        let mut frames = 0;
        for _ in 0..262 {
            if vdp.tick_scanline() {
                frames += 1;
            }
        }
        assert_eq!(frames, 1);
    }

    #[test]
    fn pal_frame_is_313_lines() {
        let mut vdp = SegaVdp::new(VdpRegion::Pal, VdpVariant::Sms2);
        let mut frames = 0;
        for _ in 0..313 {
            if vdp.tick_scanline() {
                frames += 1;
            }
        }
        assert_eq!(frames, 1);
    }

    #[test]
    fn mode4_detection() {
        let mut vdp = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2);
        assert!(!vdp.mode4_active());
        vdp.regs[0] = 0x04;
        assert!(vdp.mode4_active());
    }

    #[test]
    fn mode4_priority_tile_occludes_sprite() {
        // SMS BG-over-sprite priority: a sprite shows unless an opaque
        // background pixel belongs to a tile whose priority bit is set.
        fn setup(priority: bool, bg_opaque: bool) -> SegaVdp {
            let mut vdp = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2);
            vdp.regs[0] = 0x04; // Mode 4; no column-0 blank, no hscroll lock
            vdp.regs[1] = 0x40; // display on
            vdp.regs[2] = 0x00; // name table base $0000
            // Tile (0,0): pattern 1, palette 0, priority optional.
            let nt_entry: u16 = 0x0001 | if priority { 0x1000 } else { 0 };
            vdp.vram[0] = (nt_entry & 0xFF) as u8;
            vdp.vram[1] = (nt_entry >> 8) as u8;
            // Pattern 1, row 0: leftmost pixel (col 7) = colour index 1 if opaque.
            vdp.vram[32] = if bg_opaque { 0x80 } else { 0x00 };
            vdp.cram[1] = 0x3F; // BG colour 1 = white
            vdp.cram[16 + 5] = 0x03; // sprite colour 5 = red
            vdp.sprite_buf[0] = 5; // a sprite pixel at column 0
            vdp
        }
        let fb = SegaVdp::active_offset(0); // column 0 of the active line

        // Opaque, high-priority background occludes the sprite.
        let mut vdp = setup(true, true);
        vdp.render_pixel(0, 0);
        assert_eq!(
            vdp.framebuffer[fb],
            vdp.cram_to_argb(1),
            "opaque priority tile should occlude the sprite"
        );

        // Without the priority bit, the sprite shows over the same tile.
        let mut vdp = setup(false, true);
        vdp.render_pixel(0, 0);
        assert_eq!(
            vdp.framebuffer[fb],
            vdp.cram_to_argb(16 + 5),
            "non-priority tile must not occlude the sprite"
        );

        // Priority bit set but the background pixel is transparent (index 0):
        // a transparent BG pixel is never in front, so the sprite shows.
        let mut vdp = setup(true, false);
        vdp.render_pixel(0, 0);
        assert_eq!(
            vdp.framebuffer[fb],
            vdp.cram_to_argb(16 + 5),
            "transparent background (index 0) never occludes, even with priority"
        );
    }

    #[test]
    fn sms_palette_conversion() {
        let mut vdp = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2);
        // White: R=3, G=3, B=3 = $3F
        vdp.cram[0] = 0x3F;
        let argb = vdp.cram_to_argb(0);
        assert_eq!(argb, 0xFF_FF_FF_FF);

        // Black: $00
        vdp.cram[1] = 0x00;
        let argb = vdp.cram_to_argb(1);
        assert_eq!(argb, 0xFF_00_00_00);
    }

    #[test]
    fn gg_palette_conversion() {
        let mut vdp = SegaVdp::new_game_gear();
        // White: R=F, G=F, B=F
        vdp.cram[0] = 0xFF; // GGRR = FF
        vdp.cram[1] = 0x0F; // BB = F
        let argb = vdp.cram_to_argb(0);
        assert_eq!(argb, 0xFF_FF_FF_FF);
    }

    #[test]
    fn line_interrupt_counter() {
        let mut vdp = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2);
        vdp.regs[1] = 0x40; // Display on
        vdp.regs[0] = 0x14; // Mode 4 + line IRQ enable
        vdp.regs[10] = 5; // Fire every 5 lines

        // Tick 6 scanlines — counter should reach 0 and fire
        for _ in 0..6 {
            vdp.tick_scanline();
        }
        assert!(vdp.line_irq_pending);
        assert!(vdp.interrupt);
    }
}
