//! The SNK LSPC2 video chip and its palette: the Neo Geo's display.
//!
//! The Neo Geo has no tilemap background. Everything moving is a sprite: 381
//! vertical strips, each one tile (16 pixels) wide and up to 32 tiles tall,
//! that chain side by side into larger objects and shrink in both
//! directions. Over them sits the fix layer, a 40×32 grid of 8×8 tiles for
//! text and status. Up to 96 sprites can appear on one line.
//!
//! This crate holds the LSPC2's video RAM and registers, the palette RAM the
//! NEO-B1 reads, the beam counters, and the two timing interrupts the LSPC2
//! raises: vertical blank and the programmable display-position timer.
//!
//! # Timing and granularity
//!
//! [`Lspc2::tick`] is one 24 MHz master clock. The pixel clock is a quarter
//! of that; a line is 384 pixels and a frame 264 lines, 320×224 of them
//! visible. The interrupts land on the master clock MAME places them on.
//!
//! The picture is built a line at a time. At the start of each line the
//! chip first finishes the line before, then parses the sprite attribute
//! table for the new line into one of two sprite lists in video RAM, as the
//! hardware does while the previous line is shown. Finishing a line draws
//! the sprites that list names, then the fix layer, from video RAM as it
//! stands at that moment. A write to video RAM during a line therefore shows
//! on that line if it lands before the line ends, as on the hardware, but
//! never partway across it. Neo Geo games change video RAM between lines,
//! not within them, so per-line drawing is the granularity they observe.
//!
//! # Graphics ROMs
//!
//! Sprite tiles come from the cartridge's C-ROMs, fix tiles from its S-ROM
//! or the BIOS's SFIX, and vertical shrink from the L0 ROM on the board.
//! [`Lspc2::tick`] borrows them through [`VideoRoms`]. The graphics buses
//! belong to the video hardware alone, so a borrowed slice answers every
//! fetch the way the bus would, without a round trip through the machine.
//!
//! The facts follow MAME's Neo Geo driver (`neogeo_spr.cpp`,
//! `neogeo_v.cpp`, `neogeo.cpp`; BSD-3-Clause).

pub mod palette;

use serde::{Deserialize, Serialize};

/// Master clocks per pixel.
pub const MASTER_PER_PIXEL: u32 = 4;
/// Pixels per line, including blanking.
pub const HTOTAL: u32 = 384;
/// Master clocks per line.
pub const MASTER_PER_LINE: u32 = HTOTAL * MASTER_PER_PIXEL;
/// Lines per frame, including blanking.
pub const VTOTAL: u32 = 264;
/// The first visible pixel of a line.
pub const HBEND: u32 = 0x1c;
/// The first visible line.
pub const VBEND: u32 = 0x10;
/// The first line of vertical blank.
pub const VBSTART: u32 = 0xf0;
/// The line vertical sync starts, which steps auto-animation.
pub const VSSTART: u32 = 0x100;
/// Visible width.
pub const WIDTH: usize = 320;
/// Visible height.
pub const HEIGHT: usize = (VBSTART - VBEND) as usize;

/// Sprites in the attribute table.
pub const SPRITES: u16 = 381;
/// Sprites one line can show.
pub const SPRITES_PER_LINE: usize = 96;

/// Master clocks into line `VBSTART` the vertical-blank interrupt rises.
const VBLANK_IRQ_CLOCK: u32 = 58;
/// Master clocks into line `VBSTART` the display-position timer reloads
/// when told to reload at vertical blank.
const VBLANK_RELOAD_CLOCK: u32 = 1146;

/// Video RAM words: 32K of slow VRAM and 2K of fast.
const VRAM_WORDS: usize = 0x8800;
/// Palette words: two banks of 4096.
const PALETTE_WORDS: usize = 0x2000;

/// Display-position interrupt control, register 3 bits 4-7.
const IRQ2_ENABLE: u16 = 1 << 4;
const IRQ2_LOAD_RELATIVE: u16 = 1 << 5;
const IRQ2_RELOAD_VBLANK: u16 = 1 << 6;
const IRQ2_RELOAD_REPEAT: u16 = 1 << 7;

/// Horizontal shrink: which of a tile's 16 pixels each of the 16 shrink
/// values shows, high bit first. Verified on hardware (MAME).
const ZOOM_X: [u16; 16] = [
    0x0080, 0x0880, 0x0888, 0x2888, 0x288a, 0x2a8a, 0x2aaa, 0xaaaa, 0xaaea, 0xbaea, 0xbaeb, 0xbbeb,
    0xbbef, 0xfbef, 0xfbff, 0xffff,
];

/// The graphics ROMs the chip fetches from, borrowed for one tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct VideoRoms<'a> {
    /// Sprite tiles: the C-ROM pairs byte-interleaved, odd ROM (C1, C3, …)
    /// in even bytes and even ROM in odd bytes. Each 16×16 tile is 128
    /// bytes: four bitplanes, two from each ROM of the pair.
    pub sprites: &'a [u8],
    /// Fix tiles: the cartridge S-ROM or the BIOS SFIX, whichever the board
    /// has selected. Each 8×8 tile is 32 bytes, 4 bits a pixel, in columns.
    pub fix: &'a [u8],
    /// The L0 vertical shrink table, at least 64K. Empty means a computed
    /// approximation; see [`Lspc2`].
    pub zoom: &'a [u8],
}

/// One LSPC2 with its palette RAM.
///
/// Without an L0 ROM, vertical shrink uses a computed table. For each
/// shrink value `z` it shows `z + 1` of a sprite's 256 lines, evenly spaced
/// and repeating after that; at full size, the common case, it matches the
/// ROM exactly. The ROM is in the BIOS set as `000-lo.lo`.
#[derive(Clone, Serialize, Deserialize)]
pub struct Lspc2 {
    vram: Vec<u16>,
    palette: Vec<u16>,
    /// The palette bank the 68000 and the display use, 0 or 1.
    pub palette_bank: bool,
    /// The board's shadow line: darkens every colour.
    pub shadow: bool,
    vram_address: u16,
    vram_read: u16,
    vram_modulo: u16,
    auto_animation_speed: u8,
    auto_animation_disabled: bool,
    auto_animation_counter: u8,
    auto_animation_frame: u8,
    irq2_control: u16,
    display_counter: u32,
    /// Master clocks until the display-position timer fires.
    irq2_remaining: Option<u64>,
    /// Pending interrupts: vertical blank (level 1), display position
    /// (level 2), and reset (level 3, pending from power-on).
    pub vblank_pending: bool,
    pub raster_pending: bool,
    pub reset_pending: bool,
    /// The current line, 0 to `VTOTAL - 1`.
    line: u32,
    /// Master clocks into the line, 0 to `MASTER_PER_LINE - 1`.
    line_clock: u32,
    /// The finished picture, ARGB, `WIDTH` × `HEIGHT`.
    #[serde(skip, default = "blank_frame")]
    pub framebuffer: Vec<u32>,
    #[serde(skip)]
    frame_complete: bool,
}

fn blank_frame() -> Vec<u32> {
    vec![0xff00_0000; WIDTH * HEIGHT]
}

impl Default for Lspc2 {
    fn default() -> Self {
        Self::new()
    }
}

impl Lspc2 {
    /// The chip at power-on, at the top of the frame.
    #[must_use]
    pub fn new() -> Self {
        Self {
            vram: vec![0; VRAM_WORDS],
            palette: vec![0; PALETTE_WORDS],
            palette_bank: false,
            shadow: false,
            vram_address: 0,
            vram_read: 0,
            vram_modulo: 0,
            auto_animation_speed: 0,
            auto_animation_disabled: false,
            auto_animation_counter: 0,
            auto_animation_frame: 0,
            irq2_control: 0,
            display_counter: 0,
            irq2_remaining: None,
            vblank_pending: false,
            raster_pending: false,
            reset_pending: true,
            line: 0,
            line_clock: 0,
            framebuffer: blank_frame(),
            frame_complete: false,
        }
    }

    /// Pull the reset line. Video and palette RAM keep their contents; the
    /// reset interrupt becomes pending.
    pub fn reset(&mut self) {
        self.irq2_control = 0;
        self.irq2_remaining = None;
        self.vblank_pending = false;
        self.raster_pending = false;
        self.reset_pending = true;
        self.line = 0;
        self.line_clock = 0;
    }

    /// One master clock.
    pub fn tick(&mut self, roms: &VideoRoms) {
        if self.line_clock == 0 {
            self.start_line(roms);
        }
        if self.line == VBSTART {
            if self.line_clock == VBLANK_IRQ_CLOCK {
                self.vblank_pending = true;
            }
            if self.line_clock == VBLANK_RELOAD_CLOCK && self.irq2_control & IRQ2_RELOAD_VBLANK != 0
            {
                self.load_irq2_timer();
            }
        }
        if let Some(remaining) = self.irq2_remaining {
            if remaining <= 1 {
                self.irq2_remaining = None;
                if self.irq2_control & IRQ2_ENABLE != 0 {
                    self.raster_pending = true;
                }
                if self.irq2_control & IRQ2_RELOAD_REPEAT != 0 {
                    self.load_irq2_timer();
                }
            } else {
                self.irq2_remaining = Some(remaining - 1);
            }
        }
        self.line_clock += 1;
        if self.line_clock == MASTER_PER_LINE {
            self.line_clock = 0;
            self.line = (self.line + 1) % VTOTAL;
        }
    }

    fn start_line(&mut self, roms: &VideoRoms) {
        if let Some(previous) = self.line.checked_sub(1)
            && (VBEND..VBSTART).contains(&previous)
        {
            self.draw_line(previous, roms);
            if previous == VBSTART - 1 {
                self.frame_complete = true;
            }
        }
        if self.line == VSSTART {
            if self.auto_animation_frame == 0 {
                self.auto_animation_frame = self.auto_animation_speed;
                self.auto_animation_counter = self.auto_animation_counter.wrapping_add(1);
            } else {
                self.auto_animation_frame -= 1;
            }
        }
        self.parse_sprites(self.line);
    }

    fn load_irq2_timer(&mut self) {
        self.irq2_remaining =
            Some((u64::from(self.display_counter) + 1) * u64::from(MASTER_PER_PIXEL));
    }

    /// Whether a frame finished since the last call.
    pub fn take_frame_complete(&mut self) -> bool {
        std::mem::take(&mut self.frame_complete)
    }

    /// The current line, 0 to `VTOTAL - 1`.
    #[must_use]
    pub fn line(&self) -> u32 {
        self.line
    }

    /// The current pixel within the line, 0 to `HTOTAL - 1`.
    #[must_use]
    pub fn pixel(&self) -> u32 {
        self.line_clock / MASTER_PER_PIXEL
    }

    /// The highest interrupt level pending, or 0.
    #[must_use]
    pub fn irq_level(&self) -> u8 {
        if self.reset_pending {
            3
        } else if self.raster_pending {
            2
        } else if self.vblank_pending {
            1
        } else {
            0
        }
    }

    /// Read register `reg` (the word offset within `$3C0000`, 0-7). Registers
    /// 0 and 1 return the VRAM word at the address, 2 the modulo, 3 the
    /// line counter and auto-animation counter. The rest read as register 0.
    #[must_use]
    pub fn read_register(&self, reg: u8) -> u16 {
        match reg & 7 {
            2 => self.vram_modulo,
            3 => {
                // The counter runs $F8 to $1FF; line 0 is $100.
                let mut counter = self.line + 0x100;
                if counter >= 0x200 {
                    counter -= VTOTAL;
                }
                ((counter as u16) << 7) | u16::from(self.auto_animation_counter & 7)
            }
            _ => self.vram_read,
        }
    }

    /// Write register `reg` (the word offset within `$3C0000`, 0-7).
    pub fn write_register(&mut self, reg: u8, data: u16) {
        match reg & 7 {
            0 => self.set_vram_address(data),
            1 => {
                self.vram[usize::from(self.vram_address)] = data;
                // The address steps by the modulo; bit 15 stays put.
                let next = (self.vram_address & 0x8000)
                    | (self.vram_address.wrapping_add(self.vram_modulo) & 0x7fff);
                self.set_vram_address(next);
            }
            2 => self.vram_modulo = data,
            3 => {
                self.auto_animation_speed = (data >> 8) as u8;
                self.auto_animation_disabled = data & 0x08 != 0;
                self.irq2_control = data & 0x00f0;
            }
            4 => {
                self.display_counter = (self.display_counter & 0xffff) | (u32::from(data) << 16);
            }
            5 => {
                self.display_counter = (self.display_counter & 0xffff_0000) | u32::from(data);
                if self.irq2_control & IRQ2_LOAD_RELATIVE != 0 {
                    self.load_irq2_timer();
                }
            }
            6 => {
                if data & 1 != 0 {
                    self.reset_pending = false;
                }
                if data & 2 != 0 {
                    self.raster_pending = false;
                }
                if data & 4 != 0 {
                    self.vblank_pending = false;
                }
            }
            // The PAL timer stop; this is the NTSC chip.
            _ => {}
        }
    }

    fn set_vram_address(&mut self, data: u16) {
        // Fast VRAM is 2K words; its addresses mirror.
        self.vram_address = if data & 0x8000 != 0 {
            data & 0x87ff
        } else {
            data
        };
        // The read is made at once.
        self.vram_read = self.vram[usize::from(self.vram_address)];
    }

    /// Read palette word `offset` (0-4095) in the current bank.
    #[must_use]
    pub fn read_palette(&self, offset: u16) -> u16 {
        self.palette[self.palette_index(offset)]
    }

    /// Write palette word `offset` (0-4095) in the current bank.
    pub fn write_palette(&mut self, offset: u16, data: u16) {
        let index = self.palette_index(offset);
        self.palette[index] = data;
    }

    fn palette_index(&self, offset: u16) -> usize {
        usize::from(self.palette_bank) * 0x1000 + usize::from(offset & 0x0fff)
    }

    /// Video RAM, by word address.
    #[must_use]
    pub fn vram(&self) -> &[u16] {
        &self.vram
    }

    /// Palette RAM, both banks.
    #[must_use]
    pub fn palette(&self) -> &[u16] {
        &self.palette
    }

    /// The colour of palette entry `index` (0-4095) in the current bank.
    fn colour(&self, index: usize) -> u32 {
        palette::argb(
            self.palette[usize::from(self.palette_bank) * 0x1000 + (index & 0x0fff)],
            self.shadow,
        )
    }

    fn sprite_list_base(line: u32) -> usize {
        if line & 1 != 0 { 0x8680 } else { 0x8600 }
    }

    /// Whether a sprite at `y` with `rows` tiles covers `line`.
    fn on_line(line: u32, y: u32, rows: u32) -> bool {
        rows == 0 || rows >= 0x20 || (line.wrapping_sub(y) & 0x1ff) < rows * 0x10
    }

    /// Fill the sprite list for `line` with the sprites that cover it, in
    /// attribute-table order, up to 96, followed by zeros.
    fn parse_sprites(&mut self, line: u32) {
        let base = Self::sprite_list_base(line);
        let mut count = 0;
        let mut y = 0;
        let mut rows = 0;
        for sprite in 0..SPRITES {
            let control = u32::from(self.vram[0x8200 | usize::from(sprite)]);
            // A chained sprite takes the position and height of the one
            // before it.
            if control & 0x40 == 0 {
                y = 0x200 - (control >> 7);
                rows = control & 0x3f;
            }
            if rows == 0 || !Self::on_line(line, y, rows) {
                continue;
            }
            self.vram[base + count] = sprite;
            count += 1;
            if count == SPRITES_PER_LINE {
                break;
            }
        }
        // Zero the rest, and one more.
        self.vram[base + count..=base + SPRITES_PER_LINE].fill(0);
    }

    /// The vertical shrink table entry: tile in the high nibble, line within
    /// the tile in the low.
    fn zoom_entry(zoom: &[u8], zoom_y: u32, zoom_line: u32) -> u8 {
        if let Some(&entry) = zoom.get(((zoom_y << 8) | zoom_line) as usize) {
            return entry;
        }
        let shown = zoom_y + 1;
        ((zoom_line % shown) * 256 / shown) as u8
    }

    fn draw_line(&mut self, line: u32, roms: &VideoRoms) {
        let mut buffer = [0u32; 0x200];
        buffer.fill(self.colour(0xfff));
        self.draw_sprites(line, roms, &mut buffer);
        self.draw_fix(line, roms.fix, &mut buffer);
        let row = (line - VBEND) as usize * WIDTH;
        self.framebuffer[row..row + WIDTH].copy_from_slice(&buffer[..WIDTH]);
    }

    fn draw_sprites(&self, line: u32, roms: &VideoRoms, buffer: &mut [u32; 0x200]) {
        let list = &self.vram[Self::sprite_list_base(line)..];
        // The list ends at its last non-zero entry; the hardware would draw
        // sprite 0 in every empty slot, but one zero after the last entry
        // stands for a real sprite 0.
        let last = match list[..SPRITES_PER_LINE].iter().rposition(|&e| e != 0) {
            Some(index) if index == SPRITES_PER_LINE - 1 => index,
            Some(index) => index + 1,
            None => 0,
        };

        let sprite_mask = rom_mask(roms.sprites.len() * 2);
        let (mut x, mut y, mut rows, mut zoom_x, mut zoom_y) = (0u32, 0u32, 0u32, 0u32, 0u32);
        for &entry in &list[..=last] {
            let sprite = usize::from(entry & 0x1ff);
            let control = u32::from(self.vram[0x8200 | sprite]);
            let shrink = u32::from(self.vram[0x8000 | sprite]);
            if control & 0x40 != 0 {
                // Chained: the next strip to the right, at its own width.
                x = (x + zoom_x + 1) & 0x1ff;
                zoom_x = (shrink >> 8) & 0x0f;
            } else {
                y = 0x200 - (control >> 7);
                x = u32::from(self.vram[0x8400 | sprite]) >> 7;
                zoom_y = shrink & 0xff;
                zoom_x = (shrink >> 8) & 0x0f;
                rows = control & 0x3f;
            }
            if (0x140..=0x1f0).contains(&x) || !Self::on_line(line, y, rows) {
                continue;
            }

            let sprite_line = line.wrapping_sub(y) & 0x1ff;
            let mut zoom_line = sprite_line & 0xff;
            let mut invert = sprite_line & 0x100 != 0;
            if invert {
                zoom_line ^= 0xff;
            }
            if rows > 0x20 {
                // Taller than 32 tiles: the shrunk sprite repeats, mirrored.
                let period = (zoom_y + 1) << 1;
                zoom_line %= period;
                if zoom_line > zoom_y {
                    zoom_line = period - 1 - zoom_line;
                    invert = !invert;
                }
            }
            let entry = Self::zoom_entry(roms.zoom, zoom_y, zoom_line);
            let mut tile_line = u32::from(entry & 0x0f);
            let mut tile = u32::from(entry >> 4);
            if invert {
                tile_line ^= 0x0f;
                tile ^= 0x1f;
            }

            let map = (sprite << 6) | (tile as usize) << 1;
            let attr = u32::from(self.vram[map + 1]);
            let mut code = ((attr << 12) & 0xf_0000) | u32::from(self.vram[map]);
            if !self.auto_animation_disabled {
                let counter = u32::from(self.auto_animation_counter);
                if attr & 0x08 != 0 {
                    code = (code & !7) | (counter & 7);
                } else if attr & 0x04 != 0 {
                    code = (code & !3) | (counter & 3);
                }
            }
            if attr & 0x02 != 0 {
                tile_line ^= 0x0f;
            }
            let pens = ((attr >> 8) << 4) as usize;
            let flip = attr & 0x01 != 0;
            let mut address = ((code << 8) | (tile_line << 4)) & sprite_mask;
            if flip {
                address += 0x0f;
            }
            let mut shown = ZOOM_X[zoom_x as usize];
            let mut px = x;
            for _ in 0..16 {
                if shown & 0x8000 != 0 {
                    let pixel = sprite_pixel(roms.sprites, address);
                    if pixel != 0 {
                        buffer[(px & 0x1ff) as usize] = self.colour(pens | usize::from(pixel));
                    }
                    px += 1;
                }
                shown <<= 1;
                if shown == 0 {
                    break;
                }
                address = if flip {
                    address.wrapping_sub(1)
                } else {
                    address + 1
                };
            }
        }
    }

    fn draw_fix(&self, line: u32, fix: &[u8], buffer: &mut [u32; 0x200]) {
        let mask = rom_mask(fix.len());
        for column in 0..40 {
            let word = self.vram[0x7000 | (column << 5) | (line as usize >> 3)];
            let code = u32::from(word & 0x0fff);
            let pens = usize::from(word >> 12) << 4;
            let base = ((code << 5) | (line & 7)) & mask;
            // Each byte holds two pixels, left in the low nibble; the four
            // byte columns of a tile sit 8 bytes apart, from the middle.
            for (pair, offset) in [0x10, 0x18, 0x00, 0x08].into_iter().enumerate() {
                let byte = fix.get((base + offset) as usize).copied().unwrap_or(0);
                for (half, pixel) in [byte & 0x0f, byte >> 4].into_iter().enumerate() {
                    if pixel != 0 {
                        buffer[column * 8 + pair * 2 + half] =
                            self.colour(pens | usize::from(pixel));
                    }
                }
            }
        }
    }
}

/// The address mask for a ROM of `len` units: the smallest all-ones value
/// that covers it.
fn rom_mask(len: usize) -> u32 {
    if len == 0 {
        return 0;
    }
    (len as u32).next_power_of_two() - 1
}

/// One sprite pixel. `address` counts pixels: tile code, line and column.
/// Each 8-pixel half-line is four bytes, one bitplane each, and a tile's
/// right half comes before its left in the ROM.
fn sprite_pixel(rom: &[u8], address: u32) -> u8 {
    let offset =
        ((address & !0xff) >> 1) | (((address & 0x8) ^ 0x8) << 3) | ((address & 0xf0) >> 2);
    let bit = address & 7;
    let plane = |n: u32| -> u8 {
        rom.get((offset + n) as usize)
            .map_or(0, |byte| (byte >> bit) & 1)
    };
    (plane(3) << 3) | (plane(1) << 2) | (plane(2) << 1) | plane(0)
}

#[cfg(test)]
mod tests;
