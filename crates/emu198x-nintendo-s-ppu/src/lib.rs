//! The SNES picture processor: S-PPU1 (5C77) and S-PPU2 (5C78), modelled as
//! one chip because the console never separates them.
//!
//! # Clocking
//!
//! The host calls [`SPpu::tick`] once per dot, which is four master clocks.
//! Inside, the chip runs in two-clock steps, the unit its own counters move
//! in. Each step does the work the hardware does at that point in the line:
//!
//! - every 8 clocks from H=0 to H=1016, one sprite is checked against the
//!   next line (sprite evaluation);
//! - every 4 clocks from H=0 to H=1054, one background VRAM fetch, in the
//!   order the current mode uses;
//! - from H=56 to H=1078, one pixel every 4 clocks: the sub-screen half at
//!   56+4n and the main-screen half, the sprites, the windows and the colour
//!   math at 58+4n;
//! - from H=1080, the sprite tiles found by evaluation are fetched for the
//!   next line, two VRAM words per 8-pixel tile, 4 clocks apart.
//!
//! So a register written between ticks takes effect at the next dot it
//! touches. Scroll, mode, window, colour math and Mode 7 changes split a line
//! where they are written, and a VRAM, OAM or CGRAM access lands where the
//! hardware would place it.
//!
//! # The counters
//!
//! A line is 1364 master clocks (341 dots). On NTSC, the line at V=240 of an
//! odd non-interlaced field is 1360; on PAL, V=311 of an odd interlaced field
//! is 1368. Dots 323 and 327 are 6 clocks long on ordinary lines, which only
//! shows in the latched H counter ([`SPpu::hdot`]). A frame is 262 lines
//! (NTSC) or 312 (PAL), plus one on even interlaced fields.
//!
//! # Pins for the 5A22
//!
//! The CPU's NMI, IRQ comparator, HDMA and HVBJOY all work from the PPU's
//! position: [`SPpu::hcounter`], [`SPpu::vcounter`], [`SPpu::hblank`] and
//! [`SPpu::vblank`]. The CPU's WRIO bit 7 drives the external latch pin,
//! [`SPpu::set_ext_latch`]; a falling edge latches the counters, as does a
//! read of `$2137` while it is high.
//!
//! # Output
//!
//! [`SPpu::framebuffer`] is 512 × 480 ARGB8888, two entries per 256-pixel
//! dot so hi-res modes fit: in hi-res the sub-screen pixel is first and the
//! main-screen pixel second, otherwise both are the main screen. Rows follow
//! the PPU's own layout: a progressive frame fills rows 0–239 with the
//! picture centred 8 rows down when overscan is off, and an interlaced frame
//! uses 2 rows per line, the field picking which. [`SPpu::framebuffer_raw`]
//! holds the same pixels as the chip's output word: brightness in bits
//! 15–18 and a 15-bit BGR colour below.
//!
//! The ARGB conversion is linear in brightness, (1 + brightness) / 16 of the
//! colour, except brightness 0, which ares measures as about a quarter of
//! brightness 1 rather than black. No gamma curve is applied.
//!
//! # Power-on
//!
//! Real hardware powers on with VRAM, CGRAM and most registers holding
//! whatever the cells settled to. This model starts them at zero, with the
//! display force-blanked and the VRAM increment at one word. Software that
//! initialises the PPU, as every game does, cannot tell.
//!
//! # Provenance
//!
//! The model follows the accurate PPU in ares (ISC licence,
//! <https://github.com/ares-emulator/ares>, `ares/sfc/ppu/`, by Near and the
//! ares team), re-expressed in Rust and driven by a dot clock, and is checked
//! against it by the differential harness in `tests/oracle/`. Where ares and
//! bsnes disagree, bsnes was taken as the fact: in Mode 4, a vertical
//! offset-per-tile entry adds the BG3 word, as bsnes does, where ares adds
//! the unfetched vertical word (always zero). The register map and timing
//! were checked against `198x/reference/by-topic/ppu-snes/`.

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

mod background;
mod dac;
mod io;
mod object;
mod window;

#[cfg(test)]
mod tests;

use background::Background;
use dac::Dac;
use object::{FetchState, OamObject, Object};
use window::Window;

/// Framebuffer width: two entries per dot.
pub const FRAME_WIDTH: usize = 512;
/// Framebuffer height: 240 rows, doubled for interlace.
pub const FRAME_HEIGHT: usize = 480;

/// Master clocks in an ordinary line.
pub const LINE_CLOCKS: u32 = 1364;
/// Master clocks per [`SPpu::tick`].
pub const DOT_CLOCKS: u32 = 4;

/// The console's video standard, which sets the frame length and STAT78's
/// region bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Region {
    Ntsc,
    Pal,
}

/// The H/V counters, in master clocks and lines.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Counter {
    interlace: bool,
    field: bool,
    vperiod: u32,
    hperiod: u32,
    vcounter: u32,
    hcounter: u32,
    last_vperiod: u32,
    last_hperiod: u32,
}

impl Counter {
    fn new(region: Region) -> Self {
        Self {
            interlace: false,
            field: false,
            vperiod: if region == Region::Pal { 312 } else { 262 },
            hperiod: LINE_CLOCKS,
            vcounter: 0,
            hcounter: 0,
            last_vperiod: 0,
            last_hperiod: 0,
        }
    }

    /// Two master clocks. `interlace` is the PPU's frame interlace setting,
    /// which the counter samples at V=128 to size the frame.
    fn tick(&mut self, region: Region, interlace: bool) {
        self.hcounter += 2;
        if self.hcounter >= self.hperiod {
            self.last_hperiod = self.hperiod;
            self.hcounter -= self.hperiod;
            self.tick_scanline(region, interlace);
        }
    }

    fn tick_scanline(&mut self, region: Region, interlace: bool) {
        self.vcounter += 1;
        if self.vcounter == 128 {
            self.interlace = interlace;
            self.vperiod += u32::from(self.interlace && !self.field);
        }
        if self.vcounter == self.vperiod {
            self.last_vperiod = self.vperiod;
            self.vperiod = if region == Region::Pal { 312 } else { 262 };
            self.vcounter = 0;
            self.field = !self.field;
        }
        self.hperiod = LINE_CLOCKS;
        if region == Region::Ntsc && !self.interlace && self.field && self.vcounter == 240 {
            self.hperiod -= 4;
        }
        if region == Region::Pal && self.interlace && self.field && self.vcounter == 311 {
            self.hperiod += 4;
        }
    }

    fn hdot(&self) -> u32 {
        let h = self.hcounter;
        if self.hperiod == 1360 {
            h >> 2
        } else {
            (h - (u32::from(h > 1292) << 1) - (u32::from(h > 1310) << 1)) >> 2
        }
    }
}

/// One PPU's open bus and version.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
struct Chip {
    version: u8,
    mdr: u8,
}

/// Write latches and the internal addresses the renderer leaves behind.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
struct Latch {
    vram: u16,
    oam: u8,
    cgram: u8,
    bgofs_ppu1: u8,
    bgofs_ppu2: u8,
    mode7: u8,
    counters: bool,
    hcounter: bool,
    vcounter: bool,
    /// The sprite the renderer last touched, which OAM accesses during
    /// active display are redirected to.
    oam_address: u16,
    /// The palette entry the renderer last read, which CGRAM accesses
    /// during active display are redirected to.
    cgram_address: u8,
}

/// The registers that are not owned by a single layer.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
struct Io {
    display_brightness: u8,
    display_disable: bool,
    oam_base_address: u16,
    oam_address: u16,
    oam_priority: bool,
    bg_mode: u8,
    bg_priority: bool,
    hoffset_mode7: u16,
    voffset_mode7: u16,
    vram_increment_size: u16,
    vram_mapping: u8,
    vram_increment_mode: bool,
    vram_address: u16,
    hflip_mode7: bool,
    vflip_mode7: bool,
    repeat_mode7: u8,
    m7a: u16,
    m7b: u16,
    m7c: u16,
    m7d: u16,
    m7x: u16,
    m7y: u16,
    cgram_address: u8,
    cgram_address_latch: bool,
    interlace: bool,
    overscan: bool,
    pseudo_hires: bool,
    extbg: bool,
    hcounter: u16,
    vcounter: u16,
}

/// Settings sampled at the top of each frame.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
struct State {
    interlace: bool,
    overscan: bool,
    vdisp: u32,
}

/// The vertical mosaic counter shared by the backgrounds.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
struct Mosaic {
    size: u8,
    vcounter: u8,
}

/// The SNES picture processor pair.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SPpu {
    region: Region,
    counter: Counter,
    vram: Vec<u16>,
    vram_mask: u32,
    oam: Vec<OamObject>,
    #[serde(with = "BigArray")]
    cgram: [u16; 256],
    ppu1: Chip,
    ppu2: Chip,
    latch: Latch,
    io: Io,
    state: State,
    mosaic: Mosaic,
    bg: [Background; 4],
    obj: Object,
    fetch: FetchState,
    window: Window,
    dac: Dac,
    /// The external latch pin, driven by the 5A22's WRIO bit 7. High at
    /// power-on, as WRIO resets to `$FF`.
    ext_latch: bool,
    raw: Vec<u32>,
    argb: Vec<u32>,
    frames: u64,
    frame_ready: bool,
}

/// ARGB components for each brightness and 5-bit level.
fn level(brightness: u32, component: u32) -> u32 {
    let scale = if brightness == 0 {
        0.25 / 16.0
    } else {
        (1.0 + f64::from(brightness)) / 16.0
    };
    (scale * f64::from(component) * 255.0 / 31.0).round() as u32
}

impl SPpu {
    /// A PPU in the power-on state described in the crate documentation.
    #[must_use]
    pub fn new(region: Region) -> Self {
        let mut ppu = Self {
            region,
            counter: Counter::new(region),
            vram: vec![0; 0x10000],
            vram_mask: 0x7fff,
            oam: vec![OamObject::default(); 128],
            cgram: [0; 256],
            ppu1: Chip { version: 1, mdr: 0 },
            ppu2: Chip { version: 3, mdr: 0 },
            latch: Latch::default(),
            io: Io {
                display_disable: true,
                vram_increment_size: 1,
                ..Io::default()
            },
            state: State::default(),
            mosaic: Mosaic {
                size: 1,
                vcounter: 0,
            },
            bg: [
                Background::new(),
                Background::new(),
                Background::new(),
                Background::new(),
            ],
            obj: Object::new(),
            fetch: FetchState::default(),
            window: Window::default(),
            dac: Dac::default(),
            ext_latch: true,
            raw: vec![0; FRAME_WIDTH * FRAME_HEIGHT],
            argb: vec![0xFF00_0000; FRAME_WIDTH * FRAME_HEIGHT],
            frames: 0,
            frame_ready: false,
        };
        ppu.update_video_mode();
        ppu
    }

    /// Advance one dot: four master clocks.
    pub fn tick(&mut self) {
        self.half_step();
        self.half_step();
    }

    fn half_step(&mut self) {
        let h = self.counter.hcounter;
        let v = self.counter.vcounter;
        if h == 0 {
            self.line_start();
        }
        if v <= 240 {
            if h <= 1078 {
                self.cycle(h);
            } else {
                if h == 1080 {
                    self.obj_fetch_begin();
                }
                self.obj_fetch_step();
            }
        }
        self.counter.tick(self.region, self.state.interlace);
    }

    /// The work done at the top of every line.
    fn line_start(&mut self) {
        let v = self.counter.vcounter;
        // A fetch still running when the line ends stops here; see
        // `object::obj_fetch_abandon`.
        self.obj_fetch_abandon();
        if v == 0 {
            self.state.interlace = self.io.interlace;
            self.state.overscan = self.io.overscan;
            self.obj.io.time_over = false;
            self.obj.io.range_over = false;
        }
        self.mosaic_scanline();
        for id in 0..4 {
            self.bg_scanline(id);
        }
        self.obj_scanline();
        self.window_scanline();
        self.dac_scanline();
        if v == 240 {
            self.frames += 1;
            self.frame_ready = true;
        }
    }

    /// The work done at master clock `h` of a rendered line (V=0–240).
    fn cycle(&mut self, h: u32) {
        if h <= 1016 && h.is_multiple_of(8) {
            self.obj_evaluate((h >> 3) as u8);
        }
        if h <= 1054 && h.is_multiple_of(4) {
            self.bg_fetch_slot((h >> 2) & 7);
        }
        if h == 56 {
            for id in 0..4 {
                self.bg_begin(id);
            }
        }
        if (56..=1078).contains(&h) {
            match (h - 56) % 4 {
                0 => {
                    for id in 0..4 {
                        self.bg_run(id, true);
                    }
                }
                2 => {
                    for id in 0..4 {
                        self.bg_run(id, false);
                    }
                    self.obj_run();
                    self.window_run();
                    self.dac_run();
                }
                _ => {}
            }
        }
    }

    /// The background fetch for slot `slot` (0–7) of each 8-dot group, in
    /// the order the mode uses.
    fn bg_fetch_slot(&mut self, slot: u32) {
        const NT: u8 = 0;
        const OFF0: u8 = 1;
        const OFF8: u8 = 2;
        // (layer, action, character index, half)
        let fetch: Option<(usize, u8, u32, u32)> = match (self.io.bg_mode, slot) {
            (0, 0) => Some((3, NT, 0, 0)),
            (0, 1) => Some((2, NT, 0, 0)),
            (0, 2) => Some((1, NT, 0, 0)),
            (0, 3) => Some((0, NT, 0, 0)),
            (0, 4) => Some((3, 3, 0, 0)),
            (0, 5) => Some((2, 3, 0, 0)),
            (0, 6) => Some((1, 3, 0, 0)),
            (0, 7) => Some((0, 3, 0, 0)),
            (1, 0) => Some((2, NT, 0, 0)),
            (1, 1) => Some((1, NT, 0, 0)),
            (1, 2) => Some((0, NT, 0, 0)),
            (1, 3) => Some((2, 3, 0, 0)),
            (1, 4) => Some((1, 3, 0, 0)),
            (1, 5) => Some((1, 3, 1, 0)),
            (1, 6) => Some((0, 3, 0, 0)),
            (1, 7) => Some((0, 3, 1, 0)),
            (2..=6, 0) => Some((1, NT, 0, 0)),
            (2..=6, 1) => Some((0, NT, 0, 0)),
            (2, 2) | (4, 2) | (6, 2) => Some((2, OFF0, 0, 0)),
            (2, 3) | (6, 3) => Some((2, OFF8, 0, 0)),
            (2, 4) => Some((1, 3, 0, 0)),
            (2, 5) => Some((1, 3, 1, 0)),
            (2, 6) => Some((0, 3, 0, 0)),
            (2, 7) => Some((0, 3, 1, 0)),
            (3, 2) => Some((1, 3, 0, 0)),
            (3, 3) => Some((1, 3, 1, 0)),
            (3, 4) => Some((0, 3, 0, 0)),
            (3, 5) => Some((0, 3, 1, 0)),
            (3, 6) => Some((0, 3, 2, 0)),
            (3, 7) => Some((0, 3, 3, 0)),
            (4, 3) => Some((1, 3, 0, 0)),
            (4, 4) => Some((0, 3, 0, 0)),
            (4, 5) => Some((0, 3, 1, 0)),
            (4, 6) => Some((0, 3, 2, 0)),
            (4, 7) => Some((0, 3, 3, 0)),
            (5, 2) => Some((1, 3, 0, 0)),
            (5, 3) => Some((1, 3, 0, 1)),
            (5, 4) | (6, 4) => Some((0, 3, 0, 0)),
            (5, 5) | (6, 5) => Some((0, 3, 1, 0)),
            (5, 6) | (6, 6) => Some((0, 3, 0, 1)),
            (5, 7) | (6, 7) => Some((0, 3, 1, 1)),
            _ => None,
        };
        match fetch {
            Some((id, NT, _, _)) => self.bg_fetch_name_table(id),
            Some((id, OFF0, _, _)) => self.bg_fetch_offset(id, 0),
            Some((id, OFF8, _, _)) => self.bg_fetch_offset(id, 8),
            Some((id, _, index, half)) => self.bg_fetch_character(id, index, half),
            None => {}
        }
    }

    // -- Pins and accessors ------------------------------------------------

    /// The video standard this PPU was built for.
    #[must_use]
    pub fn region(&self) -> Region {
        self.region
    }

    /// Master clocks into the current line, 0 up to 1360, 1364 or 1368.
    #[must_use]
    pub fn hcounter(&self) -> u32 {
        self.counter.hcounter
    }

    /// The current line.
    #[must_use]
    pub fn vcounter(&self) -> u32 {
        self.counter.vcounter
    }

    /// The H position in dots, as the counter latch reads it.
    #[must_use]
    pub fn hdot(&self) -> u32 {
        self.counter.hdot()
    }

    /// The field: toggles every frame.
    #[must_use]
    pub fn field(&self) -> bool {
        self.counter.field
    }

    /// Master clocks in the current line.
    #[must_use]
    pub fn hperiod(&self) -> u32 {
        self.counter.hperiod
    }

    /// Whether the counters are running an interlaced frame (the interlace
    /// setting sampled at V=128).
    #[must_use]
    pub fn interlaced(&self) -> bool {
        self.counter.interlace
    }

    /// The first line of vertical blank: 225, or 240 with overscan set.
    #[must_use]
    pub fn vdisp(&self) -> u32 {
        self.state.vdisp
    }

    /// The HBLANK line, as HVBJOY bit 6 reads it.
    #[must_use]
    pub fn hblank(&self) -> bool {
        self.counter.hcounter <= 2 || self.counter.hcounter >= 1096
    }

    /// The VBLANK line, as HVBJOY bit 7 reads it.
    #[must_use]
    pub fn vblank(&self) -> bool {
        self.counter.vcounter >= self.state.vdisp
    }

    /// Whether the display is force-blanked (INIDISP bit 7).
    #[must_use]
    pub fn force_blank(&self) -> bool {
        self.io.display_disable
    }

    /// The external latch pin, driven by the 5A22's WRIO bit 7. A falling
    /// edge latches the H/V counters.
    pub fn set_ext_latch(&mut self, level: bool) {
        if self.ext_latch && !level {
            self.latch_counters();
        }
        self.ext_latch = level;
    }

    /// Frames completed: the count steps as the PPU reaches V=240.
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// True once per frame, when V=240 is reached; reading clears it.
    pub fn take_frame_ready(&mut self) -> bool {
        std::mem::take(&mut self.frame_ready)
    }

    /// The picture, ARGB8888, [`FRAME_WIDTH`] × [`FRAME_HEIGHT`].
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.argb
    }

    /// The picture as the chip outputs it: brightness in bits 15–18 above a
    /// 15-bit BGR colour.
    #[must_use]
    pub fn framebuffer_raw(&self) -> &[u32] {
        &self.raw
    }

    /// Choose 128 KB of VRAM rather than 64 KB, as some development units
    /// carried.
    pub fn set_vram_128k(&mut self, enabled: bool) {
        self.vram_mask = if enabled { 0xffff } else { 0x7fff };
    }

    /// One VRAM word, without side effects.
    #[must_use]
    pub fn vram(&self, address: u16) -> u16 {
        self.vram_word(u32::from(address))
    }

    /// Set one VRAM word directly, for loaders and tests.
    pub fn poke_vram(&mut self, address: u16, value: u16) {
        let index = (u32::from(address) & self.vram_mask) as usize;
        self.vram[index] = value;
    }

    /// One CGRAM entry, 15-bit BGR.
    #[must_use]
    pub fn cgram(&self, index: u8) -> u16 {
        self.cgram[usize::from(index)]
    }

    /// Set one CGRAM entry directly.
    pub fn poke_cgram(&mut self, index: u8, value: u16) {
        self.cgram[usize::from(index)] = value & 0x7fff;
    }

    /// One OAM byte as `$2138` would read it, without side effects:
    /// 0–511 the low table, 512–543 the high table.
    #[must_use]
    pub fn oam(&self, address: u16) -> u8 {
        object::oam_read(&self.oam, address & 0x3ff)
    }

    /// Set one OAM byte directly.
    pub fn poke_oam(&mut self, address: u16, value: u8) {
        object::oam_write(&mut self.oam, address & 0x3ff, value);
    }

    fn vram_word(&self, address: u32) -> u16 {
        self.vram[(address & self.vram_mask) as usize]
    }

    /// The pixel written by the DAC, raw and converted.
    fn put_pixel(&mut self, index: usize, value: u32) {
        let brightness = (value >> 15) & 15;
        let colour = value & 0x7fff;
        let r = level(brightness, colour & 31);
        let g = level(brightness, (colour >> 5) & 31);
        let b = level(brightness, (colour >> 10) & 31);
        self.raw[index] = value;
        self.argb[index] = 0xFF00_0000 | r << 16 | g << 8 | b;
    }
}
