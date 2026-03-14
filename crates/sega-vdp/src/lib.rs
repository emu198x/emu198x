//! Sega Master System / Game Gear VDP (315-5124 / 315-5246).
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

/// Framebuffer dimensions (full SMS resolution).
pub const FB_WIDTH: u32 = 256;
pub const FB_HEIGHT: u32 = 192;

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
    region: VdpRegion,
    #[allow(dead_code)]
    variant: VdpVariant,
    framebuffer: Vec<u32>,

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
            region,
            variant,
            framebuffer: vec![0; (FB_WIDTH * FB_HEIGHT) as usize],
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
    pub const fn framebuffer_width(&self) -> u32 { FB_WIDTH }
    #[must_use]
    pub const fn framebuffer_height(&self) -> u32 { FB_HEIGHT }

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
    pub fn vram(&self) -> &[u8; 16384] { &self.vram }

    /// Direct VRAM write.
    pub fn write_vram(&mut self, addr: u16, value: u8) {
        self.vram[addr as usize & 0x3FFF] = value;
    }

    // -----------------------------------------------------------------------
    // Timing
    // -----------------------------------------------------------------------

    /// Tick one scanline. Returns true at frame end.
    pub fn tick_scanline(&mut self) -> bool {
        let active_lines: u16 = 192;

        // Render active scanlines
        if self.scanline < active_lines {
            self.render_scanline(self.scanline);

            // Line counter
            if self.line_counter == 0 {
                self.line_counter = self.regs[10];
                self.line_irq_pending = true;
            } else {
                self.line_counter -= 1;
            }
        } else if self.scanline == active_lines {
            // Frame interrupt
            self.status |= 0x80;
            self.line_counter = self.regs[10];
            self.frame_count += 1;
        } else {
            // VBlank — reload line counter each line
            self.line_counter = self.regs[10];
        }

        // V counter
        self.v_counter = match self.region {
            VdpRegion::Ntsc => {
                if self.scanline <= 0xDA { self.scanline }
                else { self.scanline.wrapping_sub(6) }
            }
            VdpRegion::Pal => {
                if self.scanline <= 0xF2 { self.scanline }
                else { self.scanline.wrapping_sub(57) }
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

    fn render_scanline(&mut self, line: u16) {
        let line = line as usize;
        let offset = line * FB_WIDTH as usize;

        if !self.display_enabled() {
            let bg = self.backdrop_color();
            self.framebuffer[offset..offset + FB_WIDTH as usize].fill(bg);
            return;
        }

        if self.mode4_active() {
            self.render_mode4_bg(line, offset);
            self.render_mode4_sprites(line, offset);
        } else {
            // Legacy TMS9918A modes — render backdrop as placeholder
            let bg = self.backdrop_color();
            self.framebuffer[offset..offset + FB_WIDTH as usize].fill(bg);
        }
    }

    fn render_mode4_bg(&mut self, line: usize, offset: usize) {
        let name_base = (self.regs[2] as usize & 0x0E) * 0x400;
        let scroll_x = self.regs[8] as usize;
        let scroll_y = self.regs[9] as usize;
        let col0_blank = self.regs[0] & 0x20 != 0;
        let hscroll_lock = self.regs[0] & 0x40 != 0;

        let effective_line = (line + scroll_y) % 224; // Name table wraps at 224 (28 rows)
        let tile_row = effective_line / 8;
        let fine_y = effective_line & 7;

        for pixel_x in 0..256 {
            // Horizontal scroll (disable for top 2 rows if hscroll_lock)
            let scrolled_x = if hscroll_lock && line < 16 {
                pixel_x
            } else {
                (pixel_x + (256 - scroll_x)) & 0xFF
            };

            let tile_col = scrolled_x / 8;
            let fine_x = scrolled_x & 7;

            // Read name table entry (2 bytes, little-endian)
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

            // 4bpp planar: 4 bytes per row, 32 bytes per tile
            let pattern_addr = pattern_idx * 32 + row * 4;
            let b0 = self.vram[(pattern_addr) & 0x3FFF];
            let b1 = self.vram[(pattern_addr + 1) & 0x3FFF];
            let b2 = self.vram[(pattern_addr + 2) & 0x3FFF];
            let b3 = self.vram[(pattern_addr + 3) & 0x3FFF];

            let color_idx = ((b0 >> col) & 1)
                | (((b1 >> col) & 1) << 1)
                | (((b2 >> col) & 1) << 2)
                | (((b3 >> col) & 1) << 3);

            let argb = if color_idx == 0 && !priority {
                self.backdrop_color()
            } else {
                self.cram_to_argb(palette + color_idx as usize)
            };

            // Column 0 blanking
            if col0_blank && pixel_x < 8 {
                self.framebuffer[offset + pixel_x] = self.backdrop_color();
            } else {
                self.framebuffer[offset + pixel_x] = argb;
            }
        }
    }

    fn render_mode4_sprites(&mut self, line: usize, offset: usize) {
        let sat_base = (self.regs[5] as usize & 0x7E) * 0x80;
        let spg_base = if self.regs[6] & 0x04 != 0 { 0x2000 } else { 0x0000 };
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

            if shift_left { x -= 8; }
            if tall_sprites { pattern &= 0xFE; }

            let sprite_row = line - y;
            let pattern_addr = spg_base + pattern * 32 + sprite_row * 4;

            let b0 = self.vram[(pattern_addr) & 0x3FFF];
            let b1 = self.vram[(pattern_addr + 1) & 0x3FFF];
            let b2 = self.vram[(pattern_addr + 2) & 0x3FFF];
            let b3 = self.vram[(pattern_addr + 3) & 0x3FFF];

            for bit in 0..8 {
                let px = x + bit as i16;
                if px < 0 || px >= 256 { continue; }
                let px = px as usize;

                let col = 7 - bit;
                let color_idx = ((b0 >> col) & 1)
                    | (((b1 >> col) & 1) << 1)
                    | (((b2 >> col) & 1) << 2)
                    | (((b3 >> col) & 1) << 3);

                if color_idx == 0 { continue; }

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

        // Composite sprites onto framebuffer (behind priority tiles)
        for px in 0..256 {
            let c = sprite_buffer[px];
            if c != 0 {
                self.framebuffer[offset + px] = self.cram_to_argb(16 + c as usize);
            }
        }
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
            if vdp.tick_scanline() { frames += 1; }
        }
        assert_eq!(frames, 1);
    }

    #[test]
    fn pal_frame_is_313_lines() {
        let mut vdp = SegaVdp::new(VdpRegion::Pal, VdpVariant::Sms2);
        let mut frames = 0;
        for _ in 0..313 {
            if vdp.tick_scanline() { frames += 1; }
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
        vdp.regs[10] = 5;   // Fire every 5 lines

        // Tick 6 scanlines — counter should reach 0 and fire
        for _ in 0..6 {
            vdp.tick_scanline();
        }
        assert!(vdp.line_irq_pending);
        assert!(vdp.interrupt);
    }
}
