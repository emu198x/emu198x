//! MC6847 wiring for the Acorn Atom.
//!
//! Atom-specific thin wrapper around the shared [`motorola_vdg_6847`] helper
//! crate. The shared crate owns the canonical MC6847 character ROM, mode decode,
//! and per-line render; this wrapper keeps the Atom-specific bits:
//!
//! - a **per-line** render: the machine derives the active display line from the
//!   50 Hz field clock that also drives PC7 field-sync, and [`Mc6847::tick`]
//!   renders each active line as the beam passes it via the shared
//!   [`render_visible_argb_line_into`], sampling the *current* control/CSS and
//!   live video RAM. A program that switches mode part-way down the field — the
//!   classic split screen — therefore renders two modes in one frame, where the
//!   old whole-frame render (one control value, one VRAM snapshot at VBLANK) could
//!   only show one. The static border is painted once and never redrawn.
//! - the green-phosphor [`TextPalette`] (Atom monitor aesthetic).
//! - the $B000 control register (A/G + GM0-2 from port A; CSS from port C),
//!   decoded into the shared crate's [`VdgControl`] so all eight MC6847 modes
//!   render — text plus graphics modes 1-5.

use motorola_vdg_6847::{
    TEXT_FRAMEBUFFER_HEIGHT, TEXT_TOP_BORDER_LINES, TEXT_VISIBLE_FRAMEBUFFER_HEIGHT,
    TEXT_VISIBLE_FRAMEBUFFER_PIXELS, TEXT_VISIBLE_FRAMEBUFFER_WIDTH, TextPalette, VdgControl,
    render_visible_argb_line_into,
};
use serde::{Deserialize, Serialize};

/// Framebuffer width (active 256 + 60 + 56 border = shared 372).
pub const FB_WIDTH: u32 = TEXT_VISIBLE_FRAMEBUFFER_WIDTH as u32;
/// Framebuffer height (active 192 + 25 + 26 border = shared 243).
pub const FB_HEIGHT: u32 = TEXT_VISIBLE_FRAMEBUFFER_HEIGHT as u32;
/// MC6847 active display lines — the shared crate's 192-line active region. The
/// machine maps the field-active window onto `0..ACTIVE_LINES`.
pub const ACTIVE_LINES: u32 = TEXT_FRAMEBUFFER_HEIGHT as u32;

/// Atom green-phosphor text palette — green-on-black with green border.
const ATOM_PALETTE: TextPalette = TextPalette {
    background: 0xFF00_2000,
    foreground: 0xFF00_FF00,
    border: 0xFF00_4000,
};

/// MC6847 Video Display Generator (Atom wiring).
#[derive(Serialize, Deserialize)]
pub struct Mc6847 {
    framebuffer: Vec<u32>,
    /// VDG control register — the 8255 port-A byte. PA4 = A/G (alpha vs
    /// graphics), PA5-7 = GM0-2 (the graphics-mode select); the keyboard column
    /// index (PA0-3) shares the byte but is not a VDG signal (MAME `atom.cpp`:
    /// "4 = A/G, 5 = GM0, 6 = GM1, 7 = GM2").
    pub control: u8,
    /// CSS — MC6847 colour-set select. Wired to 8255 **port C bit 3**, not
    /// port A (Atom Technical Manual §25.5; Atomulator `8255.c`). The machine
    /// updates this on a port-C write; render reads it, not `control` bit 3
    /// (which is keyboard column PA3) — see #369.
    pub css: bool,
    frame_complete: bool,
    /// Active display lines rendered so far this field (`0..=ACTIVE_LINES`); reset
    /// to 0 when the field wraps. The gap between this and the new `active_line`
    /// is the set of lines to render this tick.
    last_active_line: u32,
}

impl Mc6847 {
    /// Create a new VDG, framebuffer pre-painted with the green border.
    #[must_use]
    pub fn new() -> Self {
        Self {
            framebuffer: vec![ATOM_PALETTE.border; TEXT_VISIBLE_FRAMEBUFFER_PIXELS],
            control: 0,
            css: false,
            frame_complete: false,
            last_active_line: 0,
        }
    }

    /// Decode the 8255 control byte and CSS line into the shared crate's control.
    fn control_lines(&self) -> VdgControl {
        VdgControl {
            // 8255 port A: PA4 = A/G, PA5-7 = GM0-2 (MAME `atom.cpp`).
            graphics: self.control & 0x10 != 0,
            // CSS comes from 8255 PC3, tracked separately — not control bit 3
            // (PA3), which is a keyboard-scan line (#369).
            css: self.css,
            // The Atom ties INT/EXT low: alphanumerics use the MC6847's internal
            // font, and semigraphics-4 is selected per character by the data bus.
            int_ext: false,
            gm: (self.control >> 5) & 0x07,
        }
    }

    /// Tick the VDG for one master clock.
    ///
    /// `active_line` (`0..=ACTIVE_LINES`) is the MC6847 active display line the
    /// beam is currently scanning, derived by the machine from the 50 Hz field
    /// clock that also drives PC7 field-sync — so the VDG tracks the field a
    /// split-screen program races. `ACTIVE_LINES` means the beam is past the
    /// active region (in the bottom border / flyback).
    ///
    /// Every active line the beam has just left is rendered with the *current*
    /// control/CSS and live video RAM, so a mid-field mode change splits the
    /// frame. Returns `true` on the field wrap that completes a frame.
    pub fn tick(&mut self, active_line: u32, read_video_ram: impl Fn(u16) -> u8) -> bool {
        let active_line = active_line.min(ACTIVE_LINES);

        // The field wrapped back to the top: the frame in the buffer is complete.
        if active_line < self.last_active_line {
            self.frame_complete = true;
            self.last_active_line = 0;
        }

        // Render each active line the beam has crossed since the last tick. The
        // static border was painted at construction and is never redrawn.
        if active_line > self.last_active_line {
            let control = self.control_lines();
            for line in self.last_active_line..active_line {
                render_visible_argb_line_into(
                    |index| read_video_ram(index as u16),
                    control,
                    ATOM_PALETTE.into(),
                    &mut self.framebuffer,
                    TEXT_TOP_BORDER_LINES + line as usize,
                );
            }
            self.last_active_line = active_line;
        }

        self.frame_complete
    }

    /// Take the frame-complete flag.
    pub fn take_frame_complete(&mut self) -> bool {
        let result = self.frame_complete;
        self.frame_complete = false;
        result
    }

    /// Reference to the framebuffer.
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    /// Framebuffer width.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        FB_WIDTH
    }

    /// Framebuffer height.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        FB_HEIGHT
    }
}

impl Default for Mc6847 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive a whole field through the VDG, optionally flipping the control byte
    /// at `split` active line. `byte` supplies the video-RAM contents.
    fn render_field(vdg: &mut Mc6847, before: u8, after: u8, split: u32, byte: u8) {
        vdg.control = before;
        for line in 0..=ACTIVE_LINES {
            if line == split {
                vdg.control = after;
            }
            vdg.tick(line, |_| byte);
        }
        vdg.tick(0, |_| byte); // field wrap -> frame complete
        assert!(vdg.take_frame_complete());
    }

    fn active_row(vdg: &Mc6847, active_y: u32) -> &[u32] {
        let visible_y = TEXT_TOP_BORDER_LINES + active_y as usize;
        let start = visible_y * TEXT_VISIBLE_FRAMEBUFFER_WIDTH;
        &vdg.framebuffer()[start..start + TEXT_VISIBLE_FRAMEBUFFER_WIDTH]
    }

    #[test]
    fn a_static_field_renders_uniformly() {
        // No mid-field change: every active row that shares content matches.
        let mut vdg = Mc6847::new();
        render_field(&mut vdg, 0x10, 0x10, ACTIVE_LINES + 1, 0b1010_1010);
        assert_eq!(active_row(&vdg, 10), active_row(&vdg, 150));
    }

    #[test]
    fn a_mid_field_mode_change_splits_the_frame() {
        // Graphics for the top half, text for the bottom: the two halves of the
        // one frame must differ — proof the render samples control per line.
        let mut vdg = Mc6847::new();
        render_field(&mut vdg, 0x10, 0x00, 96, 0b1010_1010);
        let top = active_row(&vdg, 40).to_vec();
        let bottom = active_row(&vdg, 150).to_vec();
        assert_ne!(top, bottom, "the split must render two different modes");

        // And a line above the split matches a graphics-only render; a line below
        // matches text — i.e. the boundary really is at the split.
        let mut all_graphics = Mc6847::new();
        render_field(&mut all_graphics, 0x10, 0x10, ACTIVE_LINES + 1, 0b1010_1010);
        assert_eq!(active_row(&vdg, 40), active_row(&all_graphics, 40));
    }
}
