//! Texas Instruments TMS9918A/9928A/9929A Video Display Processor.
//!
//! Adapted from `Emu198x-Oldest/crates/ti-tms9918` (port 2026-06-01) as the
//! starting point for ColecoVision and the rest of the TMS9918 family
//! (SG-1000, MSX, Sord M5, Memotech MTX, Spectravideo SV-328). The donor
//! tick model was dot-clocked but rendered each scanline as a batch at
//! line-wrap; [`tick`](Tms9918::tick) now draws **one pixel per dot** at the
//! moment it is scanned out, so a mid-line register write affects only the
//! background pixels drawn after it (beam-follows-the-registers). For a static
//! frame the output is byte-identical to the old batch model — every pixel
//! routes through the same `bg_pixel`/sprite logic.
//!
//! **Sprite fidelity boundary:** sprites are evaluated once per line (at dot 0,
//! in [`prepare_line_sprites`](Tms9918::prepare_line_sprites)), not per dot.
//! This models the real chip, which fetches the line's sprite attributes and
//! patterns during the *previous* line's horizontal blank — so a mid-line write
//! to a sprite register or the sprite tables takes effect on the next line, as
//! on hardware. Only background generation re-reads registers per dot. (#136)
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

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

// ---------------------------------------------------------------------------
// Color palette
// ---------------------------------------------------------------------------

/// Fixed 15-color palette (plus transparent). ARGB32 format.
pub const PALETTE: [u32; 16] = [
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

    /// Scan lines a set displays, which is what the framebuffer holds.
    ///
    /// Per `knowledge/decisions/the-framebuffer-is-the-sets-window.md`. The
    /// two standards differ by 48 lines, so one height cannot serve both: the
    /// single 240 this used to hold is NTSC's field, and left every PAL
    /// machine in the family showing 83% of what a set does.
    #[must_use]
    pub const fn framebuffer_height(self) -> u32 {
        match self {
            Self::Ntsc => 240,
            Self::Pal => 288,
        }
    }

    /// Scan lines of border above the active area.
    ///
    /// Halving what the field has left over would put the active area in the
    /// middle of the window, and the chip does not put it there. Table 3-3
    /// gives the 262-line frame as 27 lines of top border, 192 active, 24 of
    /// bottom border, and 19 blanked — 3 after the picture, 3 of sync, 13
    /// before it. 243 lines are scanned and a set shows 240 of them, centred,
    /// so three go: two off the larger top border and one off the bottom. 25
    /// and 23, and the picture sits a line and a half below the middle of the
    /// window because that is where the chip scans it.
    ///
    /// The manual does not table the 313-line 9929A frame — §3.6.2 gives the
    /// 262-line case only, and
    /// `reference/by-topic/vdp-tms9918/tms9918a-reference.md` records the gap.
    /// MAME's `315_5124.h`, a descendant with the same line budget and the
    /// same 27 and 24 on NTSC, gives 54 and 48 for PAL. Those restore the same
    /// 19 blanked lines and split the extra 51 in the same 27:24 ratio, which
    /// is the check that they belong to this frame and not another chip's.
    /// 294 scanned, 288 shown, three off each end.
    #[must_use]
    pub const fn border_top(self) -> u32 {
        match self {
            Self::Ntsc => 25,
            Self::Pal => 51,
        }
    }

    /// Scan lines of border below the active area.
    #[must_use]
    pub const fn border_bottom(self) -> u32 {
        self.framebuffer_height() - ACTIVE_HEIGHT - self.border_top()
    }

    /// Pixels a set displays along a line, which is the framebuffer's width.
    ///
    /// `dot_clock x active_line_seconds`: 5.369318 MHz over 52.148 µs is 280
    /// on NTSC, and 5.34375 MHz over 52.0 µs is 278 on PAL.
    ///
    /// This used to be a fixed 16 pixels of border either side of the active
    /// 256, giving 288 for both regions — 103% of an NTSC window and 104% of a
    /// PAL one. Border colour comes from VR7's low nibble (the backdrop), so
    /// those extra pixels were backdrop a set never shows.
    #[must_use]
    pub const fn framebuffer_width(self) -> u32 {
        match self {
            Self::Ntsc => 280,
            Self::Pal => 278,
        }
    }

    /// Pixels of border left of the active area.
    ///
    /// Centring is right here, which had to be checked rather than assumed
    /// after the vertical case turned out not to be. Table 3-3 splits the
    /// 342-cycle line into 13 pixels of left border, 256 active, 15 of right,
    /// and 58 of sync, colour burst and blanking — so the picture is *not*
    /// centred in the 284 the chip scans. It is all but centred in what a set
    /// shows: measured from the leading edge of sync, 26 cycles of sync and 24
    /// of back porch put the active area's midpoint 35.57 µs into the line,
    /// against a broadcast picture centre of 35.5 to 35.7 depending on which
    /// back-porch figure you take. Under a pixel either way, and less than the
    /// porch figures disagree among themselves.
    #[must_use]
    pub const fn border_left(self) -> u32 {
        (self.framebuffer_width() - ACTIVE_WIDTH) / 2
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

/// Active display area dimensions (the pixels the chip actually draws
/// tiles/sprites into).
pub const ACTIVE_WIDTH: u32 = 256;

/// Columns either side of the active area that sprite coincidence still covers.
///
/// §2.3.2: coincidence considers sprites "partially or completely off the
/// screen", and the VDP "checks each pixel position for coincidence during the
/// generation of the pixel regardless of where it is located on the screen".
///
/// Early clock subtracts 32 from a sprite's X, so the leftmost pixel a sprite
/// can occupy is -32; the rightmost is an X of 255 plus the 32 columns of a
/// magnified 16x16, so 286. One margin of 32 covers both ends.
const COINCIDENCE_MARGIN: i16 = 32;
/// The span that margin implies, either side of the active area.
const COINCIDENCE_SPAN: usize = ACTIVE_WIDTH as usize + 2 * COINCIDENCE_MARGIN as usize;
pub const ACTIVE_HEIGHT: u32 = 192;

/// Dot clock of the NTSC parts — TMS9918A and TMS9928A. Half a 10.738635 MHz
/// crystal, which is three times the colour subcarrier.
///
/// 342 dots at this rate is 63.70 µs, a shade longer than NTSC's 63.56 µs
/// line. That is the chip, not a rounding error here: a TMS9918 frame runs
/// slightly slow and sets tolerate it.
pub const NTSC_DOT_CLOCK_HZ: f64 = 5_369_318.0;

/// Dot clock of the PAL part, the TMS9929A: half a 10.6875 MHz crystal. 342
/// dots comes to exactly 64 µs here, so the PAL chip sits on the standard in
/// a way its NTSC sibling does not.
pub const PAL_DOT_CLOCK_HZ: f64 = 5_343_750.0;

/// TMS9918 Video Display Processor.
#[derive(Serialize, Deserialize)]
pub struct Tms9918 {
    // VRAM
    #[serde(with = "BigArray")]
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

    /// Per-line sprite colour-index buffer (0 = no sprite pixel). Evaluated
    /// once at the start of each active line (dot 0) and overlaid per pixel
    /// as the line is drawn. Transient — recomputed every line; carried in the
    /// snapshot only because `[u8; 256]` has no `Default` to skip cleanly.
    #[serde(with = "BigArray")]
    sprite_buf: [u8; 256],

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
            framebuffer: vec![
                0;
                (region.framebuffer_width() * region.framebuffer_height()) as usize
            ],
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

    /// Framebuffer width.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.region.framebuffer_width()
    }

    /// Framebuffer height.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        (self.framebuffer.len() / self.region.framebuffer_width() as usize) as u32
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
            self.address = u16::from(self.latch_value) | (u16::from(value & 0x3F) << 8);
        } else {
            // VRAM read setup — pre-fetch into read buffer
            self.address = u16::from(self.latch_value) | (u16::from(value & 0x3F) << 8);
            self.read_buffer = self.vram[self.address as usize & 0x3FFF];
            self.address = (self.address + 1) & 0x3FFF;
        }
    }

    /// Direct VRAM access for testing and observation.
    #[must_use]
    pub fn vram(&self) -> &[u8; 16384] {
        &self.vram
    }

    /// Direct register access for observation.
    #[must_use]
    pub fn registers(&self) -> &[u8; 8] {
        &self.regs
    }

    /// Sprite attribute table base address derived from register 5.
    #[must_use]
    pub fn sprite_attr_table_addr(&self) -> usize {
        self.sprite_attr_addr()
    }

    /// Sprite pattern generator base address derived from register 6.
    #[must_use]
    pub fn sprite_pattern_table_addr(&self) -> usize {
        self.sprite_pattern_addr()
    }

    /// Pattern generator table base address derived from register 4.
    #[must_use]
    pub fn pattern_generator_addr(&self) -> usize {
        self.pattern_table_addr()
    }

    /// Whether sprites are 16×16 (true) or 8×8 (false).
    #[must_use]
    pub fn sprites_16x16(&self) -> bool {
        self.regs[1] & 0x02 != 0
    }

    /// Whether sprite magnification is enabled (doubles pixel size).
    #[must_use]
    pub fn sprite_magnify(&self) -> bool {
        self.regs[1] & 0x01 != 0
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
    ///
    /// Rendering is **per dot**: each active pixel is drawn at the dot it is
    /// scanned out, using the register/VRAM state live at that moment. A
    /// register write part-way through a line therefore affects only the
    /// pixels drawn after it — the same beam-follows-the-registers behaviour
    /// as the real chip. For a static frame (no writes during active display)
    /// this is byte-identical to the previous scanline-batched model, because
    /// every pixel shares the same `bg_pixel`/sprite logic.
    pub fn tick(&mut self) -> bool {
        // Start of each scanline: paint that line's border pixels from the live
        // backdrop, so a mid-frame VR7 write splits the border on this frame.
        // Active pixels overwrite the 256 x 192 interior as they are drawn.
        if self.dot == 0 {
            self.paint_border_for_scanline();
        }

        // Start of an active line: evaluate this line's sprites once (sets the
        // 5th-sprite and collision status flags and fills `sprite_buf`).
        if self.scanline < 192 && self.dot == 0 {
            self.prepare_line_sprites(self.scanline as usize);
        }

        // Draw the active pixel scanned out at this dot.
        if self.scanline < 192 && self.dot < ACTIVE_WIDTH as u16 {
            self.render_pixel(self.scanline as usize, self.dot as usize);
        }

        // Advance the dot / scanline.
        self.dot += 1;
        if self.dot >= 342 {
            self.dot = 0;

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
        // Paint this line's border from the live backdrop (see `tick`).
        self.paint_border_for_scanline();

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

    /// Paint the border (backdrop) pixels for the current scanline from the
    /// **live** VR7 backdrop, so a mid-frame VR7 write splits the border on the
    /// same frame (a horizontal raster split) rather than one frame late. (#135)
    ///
    /// Active-row interiors are drawn separately by `render_pixel` /
    /// `render_scanline`; this paints the left/right backdrop columns of each
    /// active row and the full width of the top/bottom border rows.
    /// `fb_row = scanline + border_top` unifies the active rows and the bottom
    /// border; the top border (above the active area) is painted once as the
    /// frame opens. Called at the start of each scanline by both tick paths.
    fn paint_border_for_scanline(&mut self) {
        let backdrop = self.backdrop_color();
        let fbw = self.region.framebuffer_width() as usize;
        let scan = self.scanline as usize;
        let border_top = self.region.border_top() as usize;
        let border_left = self.region.border_left() as usize;

        // Top border: painted as the frame opens — it sits above the active
        // area and is scanned before any mid-frame VR7 write.
        if scan == 0 {
            for px in &mut self.framebuffer[..border_top * fbw] {
                *px = backdrop;
            }
        }

        let active_h = ACTIVE_HEIGHT as usize;
        if scan < active_h {
            // Active row: the left and right backdrop columns only.
            let row = (border_top + scan) * fbw;
            for px in &mut self.framebuffer[row..row + border_left] {
                *px = backdrop;
            }
            let right = row + border_left + ACTIVE_WIDTH as usize;
            for px in &mut self.framebuffer[right..row + fbw] {
                *px = backdrop;
            }
        } else if scan < active_h + self.region.border_bottom() as usize {
            // Bottom border row: the full width.
            let row = (border_top + scan) * fbw;
            for px in &mut self.framebuffer[row..row + fbw] {
                *px = backdrop;
            }
        }
    }

    /// Render one full active scanline by drawing every pixel through the same
    /// per-dot path used by [`tick`](Self::tick). Kept for direct/batched use
    /// (tests, `tick_scanline`); produces identical output to the per-dot loop
    /// because both share `bg_pixel` and the sprite buffer.
    fn render_scanline(&mut self, line: u16) {
        let line = line as usize;
        self.prepare_line_sprites(line);
        for x in 0..ACTIVE_WIDTH as usize {
            self.render_pixel(line, x);
        }
    }

    // -----------------------------------------------------------------------
    // Per-pixel rendering
    // -----------------------------------------------------------------------

    /// Draw the active pixel at column `x` of `line`: the background pixel for
    /// the current mode, overlaid by this line's sprite pixel if present.
    fn render_pixel(&mut self, line: usize, x: usize) {
        let bg = self.bg_pixel(line, x);
        let sprite = self.sprite_buf[x];
        let pixel = if sprite != 0 {
            PALETTE[sprite as usize]
        } else {
            bg
        };
        let idx = (self.region.border_top() as usize + line)
            * self.region.framebuffer_width() as usize
            + self.region.border_left() as usize
            + x;
        self.framebuffer[idx] = pixel;
    }

    /// The background colour at column `x` of `line`, before sprites. Returns
    /// the backdrop when the display is blanked.
    fn bg_pixel(&self, line: usize, x: usize) -> u32 {
        if !self.display_enabled() {
            return self.backdrop_color();
        }
        match self.mode() {
            Mode::GraphicsI => self.graphics_i_pixel(line, x),
            Mode::GraphicsII => self.graphics_ii_pixel(line, x),
            Mode::Text => self.text_pixel(line, x),
            Mode::Multicolor => self.multicolor_pixel(line, x),
        }
    }

    // -- Graphics I --

    fn graphics_i_pixel(&self, line: usize, x: usize) -> u32 {
        let name_base = self.name_table_addr();
        let pattern_base = self.pattern_table_addr();
        let color_base = self.color_table_addr();
        let backdrop = self.backdrop_color();

        let tile_row = line / 8;
        let row_in_tile = line & 7;
        let tile_col = x / 8;
        let bit = x & 7;

        let name = self.vram[(name_base + tile_row * 32 + tile_col) & 0x3FFF] as usize;
        let pattern_byte = self.vram[(pattern_base + name * 8 + row_in_tile) & 0x3FFF];

        // Color: one byte per group of 8 tiles
        let color_byte = self.vram[(color_base + name / 8) & 0x3FFF];
        let fg_idx = (color_byte >> 4) as usize;
        let bg_idx = (color_byte & 0x0F) as usize;

        if pattern_byte & (0x80 >> bit) != 0 {
            if fg_idx == 0 {
                backdrop
            } else {
                PALETTE[fg_idx]
            }
        } else if bg_idx == 0 {
            backdrop
        } else {
            PALETTE[bg_idx]
        }
    }

    // -- Graphics II --

    fn graphics_ii_pixel(&self, line: usize, x: usize) -> u32 {
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
        let tile_col = x / 8;
        let bit = x & 7;

        let name = self.vram[(name_base + tile_row * 32 + tile_col) & 0x3FFF] as usize;
        let effective = (name + zone * 256) & pattern_mask;

        let pattern_byte = self.vram[(pattern_base + effective * 8 + row_in_tile) & 0x3FFF];
        let color_byte = self.vram
            [(color_base + ((effective * 8 + row_in_tile) & (color_mask * 8 + 7))) & 0x3FFF];

        let fg_idx = (color_byte >> 4) as usize;
        let bg_idx = (color_byte & 0x0F) as usize;

        if pattern_byte & (0x80 >> bit) != 0 {
            if fg_idx == 0 {
                backdrop
            } else {
                PALETTE[fg_idx]
            }
        } else if bg_idx == 0 {
            backdrop
        } else {
            PALETTE[bg_idx]
        }
    }

    // -- Text --

    fn text_pixel(&self, line: usize, x: usize) -> u32 {
        // 8-pixel border on each side of the 240-pixel (40 x 6) text field.
        if !(8..248).contains(&x) {
            return self.backdrop_color();
        }

        let name_base = self.name_table_addr();
        let pattern_base = self.pattern_table_addr();

        let fg_idx = (self.regs[7] >> 4) as usize;
        let bg_idx = (self.regs[7] & 0x0F) as usize;
        let fg = if fg_idx == 0 {
            PALETTE[1]
        } else {
            PALETTE[fg_idx]
        };
        let bg = if bg_idx == 0 {
            PALETTE[1]
        } else {
            PALETTE[bg_idx]
        };

        let char_row = line / 8;
        let row_in_char = line & 7;
        let cx = x - 8;
        let col = cx / 6;
        let bit = cx % 6; // only the upper 6 bits of each pattern byte show

        let name = self.vram[(name_base + char_row * 40 + col) & 0x3FFF] as usize;
        let pattern_byte = self.vram[(pattern_base + name * 8 + row_in_char) & 0x3FFF];

        if pattern_byte & (0x80 >> bit) != 0 {
            fg
        } else {
            bg
        }
    }

    // -- Multicolor --

    fn multicolor_pixel(&self, line: usize, x: usize) -> u32 {
        let name_base = self.name_table_addr();
        let pattern_base = self.pattern_table_addr();
        let backdrop = self.backdrop_color();

        let tile_row = line / 8;
        let row_in_tile = line & 7;
        // Which 2-byte pair to use: depends on tile row mod 4
        let pattern_row = (tile_row % 4) * 2 + row_in_tile / 4;
        let tile_col = x / 8;

        let name = self.vram[(name_base + tile_row * 32 + tile_col) & 0x3FFF] as usize;
        let color_byte = self.vram[(pattern_base + name * 8 + pattern_row) & 0x3FFF];

        // Left 4 pixels use the high nibble, right 4 the low nibble.
        let idx = if x & 7 < 4 {
            (color_byte >> 4) as usize
        } else {
            (color_byte & 0x0F) as usize
        };
        if idx == 0 { backdrop } else { PALETTE[idx] }
    }

    // -----------------------------------------------------------------------
    // Sprite rendering
    // -----------------------------------------------------------------------

    /// Prepare this line's sprite overlay: clear `sprite_buf`, then (unless the
    /// display is blanked or in Text mode, which has no sprites) evaluate the
    /// 32-entry sprite table into it and update the status flags.
    fn prepare_line_sprites(&mut self, line: usize) {
        self.sprite_buf = [0u8; 256];
        if self.display_enabled() && self.mode() != Mode::Text {
            self.evaluate_sprites(line);
        }
    }

    fn evaluate_sprites(&mut self, line: usize) {
        let sat_base = self.sprite_attr_addr();
        let spg_base = self.sprite_pattern_addr();
        let size_16 = self.regs[1] & 0x02 != 0;
        let magnify = self.regs[1] & 0x01 != 0;

        let sprite_height = if size_16 { 16 } else { 8 } * if magnify { 2 } else { 1 };
        let _sprite_width = sprite_height; // Sprites are square

        let mut sprites_on_line = 0u8;
        let mut sprite_line_buffer = [0u8; 256]; // Color index per pixel (0 = none)
        // Pattern-presence buffer for coincidence detection, separate from the
        // colour buffer: records every counted sprite's non-zero pattern bit
        // regardless of colour, so transparent (colour-0) sprites still collide.
        let mut coincidence_buffer = [false; COINCIDENCE_SPAN];
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
                // The fifth sprite reports itself, but only with the frame flag
                // clear. §2.3.3: the flag "is set to a 1 whenever there are five
                // or more sprites on a horizontal line (lines 0 to 192) **and
                // the frame flag is equal to a 0**".
                //
                // So a program that never reads the status register -- leaving F
                // latched from the previous frame -- stops being told about
                // sprite overflow. Testing bit 6 alone set it regardless, which
                // is the same thing every frame rather than only the first after
                // a read.
                //
                // Both bits at once: 5S must not already be set, and F must be
                // clear.
                if self.status & 0xC0 == 0 {
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
                    &mut coincidence_buffer,
                    left_byte,
                    x,
                    color,
                    &mut collision,
                );
                let x2 = x + if magnify { 16 } else { 8 };
                self.draw_sprite_row(
                    &mut sprite_line_buffer,
                    &mut coincidence_buffer,
                    right_byte,
                    x2,
                    color,
                    &mut collision,
                );
            } else {
                // 8x8
                let pattern_byte = self.vram[(spg_base + pattern_name * 8 + pattern_line) & 0x3FFF];
                self.draw_sprite_row(
                    &mut sprite_line_buffer,
                    &mut coincidence_buffer,
                    pattern_byte,
                    x,
                    color,
                    &mut collision,
                );
            }
        }

        if collision {
            self.status |= 0x20;
        }

        // Publish the line's sprite pixels; `render_pixel` overlays them onto
        // the background as each pixel is drawn.
        self.sprite_buf = sprite_line_buffer;
    }

    fn draw_sprite_row(
        &self,
        buffer: &mut [u8; 256],
        coincidence: &mut [bool; COINCIDENCE_SPAN],
        pattern: u8,
        x: i16,
        color: usize,
        collision: &mut bool,
    ) {
        // Magnify (×2) is a per-frame register bit, so read it live rather than
        // threading it through every call site.
        let magnify = self.regs[1] & 0x01 != 0;
        let step = if magnify { 2 } else { 1 };
        for bit in 0..8 {
            if pattern & (0x80 >> bit) == 0 {
                continue;
            }
            for sub in 0..step {
                let px = x + (bit * step + sub) as i16;

                // Coincidence (status bit 5) fires wherever two counted
                // sprites' non-zero PATTERN bits overlap — independent of
                // colour, so two transparent (colour-0) sprites still collide
                // (#134), and independent of whether the pixel is on screen.
                //
                // §2.3.2 puts sprites "partially or completely off the screen"
                // in scope, so this is checked across the off-screen margins
                // and not only the 256 visible columns.
                #[allow(clippy::cast_sign_loss)]
                let slot = (px + COINCIDENCE_MARGIN) as usize;
                if px + COINCIDENCE_MARGIN >= 0 && slot < COINCIDENCE_SPAN {
                    if coincidence[slot] {
                        *collision = true;
                    }
                    coincidence[slot] = true;
                }

                // The picture is only the visible columns. Colour: the
                // highest-priority (lowest-numbered, drawn-first) opaque sprite
                // covering the pixel wins; transparent sprites contribute
                // nothing to it.
                if (0..256).contains(&px) {
                    let px = px as usize;
                    if color != 0 && buffer[px] == 0 {
                        buffer[px] = color as u8;
                    }
                }
            }
        }
    }
    // -----------------------------------------------------------------------
    // Save/load state
    // -----------------------------------------------------------------------

    /// Serialize VDP state to a byte vector.
    ///
    /// Layout: regs (8) + status (1) + read_buffer (1) + address (2) +
    /// latch_first (1) + latch_value (1) + scanline (2) + dot (2) +
    /// interrupt (1) + frame_count (8) + vram (16384) = 16411 bytes.
    pub fn save_state(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.regs);
        out.push(self.status);
        out.push(self.read_buffer);
        out.extend_from_slice(&self.address.to_le_bytes());
        out.push(u8::from(self.latch_first));
        out.push(self.latch_value);
        out.extend_from_slice(&self.scanline.to_le_bytes());
        out.extend_from_slice(&self.dot.to_le_bytes());
        out.push(u8::from(self.interrupt));
        out.extend_from_slice(&self.frame_count.to_le_bytes());
        out.extend_from_slice(&self.vram);
    }

    /// Restore VDP state from a byte slice. Returns bytes consumed or error.
    pub fn load_state(&mut self, data: &[u8]) -> Result<usize, String> {
        let needed = 8 + 1 + 1 + 2 + 1 + 1 + 2 + 2 + 1 + 8 + 16384;
        if data.len() < needed {
            return Err("TMS9918 state truncated".into());
        }
        let mut p = 0;
        self.regs.copy_from_slice(&data[p..p + 8]);
        p += 8;
        self.status = data[p];
        p += 1;
        self.read_buffer = data[p];
        p += 1;
        self.address = u16::from_le_bytes([data[p], data[p + 1]]);
        p += 2;
        self.latch_first = data[p] != 0;
        p += 1;
        self.latch_value = data[p];
        p += 1;
        self.scanline = u16::from_le_bytes([data[p], data[p + 1]]);
        p += 2;
        self.dot = u16::from_le_bytes([data[p], data[p + 1]]);
        p += 2;
        self.interrupt = data[p] != 0;
        p += 1;
        self.frame_count = u64::from_le_bytes([
            data[p],
            data[p + 1],
            data[p + 2],
            data[p + 3],
            data[p + 4],
            data[p + 5],
            data[p + 6],
            data[p + 7],
        ]);
        p += 8;
        self.vram.copy_from_slice(&data[p..p + 16384]);
        p += 16384;
        Ok(p)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #[test]
    fn each_region_holds_exactly_the_field_a_set_shows() {
        // 240 lines on NTSC, 288 on PAL — `Display::Television`'s
        // `lines_per_tv_height`, and the rule in
        // `the-framebuffer-is-the-sets-window.md`.
        //
        // One height served both, and it was NTSC's. Every PAL machine in
        // this family — MSX, ColecoVision, Sord M5, SVI-328, MTX, Einstein,
        // SG-1000, Master System — showed 240 lines of a 288-line field, and
        // the #1054 audit read all of them at 83%.
        for (region, field, border) in [(VdpRegion::Ntsc, 240, 25), (VdpRegion::Pal, 288, 51)] {
            let vdp = Tms9918::new(region);
            assert_eq!(vdp.framebuffer_height(), field, "{region:?}");
            assert_eq!(
                vdp.framebuffer().len(),
                (region.framebuffer_width() * field) as usize,
                "{region:?} allocated a buffer of the wrong size"
            );
            assert_eq!(region.border_top(), border, "{region:?}");
        }
    }

    #[test]
    fn the_border_accounts_for_every_line_the_chip_does_not_draw() {
        // The chip draws 192 lines whatever the region; the border is the
        // rest of the field. Stating it as a constant is what made 240 serve
        // a 288-line field, so it is derived and this checks the derivation
        // leaves nothing over.
        for region in [VdpRegion::Ntsc, VdpRegion::Pal] {
            assert_eq!(
                region.border_top() + ACTIVE_HEIGHT + region.border_bottom(),
                region.framebuffer_height(),
                "{region:?} does not account for every line of its field"
            );
        }
    }

    #[test]
    fn the_last_active_line_lands_above_the_bottom_border() {
        // PAL is the case that moved. Its active area starts 51 lines down,
        // so the chip's last drawn line has to be 45 lines short of the
        // bottom of the buffer, not flush with it.
        let region = VdpRegion::Pal;
        let last_active_row = region.border_top() as usize + ACTIVE_HEIGHT as usize - 1;
        let rows = region.framebuffer_height() as usize;

        assert_eq!(last_active_row, 242);
        assert_eq!(rows - 1 - last_active_row, region.border_bottom() as usize);
    }

    use super::*;

    #[test]
    fn new_vdp_has_blank_framebuffer() {
        let vdp = Tms9918::new(VdpRegion::Ntsc);
        assert_eq!(
            vdp.framebuffer().len(),
            (vdp.framebuffer_width() * vdp.framebuffer_height()) as usize
        );
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

        // First 8 pixels of the active area should be white.
        let active_start = VdpRegion::Ntsc.border_top() as usize
            * VdpRegion::Ntsc.framebuffer_width() as usize
            + VdpRegion::Ntsc.border_left() as usize;
        for x in 0..8 {
            assert_eq!(
                vdp.framebuffer[active_start + x],
                PALETTE[15],
                "active pixel {x} should be white"
            );
        }
    }

    #[test]
    fn transparent_sprites_still_set_coincidence_flag() {
        // #134: two overlapping colour-0 (transparent) sprites must set the
        // coincidence flag (status bit 5 = 0x20). On the TMS9918 collision is
        // pattern-based, not colour-based; the old single-colour-buffer logic
        // never recorded a transparent sprite's pattern, so they never collided.
        let mut vdp = Tms9918::new(VdpRegion::Ntsc);
        vdp.regs[1] = 0x40; // display on, 8x8 sprites, no magnify, Graphics I
        vdp.regs[5] = 0x70; // sprite attribute table base
        vdp.regs[6] = 0x00; // sprite pattern generator base = 0x0000

        // Solid 8x8 pattern at sprite pattern index 0.
        for row in 0..8 {
            vdp.vram[row] = 0xFF;
        }

        // Two transparent (colour 0) sprites overlapping horizontally on the
        // same lines. Bytes per sprite: Y, X, pattern, attr (colour in low nibble).
        let sat = vdp.sprite_attr_addr();
        vdp.vram[sat] = 10;
        vdp.vram[sat + 1] = 10;
        vdp.vram[sat + 2] = 0;
        vdp.vram[sat + 3] = 0;
        vdp.vram[sat + 4] = 10;
        vdp.vram[sat + 5] = 14;
        vdp.vram[sat + 6] = 0;
        vdp.vram[sat + 7] = 0;
        vdp.vram[sat + 8] = 0xD0; // terminate sprite processing

        vdp.status = 0;
        vdp.evaluate_sprites(12); // a line within both sprites (Y+1 ..= Y+8)
        assert_eq!(
            vdp.status & 0x20,
            0x20,
            "two overlapping transparent sprites should set the coincidence flag"
        );
    }

    #[test]
    fn graphics_ii_colour_mask_form_matches_tile_mask_then_multiply() {
        // #137: graphics_ii_pixel computes the colour-table offset as
        // (effective*8 + row) & (color_mask*8 + 7). That is algebraically
        // identical to the conventional (effective & color_mask)*8 + row for
        // EVERY VR3 and every row 0..8 — `row` occupies exactly the low 3 bits
        // that color_mask*8+7 sets, and effective<<3 the high bits color_mask*8
        // masks. Proven exhaustively here, so the decomposition is verified, not
        // a defect (the issue asked for verification across a VR3 sweep).
        for vr3 in 0u16..=255 {
            let color_mask = ((vr3 as usize & 0x7F) << 3) | 0x07;
            for effective in [0usize, 1, 5, 31, 32, 100, 255, 256, 511, 767] {
                for row in 0usize..8 {
                    let code_form = (effective * 8 + row) & (color_mask * 8 + 7);
                    let conventional = (effective & color_mask) * 8 + row;
                    assert_eq!(
                        code_form, conventional,
                        "mismatch at vr3={vr3:#04x} effective={effective} row={row}"
                    );
                }
            }
        }
    }

    #[test]
    fn mid_frame_backdrop_change_splits_the_border() {
        // #135: a VR7 backdrop write partway down the frame must change the
        // border from that scanline on (a horizontal raster split) — not only on
        // the next frame. The old bulk frame-start fill captured VR7 once.
        let mut vdp = Tms9918::new(VdpRegion::Ntsc);
        vdp.regs[1] = 0x40; // display on
        vdp.regs[7] = 0x01; // backdrop colour 1 for the top of the frame

        // Run the top border + the first part of the active area at colour 1.
        for _ in 0..100 {
            vdp.tick_scanline();
        }
        // Switch the backdrop mid-frame, then finish the frame.
        vdp.regs[7] = 0x02;
        while !vdp.tick_scanline() {}

        let fbw = vdp.framebuffer_width() as usize;
        let top = vdp.framebuffer()[0]; // top border, painted before the switch
        let bottom = vdp.framebuffer()[(vdp.framebuffer_height() as usize - 1) * fbw]; // bottom border, after
        assert_eq!(
            top, PALETTE[1],
            "top border should keep the pre-split backdrop"
        );
        assert_eq!(
            bottom, PALETTE[2],
            "bottom border should show the post-split backdrop"
        );
        assert_ne!(top, bottom, "a mid-frame VR7 write should split the border");
    }
}
