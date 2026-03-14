//! Texas Instruments TMS9918A/9928A/9929A Video Display Processor.
//!
//! The TMS9918 family is a tile-and-sprite video chip with 16 KB of dedicated
//! VRAM, accessed through two I/O ports (data and control). It supports four
//! display modes (Graphics I, Graphics II, Text, Multicolor), 32 sprites with
//! per-line limits and collision detection, and generates a VBlank interrupt.
//!
//! Used by ColecoVision, SG-1000, MSX, TI-99/4A, Sord M5, Memotech MTX,
//! Spectravideo SV-318/328, and others. The Sega Master System VDP and
//! Yamaha V9938/V9958 are direct descendants.
//!
//! # Variants
//!
//! | Variant   | Output          | Lines/frame | Systems                    |
//! |-----------|-----------------|-------------|----------------------------|
//! | TMS9918A  | Composite NTSC  | 262         | ColecoVision, SG-1000, M5  |
//! | TMS9928A  | Component NTSC  | 262         | MSX (Japan)                |
//! | TMS9929A  | Component PAL   | 313         | MSX (Europe), CV PAL       |
//!
//! From the programmer's perspective, the only difference is frame timing.

#![allow(clippy::cast_possible_truncation)]

// ---------------------------------------------------------------------------
// Color palette
// ---------------------------------------------------------------------------

/// Fixed 15-color palette (plus transparent). ARGB32 format.
const PALETTE: [u32; 16] = [
    0x0000_0000, // 0: Transparent
    0xFF00_0000, // 1: Black
    0xFF21_C842, // 2: Medium Green
    0xFF5E_DC78, // 3: Light Green
    0xFF54_55ED, // 4: Dark Blue
    0xFF7D_76FC, // 5: Light Blue
    0xFFD4_524D, // 6: Dark Red
    0xFF42_EBF5, // 7: Cyan
    0xFFFC_5554, // 8: Medium Red
    0xFFFF_7978, // 9: Light Red
    0xFFD4_C154, // 10: Dark Yellow
    0xFFE6_CE80, // 11: Light Yellow
    0xFF21_B03B, // 12: Dark Green
    0xFFC9_5BBA, // 13: Magenta
    0xFFCC_CCCC, // 14: Gray
    0xFFFF_FFFF, // 15: White
];

// ---------------------------------------------------------------------------
// Region
// ---------------------------------------------------------------------------

/// VDP region — determines frame timing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VdpRegion {
    /// NTSC: 262 scanlines, ~59.94 Hz (TMS9918A / TMS9928A).
    Ntsc,
    /// PAL: 313 scanlines, ~50.16 Hz (TMS9929A).
    Pal,
}

impl VdpRegion {
    /// Total scanlines per frame.
    #[must_use]
    pub const fn lines_per_frame(self) -> u16 {
        match self {
            Self::Ntsc => 262,
            Self::Pal => 313,
        }
    }
}

// ---------------------------------------------------------------------------
// Display mode
// ---------------------------------------------------------------------------

/// Active display mode, derived from M1/M2/M3 mode bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    GraphicsI,
    GraphicsII,
    Text,
    Multicolor,
}

// ---------------------------------------------------------------------------
// VDP
// ---------------------------------------------------------------------------

/// Framebuffer dimensions.
pub const FB_WIDTH: u32 = 256;
pub const FB_HEIGHT: u32 = 192;

/// TMS9918 Video Display Processor.
pub struct Tms9918 {
    // VRAM
    vram: [u8; 16384],

    // Control registers (VR0-VR7)
    regs: [u8; 8],

    // Status register
    status: u8,

    // I/O port state
    /// Read-ahead buffer for data port reads.
    read_buffer: u8,
    /// 14-bit VRAM address register.
    address: u16,
    /// First/second byte latch for control port writes.
    latch_first: bool,
    /// First byte stored during two-byte control write.
    latch_value: u8,

    // Rendering state
    /// Current scanline (0-based).
    scanline: u16,
    /// Current dot within scanline (0-341).
    dot: u16,
    /// Region (NTSC or PAL).
    region: VdpRegion,

    /// Framebuffer: 256×192 ARGB32 pixels.
    framebuffer: Vec<u32>,

    /// Whether an interrupt is being asserted (active-low INT pin).
    pub interrupt: bool,

    /// Frame counter (increments at VBlank).
    pub frame_count: u64,
}

impl Tms9918 {
    /// Create a new VDP with the given region.
    #[must_use]
    pub fn new(region: VdpRegion) -> Self {
        Self {
            vram: [0; 16384],
            regs: [0; 8],
            status: 0,
            read_buffer: 0,
            address: 0,
            latch_first: true,
            latch_value: 0,
            scanline: 0,
            dot: 0,
            region,
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

    /// Framebuffer width.
    #[must_use]
    pub const fn framebuffer_width(&self) -> u32 {
        FB_WIDTH
    }

    /// Framebuffer height.
    #[must_use]
    pub const fn framebuffer_height(&self) -> u32 {
        FB_HEIGHT
    }

    // -----------------------------------------------------------------------
    // I/O ports
    // -----------------------------------------------------------------------

    /// Read the data port. Returns the read-ahead buffer, then refills it
    /// from VRAM[address] and increments the address.
    pub fn read_data(&mut self) -> u8 {
        self.latch_first = true;
        let result = self.read_buffer;
        self.read_buffer = self.vram[self.address as usize & 0x3FFF];
        self.address = (self.address + 1) & 0x3FFF;
        result
    }

    /// Write the data port. Writes to VRAM[address] and increments.
    pub fn write_data(&mut self, value: u8) {
        self.latch_first = true;
        self.vram[self.address as usize & 0x3FFF] = value;
        self.read_buffer = value;
        self.address = (self.address + 1) & 0x3FFF;
    }

    /// Read the control port (status register). Clears flags and resets
    /// the first/second byte latch.
    pub fn read_status(&mut self) -> u8 {
        self.latch_first = true;
        let result = self.status;
        self.status &= 0x1F; // Clear F, 5S, C flags (keep 5th sprite number)
        self.status &= !0x60; // Also clear 5S and C
        self.interrupt = false;
        result
    }

    /// Write the control port. Two-byte sequence: first byte is the value
    /// or address low; second byte selects the operation.
    pub fn write_control(&mut self, value: u8) {
        if self.latch_first {
            self.latch_value = value;
            self.latch_first = false;
            return;
        }

        self.latch_first = true;

        if value & 0x80 != 0 {
            // Register write: bits 2-0 = register number
            let reg = (value & 0x07) as usize;
            self.regs[reg] = self.latch_value;
            // Update interrupt state if IE changed
            if reg == 1 {
                self.update_interrupt();
            }
        } else if value & 0x40 != 0 {
            // VRAM write setup
            self.address =
                u16::from(self.latch_value) | (u16::from(value & 0x3F) << 8);
        } else {
            // VRAM read setup — pre-fetch into read buffer
            self.address =
                u16::from(self.latch_value) | (u16::from(value & 0x3F) << 8);
            self.read_buffer = self.vram[self.address as usize & 0x3FFF];
            self.address = (self.address + 1) & 0x3FFF;
        }
    }

    /// Direct VRAM access for testing and observation.
    #[must_use]
    pub fn vram(&self) -> &[u8; 16384] {
        &self.vram
    }

    /// Direct VRAM write for testing.
    pub fn write_vram(&mut self, addr: u16, value: u8) {
        self.vram[addr as usize & 0x3FFF] = value;
    }

    // -----------------------------------------------------------------------
    // Timing
    // -----------------------------------------------------------------------

    /// Tick one dot (pixel clock). Call at the VDP dot clock rate
    /// (~5.37 MHz). Returns true when a frame is complete.
    pub fn tick(&mut self) -> bool {
        self.dot += 1;
        if self.dot >= 342 {
            self.dot = 0;

            // Render this scanline if it's in the active display area.
            if self.scanline < 192 {
                self.render_scanline(self.scanline);
            }

            // VBlank interrupt at the end of active display.
            if self.scanline == 192 {
                self.status |= 0x80; // Set F flag
                self.update_interrupt();
                self.frame_count += 1;
            }

            self.scanline += 1;
            if self.scanline >= self.region.lines_per_frame() {
                self.scanline = 0;
                return true; // Frame complete
            }
        }
        false
    }

    /// Run for one complete scanline (342 dots). Returns true at frame end.
    pub fn tick_scanline(&mut self) -> bool {
        // Render if active.
        if self.scanline < 192 {
            self.render_scanline(self.scanline);
        }

        if self.scanline == 192 {
            self.status |= 0x80;
            self.update_interrupt();
            self.frame_count += 1;
        }

        self.scanline += 1;
        if self.scanline >= self.region.lines_per_frame() {
            self.scanline = 0;
            return true;
        }
        false
    }

    /// Current scanline.
    #[must_use]
    pub fn scanline(&self) -> u16 {
        self.scanline
    }

    fn update_interrupt(&mut self) {
        let ie = self.regs[1] & 0x20 != 0;
        let f = self.status & 0x80 != 0;
        self.interrupt = ie && f;
    }

    // -----------------------------------------------------------------------
    // Mode detection
    // -----------------------------------------------------------------------

    fn mode(&self) -> Mode {
        let m1 = self.regs[1] & 0x10 != 0;
        let m2 = self.regs[1] & 0x08 != 0;
        let m3 = self.regs[0] & 0x02 != 0;
        match (m1, m2, m3) {
            (true, false, false) => Mode::Text,
            (false, true, false) => Mode::Multicolor,
            (false, false, true) => Mode::GraphicsII,
            _ => Mode::GraphicsI,
        }
    }

    fn display_enabled(&self) -> bool {
        self.regs[1] & 0x40 != 0
    }

    // -----------------------------------------------------------------------
    // Table addresses
    // -----------------------------------------------------------------------

    fn name_table_addr(&self) -> usize {
        (self.regs[2] as usize & 0x0F) * 0x400
    }

    fn color_table_addr(&self) -> usize {
        self.regs[3] as usize * 0x40
    }

    fn pattern_table_addr(&self) -> usize {
        (self.regs[4] as usize & 0x07) * 0x800
    }

    fn sprite_attr_addr(&self) -> usize {
        (self.regs[5] as usize & 0x7F) * 0x80
    }

    fn sprite_pattern_addr(&self) -> usize {
        (self.regs[6] as usize & 0x07) * 0x800
    }

    fn backdrop_color(&self) -> u32 {
        let idx = (self.regs[7] & 0x0F) as usize;
        if idx == 0 { PALETTE[1] } else { PALETTE[idx] }
    }

    // -----------------------------------------------------------------------
    // Scanline rendering
    // -----------------------------------------------------------------------

    fn render_scanline(&mut self, line: u16) {
        let line = line as usize;
        let offset = line * FB_WIDTH as usize;

        if !self.display_enabled() {
            let bg = self.backdrop_color();
            self.framebuffer[offset..offset + FB_WIDTH as usize].fill(bg);
            return;
        }

        match self.mode() {
            Mode::GraphicsI => self.render_graphics_i(line, offset),
            Mode::GraphicsII => self.render_graphics_ii(line, offset),
            Mode::Text => self.render_text(line, offset),
            Mode::Multicolor => self.render_multicolor(line, offset),
        }

        // Sprites (disabled in Text mode).
        if self.mode() != Mode::Text {
            self.render_sprites(line, offset);
        }
    }

    // -- Graphics I --

    fn render_graphics_i(&mut self, line: usize, offset: usize) {
        let name_base = self.name_table_addr();
        let pattern_base = self.pattern_table_addr();
        let color_base = self.color_table_addr();
        let backdrop = self.backdrop_color();

        let tile_row = line / 8;
        let row_in_tile = line & 7;

        for tile_col in 0..32 {
            let name_addr = name_base + tile_row * 32 + tile_col;
            let name = self.vram[name_addr & 0x3FFF] as usize;

            let pattern_byte =
                self.vram[(pattern_base + name * 8 + row_in_tile) & 0x3FFF];

            // Color: one byte per group of 8 tiles
            let color_byte = self.vram[(color_base + name / 8) & 0x3FFF];
            let fg_idx = (color_byte >> 4) as usize;
            let bg_idx = (color_byte & 0x0F) as usize;
            let fg = if fg_idx == 0 { backdrop } else { PALETTE[fg_idx] };
            let bg = if bg_idx == 0 { backdrop } else { PALETTE[bg_idx] };

            for bit in 0..8 {
                let pixel = if pattern_byte & (0x80 >> bit) != 0 { fg } else { bg };
                self.framebuffer[offset + tile_col * 8 + bit] = pixel;
            }
        }
    }

    // -- Graphics II --

    fn render_graphics_ii(&mut self, line: usize, offset: usize) {
        let name_base = self.name_table_addr();
        let backdrop = self.backdrop_color();

        // Graphics II masking
        let pattern_base = (self.regs[4] as usize & 0x04) * 0x800;
        let pattern_mask = ((self.regs[4] as usize & 0x03) << 8) | 0xFF;
        let color_base = (self.regs[3] as usize & 0x80) * 0x40;
        let color_mask = ((self.regs[3] as usize & 0x7F) << 3) | 0x07;

        let tile_row = line / 8;
        let row_in_tile = line & 7;
        let zone = tile_row / 8; // 0, 1, or 2

        for tile_col in 0..32 {
            let name_addr = name_base + tile_row * 32 + tile_col;
            let name = self.vram[name_addr & 0x3FFF] as usize;

            let effective = (name + zone * 256) & pattern_mask;

            let pattern_byte =
                self.vram[(pattern_base + effective * 8 + row_in_tile) & 0x3FFF];

            let color_byte =
                self.vram[(color_base + (effective * 8 + row_in_tile & (color_mask * 8 + 7))) & 0x3FFF];

            let fg_idx = (color_byte >> 4) as usize;
            let bg_idx = (color_byte & 0x0F) as usize;
            let fg = if fg_idx == 0 { backdrop } else { PALETTE[fg_idx] };
            let bg = if bg_idx == 0 { backdrop } else { PALETTE[bg_idx] };

            for bit in 0..8 {
                let pixel = if pattern_byte & (0x80 >> bit) != 0 { fg } else { bg };
                self.framebuffer[offset + tile_col * 8 + bit] = pixel;
            }
        }
    }

    // -- Text --

    fn render_text(&mut self, line: usize, offset: usize) {
        let name_base = self.name_table_addr();
        let pattern_base = self.pattern_table_addr();

        let fg_idx = (self.regs[7] >> 4) as usize;
        let bg_idx = (self.regs[7] & 0x0F) as usize;
        let fg = if fg_idx == 0 { PALETTE[1] } else { PALETTE[fg_idx] };
        let bg = if bg_idx == 0 { PALETTE[1] } else { PALETTE[bg_idx] };
        let border = self.backdrop_color();

        let char_row = line / 8;
        let row_in_char = line & 7;

        // 8-pixel border on each side
        for x in 0..8 {
            self.framebuffer[offset + x] = border;
            self.framebuffer[offset + 248 + x] = border;
        }

        for col in 0..40 {
            let name_addr = name_base + char_row * 40 + col;
            let name = self.vram[name_addr & 0x3FFF] as usize;

            let pattern_byte =
                self.vram[(pattern_base + name * 8 + row_in_char) & 0x3FFF];

            // Only upper 6 bits are displayed
            for bit in 0..6 {
                let pixel = if pattern_byte & (0x80 >> bit) != 0 { fg } else { bg };
                self.framebuffer[offset + 8 + col * 6 + bit] = pixel;
            }
        }
    }

    // -- Multicolor --

    fn render_multicolor(&mut self, line: usize, offset: usize) {
        let name_base = self.name_table_addr();
        let pattern_base = self.pattern_table_addr();
        let backdrop = self.backdrop_color();

        let tile_row = line / 8;
        let row_in_tile = line & 7;
        // Which 2-byte pair to use: depends on tile row mod 4
        let pattern_row = (tile_row % 4) * 2 + row_in_tile / 4;

        for tile_col in 0..32 {
            let name_addr = name_base + tile_row * 32 + tile_col;
            let name = self.vram[name_addr & 0x3FFF] as usize;

            let color_byte =
                self.vram[(pattern_base + name * 8 + pattern_row) & 0x3FFF];

            let left_idx = (color_byte >> 4) as usize;
            let right_idx = (color_byte & 0x0F) as usize;
            let left = if left_idx == 0 { backdrop } else { PALETTE[left_idx] };
            let right = if right_idx == 0 { backdrop } else { PALETTE[right_idx] };

            let px = offset + tile_col * 8;
            self.framebuffer[px..px + 4].fill(left);
            self.framebuffer[px + 4..px + 8].fill(right);
        }
    }

    // -----------------------------------------------------------------------
    // Sprite rendering
    // -----------------------------------------------------------------------

    fn render_sprites(&mut self, line: usize, offset: usize) {
        let sat_base = self.sprite_attr_addr();
        let spg_base = self.sprite_pattern_addr();
        let size_16 = self.regs[1] & 0x02 != 0;
        let magnify = self.regs[1] & 0x01 != 0;

        let sprite_height = if size_16 { 16 } else { 8 } * if magnify { 2 } else { 1 };
        let _sprite_width = sprite_height; // Sprites are square

        let mut sprites_on_line = 0u8;
        let mut sprite_line_buffer = [0u8; 256]; // Color index per pixel (0 = none)
        let mut collision = false;

        for sprite in 0..32 {
            let attr_addr = sat_base + sprite * 4;
            let y_raw = self.vram[attr_addr & 0x3FFF];

            // Y = $D0 terminates sprite processing
            if y_raw == 0xD0 {
                break;
            }

            // Sprite Y: display line = Y + 1
            let y = if y_raw > 0xD0 {
                y_raw as i16 - 256 + 1
            } else {
                y_raw as i16 + 1
            };

            let sprite_line = line as i16 - y;
            if sprite_line < 0 || sprite_line >= sprite_height as i16 {
                continue;
            }

            sprites_on_line += 1;
            if sprites_on_line > 4 {
                // 5th sprite: set flag if not already set
                if self.status & 0x40 == 0 {
                    self.status = (self.status & 0xE0) | 0x40 | sprite as u8;
                }
                break; // Don't render 5th+ sprites
            }

            let mut x = self.vram[(attr_addr + 1) & 0x3FFF] as i16;
            let pattern_name = self.vram[(attr_addr + 2) & 0x3FFF] as usize;
            let attr_byte = self.vram[(attr_addr + 3) & 0x3FFF];
            let color = (attr_byte & 0x0F) as usize;
            let early_clock = attr_byte & 0x80 != 0;

            if early_clock {
                x -= 32;
            }

            // Transparent sprites don't render but still count and collide
            let pattern_line = if magnify {
                sprite_line as usize / 2
            } else {
                sprite_line as usize
            };

            if size_16 {
                // 16x16: pattern name rounded to multiple of 4
                let base_name = pattern_name & 0xFC;
                // Quadrant layout: TL(0-7), BL(8-15), TR(16-23), BR(24-31)
                let (left_name, right_name) = if pattern_line < 8 {
                    (base_name, base_name + 2)
                } else {
                    (base_name + 1, base_name + 3)
                };
                let row = pattern_line & 7;

                let left_byte = self.vram[(spg_base + left_name * 8 + row) & 0x3FFF];
                let right_byte = self.vram[(spg_base + right_name * 8 + row) & 0x3FFF];

                self.draw_sprite_row(
                    &mut sprite_line_buffer,
                    left_byte,
                    x,
                    color,
                    magnify,
                    &mut collision,
                );
                let x2 = x + if magnify { 16 } else { 8 };
                self.draw_sprite_row(
                    &mut sprite_line_buffer,
                    right_byte,
                    x2,
                    color,
                    magnify,
                    &mut collision,
                );
            } else {
                // 8x8
                let pattern_byte =
                    self.vram[(spg_base + pattern_name * 8 + pattern_line) & 0x3FFF];
                self.draw_sprite_row(
                    &mut sprite_line_buffer,
                    pattern_byte,
                    x,
                    color,
                    magnify,
                    &mut collision,
                );
            }
        }

        if collision {
            self.status |= 0x20;
        }

        // Composite sprite pixels onto the framebuffer
        let backdrop = self.backdrop_color();
        for x in 0..256 {
            let c = sprite_line_buffer[x] as usize;
            if c != 0 {
                self.framebuffer[offset + x] = if c == 0 { backdrop } else { PALETTE[c] };
            }
        }
    }

    fn draw_sprite_row(
        &self,
        buffer: &mut [u8; 256],
        pattern: u8,
        x: i16,
        color: usize,
        magnify: bool,
        collision: &mut bool,
    ) {
        let step = if magnify { 2 } else { 1 };
        for bit in 0..8 {
            if pattern & (0x80 >> bit) == 0 {
                continue;
            }
            for sub in 0..step {
                let px = x + (bit * step + sub) as i16;
                if px < 0 || px >= 256 {
                    continue;
                }
                let px = px as usize;
                if buffer[px] != 0 {
                    *collision = true;
                } else if color != 0 {
                    buffer[px] = color as u8;
                }
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
        let vdp = Tms9918::new(VdpRegion::Ntsc);
        assert_eq!(vdp.framebuffer().len(), (FB_WIDTH * FB_HEIGHT) as usize);
        assert!(vdp.framebuffer().iter().all(|&p| p == 0));
    }

    #[test]
    fn control_port_register_write() {
        let mut vdp = Tms9918::new(VdpRegion::Ntsc);
        // Write $40 to VR1 (enable display)
        vdp.write_control(0x40); // value
        vdp.write_control(0x81); // register 1, bit 7 set
        assert_eq!(vdp.regs[1], 0x40);
        assert!(vdp.display_enabled());
    }

    #[test]
    fn vram_write_and_read() {
        let mut vdp = Tms9918::new(VdpRegion::Ntsc);
        // Set write address to $0000
        vdp.write_control(0x00);
        vdp.write_control(0x40); // bit 6 set = write mode

        // Write bytes
        vdp.write_data(0xAA);
        vdp.write_data(0xBB);
        vdp.write_data(0xCC);

        assert_eq!(vdp.vram[0], 0xAA);
        assert_eq!(vdp.vram[1], 0xBB);
        assert_eq!(vdp.vram[2], 0xCC);

        // Set read address to $0000
        vdp.write_control(0x00);
        vdp.write_control(0x00); // bit 6 clear = read mode

        // First read returns pre-fetched byte
        assert_eq!(vdp.read_data(), 0xAA);
        assert_eq!(vdp.read_data(), 0xBB);
        assert_eq!(vdp.read_data(), 0xCC);
    }

    #[test]
    fn address_auto_increment_wraps() {
        let mut vdp = Tms9918::new(VdpRegion::Ntsc);
        // Set write address to $3FFE
        vdp.write_control(0xFE);
        vdp.write_control(0x7F); // $3FFE, write mode

        vdp.write_data(0x11);
        vdp.write_data(0x22);
        vdp.write_data(0x33); // Should wrap to $0000

        assert_eq!(vdp.vram[0x3FFE], 0x11);
        assert_eq!(vdp.vram[0x3FFF], 0x22);
        assert_eq!(vdp.vram[0x0000], 0x33);
    }

    #[test]
    fn status_register_clears_flags_and_resets_latch() {
        let mut vdp = Tms9918::new(VdpRegion::Ntsc);
        // Force F flag
        vdp.status = 0x80;
        vdp.regs[1] = 0x20; // IE enabled
        vdp.update_interrupt();
        assert!(vdp.interrupt);

        // Write first byte of control sequence
        vdp.write_control(0x42);
        assert!(!vdp.latch_first);

        // Read status — should clear flags and reset latch
        let s = vdp.read_status();
        assert_eq!(s & 0x80, 0x80); // F was set
        assert!(vdp.latch_first); // Latch reset
        assert!(!vdp.interrupt); // Interrupt cleared
    }

    #[test]
    fn ntsc_frame_is_262_lines() {
        let mut vdp = Tms9918::new(VdpRegion::Ntsc);
        vdp.regs[1] = 0x40; // Enable display
        let mut frames = 0;
        for _ in 0..262 {
            if vdp.tick_scanline() {
                frames += 1;
            }
        }
        assert_eq!(frames, 1);
        assert_eq!(vdp.frame_count, 1);
    }

    #[test]
    fn pal_frame_is_313_lines() {
        let mut vdp = Tms9918::new(VdpRegion::Pal);
        vdp.regs[1] = 0x40;
        let mut frames = 0;
        for _ in 0..313 {
            if vdp.tick_scanline() {
                frames += 1;
            }
        }
        assert_eq!(frames, 1);
    }

    #[test]
    fn vblank_sets_interrupt_flag() {
        let mut vdp = Tms9918::new(VdpRegion::Ntsc);
        vdp.regs[1] = 0x60; // Display on + IE
        // Tick through active display + 1 line
        for _ in 0..193 {
            vdp.tick_scanline();
        }
        assert!(vdp.interrupt);
        assert_eq!(vdp.status & 0x80, 0x80);
    }

    #[test]
    fn sprite_y_d0_terminates_processing() {
        let mut vdp = Tms9918::new(VdpRegion::Ntsc);
        vdp.regs[1] = 0x40; // Display on
        vdp.regs[5] = 0x00; // SAT at $0000

        // Sprite 0: Y=$D0 (sentinel)
        vdp.vram[0] = 0xD0;

        // Sprite 1: visible
        vdp.vram[4] = 50;
        vdp.vram[5] = 100;
        vdp.vram[6] = 0;
        vdp.vram[7] = 0x0F; // White

        // Render a line where sprite 1 would appear — it shouldn't
        // because sprite 0's Y=$D0 terminates processing.
        vdp.render_scanline(51);

        // The framebuffer at (100, 51) should be backdrop, not white
        let fb_idx = 51 * 256 + 100;
        // Backdrop is color 0 from VR7 = 0, which maps to black
        assert_ne!(vdp.framebuffer[fb_idx], PALETTE[15]);
    }

    #[test]
    fn mode_detection() {
        let mut vdp = Tms9918::new(VdpRegion::Ntsc);
        assert_eq!(vdp.mode(), Mode::GraphicsI);

        vdp.regs[1] = 0x10; // M1
        assert_eq!(vdp.mode(), Mode::Text);

        vdp.regs[1] = 0x08; // M2
        assert_eq!(vdp.mode(), Mode::Multicolor);

        vdp.regs[1] = 0x00;
        vdp.regs[0] = 0x02; // M3
        assert_eq!(vdp.mode(), Mode::GraphicsII);
    }

    #[test]
    fn graphics_i_renders_tile() {
        let mut vdp = Tms9918::new(VdpRegion::Ntsc);
        vdp.regs[1] = 0x40; // Display on, Graphics I
        vdp.regs[2] = 0x06; // Name table at $1800
        vdp.regs[3] = 0x80; // Color table at $2000
        vdp.regs[4] = 0x00; // Pattern table at $0000
        vdp.regs[7] = 0x01; // Backdrop = black

        // Set tile 0's pattern: solid line on row 0
        vdp.vram[0] = 0xFF;

        // Set color for group 0: white on black
        vdp.vram[0x2000] = 0xF1; // FG=white(15), BG=black(1)

        // Name table: first tile = 0
        vdp.vram[0x1800] = 0;

        vdp.render_scanline(0);

        // First 8 pixels should be white
        for x in 0..8 {
            assert_eq!(vdp.framebuffer[x], PALETTE[15], "pixel {x} should be white");
        }
    }
}
