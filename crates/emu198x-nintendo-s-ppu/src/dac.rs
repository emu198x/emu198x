//! Priority, colour math and output: the DAC end of the PPU.
//!
//! Each dot picks the highest-priority opaque pixel on the main screen
//! ("above") and the sub screen ("below"), then adds or subtracts the sub
//! screen, or the fixed colour, where the windows and CGADDSUB allow. In
//! hi-res the sub screen is drawn as a pixel of its own, first.

use serde::{Deserialize, Serialize};

use crate::{FRAME_WIDTH, SPpu};

/// Layers for CGADDSUB: BG1–BG4, sprites, backdrop.
const BACK: usize = 5;
const OBJ: usize = 4;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub(crate) struct DacIo {
    pub direct_color: bool,
    pub blend_mode: bool,
    pub color_enable: [bool; 6],
    pub color_halve: bool,
    /// Subtract rather than add.
    pub color_mode: bool,
    pub color_red: u8,
    pub color_green: u8,
    pub color_blue: u8,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Screen {
    pub color: u16,
    pub color_enable: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Math {
    pub above: Screen,
    pub below: Screen,
    pub transparent: bool,
    pub blend_mode: bool,
    pub color_halve: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Dac {
    pub io: DacIo,
    pub math: Math,
    /// The framebuffer index of the next pixel, if this line is shown.
    pub cursor: Option<usize>,
}

impl SPpu {
    pub(crate) fn dac_scanline(&mut self) {
        let mut v = self.counter.vcounter;
        if !self.state.overscan {
            v += 8;
        }
        self.dac.cursor = (v < 240).then(|| {
            let row = if self.state.interlace {
                v * 2 + u32::from(self.counter.field)
            } else {
                v
            };
            row as usize * FRAME_WIDTH
        });
        let backdrop = self.palette_color(0);
        let math = &mut self.dac.math;
        math.above.color = backdrop;
        math.below.color = backdrop;
        math.above.color_enable = false;
        math.below.color_enable = false;
        math.transparent = true;
        math.blend_mode = false;
        math.color_halve =
            self.dac.io.color_halve && !self.dac.io.blend_mode && math.above.color_enable;
    }

    pub(crate) fn dac_run(&mut self) {
        if self.counter.vcounter == 0 {
            return;
        }
        let hires = self.io.pseudo_hires || self.io.bg_mode == 5 || self.io.bg_mode == 6;
        let below = self.dac_below(hires);
        let above = self.dac_above();
        let Some(index) = self.dac.cursor else {
            return;
        };
        let brightness = u32::from(self.io.display_brightness) << 15;
        let first = if hires { below } else { above };
        self.put_pixel(index, brightness | u32::from(first));
        self.put_pixel(index + 1, brightness | u32::from(above));
        self.dac.cursor = Some(index + 2);
    }

    fn blanked(&self) -> bool {
        self.io.display_disable || (!self.io.overscan && self.counter.vcounter >= 225)
    }

    /// Whether BG1 is drawing in direct colour.
    fn direct(&self) -> bool {
        self.dac.io.direct_color && matches!(self.io.bg_mode, 3 | 4 | 7)
    }

    fn dac_below(&mut self, hires: bool) -> u16 {
        if self.blanked() {
            return 0;
        }
        let mut priority = 0;
        let bg1 = self.bg[0].output.below;
        if bg1.priority != 0 {
            priority = bg1.priority;
            self.dac.math.below.color = if self.direct() {
                direct_color(bg1.palette, bg1.palette_group)
            } else {
                self.palette_color(bg1.palette)
            };
        }
        for id in 1..4 {
            let p = self.bg[id].output.below;
            if p.priority > priority {
                priority = p.priority;
                self.dac.math.below.color = self.palette_color(p.palette);
            }
        }
        let obj = self.obj.below;
        if obj.priority > priority {
            priority = obj.priority;
            self.dac.math.below.color = self.palette_color(obj.palette);
        }
        self.dac.math.transparent = priority == 0;
        if self.dac.math.transparent {
            self.dac.math.below.color = self.palette_color(0);
        }

        if !hires {
            return 0;
        }
        let math = self.dac.math;
        if !math.below.color_enable {
            return if math.above.color_enable {
                math.below.color
            } else {
                0
            };
        }
        let x = if math.above.color_enable {
            math.below.color
        } else {
            0
        };
        let y = if math.blend_mode {
            math.above.color
        } else {
            self.fixed_color()
        };
        self.blend(x, y)
    }

    fn dac_above(&mut self) -> u16 {
        if self.blanked() {
            return 0;
        }
        let mut priority = 0;
        let bg1 = self.bg[0].output.above;
        if bg1.priority != 0 {
            priority = bg1.priority;
            self.dac.math.above.color = if self.direct() {
                direct_color(bg1.palette, bg1.palette_group)
            } else {
                self.palette_color(bg1.palette)
            };
            self.dac.math.below.color_enable = self.dac.io.color_enable[0];
        }
        for id in 1..4 {
            let p = self.bg[id].output.above;
            if p.priority > priority {
                priority = p.priority;
                self.dac.math.above.color = self.palette_color(p.palette);
                self.dac.math.below.color_enable = self.dac.io.color_enable[id];
            }
        }
        let obj = self.obj.above;
        if obj.priority > priority {
            priority = obj.priority;
            self.dac.math.above.color = self.palette_color(obj.palette);
            self.dac.math.below.color_enable = self.dac.io.color_enable[OBJ] && obj.palette >= 192;
        }
        if priority == 0 {
            self.dac.math.above.color = self.palette_color(0);
            self.dac.math.below.color_enable = self.dac.io.color_enable[BACK];
        }

        if !self.window.below_color_enable {
            self.dac.math.below.color_enable = false;
        }
        self.dac.math.above.color_enable = self.window.above_color_enable;
        let math = self.dac.math;
        if !math.below.color_enable {
            return if math.above.color_enable {
                math.above.color
            } else {
                0
            };
        }
        if self.dac.io.blend_mode && math.transparent {
            self.dac.math.blend_mode = false;
            self.dac.math.color_halve = false;
        } else {
            self.dac.math.blend_mode = self.dac.io.blend_mode;
            self.dac.math.color_halve = self.dac.io.color_halve && math.above.color_enable;
        }
        let x = if math.above.color_enable {
            math.above.color
        } else {
            0
        };
        let y = if self.dac.math.blend_mode {
            math.below.color
        } else {
            self.fixed_color()
        };
        self.blend(x, y)
    }

    /// Add or subtract two 15-bit colours per component, clamping, and
    /// halving when asked.
    fn blend(&self, x: u16, y: u16) -> u16 {
        let x = u32::from(x);
        let y = u32::from(y);
        let result = if !self.dac.io.color_mode {
            if !self.dac.math.color_halve {
                let sum = x + y;
                let carry = sum.wrapping_sub((x ^ y) & 0x0421) & 0x8420;
                sum.wrapping_sub(carry) | (carry - (carry >> 5))
            } else {
                (x + y - ((x ^ y) & 0x0421)) >> 1
            }
        } else {
            let diff = x.wrapping_sub(y).wrapping_add(0x8420);
            let borrow = diff.wrapping_sub((x ^ y) & 0x8420) & 0x8420;
            let clamped = diff.wrapping_sub(borrow) & borrow.wrapping_sub(borrow >> 5);
            if self.dac.math.color_halve {
                (clamped & 0x7bde) >> 1
            } else {
                clamped
            }
        };
        (result & 0x7fff) as u16
    }

    /// A CGRAM colour. Reading it moves the address CGRAM accesses are
    /// redirected to during active display.
    fn palette_color(&mut self, palette: u8) -> u16 {
        self.latch.cgram_address = palette;
        self.cgram[usize::from(palette)]
    }

    fn fixed_color(&self) -> u16 {
        u16::from(self.dac.io.color_red)
            | u16::from(self.dac.io.color_green) << 5
            | u16::from(self.dac.io.color_blue) << 10
    }
}

/// BG1's direct colour: the 8-bit pixel value as BBGGGRRR, with the tile's
/// palette group supplying one more bit of each.
fn direct_color(palette: u8, group: u8) -> u16 {
    let p = u16::from(palette);
    let g = u16::from(group);
    (p << 7 & 0x6000)
        + (g << 10 & 0x1000)
        + (p << 4 & 0x0380)
        + (g << 5 & 0x0040)
        + (p << 2 & 0x001c)
        + (g << 1 & 0x0002)
}
