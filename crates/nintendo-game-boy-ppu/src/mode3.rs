//! Mode 3: the DMG pixel pipeline, one dot at a time.
//!
//! The pipeline is a background/window fetcher feeding an 8-pixel
//! background FIFO, an object FIFO that sprite fetches overlay into,
//! and a shifter that pops one pixel per dot. Every register the
//! pipeline depends on is read at the dot the hardware reads it, so a
//! CPU write in the middle of mode 3 lands on the right pixel:
//!
//! - the fetcher reads `LCDC` (map and tile-data select, window
//!   enable), `SCX` (coarse) and `SCY` at its tile-index and
//!   bitplane steps;
//! - the shifter reads `LCDC` (BG and OBJ enable), `SCX` (fine
//!   discard) and `BGP`/`OBP0`/`OBP1` as each pixel leaves;
//! - the window trigger compares `WX` with the shifter position each
//!   dot.
//!
//! Sources: Pan Docs "Pixel FIFO" and "Rendering"
//! (`reference/assets/web-mirrors/gbdev.io/pandocs/pixel_fifo.html`);
//! Gekkio, *Game Boy: Complete Technical Reference*
//! (`reference/assets/web-mirrors/gekkio.fi/files/gb-docs/gbctr.pdf`);
//! Matt Currie, "The Comprehensive Game Boy PPU Documentation", in the
//! Mealybug Tearoom tests (`TILE_SEL`, `WIN_EN` and `SCY` mid-fetch
//! behaviour). The control flow, step for step, is SameBoy's DMG path
//! through `GB_display_run`, `render_pixel_if_possible` and
//! `advance_fetcher_state_machine` (`emulators/gameboy/SameBoy/Core/
//! display.c`, v1.0.3). Each `GB_SLEEP(n)` there is a [`Resume`] point
//! here with `wait = n`.

use serde::{Deserialize, Serialize};

use crate::fifo::Fifo;
use crate::{Ppu, SCREEN_WIDTH, apply_palette, lcdc};

/// The shifter position at which a line starts: −16, i.e. eight dots
/// of junk pixels then up to eight dots of SCX fine-scroll discard.
pub(crate) const POSITION_START: u8 = 0u8.wrapping_sub(16);

/// Background fetcher step. VRAM reads take two dots: the address is
/// formed on the first (`T1`) and the byte arrives on the second
/// (`T2`). `Push` waits for the background FIFO to empty.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum FetchStep {
    #[default]
    TileT1,
    TileT2,
    LowT1,
    LowT2,
    HighT1,
    HighT2,
    Push,
}

/// Where the pipeline picks up on its next active dot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) enum Resume {
    /// Not in mode 3 (line finished or not started).
    #[default]
    Idle,
    /// Top of the per-dot loop: window trigger check.
    LoopTop,
    /// One-dot stall when the window starts at `WX = 0` with a
    /// non-zero `SCX & 7`.
    WindowStall,
    /// Is the next object due at this position? (No dot consumed.)
    ObjCheck,
    /// Waiting for the background fetcher before an object fetch.
    ObjWait,
    /// Object fetch: the extra fetcher step, then the OAM read.
    ObjExtra,
    /// Object fetch: low bitplane read.
    ObjOam,
    /// Object fetch: high bitplane read.
    ObjLow,
    /// Object fetch: row ready to overlay.
    ObjHigh,
    /// Shift a pixel out and step the fetcher.
    RenderTail,
}

/// An object selected by the mode-2 OAM scan.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct VisibleObject {
    /// OAM entry number (0..40).
    pub index: u8,
    /// Raw OAM X (screen X + 8).
    pub x: u8,
    /// Raw OAM Y (screen Y + 16).
    pub y: u8,
}

/// Mode-3 pipeline state.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Mode3 {
    pub resume: Resume,
    /// Dots to wait before running `resume` (0 = run this dot).
    pub wait: u8,

    pub bg_fifo: Fifo,
    pub obj_fifo: Fifo,

    pub fetch: FetchStep,
    pub tile_index_addr: u16,
    pub tile_data_addr: u16,
    pub current_tile: u8,
    pub tile_data: [u8; 2],

    /// Objects found by the OAM scan, sorted so that the next to be
    /// fetched (smallest X, then lowest OAM index) is last.
    pub objects: [VisibleObject; 10],
    /// Objects still pending on this line (shrinks as they are fetched
    /// or passed).
    pub n_objects: u8,
    /// Objects the OAM scan found on this line.
    pub n_scanned: u8,
    pub object_tile: u8,
    pub object_flags: u8,
    pub object_data: [u8; 2],
    pub during_object_fetch: bool,
    pub object_fetch_aborted: bool,

    /// `WY` matched `LY` while the window was enabled this frame.
    pub wy_triggered: bool,
    /// The fetcher is drawing the window on this line.
    pub wx_triggered: bool,
    /// Window line counter; −1 before the first window line.
    pub window_y: u8,
    pub window_tile_x: u8,
    pub window_is_being_fetched: bool,
    pub insert_bg_pixel: bool,
    pub disable_window_pixel_insertion_glitch: bool,
    /// `WX` was written on the previous dot.
    pub wx_just_changed: bool,
}

impl Mode3 {
    pub(crate) const fn new() -> Self {
        Self {
            resume: Resume::Idle,
            wait: 0,
            bg_fifo: Fifo::new(),
            obj_fifo: Fifo::new(),
            fetch: FetchStep::TileT1,
            tile_index_addr: 0,
            tile_data_addr: 0,
            current_tile: 0,
            tile_data: [0; 2],
            objects: [VisibleObject {
                index: 0,
                x: 0,
                y: 0,
            }; 10],
            n_objects: 0,
            n_scanned: 0,
            object_tile: 0,
            object_flags: 0,
            object_data: [0; 2],
            during_object_fetch: false,
            object_fetch_aborted: false,
            wy_triggered: false,
            wx_triggered: false,
            window_y: 0xFF,
            window_tile_x: 0,
            window_is_being_fetched: false,
            insert_bg_pixel: false,
            disable_window_pixel_insertion_glitch: false,
            wx_just_changed: false,
        }
    }

    /// The scanned objects in fetch order (leftmost first), for the
    /// mode-3 length estimate.
    pub(crate) fn scanned_in_fetch_order(&self) -> impl Iterator<Item = &VisibleObject> {
        self.objects[..usize::from(self.n_scanned)].iter().rev()
    }
}

impl Ppu {
    /// Mode-2 OAM scan: up to ten objects whose rows cover `LY`, using
    /// the object height in `LCDC` at scan time.
    pub(crate) fn scan_oam(&mut self, oam: &[u8]) {
        let height: i16 = if (self.lcdc & lcdc::SPRITE_HEIGHT_16) != 0 {
            16
        } else {
            8
        };
        let line = i16::from(self.ly);
        let m3 = &mut self.m3;
        m3.n_objects = 0;
        for index in 0..40u8 {
            if m3.n_objects == 10 {
                break;
            }
            let base = usize::from(index) * 4;
            let (Some(&y), Some(&x)) = (oam.get(base), oam.get(base + 1)) else {
                break;
            };
            let top = i16::from(y) - 16;
            if line < top || line >= top + height {
                continue;
            }
            // Keep the list sorted by descending X; a later OAM entry
            // with the same X goes before (is fetched after) earlier
            // ones.
            let n = usize::from(m3.n_objects);
            let at = m3.objects[..n].iter().position(|o| o.x <= x).unwrap_or(n);
            m3.objects.copy_within(at..n, at + 1);
            m3.objects[at] = VisibleObject { index, x, y };
            m3.n_objects += 1;
        }
        m3.n_scanned = m3.n_objects;
    }

    /// Re-arm the window `WY` comparison against `line`.
    pub(crate) fn wy_check(&mut self, line: u8) {
        if (self.lcdc & lcdc::ENABLE) != 0
            && (self.lcdc & lcdc::WINDOW_ENABLE) != 0
            && self.wy == line
        {
            self.m3.wy_triggered = true;
        }
    }

    /// Start mode 3: eight junk pixels in the background FIFO, fetcher
    /// at its first step, then run the first loop iteration.
    pub(crate) fn start_mode3(&mut self, vram: &[u8], oam: &[u8]) {
        let m3 = &mut self.m3;
        m3.disable_window_pixel_insertion_glitch = false;
        m3.bg_fifo.clear();
        m3.obj_fifo.clear();
        m3.bg_fifo.push_bg_row(0, 0);
        m3.fetch = FetchStep::TileT1;
        m3.resume = Resume::LoopTop;
        m3.wait = 0;
        self.lcd_x = 0;
        self.position_in_line = POSITION_START;
        self.run_mode3(vram, oam);
    }

    /// One dot of mode 3.
    pub(crate) fn mode3_dot(&mut self, vram: &[u8], oam: &[u8]) {
        if self.m3.resume == Resume::Idle {
            return;
        }
        if self.m3.wait > 0 {
            self.m3.wait -= 1;
            if self.m3.wait > 0 {
                return;
            }
        }
        self.run_mode3(vram, oam);
    }

    /// Runs pipeline steps from the current resume point until the
    /// next dot boundary (a step that sets `wait` and returns).
    fn run_mode3(&mut self, vram: &[u8], oam: &[u8]) {
        loop {
            match self.m3.resume {
                Resume::Idle => return,
                Resume::LoopTop => {
                    if self.window_trigger_check() {
                        // Activation completes after a one-dot stall.
                        self.m3.resume = Resume::WindowStall;
                        self.m3.wait = 1;
                        return;
                    }
                    self.after_window_check();
                }
                Resume::WindowStall => {
                    self.activate_window();
                    self.after_window_check();
                }
                Resume::ObjCheck => {
                    let m3 = &self.m3;
                    let matches = m3.n_objects != 0
                        && (self.lcdc & lcdc::SPRITES_ENABLE) != 0
                        && self.next_object().x == x_for_object_match(self.position_in_line);
                    self.m3.resume = if matches {
                        Resume::ObjWait
                    } else {
                        Resume::RenderTail
                    };
                }
                Resume::ObjWait => {
                    if self.m3.object_fetch_aborted {
                        self.m3.resume = Resume::RenderTail;
                        continue;
                    }
                    let waiting = self.m3.fetch < FetchStep::HighT2 || self.m3.bg_fifo.len() == 0;
                    self.advance_fetcher(vram);
                    self.m3.resume = if waiting {
                        Resume::ObjWait
                    } else {
                        Resume::ObjExtra
                    };
                    self.m3.wait = 1;
                    return;
                }
                Resume::ObjExtra => {
                    if self.m3.object_fetch_aborted {
                        self.m3.resume = Resume::RenderTail;
                        continue;
                    }
                    self.advance_fetcher(vram);
                    let base = usize::from(self.next_object().index) * 4;
                    self.m3.object_tile = oam.get(base + 2).copied().unwrap_or(0xFF);
                    self.m3.object_flags = oam.get(base + 3).copied().unwrap_or(0xFF);
                    self.m3.resume = Resume::ObjOam;
                    self.m3.wait = 2;
                    return;
                }
                Resume::ObjOam => {
                    if self.m3.object_fetch_aborted {
                        self.m3.resume = Resume::RenderTail;
                        continue;
                    }
                    let addr = self.object_line_address();
                    self.m3.object_data[0] = vram_byte(vram, addr);
                    self.m3.resume = Resume::ObjLow;
                    self.m3.wait = 2;
                    return;
                }
                Resume::ObjLow => {
                    if self.m3.object_fetch_aborted {
                        self.m3.resume = Resume::RenderTail;
                        continue;
                    }
                    self.m3.during_object_fetch = false;
                    let addr = self.object_line_address();
                    self.m3.object_data[1] = vram_byte(vram, addr.wrapping_add(1));
                    self.m3.resume = Resume::ObjHigh;
                    self.m3.wait = 1;
                    return;
                }
                Resume::ObjHigh => {
                    let m3 = &mut self.m3;
                    let flags = m3.object_flags;
                    m3.obj_fifo.overlay_object_row(
                        m3.object_data[0],
                        m3.object_data[1],
                        u8::from((flags & 0x10) != 0),
                        (flags & 0x80) != 0,
                        (flags & 0x20) != 0,
                    );
                    m3.n_objects = m3.n_objects.saturating_sub(1);
                    m3.resume = Resume::ObjCheck;
                }
                Resume::RenderTail => {
                    self.m3.object_fetch_aborted = false;
                    self.m3.during_object_fetch = false;
                    self.render_pixel_if_possible();
                    self.advance_fetcher(vram);
                    if self.position_in_line == SCREEN_WIDTH as u8 {
                        self.finish_mode3();
                    } else {
                        self.m3.resume = Resume::LoopTop;
                        self.m3.wait = 1;
                    }
                    return;
                }
            }
        }
    }

    /// Window trigger, checked once per dot before anything else.
    /// Returns `true` when activation needs the one-dot `WX = 0` stall.
    fn window_trigger_check(&mut self) -> bool {
        let m3 = &self.m3;
        if m3.wx_triggered || !m3.wy_triggered || (self.lcdc & lcdc::WINDOW_ENABLE) == 0 {
            return false;
        }
        let pos = self.position_in_line;
        let wx = self.wx;
        let mut activate = false;
        if wx == 0 {
            activate = pos == 0u8.wrapping_sub(7)
                || (pos == POSITION_START && (self.scx & 7) != 0)
                || (pos >= 0u8.wrapping_sub(15) && pos <= 0u8.wrapping_sub(8));
        } else if wx < 166 {
            if wx == pos.wrapping_add(7) {
                activate = true;
            } else if wx == pos.wrapping_add(6) && !m3.wx_just_changed {
                // The DMG's LCD and PPU fall a pixel out of step here.
                activate = true;
                self.lcd_x = self.lcd_x.saturating_sub(1);
            }
        }

        if activate {
            let m3 = &mut self.m3;
            m3.window_y = m3.window_y.wrapping_add(1);
            m3.window_tile_x = 0;
            m3.bg_fifo.clear();
            if wx == 0 && (self.scx & 7) != 0 {
                return true;
            }
            self.activate_window();
        } else if wx == 166 && wx == pos.wrapping_add(7) {
            self.m3.window_y = self.m3.window_y.wrapping_add(1);
        }
        false
    }

    fn activate_window(&mut self) {
        let m3 = &mut self.m3;
        m3.wx_triggered = true;
        m3.fetch = FetchStep::TileT1;
        m3.window_is_being_fetched = true;
    }

    /// The rest of a loop iteration after the window check: the
    /// pixel-insertion glitch and dropping objects already passed.
    fn after_window_check(&mut self) {
        let pos = self.position_in_line;
        let m3 = &mut self.m3;
        if self.wx == pos.wrapping_add(7)
            && m3.wx_triggered
            && !m3.window_is_being_fetched
            && m3.fetch == FetchStep::TileT1
            && m3.bg_fifo.len() == 8
        {
            m3.insert_bg_pixel = true;
        }

        let x_match = x_for_object_match(pos);
        while m3.n_objects != 0 && m3.objects[usize::from(m3.n_objects) - 1].x < x_match {
            m3.n_objects -= 1;
        }
        m3.during_object_fetch = true;
        m3.resume = Resume::ObjCheck;
    }

    fn next_object(&self) -> VisibleObject {
        let n = usize::from(self.m3.n_objects);
        self.m3.objects[n.saturating_sub(1)]
    }

    /// VRAM address of the current object's low bitplane for this
    /// line. The object height is read from `LCDC` at fetch time.
    fn object_line_address(&self) -> u16 {
        let height_16 = (self.lcdc & lcdc::SPRITE_HEIGHT_16) != 0;
        let mask = if height_16 { 0x0F } else { 0x07 };
        let object = self.next_object();
        let mut tile_y = self.ly.wrapping_sub(object.y) & mask;
        if (self.m3.object_flags & 0x40) != 0 {
            tile_y ^= mask;
        }
        let tile = if height_16 {
            self.m3.object_tile & 0xFE
        } else {
            self.m3.object_tile
        };
        u16::from(tile) * 16 + u16::from(tile_y) * 2
    }

    /// Pops a pixel from the FIFOs, mixes it and (once past the
    /// scroll discard) writes it to the framebuffer.
    fn render_pixel_if_possible(&mut self) {
        let objects_enabled = (self.lcdc & lcdc::SPRITES_ENABLE) != 0;
        let m3 = &mut self.m3;
        // An object at X = 0 that is still pending holds the shifter.
        if m3.n_objects != 0 && objects_enabled && m3.objects[usize::from(m3.n_objects) - 1].x == 0
        {
            return;
        }
        if m3.bg_fifo.len() == 0 {
            return;
        }

        let bg = if m3.insert_bg_pixel {
            m3.insert_bg_pixel = false;
            crate::fifo::FifoItem::default()
        } else {
            m3.bg_fifo.pop()
        };
        let mut bg_priority = false;
        let mut object = None;
        if m3.obj_fifo.len() != 0 {
            let item = m3.obj_fifo.pop();
            if item.pixel != 0 && objects_enabled {
                bg_priority = item.bg_priority;
                object = Some(item);
            }
        }

        let pos = self.position_in_line;
        if pos.wrapping_add(16) < 8 {
            // Warm-up: the first pixel whose low bits match SCX & 7
            // ends the fine-scroll discard.
            // (A window starting during the discard also ends it one
            // pixel early when SCX & 7 = 7.)
            let window_early_end =
                m3.window_is_being_fetched && (pos & 7) == 6 && (self.scx & 7) == 7;
            if (pos & 7) == (self.scx & 7) || window_early_end {
                self.position_in_line = 0u8.wrapping_sub(8);
            } else if pos == 0u8.wrapping_sub(9) {
                self.position_in_line = POSITION_START;
                return;
            }
        }
        m3.window_is_being_fetched = false;

        if self.position_in_line >= SCREEN_WIDTH as u8 {
            self.position_in_line = self.position_in_line.wrapping_add(1);
            return;
        }

        let bg_index = if (self.lcdc & lcdc::BG_ENABLE) != 0 {
            bg.pixel
        } else {
            0
        };
        let mut shade = apply_palette(self.bgp, bg_index);
        if let Some(item) = object
            && !(bg_index != 0 && bg_priority)
        {
            let palette = if item.palette != 0 {
                self.obp1
            } else {
                self.obp0
            };
            shade = apply_palette(palette, item.pixel);
        }
        self.put_pixel(shade);
        self.position_in_line = self.position_in_line.wrapping_add(1);
        self.lcd_x = self.lcd_x.wrapping_add(1);
    }

    fn put_pixel(&mut self, shade: u8) {
        if self.ly < crate::VBLANK_START && self.lcd_x < SCREEN_WIDTH as u8 {
            let idx = usize::from(self.ly) * SCREEN_WIDTH as usize + usize::from(self.lcd_x);
            self.framebuffer[idx] = shade;
        }
    }

    /// End of mode 3: fill any pixels the LCD missed (an LCD/PPU
    /// desync), then settle the window state for the next line.
    pub(crate) fn finish_mode3(&mut self) {
        self.position_in_line = POSITION_START;
        while self.lcd_x < SCREEN_WIDTH as u8 {
            let shade = if self.lcd_x == 0 || self.ly >= crate::VBLANK_START {
                0
            } else {
                let idx = usize::from(self.ly) * SCREEN_WIDTH as usize + usize::from(self.lcd_x);
                self.framebuffer[idx - 1]
            };
            self.put_pixel(shade);
            self.lcd_x += 1;
        }
        let m3 = &mut self.m3;
        if self.ly == crate::VBLANK_START - 1 {
            m3.window_y = 0xFF;
        }
        if m3.wy_triggered && (self.lcdc & lcdc::WINDOW_ENABLE) != 0 && self.wx == 166 {
            m3.wx_triggered = true;
            m3.window_tile_x = 1;
            m3.window_y = m3.window_y.wrapping_add(1);
        } else {
            m3.wx_triggered = false;
        }
        m3.resume = Resume::Idle;
        m3.wait = 0;
    }

    /// Fetcher Y: the window line counter, or `LY + SCY` (read live).
    fn fetcher_y(&self) -> u8 {
        if self.m3.wx_triggered {
            self.m3.window_y
        } else {
            self.ly.wrapping_add(self.scy)
        }
    }

    /// Bitplane address from the live `TILE_SEL` and fetcher Y.
    fn tile_data_address(&self) -> u16 {
        let base = if (self.lcdc & lcdc::BG_TILE_DATA_UNSIGNED) != 0 {
            u16::from(self.m3.current_tile) * 16
        } else {
            (0x1000_i32 + i32::from(self.m3.current_tile as i8) * 16) as u16
        };
        base + u16::from(self.fetcher_y() & 7) * 2
    }

    /// One step of the background/window fetcher.
    fn advance_fetcher(&mut self, vram: &[u8]) {
        match self.m3.fetch {
            FetchStep::TileT1 => {
                if !self.fetcher_window_enabled() {
                    self.m3.wx_triggered = false;
                }
                let window = self.m3.wx_triggered;
                let map: u16 = if (!window && (self.lcdc & lcdc::BG_TILE_MAP) != 0)
                    || (window && (self.lcdc & lcdc::WINDOW_TILE_MAP) != 0)
                {
                    0x1C00
                } else {
                    0x1800
                };
                let y = self.fetcher_y();
                let pos = self.position_in_line;
                let x = if window {
                    self.m3.window_tile_x
                } else if pos.wrapping_add(16) < 8 {
                    self.scx >> 3
                } else {
                    (((u16::from(self.scx) + u16::from(pos) + 8) / 8) & 0x1F) as u8
                };
                self.m3.tile_index_addr = map + u16::from(x) + u16::from(y / 8) * 32;
                self.m3.fetch = FetchStep::TileT2;
            }
            FetchStep::TileT2 => {
                self.m3.current_tile = vram_byte(vram, self.m3.tile_index_addr);
                self.m3.fetch = FetchStep::LowT1;
            }
            FetchStep::LowT1 => {
                self.m3.tile_data_addr = self.tile_data_address();
                self.m3.fetch = FetchStep::LowT2;
            }
            FetchStep::LowT2 => {
                self.m3.tile_data[0] = vram_byte(vram, self.m3.tile_data_addr);
                self.m3.fetch = FetchStep::HighT1;
            }
            FetchStep::HighT1 => {
                self.m3.tile_data_addr = self.tile_data_address() + 1;
                self.m3.fetch = FetchStep::HighT2;
            }
            FetchStep::HighT2 => {
                self.m3.tile_data[1] = vram_byte(vram, self.m3.tile_data_addr);
                if self.m3.wx_triggered {
                    self.m3.window_tile_x = (self.m3.window_tile_x + 1) & 0x1F;
                }
                self.fetcher_push();
            }
            FetchStep::Push => self.fetcher_push(),
        }
    }

    fn fetcher_push(&mut self) {
        let pos = self.position_in_line;
        let m3 = &mut self.m3;
        m3.fetch = FetchStep::Push;
        if m3.bg_fifo.len() > 0 {
            return;
        }
        if m3.wy_triggered
            && (self.lcdc & lcdc::WINDOW_ENABLE) == 0
            && !m3.disable_window_pixel_insertion_glitch
        {
            // The window was switched off mid-line: if WX matches the
            // position, one blank pixel is pushed instead of a tile.
            let mut logical = pos.wrapping_add(7);
            if logical > 167 {
                logical = 0;
            }
            if self.wx == logical {
                m3.bg_fifo.push_single_blank();
                return;
            }
        }
        m3.bg_fifo.push_bg_row(m3.tile_data[0], m3.tile_data[1]);
        m3.fetch = FetchStep::TileT1;
    }
}

/// The X an object must have to be fetched at shifter position `pos`.
/// Through the warm-up every position maps to 0.
fn x_for_object_match(pos: u8) -> u8 {
    let x = pos.wrapping_add(8);
    if x > 0u8.wrapping_sub(16) { 0 } else { x }
}

fn vram_byte(vram: &[u8], addr: u16) -> u8 {
    vram.get(usize::from(addr & 0x1FFF))
        .copied()
        .unwrap_or(0xFF)
}
