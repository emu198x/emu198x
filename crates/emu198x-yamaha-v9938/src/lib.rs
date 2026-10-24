//! Yamaha V9938 Video Display Processor, the MSX2's video chip.
//!
//! The V9938 is the TMS9918's successor. It keeps the four TMS9918 screen
//! modes and adds six of its own, a 128 KB VRAM, a 512-colour palette,
//! sprite mode 2, a line interrupt, vertical scrolling and a command engine
//! that draws into VRAM while the CPU does something else.
//!
//! # The CPU's view
//!
//! | Port | Read                         | Write                                  |
//! |------|------------------------------|----------------------------------------|
//! | `#98`| VRAM data (read-ahead latch) | VRAM data                              |
//! | `#99`| status register `S#(R#15)`   | address set-up or register write      |
//! | `#9A`| —                            | palette data, two bytes per entry      |
//! | `#9B`| —                            | indirect register write via `R#17`     |
//!
//! A VRAM address is the 14-bit pointer the control port sets plus three
//! high bits in `R#14`. In the V9938's own modes the pointer carries into
//! `R#14`, so the CPU walks all 128 KB; in the TMS9918 modes it wraps at
//! 16 KB, as it does on the older chip.
//!
//! # Screen modes
//!
//! | Mode    | MSX BASIC | Resolution   | Colours           |
//! |---------|-----------|--------------|-------------------|
//! | G1      | SCREEN 1  | 32×24 tiles  | 2 per 8 tiles     |
//! | G2      | SCREEN 2  | 32×24 tiles  | 2 per tile row    |
//! | MC      | SCREEN 3  | 64×48 blocks | 16                |
//! | T1      | SCREEN 0  | 40×24 text   | 2                 |
//! | T2      | SCREEN 0  | 80×24 text   | 2, blink          |
//! | G3      | SCREEN 4  | G2 + sprite mode 2                ||
//! | G4      | SCREEN 5  | 256×212      | 16 of 512         |
//! | G5      | SCREEN 6  | 512×212      | 4 of 512          |
//! | G6      | SCREEN 7  | 512×212      | 16 of 512         |
//! | G7      | SCREEN 8  | 256×212      | 256 fixed (GRB332)|
//!
//! G6 and G7 use 256 bytes a line and store them *planar*: even logical
//! addresses in the first 64 KB bank and odd ones in the second, so the
//! chip can fetch two bytes in one memory cycle. This crate keeps VRAM in
//! physical order and maps logical addresses through [`V9938::planar`]
//! wherever the chip does: CPU access in those modes, the renderer, and the
//! command engine.
//!
//! # Timing
//!
//! [`V9938::tick`] is one dot, four of the 21.48 MHz master clocks — the
//! same 5.37 MHz dot clock and 342-dot line as the TMS9918, so an MSX2
//! machine steps it exactly as it stepped the older chip. A frame is 262
//! lines, or 313 when `R#9` bit 1 selects PAL.
//!
//! **Rendering granularity.** The background is drawn per dot: each active
//! pixel (two in the 512-wide modes) is fetched at the dot it is scanned
//! out, from the registers and VRAM live at that moment, so a mid-line
//! write splits the line where it lands. Sprites are evaluated once per
//! line at dot 0, as on the TMS9918, because the chip fetches a line's
//! sprites in the previous line's horizontal blank. The command engine runs
//! per dot too.
//!
//! **What is simplified**, each documented where it lives:
//!
//! - CPU VRAM accesses are performed at once, not in the access slots the
//!   display leaves free (the TMS9918 crate models those slots; the V9938's
//!   are far more numerous and are not yet modelled here).
//! - The command engine charges each operation openMSX's per-operation
//!   delay but does not wait for access slots, so during active display it
//!   finishes sooner than the chip.
//! - Interlace and even/odd page flipping (`R#9` bits 2-3) are not rendered;
//!   `S#2`'s EO bit does toggle each frame.
//! - `R#18`'s display adjust is ignored.
//! - Colour is a linear map of each 3-bit level to 8 bits.
//!
//! # Provenance
//!
//! Register masks, the planar mapping, the default palette, the sprite
//! palette of Graphic 7 and the command delays are facts taken from
//! openMSX (GPL-2.0, read for facts only, not code), at commit
//! `38af174e2785b490de07bb0ff891e950e64a3bcd`. The behaviour follows the
//! Yamaha V9938 Technical Data Book as openMSX documents it.

#![allow(clippy::cast_possible_truncation)]

mod command;
mod render;
mod sprites;
#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

use command::CommandEngine;

/// VRAM size: 128 KB.
pub const VRAM_SIZE: usize = 128 * 1024;

/// Dots in one scan line.
pub const DOTS_PER_LINE: u16 = 342;

/// Active pixels across a line in the 256-wide modes.
pub const ACTIVE_WIDTH: u32 = 256;

/// Register value masks, `R#0`-`R#23` then `R#32`-`R#46`. Bits a register
/// does not have read back as zero. From openMSX's `VALUE_MASKS_MSX2` and
/// its command register masks.
const REGISTER_MASKS: [u8; 64] = [
    0x7E, 0x7F, 0x7F, 0xFF, 0x3F, 0xFF, 0x3F, 0xFF, // 0-7
    0xFB, 0xBF, 0x07, 0x03, 0xFF, 0xFF, 0x07, 0x0F, // 8-15
    0x0F, 0xBF, 0xFF, 0xFF, 0x3F, 0x3F, 0x3F, 0xFF, // 16-23
    0, 0, 0, 0, 0, 0, 0, 0, // 24-31: not on the V9938
    0xFF, 0x01, 0xFF, 0x03, 0xFF, 0x01, 0xFF, 0x03, // 32-39: SX, SY, DX, DY
    0xFF, 0x03, 0xFF, 0x03, 0xFF, 0x7F, 0xFF, 0, // 40-46: NX, NY, CLR, ARG, CMD
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 48-63
];

/// The palette after reset, as `0x0GRB` with three bits per gun.
const DEFAULT_PALETTE: [u16; 16] = [
    0x000, 0x000, 0x611, 0x733, 0x117, 0x327, 0x151, 0x627, 0x171, 0x373, 0x661, 0x664, 0x411,
    0x265, 0x555, 0x777,
];

/// The video standard the framebuffer is sized for: which lines a set shows.
///
/// The chip's own frame length follows `R#9` bit 1, which the BIOS sets to
/// match; a mismatched setting still runs, and draws what fits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VdpRegion {
    /// 240 visible lines.
    Ntsc,
    /// 288 visible lines.
    Pal,
}

impl VdpRegion {
    /// Lines the framebuffer holds.
    #[must_use]
    pub const fn framebuffer_height(self) -> u32 {
        match self {
            Self::Ntsc => 240,
            Self::Pal => 288,
        }
    }

    /// Pixels the framebuffer holds across a line: twice the TMS9918
    /// crate's window, so the 512-wide modes keep every pixel.
    #[must_use]
    pub const fn framebuffer_width(self) -> u32 {
        match self {
            Self::Ntsc => 560,
            Self::Pal => 556,
        }
    }

    /// Border lines above a 192-line picture, matching the TMS9918 crate.
    const fn border_top_192(self) -> u32 {
        match self {
            Self::Ntsc => 25,
            Self::Pal => 51,
        }
    }

    /// Framebuffer pixels left of the active area (512-wide units).
    #[must_use]
    pub const fn border_left(self) -> u32 {
        (self.framebuffer_width() - 2 * ACTIVE_WIDTH) / 2
    }
}

/// The screen mode, decoded from `M1`-`M5`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    G1,
    G2,
    Mc,
    T1,
    T2,
    G3,
    G4,
    G5,
    G6,
    G7,
    /// A combination the chip does not define: the backdrop is shown.
    Undefined,
}

impl Mode {
    /// Whether a line is 512 pixels wide.
    #[must_use]
    pub const fn is_hires(self) -> bool {
        matches!(self, Self::T2 | Self::G5 | Self::G6)
    }

    /// Whether the mode is a bitmap mode, the only ones the command
    /// engine's pixel addressing is defined for.
    #[must_use]
    pub const fn is_bitmap(self) -> bool {
        matches!(self, Self::G4 | Self::G5 | Self::G6 | Self::G7)
    }

    /// Whether VRAM is stored planar (G6, G7).
    #[must_use]
    pub const fn is_planar(self) -> bool {
        matches!(self, Self::G6 | Self::G7)
    }

    /// Whether sprites use mode 2 (G3-G7) rather than mode 1.
    #[must_use]
    pub const fn sprite_mode_2(self) -> bool {
        matches!(self, Self::G3 | Self::G4 | Self::G5 | Self::G6 | Self::G7)
    }

    /// Whether the mode is one of the V9938's own, where the VRAM pointer
    /// carries into `R#14`.
    #[must_use]
    pub const fn is_v9938_mode(self) -> bool {
        matches!(
            self,
            Self::T2 | Self::G3 | Self::G4 | Self::G5 | Self::G6 | Self::G7
        )
    }
}

/// Yamaha V9938.
#[derive(Serialize, Deserialize)]
pub struct V9938 {
    vram: Vec<u8>,
    /// `R#0`-`R#63`; only those in [`REGISTER_MASKS`] exist.
    regs: Vec<u8>,
    /// `S#0` bits F, 5S, C and the fifth/ninth sprite number.
    status0: u8,
    /// `S#1` FH: the line interrupt flag.
    line_flag: bool,
    /// Sprite collision coordinates for `S#3`-`S#6`.
    collision_x: u16,
    collision_y: u16,
    /// `S#2` EO: toggles each field.
    even_odd: bool,
    /// Palette, `0x0GRB`.
    palette: [u16; 16],

    /// The byte held for the second write to `#99` or `#9A`.
    data_latch: u8,
    control_pending: bool,
    palette_pending: bool,
    /// The low 14 bits of the VRAM address.
    vram_pointer: u16,
    /// The read-ahead byte `#98` returns.
    read_ahead: u8,

    region: VdpRegion,
    line: u16,
    dot: u16,
    framebuffer: Vec<u32>,
    /// The line's sprite pixels, 256 wide: bit 7 set where a sprite shows,
    /// colour in the low four bits.
    sprite_line: Vec<u8>,

    /// Text 2 blink: whether the alternate colours show, and frames left.
    blink_on: bool,
    blink_frames: u16,

    command: CommandEngine,

    /// The INT output, active while a pending interrupt is enabled.
    pub interrupt: bool,
    /// Frames completed.
    pub frame_count: u64,
}

impl V9938 {
    /// A V9938 after reset, with a framebuffer sized for `region`.
    #[must_use]
    pub fn new(region: VdpRegion) -> Self {
        let mut regs = vec![0u8; 64];
        if region == VdpRegion::Pal {
            regs[9] = 0x02;
        }
        Self {
            vram: vec![0; VRAM_SIZE],
            regs,
            status0: 0,
            line_flag: false,
            collision_x: 0,
            collision_y: 0,
            even_odd: false,
            palette: DEFAULT_PALETTE,
            data_latch: 0,
            control_pending: false,
            palette_pending: false,
            vram_pointer: 0,
            read_ahead: 0,
            region,
            line: 0,
            dot: 0,
            framebuffer: vec![
                0;
                (region.framebuffer_width() * region.framebuffer_height()) as usize
            ],
            sprite_line: vec![0; 256],
            blink_on: false,
            blink_frames: 0,
            command: CommandEngine::default(),
            interrupt: false,
            frame_count: 0,
        }
    }

    // -----------------------------------------------------------------
    // Ports
    // -----------------------------------------------------------------

    /// `#98` read: the read-ahead byte, then fetch the next.
    pub fn read_data(&mut self) -> u8 {
        self.control_pending = false;
        let value = self.read_ahead;
        self.read_ahead = self.vram[self.cpu_address()];
        self.advance_pointer();
        value
    }

    /// `#98` write.
    pub fn write_data(&mut self, value: u8) {
        self.control_pending = false;
        let address = self.cpu_address();
        self.vram[address] = value;
        // A read straight after a write returns the byte written: one latch
        // serves both directions.
        self.read_ahead = value;
        self.advance_pointer();
    }

    /// `#99` read: the status register `R#15` selects, clearing what that
    /// read clears.
    pub fn read_status(&mut self) -> u8 {
        self.control_pending = false;
        let selected = self.regs[15];
        let value = self.peek_status(selected);
        match selected {
            0 => {
                self.status0 &= 0x1F;
                self.update_interrupt();
            }
            1 => {
                if self.regs[0] & 0x10 != 0 {
                    self.line_flag = false;
                    self.update_interrupt();
                }
            }
            5 => {
                self.collision_x = 0;
                self.collision_y = 0;
            }
            7 => self.command_colour_read(),
            9 => self.command.border_found = false,
            _ => {}
        }
        value
    }

    /// Status register `n` without the side effects of reading it.
    #[must_use]
    pub fn peek_status(&self, n: u8) -> u8 {
        match n {
            0 => self.status0,
            1 => u8::from(self.line_flag),
            2 => {
                let tr = if self.command.transfer_ready { 0x80 } else { 0 };
                let vr = if self.in_vertical_blank() { 0x40 } else { 0 };
                let hr = if self.dot >= ACTIVE_WIDTH as u16 {
                    0x20
                } else {
                    0
                };
                let bd = if self.command.border_found { 0x10 } else { 0 };
                let eo = if self.even_odd { 0x02 } else { 0 };
                let ce = u8::from(self.command.active());
                tr | vr | hr | bd | 0x0C | eo | ce
            }
            3 => self.collision_x as u8,
            4 => (self.collision_x >> 8) as u8 | 0xFE,
            5 => self.collision_y as u8,
            6 => (self.collision_y >> 8) as u8 | 0xFC,
            7 => self.command.colour,
            8 => self.command.border_x as u8,
            9 => (self.command.border_x >> 8) as u8 | 0xFE,
            _ => 0xFF,
        }
    }

    /// `#99` write: the first byte is latched; the second either sets up a
    /// VRAM access (bit 7 clear) or writes a register (bit 7 set).
    pub fn write_control(&mut self, value: u8) {
        if !self.control_pending {
            self.data_latch = value;
            self.control_pending = true;
            return;
        }
        self.control_pending = false;
        if value & 0x80 != 0 {
            // Bit 6 set too is not a register write on the V9938.
            if value & 0x40 == 0 {
                self.write_register(value & 0x3F, self.data_latch);
            }
        } else {
            self.vram_pointer = (u16::from(value & 0x3F) << 8) | u16::from(self.data_latch);
            if value & 0x40 == 0 {
                // Read set-up fetches ahead.
                self.read_ahead = self.vram[self.cpu_address()];
                self.advance_pointer();
            }
        }
    }

    /// `#9A` write: two bytes per entry, `0RRR0BBB` then `00000GGG`; the
    /// index in `R#16` advances after each pair.
    pub fn write_palette(&mut self, value: u8) {
        if !self.palette_pending {
            self.data_latch = value;
            self.palette_pending = true;
            return;
        }
        self.palette_pending = false;
        let index = usize::from(self.regs[16] & 0x0F);
        self.palette[index] = ((u16::from(value) << 8) | u16::from(self.data_latch)) & 0x777;
        self.regs[16] = (self.regs[16] + 1) & 0x0F;
    }

    /// `#9B` write: the register `R#17` names, which then advances unless
    /// its bit 7 inhibits it.
    pub fn write_indirect(&mut self, value: u8) {
        let target = self.regs[17];
        self.write_register(target & 0x3F, value);
        if target & 0x80 == 0 {
            self.regs[17] = (target + 1) & 0x3F;
        }
    }

    /// Write register `n`, masked to the bits it has.
    pub fn write_register(&mut self, n: u8, value: u8) {
        let n = usize::from(n & 0x3F);
        let value = value & REGISTER_MASKS[n];
        if REGISTER_MASKS[n] == 0 {
            return;
        }
        self.regs[n] = value;
        match n {
            0 | 1 => self.update_interrupt(),
            13 => {
                // Writing R#13 restarts blinking in its ON phase.
                self.blink_on = value & 0xF0 != 0;
                self.blink_frames = self.blink_period();
            }
            44 => self.command_colour_written(),
            46 => self.start_command(value),
            _ => {}
        }
    }

    // -----------------------------------------------------------------
    // Accessors
    // -----------------------------------------------------------------

    /// Register `n`.
    #[must_use]
    pub fn register(&self, n: u8) -> u8 {
        self.regs[usize::from(n & 0x3F)]
    }

    /// VRAM in physical order.
    #[must_use]
    pub fn vram(&self) -> &[u8] {
        &self.vram
    }

    /// Write VRAM directly at a physical address, bypassing the ports.
    pub fn poke_vram(&mut self, address: usize, value: u8) {
        self.vram[address % VRAM_SIZE] = value;
    }

    /// The palette, `0x0GRB` with three bits per gun.
    #[must_use]
    pub fn palette(&self) -> &[u16; 16] {
        &self.palette
    }

    /// The current screen mode.
    #[must_use]
    pub fn mode(&self) -> Mode {
        let r0 = self.regs[0];
        let r1 = self.regs[1];
        let bits = ((r0 & 0x0E) << 1) | ((r1 >> 4) & 1) | (((r1 >> 3) & 1) << 1);
        match bits {
            0x00 => Mode::G1,
            0x01 => Mode::T1,
            0x02 => Mode::Mc,
            0x04 => Mode::G2,
            0x08 => Mode::G3,
            0x09 => Mode::T2,
            0x0C => Mode::G4,
            0x10 => Mode::G5,
            0x14 => Mode::G6,
            0x1C => Mode::G7,
            _ => Mode::Undefined,
        }
    }

    /// The framebuffer, ARGB32.
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    /// Framebuffer width in pixels.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.region.framebuffer_width()
    }

    /// Framebuffer height in pixels.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.region.framebuffer_height()
    }

    /// The scan line being drawn, 0 being the first active line.
    #[must_use]
    pub fn line(&self) -> u16 {
        self.line
    }

    /// The dot within the line.
    #[must_use]
    pub fn dot(&self) -> u16 {
        self.dot
    }

    /// Lines in a frame: 262, or 313 when `R#9` selects PAL.
    #[must_use]
    pub fn lines_per_frame(&self) -> u16 {
        if self.regs[9] & 0x02 != 0 { 313 } else { 262 }
    }

    /// Active lines: 192, or 212 when `R#9` bit 7 is set.
    #[must_use]
    pub fn active_lines(&self) -> u16 {
        if self.regs[9] & 0x80 != 0 { 212 } else { 192 }
    }

    /// Whether the command engine is running (`S#2` CE).
    #[must_use]
    pub fn command_busy(&self) -> bool {
        self.command.active()
    }

    // -----------------------------------------------------------------
    // Timing
    // -----------------------------------------------------------------

    /// Advance one dot. Returns true when a frame completes.
    pub fn tick(&mut self) -> bool {
        let active_lines = self.active_lines();
        if self.dot == 0 {
            self.paint_border_for_line();
            if self.line < active_lines {
                self.prepare_line_sprites();
            }
        }
        if self.line < active_lines && self.dot < ACTIVE_WIDTH as u16 {
            self.render_dot();
        }

        self.step_command();

        // The line interrupt: FH rises at the start of the right border of
        // the line whose scrolled number matches R#19.
        if self.dot == ACTIVE_WIDTH as u16 && self.line < active_lines {
            let scrolled = (self.line + u16::from(self.regs[23])) & 0xFF;
            if scrolled == u16::from(self.regs[19]) {
                self.line_flag = true;
                self.update_interrupt();
            }
        }

        self.dot += 1;
        if self.dot >= DOTS_PER_LINE {
            self.dot = 0;
            if self.regs[0] & 0x10 == 0 {
                // With the line interrupt disabled FH falls at the next line.
                self.line_flag = false;
            }
            self.line += 1;
            if self.line == active_lines {
                self.status0 |= 0x80;
                self.update_interrupt();
                self.frame_count += 1;
            }
            if self.line >= self.lines_per_frame() {
                self.line = 0;
                self.end_frame();
                return true;
            }
        }
        false
    }

    /// Run a whole frame of dots.
    pub fn run_frame(&mut self) {
        while !self.tick() {}
    }

    fn end_frame(&mut self) {
        self.even_odd = !self.even_odd;
        if self.blink_frames > 0 {
            self.blink_frames -= 1;
        }
        if self.blink_frames == 0 {
            let on = self.regs[13] >> 4;
            let off = self.regs[13] & 0x0F;
            // A zero period holds the other state.
            if self.blink_on && off != 0 {
                self.blink_on = false;
            } else if !self.blink_on && on != 0 {
                self.blink_on = true;
            }
            self.blink_frames = self.blink_period();
        }
    }

    /// Frames in the current blink phase: each step of `R#13` is ten frames.
    fn blink_period(&self) -> u16 {
        let steps = if self.blink_on {
            self.regs[13] >> 4
        } else {
            self.regs[13] & 0x0F
        };
        u16::from(steps.max(1)) * 10
    }

    fn in_vertical_blank(&self) -> bool {
        self.line >= self.active_lines()
    }

    fn update_interrupt(&mut self) {
        let frame = self.status0 & 0x80 != 0 && self.regs[1] & 0x20 != 0;
        let line = self.line_flag && self.regs[0] & 0x10 != 0;
        self.interrupt = frame || line;
    }

    // -----------------------------------------------------------------
    // Addressing
    // -----------------------------------------------------------------

    /// The physical address of the CPU's current VRAM access.
    fn cpu_address(&self) -> usize {
        let logical = (u32::from(self.regs[14]) << 14) | u32::from(self.vram_pointer);
        if self.mode().is_planar() {
            Self::planar(logical)
        } else {
            logical as usize
        }
    }

    fn advance_pointer(&mut self) {
        self.vram_pointer = (self.vram_pointer + 1) & 0x3FFF;
        if self.vram_pointer == 0 && self.mode().is_v9938_mode() {
            self.regs[14] = (self.regs[14] + 1) & 0x07;
        }
    }

    /// The physical address of logical address `logical` in the planar
    /// modes: even addresses in the first 64 KB, odd ones in the second.
    #[must_use]
    pub const fn planar(logical: u32) -> usize {
        (((logical << 16) | (logical >> 1)) & 0x1FFFF) as usize
    }

    /// A colour as ARGB32 from a `0x0GRB` value.
    #[must_use]
    pub const fn grb_to_argb(grb: u16) -> u32 {
        let g = ((grb >> 8) & 7) as u32;
        let r = ((grb >> 4) & 7) as u32;
        let b = (grb & 7) as u32;
        0xFF00_0000 | (level(r) << 16) | (level(g) << 8) | level(b)
    }
}

/// A 3-bit gun level as 8 bits.
const fn level(v: u32) -> u32 {
    v * 255 / 7
}
