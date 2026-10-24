//! The two windows: per-layer masking and the colour-math window.

use serde::{Deserialize, Serialize};

use crate::SPpu;

/// One layer's window settings. Layers 0–3 are BG1–BG4, 4 is sprites.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub(crate) struct LayerIo {
    pub one_invert: bool,
    pub one_enable: bool,
    pub two_invert: bool,
    pub two_enable: bool,
    pub mask: u8,
    pub above_enable: bool,
    pub below_enable: bool,
}

impl LayerIo {
    /// Set from a W12SEL/W34SEL/WOBJSEL nibble.
    pub fn set_select(&mut self, nibble: u8) {
        self.one_invert = nibble & 1 != 0;
        self.one_enable = nibble & 2 != 0;
        self.two_invert = nibble & 4 != 0;
        self.two_enable = nibble & 8 != 0;
    }
}

/// The colour window's settings.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub(crate) struct ColorIo {
    pub one_enable: bool,
    pub one_invert: bool,
    pub two_enable: bool,
    pub two_invert: bool,
    pub mask: u8,
    pub above_mask: u8,
    pub below_mask: u8,
}

impl ColorIo {
    pub fn set_select(&mut self, nibble: u8) {
        self.one_invert = nibble & 1 != 0;
        self.one_enable = nibble & 2 != 0;
        self.two_invert = nibble & 4 != 0;
        self.two_enable = nibble & 8 != 0;
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub(crate) struct WindowIo {
    pub layer: [LayerIo; 5],
    pub col: ColorIo,
    pub one_left: u8,
    pub one_right: u8,
    pub two_left: u8,
    pub two_right: u8,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Window {
    pub io: WindowIo,
    /// Whether colour math may apply on the main and sub screens here.
    pub above_color_enable: bool,
    pub below_color_enable: bool,
    x: u32,
    one_active: bool,
    one_stopped: bool,
    two_active: bool,
    two_stopped: bool,
}

/// Combine the two windows for one layer.
fn test(one_enable: bool, one: bool, two_enable: bool, two: bool, mask: u8) -> bool {
    if !one_enable {
        return two && two_enable;
    }
    if !two_enable {
        return one;
    }
    match mask {
        0 => one | two,
        1 => one & two,
        _ => u8::from(one ^ two) == 3 - mask,
    }
}

impl SPpu {
    pub(crate) fn window_scanline(&mut self) {
        let w = &mut self.window;
        w.x = 0;
        w.one_active = false;
        w.one_stopped = false;
        w.two_active = false;
        w.two_stopped = false;
    }

    pub(crate) fn window_run(&mut self) {
        let w = &mut self.window;
        let x = w.x;
        if x == u32::from(w.io.one_left) {
            w.one_active = true;
        }
        if x == u32::from(w.io.two_left) {
            w.two_active = true;
        }
        let one = w.one_active && !w.one_stopped;
        let two = w.two_active && !w.two_stopped;
        if x == u32::from(w.io.one_right) {
            w.one_stopped = true;
        }
        if x == u32::from(w.io.two_right) {
            w.two_stopped = true;
        }
        w.x += 1;

        let io = w.io;
        for (layer, l) in io.layer.iter().enumerate() {
            if test(
                l.one_enable,
                one ^ l.one_invert,
                l.two_enable,
                two ^ l.two_invert,
                l.mask,
            ) {
                let (above, below) = if layer < 4 {
                    let out = &mut self.bg[layer].output;
                    (&mut out.above.priority, &mut out.below.priority)
                } else {
                    (&mut self.obj.above.priority, &mut self.obj.below.priority)
                };
                if l.above_enable {
                    *above = 0;
                }
                if l.below_enable {
                    *below = 0;
                }
            }
        }

        let c = io.col;
        let value = test(
            c.one_enable,
            one ^ c.one_invert,
            c.two_enable,
            two ^ c.two_invert,
            c.mask,
        );
        let choose = |mask: u8| match mask {
            0 => true,
            1 => value,
            2 => !value,
            _ => false,
        };
        self.window.above_color_enable = choose(c.above_mask);
        self.window.below_color_enable = choose(c.below_mask);
    }
}
