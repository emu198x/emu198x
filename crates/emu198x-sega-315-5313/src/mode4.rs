//! Mode 4: the Master System mode, which a Mega Drive runs Master System
//! cartridges in through the Power Base Converter.
//!
//! With register 1's M5 bit clear and register 0's M4 bit set, the
//! 315-5313 behaves as a Master System VDP: one 32×28 (or 32×32) tile
//! layer, 64 sprites of 8×8 or 8×16, 32 colours of six bits, and the
//! Master System's 8-bit ports. The line runs on the H32 slot clock, 171
//! slots of 20 master clocks, with its own schedule:
//!
//! | Slots | Work |
//! |---|---|
//! | 248 | The line changes |
//! | 251, 252 | External; 252 latches the horizontal scroll |
//! | 253-4 | Sprite table scan, two entries a slot |
//! | 5-132 | 32 columns of four slots: name table, then scan or external (every fourth column), pattern, draw |
//! | 133-136 | External; 136 clears the sprite line for the next line |
//! | 137-147, 233, 239-250 | Four groups of six: two sprite X reads, then two pattern fetch and draw pairs |
//! | 234-238 | External |
//!
//! So a line's sprites are found during that line and drawn into the line
//! buffer at its end, ready for the next line's columns: a sprite at Y
//! shows from line Y+1. The schedule, the V-int slot (4, on the line after
//! the picture) and the line change are BlastEm's, measured on hardware
//! (`vdp_h32_mode4` in its `vdp.c`); the port behaviour follows Genesis
//! Plus GX (`vdp_z80_*_m4` in its `vdp_ctrl.c`). Both are used for facts
//! only.
//!
//! # What the 315-5313 does differently from a Master System VDP
//!
//! - **No legacy modes.** With neither M4 nor M5 set, the TMS9918 modes a
//!   Master System VDP inherited are not there: the picture is black.
//! - **No sprite zoom.** Register 1's MAG bit doubles nothing; a Mega
//!   Drive in Mode 4 never magnifies sprites.
//! - **None of the 315-5124's bugs.** Register 2's bit 0 does not mask
//!   the name table, and every sprite on a line is treated alike.
//! - **Status bits 4-0** read the Mega Drive's own status flags (V-blank,
//!   H-blank, PAL) rather than ones.
//! - **The address increment** is register 15 plus one. Register 15 cannot
//!   be written in Mode 4, so it keeps whatever Mode 5 left, normally 0.
//! - **VRAM** is laid out differently: a Mode 4 address has bits 1 and 9
//!   swapped on its way to the chips (BlastEm's `mode4_address_map`), so
//!   what Mode 4 writes is not where Mode 5 would read it. Whether the
//!   byte lanes also swap is not measured; this model swaps them too, as
//!   BlastEm's write path implies.
//!
//! # Simplifications, documented
//!
//! - Port accesses are served at once, as Genesis Plus GX serves them,
//!   rather than through the FIFO in the external slots.
//! - A column's eight pixels are composited and written when it is drawn,
//!   through CRAM as it is then; the output stage's few slots of delay are
//!   not modelled, so a mid-line CRAM write lands a few pixels early.
//! - Colours use the Master System's levels, 0, 85, 170 and 255 a channel.
//!   The 315-5313 drives Mode 4 through its own DAC tap, which neither
//!   reference has measured.
//! - The 224- and 240-line heights are available, as Genesis Plus GX makes
//!   them on the 315-5313. BlastEm allows only 192.

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::Vdp;
use crate::render::{BORDER_LEFT, BORDER_RIGHT, LINE_WIDTH};
use crate::timing::{Slot, Width};

/// The slot at which a Mode 4 line changes.
pub const LINE_CHANGE_MODE4: u8 = 248;
/// The slot at which V-int is raised, on the line after the picture.
pub const VINT_SLOT_MODE4: u8 = 4;
/// Sprites a line can draw.
const SPRITES_PER_LINE: usize = 8;
/// Sprites in the table.
const SPRITES: u8 = 64;
/// The Y value that ends the sprite list in the 192-line mode.
const SPRITE_LIST_END: u8 = 0xD0;
/// The sprite line buffer: 256 pixels and room for a sprite past the edge.
const SPRITE_LINE: usize = 256 + 8;
/// The width of a Mode 4 line in the framebuffer, borders included.
pub const MODE4_FRAME_WIDTH: usize = 256 + BORDER_LEFT + BORDER_RIGHT;

/// Register 0's M4 bit.
pub(crate) const BIT_MODE_4: u8 = 0x04;

/// A sprite found by the scan: its table index and the row of it on the
/// next line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Found {
    index: u8,
    row: u8,
}

/// A sprite placed by its X read, ready to draw.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Draw {
    x: u8,
    address: u16,
}

/// Mode 4's state, beside the chip's Mode 5 state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Mode4 {
    /// Colour RAM: 32 colours, `--BBGGRR`.
    pub(crate) cram: [u8; 32],
    /// The data port's read buffer.
    read_buffer: u8,
    /// The control port's first byte, while the second is awaited.
    first_byte: Option<u8>,
    /// The scrolls latched for the line, and the frame.
    hscroll: u8,
    vscroll: u8,
    /// The column being fetched: its name table entry and pattern row.
    entry: u16,
    pattern: [u8; 4],
    /// The sprite scan.
    scan_index: u8,
    scan_done: bool,
    found: [Found; SPRITES_PER_LINE],
    found_count: u8,
    /// The sprites placed for drawing, in table order, and the next to
    /// place and to draw.
    draws: [Draw; SPRITES_PER_LINE],
    draw_count: u8,
    next_draw: u8,
    /// The sprite line: colour 1-15 per pixel, 0 where no sprite is.
    #[serde(with = "BigArray")]
    sprite_line: [u8; SPRITE_LINE],
}

impl Default for Mode4 {
    fn default() -> Self {
        Self {
            cram: [0; 32],
            read_buffer: 0,
            first_byte: None,
            hscroll: 0,
            vscroll: 0,
            entry: 0,
            pattern: [0; 4],
            scan_index: 0,
            scan_done: true,
            found: [Found::default(); SPRITES_PER_LINE],
            found_count: 0,
            draws: [Draw::default(); SPRITES_PER_LINE],
            draw_count: 0,
            next_draw: 0,
            sprite_line: [0; SPRITE_LINE],
        }
    }
}

/// A Mode 4 colour, `--BBGGRR`, as ARGB.
#[must_use]
pub fn argb_mode4(colour: u8) -> u32 {
    let level = |c: u8| u32::from(c & 3) * 85;
    0xFF00_0000 | level(colour) << 16 | level(colour >> 2) << 8 | level(colour >> 4)
}

/// Where a Mode 4 VRAM address lands in the chips: bits 1 and 9 swap, and
/// so do the byte lanes.
fn vram_index(address: u32) -> usize {
    let a = address & 0x3FFF;
    (((a & 0x3DFD) | (a << 8 & 0x200) | (a >> 8 & 2)) ^ 1) as usize
}

impl Vdp {
    // ---- Configuration. ----

    /// Register 0's M4 bit, with M5 clear: the Master System mode.
    pub(crate) fn mode_4(&self) -> bool {
        !self.mode_5() && self.regs[0] & BIT_MODE_4 != 0
    }

    /// The picture's height in Mode 4. The taller pictures need M2 as well
    /// as M4, and M1 and M3 as exact matches: 224 is M1 alone, 240 M3
    /// alone; anything else is 192.
    pub(crate) fn mode4_height(&self) -> u16 {
        let m2 = self.regs[0] & 0x02 != 0;
        let m1 = self.regs[1] & 0x10 != 0;
        let m3 = self.regs[1] & 0x08 != 0;
        match (m2, m1, m3) {
            (true, true, false) => 224,
            (true, false, true) => 240,
            _ => 192,
        }
    }

    fn mode4_display_enabled(&self) -> bool {
        self.mode_4() && self.regs[1] & 0x40 != 0
    }

    /// The current line as Mode 4 counts it: the lines after the picture
    /// that wrap to the top read as negative.
    fn mode4_line(&self) -> i16 {
        let line = (self.vcounter & 0xFF) as i16;
        if line > self.mode4_height() as i16 {
            line - 0x100
        } else {
            line
        }
    }

    /// Whether the picture is being drawn: the display is on and the line
    /// is inside it.
    pub(crate) fn mode4_active(&self) -> bool {
        self.mode4_display_enabled() && self.vcounter < self.mode4_height()
    }

    fn vram4(&self, address: u32) -> u8 {
        self.vram[vram_index(address)]
    }

    fn increment4(&mut self) {
        self.address = (self.address + u32::from(self.regs[0x0F]) + 1) & 0x3FFF;
    }

    // ---- The Master System ports. ----

    /// Write the control port (`$BF` on a Master System): two bytes, the
    /// address's low byte then its high six bits and a two-bit code. Code
    /// 2 writes a register, code 0 sets up a read.
    pub fn write_control_byte(&mut self, value: u8) {
        let Some(low) = self.m4.first_byte.take() else {
            self.m4.first_byte = Some(value);
            return;
        };
        self.address = u32::from(value & 0x3F) << 8 | u32::from(low);
        self.cd = (self.cd & !3) | (value >> 6);
        match value >> 6 {
            2 => self.write_register(usize::from(value & 0x1F), low),
            0 => {
                self.m4.read_buffer = self.vram4(self.address);
                self.increment4();
            }
            _ => {}
        }
    }

    /// Write the data port (`$BE`): to CRAM after a code 2 or 3, else to
    /// VRAM.
    pub fn write_data_byte(&mut self, value: u8) {
        self.m4.first_byte = None;
        if self.cd & 2 != 0 {
            self.m4.cram[(self.address & 0x1F) as usize] = value & 0x3F;
        } else {
            let index = vram_index(self.address);
            self.vram[index] = value;
        }
        self.increment4();
    }

    /// Read the data port: the read buffer, which then refills from the
    /// next address.
    pub fn read_data_byte(&mut self) -> u8 {
        self.m4.first_byte = None;
        let value = self.m4.read_buffer;
        self.m4.read_buffer = self.vram4(self.address);
        self.increment4();
        value
    }

    /// Read the status port: the frame interrupt, sprite overflow and
    /// collision flags in bits 7-5, which reading clears with both
    /// interrupts, and the Mega Drive's own flags below.
    pub fn read_status_byte(&mut self) -> u8 {
        self.m4.first_byte = None;
        let value = self.status() as u8;
        self.vint_pending = false;
        self.hint_pending = false;
        self.sprite_overflow = false;
        self.sprite_collision = false;
        value
    }

    /// The V counter port (`$7E`).
    #[must_use]
    pub fn read_v_counter(&self) -> u8 {
        self.vcounter as u8
    }

    /// The H counter port (`$7F`): the H32 slot counter.
    #[must_use]
    pub fn read_h_counter(&self) -> u8 {
        self.hslot
    }

    /// Whether the VDP is pulling the Z80's /INT low: a pending frame or
    /// line interrupt that is enabled.
    #[must_use]
    pub fn z80_interrupt(&self) -> bool {
        self.interrupt_level() != 0
    }

    /// Mode 4's 32 colours, `--BBGGRR`.
    #[must_use]
    pub fn cram_mode4(&self) -> &[u8; 32] {
        &self.m4.cram
    }

    // ---- The line. ----

    /// Everything at the start of a slot in Mode 4, or with neither mode
    /// set.
    pub(crate) fn begin_slot_mode4(&mut self) {
        let hslot = self.hslot;
        if hslot == LINE_CHANGE_MODE4 {
            self.advance_line_mode4();
        }
        if self.vcounter == self.mode4_height() + 1 && hslot == VINT_SLOT_MODE4 {
            self.vint_pending = true;
        }
        let active = self.mode4_display_enabled() && {
            let line = self.mode4_line();
            (-1..self.mode4_height() as i16).contains(&line)
        };
        let kind = if active {
            self.mode4_slot(hslot)
        } else if Width::H32.blank_refresh(hslot) {
            Slot::Refresh
        } else {
            Slot::External
        };
        if kind == Slot::External {
            self.external_slot();
        }
    }

    /// The line changes: the counters step, the frame interrupt's counter
    /// runs, and the framebuffer row for the new line is chosen and given
    /// its borders.
    fn advance_line_mode4(&mut self) {
        let height = self.mode4_height();
        self.vcounter = self.v_timing().next(self.vcounter);
        // The line counter runs on the picture's lines and the one after.
        if self.vcounter <= height {
            if self.hint_counter == 0 {
                self.hint_pending = true;
                self.hint_counter = self.regs[0x0A];
            } else {
                self.hint_counter -= 1;
            }
        } else {
            self.hint_counter = self.regs[0x0A];
        }
        if self.vcounter == 0 {
            self.m4.vscroll = self.regs[0x09];
        }
        self.output_row_mode4();
    }

    /// The borders above and below a Mode 4 picture: the Mode 5 frame's,
    /// with the shorter pictures centred in it.
    fn mode4_borders(&self) -> (u16, u16) {
        match (self.pal_machine, self.mode4_height()) {
            (false, 192) => (27, 24),
            (false, 240) => (1, 3),
            (false, _) => (11, 8),
            (true, 192) => (54, 48),
            (true, 240) => (30, 24),
            (true, _) => (38, 32),
        }
    }

    fn border_colour(&self) -> u32 {
        if self.mode_4() {
            argb_mode4(self.m4.cram[16 + usize::from(self.regs[0x07] & 0xF)])
        } else {
            // Neither mode: the 315-5313 has no TMS9918 modes, and shows
            // black.
            0xFF00_0000
        }
    }

    fn output_row_mode4(&mut self) {
        let (top, bottom) = self.mode4_borders();
        let height = self.mode4_height();
        let line = self.vcounter;
        self.output_row = if line < height + bottom {
            Some(usize::from(top + line))
        } else if line >= 0x200 - top {
            Some(usize::from(line - (0x200 - top)))
        } else {
            None
        };
        if line == height + bottom {
            self.frames += 1;
        }
        self.frame_height = usize::from(top + height + bottom);
        self.frame_width = MODE4_FRAME_WIDTH;
        let Some(row) = self
            .output_row
            .filter(|r| (r + 1) * LINE_WIDTH <= self.frame.len())
        else {
            self.output_row = None;
            return;
        };
        let border = self.border_colour();
        let start = row * LINE_WIDTH;
        let picture = self.mode4_display_enabled() && line < height;
        if picture {
            self.frame[start..start + BORDER_LEFT].fill(border);
            let right = start + BORDER_LEFT + 256;
            self.frame[right..right + BORDER_RIGHT].fill(border);
        } else {
            self.frame[start..start + MODE4_FRAME_WIDTH].fill(border);
        }
    }

    /// What slot `hslot` does on a line with work to do, and the work.
    fn mode4_slot(&mut self, hslot: u8) -> Slot {
        let picture_line = self.mode4_line() >= 0;
        match hslot {
            252 => {
                let lock = self.regs[0] & 0x40 != 0 && self.mode4_line() < 16;
                self.m4.hscroll = if lock { 0 } else { self.regs[0x08] };
                Slot::External
            }
            251 | 133..=135 | 234..=238 => Slot::External,
            253 => {
                self.m4.scan_index = 0;
                self.m4.scan_done = false;
                self.m4.found_count = 0;
                self.scan_sprites_mode4();
                Slot::Fetch
            }
            254 | 255 | 0..=4 => {
                self.scan_sprites_mode4();
                if hslot == 4 && picture_line {
                    self.leading_pixels_mode4();
                }
                Slot::Fetch
            }
            5..=132 => {
                let column = (hslot - 5) / 4;
                match (hslot - 5) % 4 {
                    0 => {
                        if picture_line {
                            self.read_name_mode4(column);
                        }
                        Slot::Fetch
                    }
                    1 if column.is_multiple_of(4) => Slot::External,
                    1 => {
                        self.scan_sprites_mode4();
                        Slot::Fetch
                    }
                    2 => {
                        if picture_line {
                            self.fetch_pattern_mode4(column);
                        }
                        Slot::Fetch
                    }
                    _ => {
                        if picture_line {
                            self.draw_column_mode4(column);
                        }
                        Slot::Fetch
                    }
                }
            }
            136 => {
                self.m4.sprite_line.fill(0);
                self.m4.draw_count = 0;
                self.m4.next_draw = 0;
                Slot::External
            }
            _ => {
                // The four sprite groups: two X reads, then fetch and draw
                // twice.
                let group_slot = match hslot {
                    137..=142 => hslot - 137,
                    143..=147 => hslot - 143,
                    233 => 5,
                    239..=244 => hslot - 239,
                    245..=250 => hslot - 245,
                    _ => return Slot::Fetch,
                };
                match group_slot {
                    0 | 1 => self.read_sprite_x_mode4(),
                    3 | 5 => self.draw_sprite_mode4(),
                    _ => {}
                }
                Slot::Fetch
            }
        }
    }

    // ---- The background. ----

    /// The name table row and fine row a column on this line reads, with
    /// the vertical scroll, which columns 24-31 can be locked out of.
    fn background_row(&self, column: u8) -> u16 {
        let line = self.mode4_line().max(0) as u16;
        let lock = self.regs[0] & 0x80 != 0 && column >= 24;
        let scroll = if lock { 0 } else { u16::from(self.m4.vscroll) };
        let rows = if self.mode4_height() == 192 { 224 } else { 256 };
        (line + scroll) % rows
    }

    fn name_table_base(&self) -> u32 {
        let r2 = u32::from(self.regs[0x02]);
        if self.mode4_height() == 192 {
            (r2 & 0x0E) << 10
        } else {
            ((r2 & 0x0C) << 10) | 0x0700
        }
    }

    fn read_name_mode4(&mut self, column: u8) {
        let coarse = self.m4.hscroll >> 3;
        let tile_column = u32::from(column.wrapping_sub(coarse) & 31);
        let row = u32::from(self.background_row(column) >> 3);
        let address = self.name_table_base() + (row * 32 + tile_column) * 2;
        self.m4.entry = u16::from(self.vram4(address)) | u16::from(self.vram4(address + 1)) << 8;
    }

    /// Fetch the pattern row the column's name table entry shows on this
    /// line, flipped vertically if the entry asks.
    fn fetch_pattern_mode4(&mut self, column: u8) {
        let entry = self.m4.entry;
        let row = self.background_row(column) & 7;
        let row = if entry & 0x0400 != 0 { 7 - row } else { row };
        let tile = u32::from(entry & 0x1FF);
        let address = tile * 32 + u32::from(row) * 4;
        self.m4.pattern = std::array::from_fn(|i| self.vram4(address + i as u32));
    }

    /// The left edge: the pixels the fine scroll pushes the first column
    /// past are colour 0 of the background palette.
    fn leading_pixels_mode4(&mut self) {
        let fine = usize::from(self.m4.hscroll & 7);
        for x in 0..fine {
            self.put_pixel_mode4(x, 0, false);
        }
    }

    /// Composite and write the column's eight pixels.
    fn draw_column_mode4(&mut self, column: u8) {
        let entry = self.m4.entry;
        let palette = if entry & 0x0800 != 0 { 16 } else { 0 };
        let priority = entry & 0x1000 != 0;
        let h_flip = entry & 0x0200 != 0;
        let fine = usize::from(self.m4.hscroll & 7);
        let pattern = self.m4.pattern;
        for i in 0..8 {
            let x = usize::from(column) * 8 + fine + i;
            if x >= 256 {
                break;
            }
            let bit = if h_flip { i } else { 7 - i };
            let colour = planar(pattern, bit);
            self.put_pixel_mode4(x, palette + colour, priority && colour != 0);
        }
    }

    /// One picture pixel: the background's colour index, over or under the
    /// sprite at `x`.
    fn put_pixel_mode4(&mut self, x: usize, background: u8, in_front: bool) {
        let Some(row) = self.output_row else {
            return;
        };
        let sprite = self.m4.sprite_line[x];
        let index = if self.regs[0] & 0x20 != 0 && x < 8 {
            16 + (self.regs[0x07] & 0xF)
        } else if sprite != 0 && !in_front {
            16 + sprite
        } else {
            background
        };
        let colour = argb_mode4(self.m4.cram[usize::from(index & 0x1F)]);
        if let Some(pixel) = self.frame.get_mut(row * LINE_WIDTH + BORDER_LEFT + x) {
            *pixel = colour;
        }
    }

    // ---- Sprites. ----

    fn sprite_table(&self) -> u32 {
        u32::from(self.regs[0x05] & 0x7E) << 7
    }

    fn sprite_height(&self) -> i16 {
        if self.regs[1] & 0x02 != 0 { 16 } else { 8 }
    }

    /// Scan two sprite table entries for the next line.
    fn scan_sprites_mode4(&mut self) {
        let line = self.mode4_line();
        let height = self.mode4_height() as i16;
        let size = self.sprite_height();
        for _ in 0..2 {
            if self.m4.scan_done {
                return;
            }
            let index = self.m4.scan_index;
            let raw = self.vram4(self.sprite_table() + u32::from(index));
            if height == 192 && raw == SPRITE_LIST_END {
                self.m4.scan_done = true;
                return;
            }
            let mut y = i16::from(raw);
            if y > height {
                y -= 0x100;
            }
            if y <= line && line < y + size {
                if usize::from(self.m4.found_count) == SPRITES_PER_LINE {
                    self.sprite_overflow = true;
                    self.m4.scan_done = true;
                    return;
                }
                self.m4.found[usize::from(self.m4.found_count)] = Found {
                    index,
                    row: (line - y) as u8,
                };
                self.m4.found_count += 1;
            }
            self.m4.scan_index += 1;
            if self.m4.scan_index == SPRITES {
                self.m4.scan_done = true;
            }
        }
    }

    /// Read the next found sprite's X and pattern number, placing it.
    fn read_sprite_x_mode4(&mut self) {
        let k = self.m4.draw_count;
        if k >= self.m4.found_count {
            return;
        }
        let found = self.m4.found[usize::from(k)];
        let attributes = self.sprite_table() + 0x80 + u32::from(found.index) * 2;
        let x = self.vram4(attributes);
        let mut tile = u16::from(self.vram4(attributes + 1));
        if self.regs[0x06] & 0x04 != 0 {
            tile |= 0x100;
        }
        if self.sprite_height() == 16 {
            tile &= !1;
        }
        self.m4.draws[usize::from(k)] = Draw {
            x,
            address: tile * 32 + u16::from(found.row) * 4,
        };
        self.m4.draw_count += 1;
    }

    /// Draw the next placed sprite into the line buffer. An earlier sprite
    /// in the table keeps its pixels; a later one landing on them sets the
    /// collision flag.
    fn draw_sprite_mode4(&mut self) {
        let k = self.m4.next_draw;
        if k >= self.m4.draw_count {
            return;
        }
        self.m4.next_draw += 1;
        let draw = self.m4.draws[usize::from(k)];
        let pattern: [u8; 4] =
            std::array::from_fn(|i| self.vram4(u32::from(draw.address) + i as u32));
        let shift = if self.regs[0] & 0x08 != 0 { 8 } else { 0 };
        let left = i16::from(draw.x) - shift;
        for i in 0..8 {
            let x = left + i as i16;
            let colour = planar(pattern, 7 - i);
            if colour == 0 || !(0..256).contains(&x) {
                continue;
            }
            let slot = &mut self.m4.sprite_line[x as usize];
            if *slot == 0 {
                *slot = colour;
            } else {
                self.sprite_collision = true;
            }
        }
    }
}

/// A planar pattern row's pixel `bit` (7 is the leftmost): one bit from
/// each of the four bytes.
fn planar(pattern: [u8; 4], bit: usize) -> u8 {
    (0..4).fold(0, |colour, plane| {
        colour | ((pattern[plane] >> bit) & 1) << plane
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_4_addresses_swap_bits_1_and_9_and_the_byte_lanes() {
        assert_eq!(vram_index(0x0000), 0x0001);
        assert_eq!(vram_index(0x0002), 0x0201);
        assert_eq!(vram_index(0x0200), 0x0003);
        assert_eq!(vram_index(0x3FFF), 0x3FFE);
        // A bijection on the 16 KB a Master System addresses.
        let mut seen = vec![false; 0x4000];
        for a in 0..0x4000 {
            let i = vram_index(a);
            assert!(!seen[i]);
            seen[i] = true;
        }
    }

    #[test]
    fn colours_take_the_master_system_levels() {
        assert_eq!(argb_mode4(0x00), 0xFF00_0000);
        assert_eq!(argb_mode4(0x03), 0xFFFF_0000);
        assert_eq!(argb_mode4(0x0C), 0xFF00_FF00);
        assert_eq!(argb_mode4(0x30), 0xFF00_00FF);
        assert_eq!(argb_mode4(0x15), 0xFF55_5555);
    }

    #[test]
    fn planar_rows_read_one_bit_from_each_plane() {
        let row = [0b1000_0001, 0b1000_0000, 0b0000_0000, 0b1000_0001];
        assert_eq!(planar(row, 7), 0b1011);
        assert_eq!(planar(row, 0), 0b1001);
        assert_eq!(planar(row, 3), 0);
    }
}
