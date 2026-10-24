//! The command engine: block moves, fills, lines and searches in VRAM,
//! run by the VDP while the CPU does something else.
//!
//! A command starts when `R#46` is written: its high nibble is the
//! operation, its low nibble the logical operation for the pixel commands.
//! `R#32`-`R#45` hold the source and destination coordinates, the size, the
//! colour and the direction bits. `S#2` CE is set while it runs; TR handles
//! the CPU transfers of HMMC, LMMC and LMCM through `R#44` and `S#7`.
//!
//! | `CMD` | Name  | Does                                         |
//! |-------|-------|----------------------------------------------|
//! | `0`   | STOP  | abort                                         |
//! | `4`   | POINT | read a pixel into `S#7`                       |
//! | `5`   | PSET  | draw a pixel                                  |
//! | `6`   | SRCH  | find a colour along a line, into `S#8`/`S#9`  |
//! | `7`   | LINE  | draw a line                                   |
//! | `8`   | LMMV  | fill a rectangle, pixel by pixel              |
//! | `9`   | LMMM  | copy a rectangle, pixel by pixel              |
//! | `A`   | LMCM  | rectangle to the CPU, pixel by pixel          |
//! | `B`   | LMMC  | CPU to a rectangle, pixel by pixel            |
//! | `C`   | HMMV  | fill a rectangle, byte by byte                |
//! | `D`   | HMMM  | copy a rectangle, byte by byte                |
//! | `E`   | YMMM  | copy rows to the screen edge, byte by byte    |
//! | `F`   | HMMC  | CPU to a rectangle, byte by byte              |
//!
//! Coordinates are in pixels of the current bitmap mode; the byte commands
//! ignore the low bits that address within a byte. Outside the bitmap modes
//! VRAM is treated as 256 bytes a line, one pixel a byte, as openMSX does.
//!
//! **Timing is approximate.** Each operation is charged openMSX's delay for
//! it, in master clocks (`CMD_START_*` to begin and the per-access deltas
//! after), but the engine does not wait for the access slots the display
//! leaves, which depend on the mode, the sprites and blanking. During
//! active display the chip is therefore slower than this model; in
//! blanking the two agree to within those deltas.

use serde::{Deserialize, Serialize};

use crate::{Mode, V9938};

const STOP: u8 = 0x0;
const POINT: u8 = 0x4;
const PSET: u8 = 0x5;
const SRCH: u8 = 0x6;
const LINE: u8 = 0x7;
const LMMV: u8 = 0x8;
const LMMM: u8 = 0x9;
const LMCM: u8 = 0xA;
const LMMC: u8 = 0xB;
const HMMV: u8 = 0xC;
const HMMM: u8 = 0xD;
const YMMM: u8 = 0xE;
const HMMC: u8 = 0xF;

/// Master clocks to begin each operation (openMSX `CMD_START_*`).
fn start_cost(op: u8) -> i32 {
    match op {
        POINT => 63,
        PSET | SRCH | LMMV | LMMC => 88,
        LMMM => 64,
        LMCM => 76,
        LINE | HMMV | HMMC => 112,
        HMMM | YMMM => 100,
        _ => 0,
    }
}

/// Master clocks for one unit of each operation: a pixel or a byte.
fn unit_cost(op: u8) -> i32 {
    match op {
        SRCH => 88,
        LINE => 24 + 84,
        LMMV | LMMC => 24 + 72,
        LMMM => 32 + 24 + 60,
        LMCM => 24 + 36,
        HMMV | HMMC => 46,
        HMMM => 24 + 60,
        YMMM => 24 + 36,
        _ => 24,
    }
}

/// Extra master clocks at the end of each line of a rectangle.
fn line_end_cost(op: u8) -> i32 {
    match op {
        LMMV | HMMV => 58,
        LMMM | HMMM | YMMM => 68,
        _ => 0,
    }
}

/// The command engine's state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct CommandEngine {
    /// The running operation, or [`STOP`] when idle.
    op: u8,
    lop: u8,
    /// Source and destination of the current unit.
    sx: u16,
    sy: u16,
    dx: u16,
    dy: u16,
    /// The size as programmed.
    nx: u16,
    ny: u16,
    /// Units left on the current line, and lines left.
    line_left: u16,
    lines_left: u16,
    /// Where each line starts.
    sx_start: u16,
    dx_start: u16,
    /// LINE's error term and its count of pixels drawn.
    error: u16,
    count: u16,
    /// Master clocks owed (negative) before the next unit may run.
    budget: i32,
    /// A byte or pixel the CPU has written through `R#44`, not yet stored.
    cpu_data: Option<u8>,
    /// `S#2` TR.
    pub(crate) transfer_ready: bool,
    /// `S#2` BD: SRCH found its colour.
    pub(crate) border_found: bool,
    /// `S#8`/`S#9`: where SRCH stopped.
    pub(crate) border_x: u16,
    /// `S#7`: POINT and LMCM results.
    pub(crate) colour: u8,
}

impl CommandEngine {
    pub(crate) fn active(&self) -> bool {
        self.op != STOP
    }
}

/// Pixel addressing for the current mode.
#[derive(Debug, Clone, Copy)]
struct Geometry {
    width: u16,
    pixels_per_byte: u16,
    bytes_per_line: u32,
    line_mask: u16,
    planar: bool,
    bits: u8,
}

impl Geometry {
    fn of(mode: Mode) -> Self {
        let (width, pixels_per_byte, bytes_per_line, line_mask, planar, bits) = match mode {
            Mode::G4 => (256, 2, 128, 1023, false, 4),
            Mode::G5 => (512, 4, 128, 1023, false, 2),
            Mode::G6 => (512, 2, 256, 511, true, 4),
            Mode::G7 => (256, 1, 256, 511, true, 8),
            _ => (256, 1, 256, 511, false, 8),
        };
        Self {
            width,
            pixels_per_byte,
            bytes_per_line,
            line_mask,
            planar,
            bits,
        }
    }

    fn mask(self) -> u8 {
        if self.bits == 8 {
            0xFF
        } else {
            (1u8 << self.bits) - 1
        }
    }

    fn address(self, x: u16, y: u16) -> usize {
        let logical = u32::from(y & self.line_mask) * self.bytes_per_line
            + u32::from((x & (self.width - 1)) / self.pixels_per_byte);
        if self.planar {
            V9938::planar(logical)
        } else {
            (logical as usize) & 0x1FFFF
        }
    }

    fn shift(self, x: u16) -> u8 {
        let within = (x % self.pixels_per_byte) as u8;
        (self.pixels_per_byte as u8 - 1 - within) * self.bits
    }
}

/// Apply logical operation `lop` to a source and destination colour.
fn logic(lop: u8, src: u8, dst: u8, mask: u8) -> u8 {
    if lop & 0x08 != 0 && src == 0 {
        // The T variants leave the destination where the source is 0.
        return dst;
    }
    match lop & 0x07 {
        0 => src,
        1 => src & dst,
        2 => src | dst,
        3 => src ^ dst,
        4 => !src & mask,
        _ => dst,
    }
}

impl V9938 {
    fn geometry(&self) -> Geometry {
        Geometry::of(self.mode())
    }

    fn reg_word(&self, low: usize, mask: u16) -> u16 {
        (u16::from(self.regs[low]) | (u16::from(self.regs[low + 1]) << 8)) & mask
    }

    fn set_reg_word(&mut self, low: usize, value: u16) {
        self.regs[low] = value as u8;
        self.regs[low + 1] = (value >> 8) as u8 & crate::REGISTER_MASKS[low + 1];
    }

    fn read_pixel(&self, x: u16, y: u16) -> u8 {
        let g = self.geometry();
        (self.vram[g.address(x, y)] >> g.shift(x)) & g.mask()
    }

    fn write_pixel(&mut self, x: u16, y: u16, colour: u8, lop: u8) {
        let g = self.geometry();
        let address = g.address(x, y);
        let shift = g.shift(x);
        let mask = g.mask();
        let old = (self.vram[address] >> shift) & mask;
        let new = logic(lop, colour & mask, old, mask);
        self.vram[address] = (self.vram[address] & !(mask << shift)) | (new << shift);
    }

    fn read_byte(&self, x: u16, y: u16) -> u8 {
        self.vram[self.geometry().address(x, y)]
    }

    fn write_byte(&mut self, x: u16, y: u16, value: u8) {
        let address = self.geometry().address(x, y);
        self.vram[address] = value;
    }

    /// `R#46` was written.
    pub(crate) fn start_command(&mut self, value: u8) {
        let op = value >> 4;
        let g = self.geometry();
        let c = &mut self.command;
        c.op = op;
        c.lop = value & 0x0F;
        c.transfer_ready = false;
        if op == STOP || matches!(op, 1..=3) {
            c.op = STOP;
            return;
        }
        let sx = self.reg_word(32, 0x1FF);
        let sy = self.reg_word(34, 0x3FF);
        let dx = self.reg_word(36, 0x1FF);
        let dy = self.reg_word(38, 0x3FF);
        let nx = self.reg_word(40, 0x3FF);
        let ny = self.reg_word(42, 0x3FF);
        let byte_op = matches!(op, HMMV | HMMM | YMMM | HMMC);
        let step = if byte_op { g.pixels_per_byte } else { 1 };
        let c = &mut self.command;
        c.sx = sx;
        c.sy = sy;
        c.dx = dx;
        c.dy = dy;
        c.nx = nx;
        c.ny = ny;
        c.sx_start = sx;
        c.dx_start = dx;
        c.lines_left = if ny == 0 { 1024 } else { ny };
        c.budget = -start_cost(op);
        c.cpu_data = None;
        if byte_op {
            // The byte commands start on a byte boundary.
            c.sx_start = sx - sx % step;
            c.dx_start = dx - dx % step;
            c.sx = c.sx_start;
            c.dx = c.dx_start;
        }
        if op == SRCH {
            c.border_found = false;
        }
        if op == LINE {
            c.error = nx.wrapping_sub(1) >> 1;
            c.count = 0;
        }
        let arg = self.regs[45];
        self.command.line_left = self.line_units(op, arg, g);
        if matches!(op, HMMC | LMMC) {
            // The first byte comes from R#44 as the command starts.
            self.command.cpu_data = Some(self.regs[44]);
        }
    }

    /// Units on one line of a rectangle: NX, clipped to the screen edge in
    /// the direction of travel for both source and destination.
    fn line_units(&self, op: u8, arg: u8, g: Geometry) -> u16 {
        let c = &self.command;
        let byte_op = matches!(op, HMMV | HMMM | YMMM | HMMC);
        let step = if byte_op { g.pixels_per_byte } else { 1 };
        let left = arg & 0x04 != 0;
        let room = |x: u16| {
            if left {
                x / step + 1
            } else {
                (g.width - (x & (g.width - 1))) / step
            }
        };
        let mut units = if op == YMMM {
            room(c.dx)
        } else {
            let nx = if c.nx == 0 { 512 } else { c.nx };
            (nx / step).max(1).min(room(c.dx))
        };
        if matches!(op, LMMM | HMMM) {
            units = units.min(room(c.sx));
        }
        units
    }

    /// `R#44` was written: during HMMC or LMMC it is the next datum.
    pub(crate) fn command_colour_written(&mut self) {
        if matches!(self.command.op, HMMC | LMMC) {
            self.command.cpu_data = Some(self.regs[44]);
            self.command.transfer_ready = false;
        }
    }

    /// `S#7` was read: during LMCM that releases the next pixel.
    pub(crate) fn command_colour_read(&mut self) {
        if self.command.op == LMCM {
            self.command.transfer_ready = false;
        }
    }

    /// Advance the engine by one dot's worth of master clocks.
    pub(crate) fn step_command(&mut self) {
        if !self.command.active() {
            return;
        }
        self.command.budget += 4;
        while self.command.active() && self.command.budget >= 0 {
            if !self.command_unit() {
                break;
            }
        }
    }

    /// Run one unit. Returns false when waiting for the CPU.
    fn command_unit(&mut self) -> bool {
        let op = self.command.op;
        let lop = self.command.lop;
        let arg = self.regs[45];
        let g = self.geometry();
        let tx: i32 = if arg & 0x04 != 0 { -1 } else { 1 };
        let ty: i32 = if arg & 0x08 != 0 { -1 } else { 1 };
        let clr = self.regs[44];
        let c = self.command.clone();
        match op {
            POINT => {
                self.command.colour = self.read_pixel(c.sx, c.sy);
                self.finish_command();
                return true;
            }
            PSET => {
                self.write_pixel(c.dx, c.dy, clr, lop);
                self.finish_command();
                return true;
            }
            SRCH => {
                let colour = self.read_pixel(c.sx, c.sy);
                let equal = colour == clr & g.mask();
                let stop_when_unequal = arg & 0x02 != 0;
                self.command.budget -= unit_cost(op);
                if equal != stop_when_unequal {
                    self.command.border_found = true;
                    self.command.border_x = c.sx;
                    self.finish_command();
                    return true;
                }
                let next = i32::from(c.sx) + tx;
                if next < 0 || next >= i32::from(g.width) {
                    self.command.border_x = next.clamp(0, 0x1FF) as u16;
                    self.finish_command();
                    return true;
                }
                self.command.sx = next as u16;
                return true;
            }
            LINE => {
                self.line_unit(c, clr, lop, tx, ty, g);
                return true;
            }
            _ => {}
        }

        // The rectangle commands.
        let byte_op = matches!(op, HMMV | HMMM | YMMM | HMMC);
        match op {
            LMMV => self.write_pixel(c.dx, c.dy, clr, lop),
            LMMM => {
                let colour = self.read_pixel(c.sx, c.sy);
                self.write_pixel(c.dx, c.dy, colour, lop);
            }
            LMCM => {
                if c.transfer_ready {
                    return false;
                }
                self.command.colour = self.read_pixel(c.sx, c.sy);
                self.command.transfer_ready = true;
            }
            LMMC => {
                let Some(data) = c.cpu_data else {
                    self.command.transfer_ready = true;
                    return false;
                };
                self.write_pixel(c.dx, c.dy, data, lop);
                self.command.cpu_data = None;
                self.command.transfer_ready = true;
            }
            HMMV => self.write_byte(c.dx, c.dy, clr),
            HMMM => {
                let byte = self.read_byte(c.sx, c.sy);
                self.write_byte(c.dx, c.dy, byte);
            }
            YMMM => {
                let byte = self.read_byte(c.dx, c.sy);
                self.write_byte(c.dx, c.dy, byte);
            }
            HMMC => {
                let Some(data) = c.cpu_data else {
                    self.command.transfer_ready = true;
                    return false;
                };
                self.write_byte(c.dx, c.dy, data);
                self.command.cpu_data = None;
                self.command.transfer_ready = true;
            }
            _ => {
                self.finish_command();
                return true;
            }
        }
        self.command.budget -= unit_cost(op);
        let step = if byte_op {
            i32::from(g.pixels_per_byte)
        } else {
            1
        } * tx;
        let e = &mut self.command;
        e.sx = (i32::from(e.sx) + step) as u16 & 0x1FF;
        e.dx = (i32::from(e.dx) + step) as u16 & 0x1FF;
        e.line_left -= 1;
        if e.line_left == 0 {
            e.budget -= line_end_cost(op);
            e.lines_left -= 1;
            e.sy = (i32::from(e.sy) + ty) as u16 & 0x3FF;
            e.dy = (i32::from(e.dy) + ty) as u16 & 0x3FF;
            e.sx = e.sx_start;
            e.dx = e.dx_start;
            if e.lines_left == 0 {
                self.finish_command();
            } else {
                self.command.line_left = self.line_units(op, arg, g);
            }
        }
        true
    }

    /// One pixel of LINE, as openMSX steps it: the long side is NX, the
    /// short side NY, and an error term decides when the short side moves.
    fn line_unit(&mut self, c: CommandEngine, clr: u8, lop: u8, tx: i32, ty: i32, g: Geometry) {
        self.write_pixel(c.dx, c.dy, clr, lop);
        let major_y = self.regs[45] & 0x01 != 0;
        let e = &mut self.command;
        e.budget -= unit_cost(LINE);
        let mut dx = i32::from(c.dx);
        let mut dy = i32::from(c.dy);
        let mut error = c.error;
        if major_y {
            if error < c.ny {
                error = error.wrapping_add(c.nx);
                dx += tx;
                e.budget -= 36;
            }
            error = error.wrapping_sub(c.ny) & 1023;
            dy += ty;
        } else {
            dx += tx;
            if error < c.ny {
                error = error.wrapping_add(c.nx);
                dy += ty;
                e.budget -= 36;
            }
            error = error.wrapping_sub(c.ny) & 1023;
        }
        e.error = error;
        e.dx = dx as u16 & 0x1FF;
        e.dy = dy as u16 & 0x3FF;
        let count = c.count;
        e.count = count + 1;
        let off_screen = dx < 0 || dx >= i32::from(g.width);
        if count == c.nx || off_screen {
            self.finish_command();
        }
    }

    /// End the command and write back where it stopped.
    fn finish_command(&mut self) {
        let c = self.command.clone();
        match c.op {
            LINE => self.set_reg_word(38, c.dy),
            LMMV | LMMM | LMCM | LMMC | HMMV | HMMM | YMMM | HMMC => {
                let done = if c.ny == 0 { 1024 } else { c.ny } - c.lines_left;
                self.set_reg_word(34, c.sy);
                self.set_reg_word(38, c.dy);
                self.set_reg_word(42, c.ny.wrapping_sub(done) & 0x3FF);
            }
            _ => {}
        }
        self.command.op = STOP;
        if c.op != LMCM {
            self.command.transfer_ready = false;
        }
        self.regs[46] &= 0x0F;
    }
}
