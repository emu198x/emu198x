//! MC6847 wiring for the Acorn Atom.
//!
//! Atom-specific thin wrapper around the shared
//! [`motorola_vdg_6847`] helper crate. The shared crate owns the
//! canonical MC6847 character ROM, mode decode, and per-line render;
//! this wrapper keeps the Atom-specific bits:
//!
//! - per-master-clock `tick()` that advances scanline / frame timing
//!   to drive `take_frame_complete()` (PAL: 312 lines × 228 ticks).
//! - the green-phosphor `TextPalette` (Atom monitor aesthetic).
//! - the $B000 control register (A/G bit only — Atom v1 is text-mode).
//! - a per-frame render that calls the shared
//!   [`render_visible_argb_into`] at VBLANK so screenshots see a
//!   clean frame.
//!
//! Before this slice the Atom carried its own duplicate per-pixel
//! renderer plus a tiny 5×7 ASCII subset CHAR_ROM. Switching to the
//! shared crate aligns Atom with Dragon-32's render path and removes
//! ~200 lines of duplicated code.

use motorola_vdg_6847::{
    TEXT_VISIBLE_FRAMEBUFFER_HEIGHT, TEXT_VISIBLE_FRAMEBUFFER_PIXELS,
    TEXT_VISIBLE_FRAMEBUFFER_WIDTH, TextPalette, VdgControl, render_visible_argb_into,
};

/// Framebuffer width (active 256 + 60 + 56 border = shared 372).
pub const FB_WIDTH: u32 = TEXT_VISIBLE_FRAMEBUFFER_WIDTH as u32;
/// Framebuffer height (active 192 + 25 + 26 border = shared 243).
pub const FB_HEIGHT: u32 = TEXT_VISIBLE_FRAMEBUFFER_HEIGHT as u32;

/// PAL line count for the Atom's MC6847.
const TOTAL_LINES: u32 = 312;
/// Ticks per scanline for the Atom's MC6847.
const TICKS_PER_LINE: u32 = 228;

/// Atom green-phosphor text palette — green-on-black with green border.
const ATOM_PALETTE: TextPalette = TextPalette {
    background: 0xFF00_2000,
    foreground: 0xFF00_FF00,
    border: 0xFF00_4000,
};

/// MC6847 Video Display Generator (Atom wiring).
pub struct Mc6847 {
    framebuffer: Vec<u32>,
    /// VDG control register (A/G bit — Atom v1 only checks alpha vs
    /// graphics; CSS/INT_EXT/GM bits are stored but not honoured).
    pub control: u8,
    /// Cached last-frame video RAM contents, used by `render_frame`.
    last_video_ram: [u8; 512],
    frame_complete: bool,
    scanline: u32,
    pixel_x: u32,
    /// Whether the latest frame has been rendered into the framebuffer.
    /// Set false at frame start; render happens lazily at frame end.
    needs_render: bool,
}

impl Mc6847 {
    /// Create a new VDG, framebuffer pre-painted with the green border.
    #[must_use]
    pub fn new() -> Self {
        Self {
            framebuffer: vec![ATOM_PALETTE.border; TEXT_VISIBLE_FRAMEBUFFER_PIXELS],
            control: 0,
            last_video_ram: [0; 512],
            frame_complete: false,
            scanline: 0,
            pixel_x: 0,
            needs_render: false,
        }
    }

    /// Tick one VDG clock. The Atom's `tick` loop advances scanline /
    /// pixel counters and snapshots video RAM continuously; the actual
    /// framebuffer render runs once per frame at VBLANK.
    pub fn tick(&mut self, read_video_ram: impl Fn(u16) -> u8) -> bool {
        // Snapshot the video RAM into the cached buffer so the frame-
        // end render sees the post-CPU contents without needing the
        // closure across the render boundary.
        if self.pixel_x == 0 && self.scanline == 0 {
            for index in 0..512u16 {
                self.last_video_ram[index as usize] = read_video_ram(index);
            }
            self.needs_render = true;
        }

        self.pixel_x += 1;
        if self.pixel_x >= TICKS_PER_LINE {
            self.pixel_x = 0;
            self.scanline += 1;
            if self.scanline >= TOTAL_LINES {
                self.scanline = 0;
                self.frame_complete = true;
                if self.needs_render {
                    self.render_frame();
                    self.needs_render = false;
                }
                return true;
            }
        }
        false
    }

    /// Render the latest snapshot of video RAM into the framebuffer
    /// via the shared MC6847 helper crate.
    fn render_frame(&mut self) {
        let control = VdgControl {
            graphics: self.control & 0x80 != 0,
            css: self.control & 0x08 != 0,
            int_ext: self.control & 0x10 != 0,
            gm: (self.control >> 4) & 0x07,
        };
        let video_ram = &self.last_video_ram;
        render_visible_argb_into(
            |index| video_ram[index & 0x01FF],
            control,
            ATOM_PALETTE.into(),
            &mut self.framebuffer,
        );
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
