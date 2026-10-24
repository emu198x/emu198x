//! The 8563 VDC: the C128's 80-column display.
//!
//! The CPU reaches it through two registers: the address register at `$D600`
//! and the data register at `$D601`. Behind them are 38 registers and 16 KB
//! of video RAM. The update address (R18/R19) is where R31 reads and writes,
//! and a write to R30 starts a block fill or copy. That part follows VICE's
//! `vdc-mem.c`.
//!
//! The VDC runs on its own 16 MHz dot clock, which [`Vdc::tick`] advances
//! against the C128's phi2. Each time the beam finishes a raster line, that
//! line is drawn from video RAM as VICE's `vdc-draw.c` draws it:
//! - text or bitmap (R25 bit 7), with or without attributes (R25 bit 6);
//! - double-width pixels (R25 bit 4);
//! - the character cell's size and displayed part (R9, R22, R23);
//! - the alternate character set, underline (R29), blink and reverse
//!   attributes;
//! - the cursor (R10, R11, R14, R15) and whole-screen reverse (R24 bit 6).
//!
//! The status register's blanking flag follows the same beam.
//!
//! Simplifications, which a later pass can take on:
//! - The display is placed at a fixed position in the framebuffer rather
//!   than by the sync registers.
//! - The smooth-scroll registers (R24, R25 low bits) are not applied.
//! - Interlace (R8) is not modelled.
//! - The VDC is always ready: VICE approximates how long an access keeps it
//!   busy, and the KERNAL only waits on the ready bit.

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

/// The flat C128's VDC has 16 KB.
const VRAM_SIZE: usize = 0x4000;

/// Bits that read as 1 in each register (VICE `regmask`).
const REGISTER_MASK: [u8; 38] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0xE0, 0x00, 0x00, 0xFC, 0xE0, 0x80, 0xE0, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xE0, 0x00, 0x00, 0x00, 0x00, 0x0F, 0xE0, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0xF0, 0x3F,
];

/// The revision in the status register's low bits: an 8563 R9.
const REVISION: u8 = 2;

/// The dot clock, and the phi2 clock it is measured against (PAL).
const DOT_HZ: u64 = 16_000_000;
const PHI2_HZ: u64 = 985_248;

/// The framebuffer: 80 columns of 8 pixels with a 32-pixel border either
/// side, and 25 rows of 8 lines with a 35-line border above and below.
pub(crate) const VDC_FB_WIDTH: usize = 704;
pub(crate) const VDC_FB_HEIGHT: usize = 270;
const BORDER_LEFT: usize = 32;
const BORDER_TOP: usize = 35;

/// The VDC's sixteen RGBI colours (VICE `vdc-color.c`).
const PALETTE: [u32; 16] = [
    0xFF00_0000,
    0xFF55_5555,
    0xFF00_00AA,
    0xFF55_55FF,
    0xFF00_AA00,
    0xFF55_FF55,
    0xFF00_AAAA,
    0xFF55_FFFF,
    0xFFAA_0000,
    0xFFFF_5555,
    0xFFAA_00AA,
    0xFFFF_55FF,
    0xFFAA_5500,
    0xFFFF_FF55,
    0xFFAA_AAAA,
    0xFFFF_FFFF,
];

/// How many of a character's eight pixels R22's low nibble displays.
const DISPLAYED_PIXELS: [u8; 16] = [
    0x80, 0xC0, 0xE0, 0xF0, 0xF8, 0xFC, 0xFE, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
];

/// Cursor modes from R10 bits 5-6: solid, none, blinking every 16 frames,
/// every 32 (VICE `crsrblink`).
const CURSOR_BLINK: [u64; 4] = [0x01, 0x00, 0x08, 0x10];

/// Attribute bits.
const ATTR_BLINK: u8 = 0x10;
const ATTR_UNDERLINE: u8 = 0x20;
const ATTR_REVERSE: u8 = 0x40;
const ATTR_ALTERNATE: u8 = 0x80;

/// The 8563 VDC.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Vdc {
    #[serde(with = "BigArray")]
    regs: [u8; 38],
    /// The register `$D601` reaches.
    selected: u8,
    #[serde(with = "BigArray")]
    vram: [u8; VRAM_SIZE],
    /// The beam's raster line from the top of the frame.
    line: u16,
    /// Dot-clock time banked towards the end of the line, in units of
    /// 1/PHI2_HZ dots.
    dot_time: u64,
    /// Frames drawn, for blinking.
    frame_counter: u64,
    framebuffer: Vec<u32>,
}

impl Vdc {
    pub(crate) fn new() -> Self {
        Self {
            regs: [0; 38],
            selected: 0,
            vram: [0; VRAM_SIZE],
            line: 0,
            dot_time: 0,
            frame_counter: 0,
            framebuffer: vec![PALETTE[0]; VDC_FB_WIDTH * VDC_FB_HEIGHT],
        }
    }

    /// The picture, `VDC_FB_WIDTH` by `VDC_FB_HEIGHT`.
    pub(crate) fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    /// Advance the dot clock by one phi2 cycle, drawing each raster line as
    /// the beam leaves it.
    pub(crate) fn tick(&mut self) {
        self.dot_time += DOT_HZ;
        let line_time = u64::from(self.line_dots()) * PHI2_HZ;
        while self.dot_time >= line_time {
            self.dot_time -= line_time;
            self.end_line();
        }
    }

    /// Dots in one raster line: R0 + 1 characters of R22's high nibble + 1
    /// dots. Before the KERNAL programs R0, a line of 127 eight-dot cells.
    fn line_dots(&self) -> u32 {
        let chars = if self.regs[0] == 0 {
            127
        } else {
            u32::from(self.regs[0]) + 1
        };
        chars * self.cell_dots()
    }

    fn cell_dots(&self) -> u32 {
        u32::from(self.regs[22] >> 4) + 1
    }

    /// Raster lines per character row (R9 + 1).
    fn row_lines(&self) -> u16 {
        u16::from(self.regs[9] & 0x1F) + 1
    }

    /// Raster lines in a frame: R4 + 1 rows plus R5 adjustment lines, or a
    /// PAL frame's 312 before the KERNAL programs R4.
    fn frame_lines(&self) -> u16 {
        if self.regs[4] == 0 {
            return 312;
        }
        (u16::from(self.regs[4]) + 1) * self.row_lines() + u16::from(self.regs[5] & 0x1F)
    }

    /// Whether the beam is in the rows R6 displays.
    fn displaying(&self) -> bool {
        self.line < u16::from(self.regs[6]) * self.row_lines()
    }

    fn end_line(&mut self) {
        if self.displaying() {
            self.draw_line();
        }
        self.line += 1;
        if self.line >= self.frame_lines() {
            self.line = 0;
            self.frame_counter += 1;
        }
    }

    fn vram_at(&self, address: u16) -> u8 {
        self.vram[usize::from(address) & (VRAM_SIZE - 1)]
    }

    /// Draw the raster line the beam has just finished.
    fn draw_line(&mut self) {
        let y = BORDER_TOP + usize::from(self.line);
        if y >= VDC_FB_HEIGHT {
            return;
        }
        let row_lines = self.row_lines();
        let (row, scan) = (self.line / row_lines, (self.line % row_lines) as u8);
        let stride = u16::from(self.regs[1]) + u16::from(self.regs[27]);
        let screen = u16::from(self.regs[12]) << 8 | u16::from(self.regs[13]);
        let attributes = u16::from(self.regs[20]) << 8 | u16::from(self.regs[21]);
        let cursor = u16::from(self.regs[14]) << 8 | u16::from(self.regs[15]);
        let bitmap = self.regs[25] & 0x80 != 0;
        let with_attributes = self.regs[25] & 0x40 != 0;
        let double = self.regs[25] & 0x10 != 0;
        let bytes_per_char: u16 = if self.regs[9] & 0x1F < 16 { 16 } else { 32 };
        let charset = u16::from(self.regs[28] & 0xE0) << 8;
        let background = PALETTE[usize::from(self.regs[26] & 0x0F)];
        let cell = self.cell_dots() as usize * if double { 2 } else { 1 };
        let mask = DISPLAYED_PIXELS[usize::from(self.regs[22] & 0x0F)];
        let cursor_on =
            (self.frame_counter | 1) & CURSOR_BLINK[usize::from((self.regs[10] >> 5) & 3)] != 0;
        let (cursor_start, cursor_end) = (self.regs[10] & 0x1F, self.regs[11] & 0x1F);
        let in_cursor = (scan >= cursor_start && scan < cursor_end)
            || (scan == cursor_start && scan == cursor_end)
            || (cursor_start > cursor_end && (scan >= cursor_start || scan < cursor_end));
        let blink_rate = if self.regs[24] & 0x20 != 0 { 16 } else { 8 };
        let blink_phase = self.frame_counter & blink_rate != 0;

        let mut line = vec![background; VDC_FB_WIDTH];
        for col in 0..u16::from(self.regs[1]) {
            let cell_address = screen.wrapping_add(row * stride).wrapping_add(col);
            let attribute = if with_attributes {
                self.vram_at(attributes.wrapping_add(row * stride).wrapping_add(col))
            } else {
                0
            };
            let mut pixels = if bitmap {
                self.vram_at(screen.wrapping_add(self.line * stride).wrapping_add(col))
            } else if scan > self.regs[23] & 0x1F {
                0
            } else {
                let code = u16::from(self.vram_at(cell_address));
                let set = if attribute & ATTR_ALTERNATE != 0 {
                    0x100 * bytes_per_char
                } else {
                    0
                };
                self.vram_at(charset + set + code * bytes_per_char + u16::from(scan))
            };
            pixels &= mask;
            let mut gap = 0u8;
            if with_attributes {
                if scan == self.regs[29] & 0x1F && attribute & ATTR_UNDERLINE != 0 {
                    pixels = 0xFF;
                    gap = 0xFF;
                }
                if blink_phase && attribute & ATTR_BLINK != 0 {
                    pixels = 0;
                    gap = 0;
                }
                if attribute & ATTR_REVERSE != 0 {
                    pixels ^= 0xFF;
                    gap ^= 0xFF;
                }
            }
            if !bitmap && cell_address == cursor && cursor_on && in_cursor {
                pixels ^= 0xFF;
                gap ^= 0xFF;
            }
            if self.regs[24] & 0x40 != 0 {
                pixels ^= 0xFF;
                gap ^= 0xFF;
            }
            let foreground = PALETTE[usize::from(if with_attributes {
                attribute & 0x0F
            } else {
                self.regs[26] >> 4
            })];
            let x0 = BORDER_LEFT + usize::from(col) * cell;
            for dot in 0..cell {
                let Some(out) = line.get_mut(x0 + dot) else {
                    break;
                };
                let source_dot = if double { dot / 2 } else { dot };
                let on = if source_dot < 8 {
                    pixels & (0x80 >> source_dot) != 0
                } else {
                    gap != 0
                };
                if on {
                    *out = foreground;
                }
            }
        }
        self.framebuffer[y * VDC_FB_WIDTH..(y + 1) * VDC_FB_WIDTH].copy_from_slice(&line);
    }

    /// Read `$D600` (even addresses) or `$D601` (odd).
    pub(crate) fn read(&mut self, addr: u16) -> u8 {
        if addr & 1 == 0 {
            return self.status();
        }
        match self.selected {
            31 => {
                let value = self.vram_read(self.update_address());
                self.set_update_address(self.update_address().wrapping_add(1));
                value
            }
            reg @ 0..=37 => self.regs[usize::from(reg)] | REGISTER_MASK[usize::from(reg)],
            _ => 0xFF,
        }
    }

    /// Read without side effects.
    pub(crate) fn peek(&self, addr: u16) -> u8 {
        if addr & 1 == 0 {
            return self.status();
        }
        match self.selected {
            31 => self.vram_read(self.update_address()),
            reg @ 0..=37 => self.regs[usize::from(reg)] | REGISTER_MASK[usize::from(reg)],
            _ => 0xFF,
        }
    }

    /// Write `$D600` (select a register) or `$D601` (write it).
    pub(crate) fn write(&mut self, addr: u16, value: u8) {
        if addr & 1 == 0 {
            self.selected = value & 0x3F;
            return;
        }
        let reg = self.selected;
        if reg >= 38 {
            return;
        }
        match reg {
            // The light pen position is read-only.
            16 | 17 => {}
            30 => {
                self.regs[30] = value;
                self.fill_or_copy();
            }
            31 => {
                self.regs[31] = value;
                let address = self.update_address();
                self.vram_write(address, value);
                self.set_update_address(address.wrapping_add(1));
            }
            _ => self.regs[usize::from(reg)] = value,
        }
    }

    /// The status register: always ready, the revision, and bit 5 set while
    /// the beam is outside the displayed rows.
    fn status(&self) -> u8 {
        0x80 | REVISION | if self.displaying() { 0 } else { 0x20 }
    }

    /// A byte of video RAM, for inspection.
    pub(crate) fn vram(&self) -> &[u8] {
        &self.vram
    }

    fn update_address(&self) -> u16 {
        u16::from(self.regs[18]) << 8 | u16::from(self.regs[19])
    }

    fn set_update_address(&mut self, address: u16) {
        self.regs[18] = (address >> 8) as u8;
        self.regs[19] = address as u8;
    }

    fn vram_read(&self, address: u16) -> u8 {
        self.vram[usize::from(address) & (VRAM_SIZE - 1)]
    }

    fn vram_write(&mut self, address: u16, value: u8) {
        self.vram[usize::from(address) & (VRAM_SIZE - 1)] = value;
    }

    /// R30 starts a block operation of R30 bytes (0 means 256) at the update
    /// address: a copy from the block start (R32/R33) when R24 bit 7 is set,
    /// otherwise a fill with R31. Both leave the addresses past the block.
    fn fill_or_copy(&mut self) {
        let length = if self.regs[30] == 0 {
            256
        } else {
            u16::from(self.regs[30])
        };
        let start = self.update_address();
        if self.regs[24] & 0x80 != 0 {
            let source = u16::from(self.regs[32]) << 8 | u16::from(self.regs[33]);
            for i in 0..length {
                let byte = self.vram_read(source.wrapping_add(i));
                self.vram_write(start.wrapping_add(i), byte);
            }
            let end = source.wrapping_add(length);
            self.regs[31] = self.vram_read(end.wrapping_sub(1));
            self.regs[32] = (end >> 8) as u8;
            self.regs[33] = end as u8;
        } else {
            for i in 0..length {
                self.vram_write(start.wrapping_add(i), self.regs[31]);
            }
        }
        self.set_update_address(start.wrapping_add(length));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(vdc: &mut Vdc, reg: u8, value: u8) {
        vdc.write(0xD600, reg);
        vdc.write(0xD601, value);
    }

    #[test]
    fn data_goes_through_the_update_address_and_advances_it() {
        let mut vdc = Vdc::new();
        set(&mut vdc, 18, 0x10);
        set(&mut vdc, 19, 0x00);
        set(&mut vdc, 31, 0x41);
        set(&mut vdc, 31, 0x42);
        assert_eq!(&vdc.vram()[0x1000..0x1002], &[0x41, 0x42]);
        set(&mut vdc, 19, 0x00);
        vdc.write(0xD600, 31);
        assert_eq!(vdc.read(0xD601), 0x41);
        assert_eq!(vdc.read(0xD601), 0x42);
    }

    #[test]
    fn r30_fills_and_copies_blocks() {
        let mut vdc = Vdc::new();
        set(&mut vdc, 18, 0x00);
        set(&mut vdc, 19, 0x00);
        set(&mut vdc, 31, 0x20); // writes one byte, then...
        set(&mut vdc, 30, 0x09); // ...nine more of it
        assert!(vdc.vram()[..10].iter().all(|&b| b == 0x20));
        assert_eq!(vdc.vram()[10], 0);
        set(&mut vdc, 24, 0x80); // copy mode
        set(&mut vdc, 32, 0x00);
        set(&mut vdc, 33, 0x00);
        set(&mut vdc, 18, 0x08);
        set(&mut vdc, 19, 0x00);
        set(&mut vdc, 30, 0x00); // 256 bytes
        assert!(vdc.vram()[0x800..0x80A].iter().all(|&b| b == 0x20));
        vdc.write(0xD600, 18);
        assert_eq!(
            vdc.read(0xD601),
            0x09,
            "the update address is past the block"
        );
    }

    #[test]
    fn the_status_register_is_ready_and_reports_blanking() {
        let mut vdc = Vdc::new();
        set(&mut vdc, 6, 25);
        set(&mut vdc, 9, 7);
        assert_eq!(vdc.read(0xD600), 0x82, "line 0 is displayed");
        vdc.line = 200;
        assert_eq!(vdc.read(0xD600), 0xA2, "below the 25 rows it blanks");
        vdc.write(0xD600, 5);
        assert_eq!(vdc.read(0xD601), 0xE0, "unimplemented bits read as 1");
    }

    /// The KERNAL's 80-column setup, near enough: 80 by 25 cells of 8 by 8
    /// within 8 by 16 bytes, screen at 0, attributes at $0800, characters at
    /// $2000, white on black, attributes on.
    fn kernal_like() -> Vdc {
        let mut vdc = Vdc::new();
        for (reg, value) in [
            (0, 126),
            (1, 80),
            (4, 38),
            (6, 25),
            (9, 7),
            (10, 0x20), // no cursor
            (12, 0),
            (13, 0),
            (20, 0x08),
            (21, 0),
            (22, 0x78),
            (23, 8),
            (25, 0x40),
            (26, 0xF0),
            (28, 0x20),
        ] {
            set(&mut vdc, reg, value);
        }
        vdc
    }

    fn render_frame(vdc: &mut Vdc) {
        let start = vdc.frame_counter;
        while vdc.frame_counter == start {
            vdc.tick();
        }
    }

    fn pixel(vdc: &Vdc, x: usize, y: usize) -> u32 {
        vdc.framebuffer()[(BORDER_TOP + y) * VDC_FB_WIDTH + BORDER_LEFT + x]
    }

    #[test]
    fn a_character_with_its_attribute_colour_is_drawn() {
        let mut vdc = kernal_like();
        // Character 1 is a diagonal line; put it at row 0, column 2 in red.
        for line in 0..8 {
            vdc.vram[0x2000 + 16 + line] = 0x80 >> line;
        }
        vdc.vram[2] = 1;
        vdc.vram[0x0802] = 0x08;
        render_frame(&mut vdc);
        assert_eq!(
            pixel(&vdc, 16, 0),
            PALETTE[8],
            "the first dot of column 2 is red"
        );
        assert_eq!(pixel(&vdc, 17, 1), PALETTE[8], "and the diagonal goes on");
        assert_eq!(
            pixel(&vdc, 17, 0),
            PALETTE[0],
            "around it is the background"
        );
        assert_eq!(
            vdc.framebuffer()[0],
            PALETTE[0],
            "the border is the background"
        );
    }

    #[test]
    fn reverse_and_whole_screen_reverse_invert_the_cell() {
        let mut vdc = kernal_like();
        vdc.vram[0x0800] = 0x0F | ATTR_REVERSE; // white, reversed, on a space
        render_frame(&mut vdc);
        assert_eq!(pixel(&vdc, 0, 0), PALETTE[15], "a reversed space is solid");
        assert_eq!(pixel(&vdc, 8, 0), PALETTE[0], "the next cell is not");
        set(&mut vdc, 24, 0x40);
        render_frame(&mut vdc);
        assert_eq!(
            pixel(&vdc, 0, 0),
            PALETTE[0],
            "reversing everything turns it back"
        );
    }

    #[test]
    fn a_solid_cursor_reverses_its_cell_between_its_start_and_end_lines() {
        let mut vdc = kernal_like();
        set(&mut vdc, 10, 0x02); // solid, from line 2
        set(&mut vdc, 11, 0x05); // to line 5
        set(&mut vdc, 14, 0x00);
        set(&mut vdc, 15, 0x51); // row 1, column 1
        vdc.vram[0x0851] = 0x0D; // light yellow
        render_frame(&mut vdc);
        assert_eq!(
            pixel(&vdc, 8, 8 + 1),
            PALETTE[0],
            "above the cursor's lines"
        );
        assert_eq!(
            pixel(&vdc, 8, 8 + 2),
            PALETTE[13],
            "the cursor's first line"
        );
        assert_eq!(pixel(&vdc, 15, 8 + 4), PALETTE[13]);
        assert_eq!(
            pixel(&vdc, 8, 8 + 5),
            PALETTE[0],
            "the end line is not included"
        );
    }

    #[test]
    fn a_frame_takes_the_time_its_registers_say() {
        let mut vdc = kernal_like();
        render_frame(&mut vdc);
        let mut ticks = 0u64;
        let start = vdc.frame_counter;
        while vdc.frame_counter == start {
            vdc.tick();
            ticks += 1;
        }
        // 39 rows of 8 lines, each 127 cells of 8 dots at 16 MHz, against
        // phi2: 312 lines of 63.5 us, about 19,500 cycles.
        let expected = 312 * 127 * 8 * PHI2_HZ / DOT_HZ;
        assert!(ticks.abs_diff(expected) <= 1, "{ticks} against {expected}");
    }
}
