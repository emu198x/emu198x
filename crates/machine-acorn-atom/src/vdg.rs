//! MC6847 wiring for the Acorn Atom.
//!
//! Atom-specific thin wrapper around the shared [`motorola_vdg_6847`] helper
//! crate. The shared crate owns the canonical MC6847 character ROM, mode decode,
//! and beam-byte render; this wrapper keeps the Atom-specific bits:
//!
//! - a **per-byte** render: the machine derives the beam's *dot* position from the
//!   50 Hz field clock that also drives PC7 field-sync, and [`Mc6847::tick`]
//!   renders each active display byte as the beam crosses it via the shared
//!   [`decode_beam_byte`] + [`motorola_vdg_6847::VdgBeamByte::render_range_into`]
//!   (the same primitives Dragon-32 uses), sampling the *current* control/CSS and
//!   live video RAM. A program that switches mode part-way *across* a line — not
//!   just between lines — therefore renders two modes within one scanline, where
//!   the old whole-frame render (one control value, one VRAM snapshot at VBLANK)
//!   could only show one. The static border is painted once and never redrawn.
//! - the green-phosphor [`TextPalette`] (Atom monitor aesthetic).
//! - the $B000 control register (A/G + GM0-2 from port A; CSS from port C),
//!   decoded into the shared crate's [`VdgControl`] so all eight MC6847 modes
//!   render — text plus graphics modes 1-5.

use motorola_vdg_6847::{
    TEXT_FRAMEBUFFER_HEIGHT, TEXT_FRAMEBUFFER_WIDTH, TEXT_VISIBLE_FRAMEBUFFER_WIDTH, TextPalette,
    VdgControl, decode_beam_byte,
};
use serde::{Deserialize, Serialize};

/// Framebuffer width (active 256 + 60 + 56 border = shared 372).
///
/// Within three pixels of a set's window: 7.093788 MHz over 52.0 µs is 369,
/// and this holds 372. The #1054 audit reads it as 101%, and the asymmetric
/// 60/56 border is the shared crate's, not a figure chosen here — unlike the
/// height, which this machine now states for itself.
pub const FB_WIDTH: u32 = TEXT_VISIBLE_FRAMEBUFFER_WIDTH as u32;

/// Scan lines a PAL set displays, and so the height of the framebuffer.
///
/// The Atom is PAL only, so this is a constant here rather than a region
/// parameter like GTIA's or the VDPs'.
pub const PAL_ACTIVE_LINES: u32 = 288;

/// Framebuffer height: the whole PAL field.
///
/// This used to be the shared crate's `TEXT_VISIBLE_FRAMEBUFFER_HEIGHT`, which
/// is 25 + 192 + 26 = 243 — a VDG-generic "visible" figure that the Dragon
/// places as a sub-window inside its 312-line overscan frame. Borrowed here it
/// meant the Atom showed 243 of a set's 288 lines, which the #1054 audit read
/// as 84%. The shared constant is right for what the Dragon does with it and
/// stays; the Atom states its own.
pub const FB_HEIGHT: u32 = PAL_ACTIVE_LINES;

/// Scan lines of border above the active area — what the field has left over
/// around the 192 the VDG draws, halved. Written as the arithmetic, because
/// the arithmetic is the justification.
pub const BORDER_TOP: u32 = (PAL_ACTIVE_LINES - TEXT_FRAMEBUFFER_HEIGHT as u32) / 2;
/// MC6847 active display lines — the shared crate's 192-line active region.
pub const ACTIVE_LINES: u32 = TEXT_FRAMEBUFFER_HEIGHT as u32;
/// Active display width in dots (pixels) — the bytes of a line, whatever the mode,
/// always span this width.
const ACTIVE_WIDTH: u32 = TEXT_FRAMEBUFFER_WIDTH as u32;
/// Total active beam dots in a field (`192 × 256`). The machine maps the
/// field-active window onto `0..ACTIVE_DOTS`; the VDG walks the beam across it.
pub const ACTIVE_DOTS: u32 = ACTIVE_LINES * ACTIVE_WIDTH;

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
    /// Beam state: the active line being scanned, the next display byte to draw on
    /// it, and that byte's left edge in active pixels. Reset to the top-left when
    /// the field wraps. The gap between this and the new beam dot is the set of
    /// bytes to render this tick.
    beam_line: u32,
    next_byte: usize,
    active_x: usize,
    last_dot: u32,
}

impl Mc6847 {
    /// Create a new VDG, framebuffer pre-painted with the green border.
    #[must_use]
    pub fn new() -> Self {
        Self {
            framebuffer: vec![ATOM_PALETTE.border; (FB_WIDTH * FB_HEIGHT) as usize],
            control: 0,
            css: false,
            frame_complete: false,
            beam_line: 0,
            next_byte: 0,
            active_x: 0,
            last_dot: 0,
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
    /// `active_dot` (`0..=ACTIVE_DOTS`) is the active beam position — `line × 256 +
    /// pixel` — derived by the machine from the 50 Hz field clock that also drives
    /// PC7 field-sync, so the VDG tracks the field a beam-racing program races.
    /// `ACTIVE_DOTS` means the beam is past the active region (bottom border /
    /// flyback).
    ///
    /// Every display byte the beam has just crossed is rendered with the *current*
    /// control/CSS and live video RAM, so a mid-line mode change splits the line.
    /// Returns `true` on the field wrap that completes a frame.
    pub fn tick(&mut self, active_dot: u32, read_video_ram: impl Fn(u16) -> u8) -> bool {
        let active_dot = active_dot.min(ACTIVE_DOTS);

        // The field wrapped back to the top-left: the frame in the buffer is done.
        if active_dot < self.last_dot {
            self.frame_complete = true;
            self.beam_line = 0;
            self.next_byte = 0;
            self.active_x = 0;
        }

        self.render_up_to(active_dot, &read_video_ram);
        self.last_dot = active_dot;
        self.frame_complete
    }

    /// Render every active display byte from the beam's current position up to
    /// `active_dot`, advancing line by line. The static border was painted at
    /// construction and is never redrawn.
    fn render_up_to(&mut self, active_dot: u32, read_video_ram: &impl Fn(u16) -> u8) {
        let control = self.control_lines();
        let palette = ATOM_PALETTE.into();
        while self.beam_line < ACTIVE_LINES {
            let line_base = self.beam_line * ACTIVE_WIDTH;
            // How far across this line the beam has reached (clamped to the line).
            let reached = active_dot.saturating_sub(line_base).min(ACTIVE_WIDTH);
            while (self.active_x as u32) < reached {
                let line = self.beam_line as usize;
                let byte = decode_beam_byte(
                    |i| read_video_ram(i as u16),
                    control,
                    palette,
                    line,
                    self.next_byte,
                );
                let width = byte.width();
                if width == 0 {
                    // No further bytes in this mode; treat the line as filled.
                    self.active_x = ACTIVE_WIDTH as usize;
                    break;
                }
                let active_x = self.active_x;
                byte.render_range_into(
                    &mut self.framebuffer,
                    BORDER_TOP as usize + line,
                    active_x,
                    0,
                    width,
                );
                self.active_x += width;
                self.next_byte += 1;
            }
            // Once the beam has passed this line's end, move to the next line.
            if active_dot >= line_base + ACTIVE_WIDTH {
                self.beam_line += 1;
                self.next_byte = 0;
                self.active_x = 0;
            } else {
                break;
            }
        }
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
    use motorola_vdg_6847::render_visible_argb;

    /// Drive a whole field through the VDG, flipping the control byte at active
    /// beam dot `split`. `byte` supplies the video-RAM contents.
    fn render_field(vdg: &mut Mc6847, before: u8, after: u8, split: u32, byte: u8) {
        vdg.control = before;
        for dot in 0..=ACTIVE_DOTS {
            if dot == split {
                vdg.control = after;
            }
            vdg.tick(dot, |_| byte);
        }
        vdg.tick(0, |_| byte); // field wrap -> frame complete
        assert!(vdg.take_frame_complete());
    }

    fn active_row(vdg: &Mc6847, active_y: u32) -> &[u32] {
        let visible_y = BORDER_TOP as usize + active_y as usize;
        let start = visible_y * TEXT_VISIBLE_FRAMEBUFFER_WIDTH;
        &vdg.framebuffer()[start..start + TEXT_VISIBLE_FRAMEBUFFER_WIDTH]
    }

    #[test]
    fn a_static_field_renders_uniformly() {
        let mut vdg = Mc6847::new();
        render_field(&mut vdg, 0x10, 0x10, ACTIVE_DOTS + 1, 0b1010_1010);
        assert_eq!(active_row(&vdg, 10), active_row(&vdg, 150));
    }

    #[test]
    fn a_static_field_matches_the_whole_frame_render() {
        // The per-byte beam walk must produce exactly what the shared crate's
        // whole-frame render does for an unchanging control.
        let mut vdg = Mc6847::new();
        render_field(&mut vdg, 0x10, 0x10, ACTIVE_DOTS + 1, 0b1010_1010);
        let control = vdg.control_lines();
        let whole = render_visible_argb(|_| 0b1010_1010, control, ATOM_PALETTE.into());

        // Row by row across the active band, because the two frames are no
        // longer the same height: the shared render is the VDG-generic 243,
        // and this machine holds the 288 a PAL set shows. The active lines are
        // the claim — the rest is border, identical in both by construction.
        let width = FB_WIDTH as usize;
        let shared_top = motorola_vdg_6847::TEXT_TOP_BORDER_LINES;
        for line in 0..TEXT_FRAMEBUFFER_HEIGHT {
            let mine = (BORDER_TOP as usize + line) * width;
            let theirs = (shared_top + line) * width;
            assert_eq!(
                &vdg.framebuffer()[mine..mine + width],
                &whole[theirs..theirs + width],
                "active line {line} differs from the whole-frame render"
            );
        }
    }

    #[test]
    fn a_mid_field_mode_change_splits_between_lines() {
        // Graphics for the top half, text for the bottom (split on a line edge):
        // the two halves differ, and the above-split line matches all-graphics.
        let mut vdg = Mc6847::new();
        render_field(&mut vdg, 0x10, 0x00, 96 * ACTIVE_WIDTH, 0b1010_1010);
        assert_ne!(active_row(&vdg, 40), active_row(&vdg, 150));

        let mut all_graphics = Mc6847::new();
        render_field(&mut all_graphics, 0x10, 0x10, ACTIVE_DOTS + 1, 0b1010_1010);
        assert_eq!(active_row(&vdg, 40), active_row(&all_graphics, 40));
    }

    #[test]
    fn a_mid_line_mode_change_splits_within_one_line() {
        // Flip mode half-way across line 100: that single scanline must be neither
        // all-graphics nor all-text — only a per-byte render can do that.
        let line = 100;
        let split = line * ACTIVE_WIDTH + ACTIVE_WIDTH / 2;
        let mut vdg = Mc6847::new();
        render_field(&mut vdg, 0x10, 0x00, split, 0b1010_1010);
        let split_row = active_row(&vdg, line).to_vec();

        let mut all_graphics = Mc6847::new();
        render_field(&mut all_graphics, 0x10, 0x10, ACTIVE_DOTS + 1, 0b1010_1010);
        let mut all_text = Mc6847::new();
        render_field(&mut all_text, 0x00, 0x00, ACTIVE_DOTS + 1, 0b1010_1010);

        assert_ne!(
            split_row.as_slice(),
            active_row(&all_graphics, line),
            "the right of the line switched to text"
        );
        assert_ne!(
            split_row.as_slice(),
            active_row(&all_text, line),
            "the left of the line stayed graphics"
        );
    }
}
