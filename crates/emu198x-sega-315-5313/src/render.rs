//! The picture: plane A, plane B and the window, composited and output.
//!
//! Each two-cell column takes eight slots: plane A's name table entries, an
//! external (or refresh) slot, A's two pattern rows, plane B's entries, a
//! sprite slot, B's first pattern row, and then B's second row together with
//! compositing the column. Patterns go into two 32-pixel scroll buffers, a
//! column ahead of the one being composited, which is how fine horizontal
//! scroll works: the compositor reads the buffer `hscroll & 15` pixels back.
//! The column before the visible ones is fetched at the end of the previous
//! line, so a scrolled picture has pixels for its left edge.
//!
//! Pixels leave the chip two per slot from slot 6, through CRAM as it is at
//! that moment. So a palette change in mid-line splits the line where the
//! write lands, and a CRAM write during the picture puts its colour on the
//! pixel being output: the "CRAM dot".
//!
//! Sprites reach the compositor through [`Vdp::sprite_line`], which the
//! sprite engine (see [`crate::sprites`]) fills in slots of its own.

use crate::timing::Width;
use crate::{CRAM_SIZE, State, Vdp};

/// Left and right border widths, in pixels.
pub const BORDER_LEFT: usize = 13;
pub const BORDER_RIGHT: usize = 14;
/// The widest line: H40 plus both borders.
pub const LINE_WIDTH: usize = 320 + BORDER_LEFT + BORDER_RIGHT;
/// The scroll buffers' size and how far a column moves them.
const SCROLL_BUFFER: usize = 32;
const SCROLL_STEP: u8 = 16;
/// The first slot that outputs pixels.
const OUTPUT_START_SLOT: u8 = 6;

/// The composite buffer's priority bit and its shadow and highlight offsets.
const PRIORITY: u8 = 0x40;
const SHADOW: u8 = 0x40;
const HIGHLIGHT: u8 = 0x80;

/// Name table entry bits.
const MAP_V_FLIP: u16 = 0x1000;
const MAP_H_FLIP: u16 = 0x0800;

/// The VDP's DAC output for each of its 15 levels, measured on hardware.
/// Normal colours use every other step; shadow uses the bottom eight and
/// highlight the top eight.
const LEVELS: [u8; 15] = [
    0, 27, 49, 71, 87, 103, 119, 130, 146, 157, 174, 190, 206, 228, 255,
];

/// A CRAM entry, `0000 BBB0 GGG0 RRR0`, as ARGB at normal, shadow (`0x40`)
/// or highlight (`0x80`) brightness.
#[must_use]
pub fn argb(colour: u16, intensity: u8) -> u32 {
    let level = |c: u16| -> u8 {
        let c = usize::from(c & 7);
        match intensity {
            SHADOW => LEVELS[c],
            HIGHLIGHT => LEVELS[c + 7],
            _ => LEVELS[c * 2],
        }
    };
    let r = level(colour >> 1);
    let g = level(colour >> 5);
    let b = level(colour >> 9);
    0xFF00_0000 | u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
}

/// Frame geometry for a region and height: border lines above and below.
const fn borders(pal_machine: bool, v30: bool) -> (u16, u16) {
    match (pal_machine, v30) {
        (false, false) => (11, 8),
        (false, true) => (1, 3),
        (true, false) => (38, 32),
        (true, true) => (30, 24),
    }
}

impl Vdp {
    /// The register bits this module reads.
    fn reg(&self, reg: usize) -> u8 {
        self.regs[reg]
    }

    pub(crate) fn double_res(&self) -> bool {
        self.reg(0x0C) & 0x06 == 0x06
    }

    /// The last slot that outputs pixels, after which the output moves to
    /// the next line.
    pub(crate) fn output_end_slot(width: Width) -> u8 {
        let pixels = match width {
            Width::H40 => LINE_WIDTH,
            Width::H32 => 256 + BORDER_LEFT + BORDER_RIGHT,
        };
        // Two pixels a slot; the last slot outputs the odd one.
        OUTPUT_START_SLOT + (pixels.div_ceil(2) - 1) as u8
    }

    /// The display fetch, or other work, done in display slot `hslot`.
    pub(crate) fn display_slot(&mut self, width: Width, hslot: u8) {
        let last_column_slot = match width {
            Width::H40 => 160,
            Width::H32 => 128,
        };
        if (1..=last_column_slot).contains(&hslot) {
            let column = u16::from((hslot - 1) / 8) * 2 + 2;
            match (hslot - 1) % 8 {
                0 => self.read_map(column, 0),
                2 => self.render_map(0, 0),
                3 => self.render_map(1, 0),
                4 => self.read_map(column, 1),
                5 => self.sprite_x_slot(),
                6 => self.render_map(0, 1),
                7 => self.output_column(column),
                _ => {}
            }
            return;
        }
        match hslot {
            244 => self.read_hscroll(),
            249 => self.read_map(0, 0),
            251 => {
                self.render_map(0, 0);
                self.scan_sprites();
            }
            252 => {
                self.render_map(1, 0);
                self.scan_sprites();
            }
            253 => self.read_map(0, 1),
            255 => {
                self.render_map(0, 1);
                self.scan_sprites();
            }
            0 => {
                self.output_column(0);
                self.scan_sprites();
                self.begin_sprite_reads();
            }
            _ => self.sprite_slot(width, hslot),
        }
        let right_border_slot = match width {
            Width::H40 => 169,
            Width::H32 => 137,
        };
        if hslot == right_border_slot {
            self.draw_right_border(width);
        }
    }

    /// Latch the full-screen vertical scroll for the coming line.
    pub(crate) fn latch_vscroll(&mut self) {
        self.vscroll_latch = [self.vsram[0], self.vsram[1]];
    }

    /// Whether the full-screen vertical scroll is latched in this slot.
    pub(crate) fn vscroll_latch_slot(&self, width: Width, hslot: u8, active: bool) -> bool {
        let column_mode = self.reg(0x0B) & 0x04 != 0;
        if active {
            !column_mode
                && hslot
                    == match width {
                        Width::H40 => 165,
                        Width::H32 => 243,
                    }
        } else if column_mode {
            hslot == 220
        } else {
            hslot
                == match width {
                    Width::H40 => 165,
                    Width::H32 => 243,
                }
        }
    }

    /// The horizontal scroll values for this line, from the table at
    /// register 13: one pair for the screen, per cell row, or per line.
    fn read_hscroll(&mut self) {
        let mut address = usize::from(self.reg(0x0D) & 0x3F) << 10;
        let mut mask = 0;
        if self.reg(0x0B) & 2 != 0 {
            mask |= 0xF8;
        }
        if self.reg(0x0B) & 1 != 0 {
            mask |= 0x07;
        }
        address += usize::from(self.vcounter & mask) * 4;
        let word = |a: usize| {
            u16::from(self.vram[a & 0xFFFF]) << 8 | u16::from(self.vram[(a + 1) & 0xFFFF])
        };
        self.hscroll = [word(address), word(address + 2)];
    }

    /// Read plane A's (`plane` 0) or B's (1) two name table entries for
    /// `column`, or the window's where it covers the column.
    fn read_map(&mut self, column: u16, plane: usize) {
        let mut line = u32::from(self.vcounter);
        let (window_shift, v_offset_mask, vscroll_shift) = if self.double_res() {
            line *= 2;
            if self.even_field {
                line += 1;
            }
            (4, 0xF, 4)
        } else {
            (3, 0x7, 3)
        };
        let h40 = self.reg(0x0C) & 1 != 0;
        if self.reg(0x0B) & 0x04 != 0 {
            if column == 0 {
                if h40 {
                    // With 40 VSRAM entries, column 0 sees A AND B.
                    if plane == 0 {
                        self.vscroll_latch[0] &= self.vscroll_latch[1];
                        self.vscroll_latch[1] = self.vscroll_latch[0];
                    }
                } else {
                    self.vscroll_latch = [0, 0];
                }
            } else {
                let index = usize::from(column) - 2 + plane;
                self.vscroll_latch[plane] = self.vsram.get(index).copied().unwrap_or(0);
            }
        }

        if plane == 0 {
            let (left, right) = if self.window_h_latch & 0x80 != 0 {
                (u16::from(self.window_h_latch & 0x1F) * 2 + 2, 42)
            } else {
                let right = u16::from(self.window_h_latch & 0x1F) * 2;
                (0, if right != 0 { right + 2 } else { 0 })
            };
            let (top, bottom) = if self.window_v_latch & 0x80 != 0 {
                (
                    u32::from(self.window_v_latch & 0x1F) << window_shift,
                    if self.double_res() { 481 } else { 241 },
                )
            } else {
                (0, u32::from(self.window_v_latch & 0x1F) << window_shift)
            };
            if (column >= left && column < right) || (line >= top && line < bottom) {
                let base = usize::from(self.reg(0x03)) << 10;
                let (base, line_offset, mut mask) = if h40 {
                    (
                        base & 0xF000,
                        ((line >> vscroll_shift) as usize * 128) & 0xFFF,
                        0x7F,
                    )
                } else {
                    (
                        base & 0xF800,
                        ((line >> vscroll_shift) as usize * 64) & 0xFFF,
                        0x3F,
                    )
                };
                if self.double_res() {
                    mask = (mask << 1) | 1;
                }
                let entry = |c: u16| {
                    let offset =
                        (base + line_offset + ((usize::from(c.wrapping_mul(2))) & mask)) & 0xFFFF;
                    u16::from(self.vram[offset]) << 8 | u16::from(self.vram[(offset + 1) & 0xFFFF])
                };
                self.map_entries = [entry(column.wrapping_sub(2)), entry(column.wrapping_sub(1))];
                self.v_offset = (line & v_offset_mask) as u8;
                self.in_window = true;
                return;
            } else if column == right {
                self.window_edge = true;
                self.in_window = false;
            } else {
                self.window_edge = false;
                self.in_window = false;
            }
        }

        let base = if plane == 0 {
            usize::from(self.reg(0x02) & 0x38) << 10
        } else {
            usize::from(self.reg(0x04) & 0x07) << 13
        };
        let size = self.reg(0x10);
        let mut vscroll = 0xFF | u32::from(size & 0x30) << 4;
        if self.double_res() {
            vscroll = (vscroll << 1) | 1;
        }
        vscroll &= u32::from(self.vscroll_latch[plane]) + line;
        self.v_offset = (vscroll & v_offset_mask) as u8;
        vscroll >>= vscroll_shift;
        const HSCROLL_MASKS: [u16; 4] = [0x1F, 0x3F, 0x1F, 0x7F];
        const V_SHIFTS: [u32; 4] = [6, 7, 16, 8];
        let hscroll_mask = HSCROLL_MASKS[usize::from(size & 3)];
        let v_shift = V_SHIFTS[usize::from(size & 3)];
        let coarse = (self.hscroll[plane] / 8) & 0xFFFE;
        for i in 0..2 {
            let x = (column.wrapping_sub(2).wrapping_add(i).wrapping_sub(coarse)) & hscroll_mask;
            let offset = base + ((((vscroll << v_shift) as usize) + usize::from(x) * 2) & 0x1FFF);
            let offset = offset & 0xFFFF;
            self.map_entries[usize::from(i)] =
                u16::from(self.vram[offset]) << 8 | u16::from(self.vram[(offset + 1) & 0xFFFF]);
        }
    }

    /// Draw one name table entry's pattern row into a plane's scroll buffer:
    /// entry `which` (0 or 1) of the last map read, into plane `plane`'s
    /// buffer at its current offset, plus eight for the second entry.
    fn render_map(&mut self, which: usize, plane: usize) {
        let entry = self.map_entries[which];
        let (mut address, vflip_base) = if self.double_res() {
            (usize::from(entry & 0x3FF) << 6, 60)
        } else {
            (usize::from(entry & 0x7FF) << 5, 28)
        };
        if entry & MAP_V_FLIP != 0 {
            address += vflip_base - 4 * usize::from(self.v_offset);
        } else {
            address += 4 * usize::from(self.v_offset);
        }
        let attributes = ((entry >> 9) & 0x70) as u8;
        // The second entry of a pair lands eight pixels on.
        let start = usize::from(self.buf_off[plane]) + 8 * which;
        let bytes: [u8; 4] = std::array::from_fn(|i| self.vram[(address + i) & 0xFFFF]);
        let flip = entry & MAP_H_FLIP != 0;
        for i in 0..8 {
            let source = if flip { 7 - i } else { i };
            let byte = bytes[source / 2];
            let nibble = if source % 2 == 0 {
                byte >> 4
            } else {
                byte & 0xF
            };
            self.plane_buf[plane][(start + i) % SCROLL_BUFFER] = attributes | nibble;
        }
    }

    /// Finish plane B's second cell and composite the column before
    /// `column` into the line buffer. Column 0 is the left border.
    fn output_column(&mut self, column: u16) {
        if self.state == State::Preparing {
            // The line before the picture composites only border.
            if column == 0 {
                self.composite[..BORDER_LEFT].fill(0);
                self.composite_valid = true;
            } else {
                let start = BORDER_LEFT + usize::from(column - 2) * 8;
                self.composite[start..start + 16].fill(0);
            }
            return;
        }
        self.render_map(1, 1);
        if column == 0 {
            self.composite[..BORDER_LEFT].fill(0);
            self.composite_valid = true;
        } else {
            let col = usize::from(column - 2);
            let dst = BORDER_LEFT + col * 8;
            let fine_a = (self.hscroll[0] & 0xF) as usize;
            let fine_b = (self.hscroll[1] & 0xF) as usize;
            let buf_a = usize::from(self.buf_off[0]);
            let buf_b = usize::from(self.buf_off[1]);
            let bg = self.reg(0x07) & 0x3F;
            let shadow_highlight = self.reg(0x0C) & 0x08 != 0;
            let masked = col == 0 && self.reg(0x00) & 0x20 != 0;
            for i in 0..16 {
                let a = if self.in_window {
                    self.plane_buf[0][(buf_a + i) % SCROLL_BUFFER]
                } else if self.window_edge {
                    // A column after the window takes plane A from within
                    // the current column, wrapped to 16 pixels.
                    let index = (i as isize - fine_a as isize).rem_euclid(16) as usize;
                    self.plane_buf[0][(buf_a + index) % SCROLL_BUFFER]
                } else {
                    self.plane_buf[0][(buf_a + SCROLL_BUFFER + i - fine_a) % SCROLL_BUFFER]
                };
                let b = self.plane_buf[1][(buf_b + SCROLL_BUFFER + i - fine_b) % SCROLL_BUFFER];
                let sprite = self.sprite_line[(col * 8 + i).min(self.sprite_line.len() - 1)];
                let pixel = if masked && i < 8 {
                    if shadow_highlight { SHADOW + bg } else { 0 }
                } else if shadow_highlight {
                    composite_shadow_highlight(sprite, a, b, bg)
                } else {
                    composite(sprite, a, b, bg) & 0x3F
                };
                self.composite[dst + i] = pixel;
            }
        }
        self.buf_off[0] = (self.buf_off[0] + SCROLL_STEP) % SCROLL_BUFFER as u8;
        self.buf_off[1] = (self.buf_off[1] + SCROLL_STEP) % SCROLL_BUFFER as u8;
    }

    fn draw_right_border(&mut self, width: Width) {
        let start = BORDER_LEFT + if width == Width::H40 { 320 } else { 256 };
        self.composite[start..start + BORDER_RIGHT].fill(0);
        self.buf_off[0] = (self.buf_off[0] + SCROLL_STEP) % SCROLL_BUFFER as u8;
        self.buf_off[1] = (self.buf_off[1] + SCROLL_STEP) % SCROLL_BUFFER as u8;
    }

    /// Output this slot's pixels, and at the end of the line move the
    /// output to the next one.
    pub(crate) fn output_pixels(&mut self, width: Width, hslot: u8, active: bool) {
        let end = Self::output_end_slot(width);
        if let Some(row) = self
            .output_row
            .filter(|_| hslot >= OUTPUT_START_SLOT && hslot <= end && hslot < 229)
        {
            let x = usize::from(hslot - OUTPUT_START_SLOT) * 2;
            let count = if hslot == end { 1 } else { 2 };
            let bg = self.reg(0x07) & 0x3F;
            for i in x..x + count {
                let pixel = if active || self.composite_valid {
                    let p = self.composite[i];
                    if p & 0x3F == 0 { (p & 0xC0) | bg } else { p }
                } else {
                    bg
                };
                let colour = argb(
                    self.cram[usize::from(pixel & 0x3F) % CRAM_SIZE],
                    pixel & 0xC0,
                );
                self.frame[row * LINE_WIDTH + i] = colour;
            }
        }
        if hslot == end {
            self.composite_valid = false;
            self.advance_output_row(width);
        }
    }

    /// Choose the framebuffer row the next line's pixels go to.
    fn advance_output_row(&mut self, width: Width) {
        let (top, bottom) = borders(self.pal_machine, self.reg(0x01) & 0x08 != 0);
        let blank_start = self.v_timing().blank_start;
        let line = self.vcounter;
        self.output_row = if line < blank_start + bottom {
            Some(usize::from(top + line))
        } else if line >= 0x200 - top {
            Some(usize::from(line - (0x200 - top)))
        } else {
            None
        };
        if line == blank_start + bottom {
            self.frames += 1;
        }
        let rows = usize::from(top + blank_start + bottom);
        self.frame_height = rows;
        self.frame_width = match width {
            Width::H40 => LINE_WIDTH,
            Width::H32 => 256 + BORDER_LEFT + BORDER_RIGHT,
        };
        if self
            .output_row
            .is_some_and(|r| r * LINE_WIDTH >= self.frame.len())
        {
            self.output_row = None;
        }
    }

    /// A CRAM write during the picture shows its colour on the pixel being
    /// output at that moment.
    pub(crate) fn cram_dot(&mut self, colour: u16) {
        let width = self.width();
        let end = Self::output_end_slot(width);
        if let Some(row) = self
            .output_row
            .filter(|_| self.hslot >= OUTPUT_START_SLOT && self.hslot < end)
        {
            let x = usize::from(self.hslot - OUTPUT_START_SLOT) * 2 + 1;
            self.frame[row * LINE_WIDTH + x] = argb(colour, 0);
        }
    }

    /// The framebuffer: `frame_width() x frame_height()` ARGB pixels at a
    /// stride of [`LINE_WIDTH`], borders included.
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.frame
    }

    /// The width of the last line output: 347 in H40, 283 in H32.
    #[must_use]
    pub fn frame_width(&self) -> usize {
        self.frame_width
    }

    /// The lines in a frame, borders included.
    #[must_use]
    pub fn frame_height(&self) -> usize {
        self.frame_height
    }

    /// Frames completed since power-on.
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// The sprite line buffer the compositor reads: one entry per pixel,
    /// priority in bit 6, palette in bits 5-4, colour in bits 3-0.
    #[must_use]
    pub fn sprite_line(&self) -> &[u8] {
        &self.sprite_line
    }
}

/// The normal compositor: sprite over A over B over the backdrop, with a
/// layer's priority bit lifting it over lower layers' priority pixels.
fn composite(sprite: u8, a: u8, b: u8, bg: u8) -> u8 {
    let mut pixel = bg;
    if b & 0xF != 0 {
        pixel = b;
    }
    if a & 0xF != 0 && (a & PRIORITY) >= (pixel & PRIORITY) {
        pixel = a;
    }
    if sprite & 0xF != 0 && (sprite & PRIORITY) >= (pixel & PRIORITY) {
        pixel = sprite;
    }
    pixel
}

/// Shadow and highlight: low-priority planes darken the pixel; sprite
/// colours 3:14 and 3:15 are operators that lighten or darken what lies
/// under them instead of drawing.
fn composite_shadow_highlight(sprite: u8, a: u8, b: u8, bg: u8) -> u8 {
    let mut pixel = bg;
    if b & 0xF != 0 {
        pixel = b;
    }
    let mut intensity = b & PRIORITY;
    if a & 0xF != 0 && (a & PRIORITY) >= (pixel & PRIORITY) {
        pixel = a;
    }
    intensity |= a & PRIORITY;
    if sprite & 0xF != 0 && (sprite & PRIORITY) >= (pixel & PRIORITY) {
        if sprite & 0x3F == 0x3E {
            intensity += PRIORITY;
        } else if sprite & 0x3F == 0x3F {
            intensity = 0;
        } else {
            pixel = sprite;
            if pixel & 0xF == 0xE {
                intensity = PRIORITY;
            } else {
                intensity |= pixel & PRIORITY;
            }
        }
    }
    let index = pixel & 0x3F;
    if intensity == PRIORITY << 1 {
        index + HIGHLIGHT
    } else if intensity != 0 {
        index
    } else {
        index + SHADOW
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_use_the_measured_ladder() {
        assert_eq!(argb(0x0EEE, 0), 0xFFFF_FFFF);
        assert_eq!(argb(0x000E, 0), 0xFFFF_0000);
        assert_eq!(argb(0x0EEE, SHADOW), 0xFF82_8282);
        assert_eq!(argb(0x0000, HIGHLIGHT), 0xFF82_8282);
        assert_eq!(argb(0x0E00, HIGHLIGHT), 0xFF82_82FF);
    }

    #[test]
    fn priority_lifts_a_lower_layer_over_a_higher_one() {
        let b_high = PRIORITY | 0x12;
        let a_low = 0x23;
        assert_eq!(composite(0, a_low, b_high, 0x01), b_high);
        assert_eq!(composite(0, a_low, 0x12, 0x01), a_low);
        assert_eq!(
            composite(0x31, a_low, b_high, 0x01),
            b_high,
            "low sprite under high B"
        );
        assert_eq!(
            composite(0, 0x20, 0x10, 0x05),
            0x05,
            "transparent planes show the backdrop"
        );
    }

    #[test]
    fn shadow_highlight_darkens_low_planes_and_obeys_the_operators() {
        assert_eq!(
            composite_shadow_highlight(0, 0x23, 0x12, 0x01),
            0x23 + SHADOW
        );
        assert_eq!(
            composite_shadow_highlight(0, PRIORITY | 0x23, 0x12, 0x01),
            0x23
        );
        assert_eq!(
            composite_shadow_highlight(0x3E, 0x23, 0x12, 0x01),
            0x23,
            "highlight a shadow: normal"
        );
        assert_eq!(
            composite_shadow_highlight(PRIORITY | 0x3E, PRIORITY | 0x23, 0, 0),
            0x23 + HIGHLIGHT
        );
        assert_eq!(
            composite_shadow_highlight(PRIORITY | 0x3F, PRIORITY | 0x23, 0, 0),
            0x23 + SHADOW
        );
    }
}
