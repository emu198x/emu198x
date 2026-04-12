//! Shared Spectrum-family ULA rendering engine.
//!
//! Source references:
//! - `wiki/systems/spectrum/overview.md`
//! - `wiki/systems/spectrum/contention.md`
//! - Adapted from `/Users/stevehill/Projects/Emu198x-Older/crates/common-sinclair-zx-spectrum/src/ula_engine.rs`
//!
//! Shared ULA rendering engine — the common display logic across all
/// Spectrum ULA variants (Ferranti, Sinclair 7K, Amstrad 40077, etc.).
///
/// Each variant configures this with its timing constants. The engine
/// handles video fetch, pixel output, interrupt timing, and flash. The
/// variant-specific ULA wrapper adds contention and I/O handling.
use crate::memory::MemoryBus;
use crate::palette;
use crate::timing::{SCREEN_HEIGHT, SCREEN_WIDTH};

/// Timing and layout constants for a specific ULA variant.
#[derive(Clone, Debug)]
pub struct UlaConfig {
    /// ULA clocks per scanline (448 for 48K, 456 for 128K).
    pub pixels_per_line: u16,
    /// Scanlines per frame (312 for 48K, 311 for 128K).
    pub lines_per_frame: u16,

    /// Pixel position where video fetch begins (always 8 — prefetch cell).
    pub fetch_start: u16,
    /// Pixel position where video fetch ends (always 264).
    pub fetch_end: u16,
    /// Pixel position where screen data starts rendering (always 12 — pipeline delay).
    pub screen_start: u16,
    /// Pixel position where screen data ends (screen_start + 256).
    pub screen_end: u16,

    /// Scanline where INT is first asserted.
    pub int_scan: u16,
    /// Pixel within int_scan where INT asserts.
    pub int_start_pixel: u16,
    /// Pixel within int_scan where INT deasserts.
    pub int_end_pixel: u16,

    /// Framebuffer X offset: fb_x = pixel + fb_x_offset (for pixels < fb_x_cutoff).
    pub fb_x_offset: i32,
    /// Pixel value above which fb_x wraps: fb_x = pixel - fb_x_wrap_offset.
    pub fb_x_wrap_start: u16,
    /// Offset for wrapped pixels.
    pub fb_x_wrap_offset: i32,
    /// Pixel value above which fb_x = -1 (HBlank).
    pub fb_x_hblank_start: u16,

    /// Bottom border starts at this scan line.
    pub bottom_border_start: u16,
    /// VSync starts at this scan line (fb_y = -1).
    pub vsync_start: u16,
    /// VSync ends at this scan line (top border resumes).
    pub vsync_end: u16,
}

/// 48K timing (Ferranti 6C001E): 448 pixels/line, 312 lines.
pub const CONFIG_48K: UlaConfig = UlaConfig {
    pixels_per_line: 448,
    lines_per_frame: 312,
    fetch_start: 8,
    fetch_end: 264,
    screen_start: 12,
    screen_end: 12 + 256,
    int_scan: 248,
    int_start_pixel: 1,
    int_end_pixel: 65,
    fb_x_offset: 36,
    fb_x_wrap_start: 412,
    fb_x_wrap_offset: 412,
    fb_x_hblank_start: 316,
    bottom_border_start: 192,
    vsync_start: 248,
    vsync_end: 264,
};

/// 128K / +2 timing (Sinclair 7K010E): 456 pixels/line, 311 lines.
pub const CONFIG_128K: UlaConfig = UlaConfig {
    pixels_per_line: 456,
    lines_per_frame: 311,
    fetch_start: 8,
    fetch_end: 264,
    screen_start: 12,
    screen_end: 12 + 256,
    int_scan: 248,
    int_start_pixel: 1,
    int_end_pixel: 65,
    fb_x_offset: 36,
    fb_x_wrap_start: 420, // 456 - 36
    fb_x_wrap_offset: 420,
    fb_x_hblank_start: 320, // 456 - 136 (wider HBlank)
    bottom_border_start: 192,
    vsync_start: 248,
    vsync_end: 263,
};

/// +2A / +2B / +3 timing (Amstrad 40077): same as 128K.
pub const CONFIG_PLUS2A: UlaConfig = CONFIG_128K;

/// TS2068 NTSC timing: 448 pixels/line, 262 lines, 14.112 MHz crystal.
pub const CONFIG_TS2068: UlaConfig = UlaConfig {
    pixels_per_line: 448,
    lines_per_frame: 262,
    fetch_start: 8,
    fetch_end: 264,
    screen_start: 12,
    screen_end: 12 + 256,
    int_scan: 224, // NTSC: fewer lines, INT earlier
    int_start_pixel: 1,
    int_end_pixel: 65,
    fb_x_offset: 36,
    fb_x_wrap_start: 412,
    fb_x_wrap_offset: 412,
    fb_x_hblank_start: 316,
    bottom_border_start: 192,
    vsync_start: 224,
    vsync_end: 230,
};

/// Pentagon timing: 448 pixels/line (same as 48K), 320 lines (extra VBlank).
/// No contention — the ULA never withholds the CPU clock.
pub const CONFIG_PENTAGON: UlaConfig = UlaConfig {
    pixels_per_line: 448,
    lines_per_frame: 320,
    fetch_start: 8,
    fetch_end: 264,
    screen_start: 12,
    screen_end: 12 + 256,
    int_scan: 256, // Pentagon INT is later than Sinclair (line 256, not 248)
    int_start_pixel: 1,
    int_end_pixel: 65,
    fb_x_offset: 36,
    fb_x_wrap_start: 412,
    fb_x_wrap_offset: 412,
    fb_x_hblank_start: 316,
    bottom_border_start: 192,
    vsync_start: 256,
    vsync_end: 272,
};

/// The rendering and timing state shared by all ULA variants.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct UlaEngine {
    /// Current pixel position within the scanline.
    pub pixel: u16,
    /// Current scanline.
    pub scan: u16,
    /// Border colour (0-7).
    pub border: u8,
    /// Beeper state (bit 4 of port 0xFE).
    pub beeper: bool,
    /// MIC state (bit 3 of port 0xFE).
    pub mic: bool,
    /// Flash counter: toggles every 16 frames (counts 0..31).
    pub flash_counter: u8,
    /// Flash state: true when ink/paper should be swapped.
    pub flash_active: bool,
    /// CPU clock output: true = CPU may tick.
    pub cpu_clock: bool,
    /// Z80 internal clock phase. Toggles when CPU ticks.
    pub z80_clock_high: bool,
    /// Interrupt signal.
    pub int_active: bool,
    /// Current byte on the ULA data bus (floating bus).
    pub bus_data: u8,
    /// Is the ULA idle (not reading VRAM)?
    pub idle: bool,
    // Video fetch state
    pub data_reg: u8,
    pub attr_reg: u8,
    pub data_latch: u8,
    pub attr_latch: u8,
    pub data_addr: u16,
    pub attr_addr: u16,
    /// Active video (scan < 192 and within fetch range).
    pub video: bool,
    /// Border area flag.
    pub border_active: bool,
    // Contention tracking
    pub z80_mreq_prev: bool,
    pub z80_iorq_prev: bool,
    pub z80_iorq_prev2: bool,

    /// Framebuffer width (352 for standard, 704 for Timex hi-res capable).
    pub fb_width: usize,

    /// SCLD video mode (port $FF bits 0-2, Timex only). 0 = standard.
    /// Bit 0: dual-screen, Bit 1: hi-colour (8×1 attrs), Bit 2: hi-res (512×192).
    pub scld_mode: u8,
    /// SCLD hi-res ink colour (port $FF bits 3-5, Timex only).
    pub scld_hires_ink: u8,
    /// Second data register for hi-res mode (odd columns from $6000).
    pub data_reg2: u8,
    pub data_latch2: u8,

    /// Timing configuration for this variant.
    /// Skipped during serialization — the owning machine must re-set this
    /// after deserialization via `set_config()`.
    #[serde(skip, default = "default_config")]
    config: &'static UlaConfig,
}

/// VRAM read pattern: indexed by pixel & 0x0F.
/// false = ULA reads from VRAM this clock.
pub const MEM_TABLE: [bool; 16] = [
    true, true, true, true, true, true, true, true, false, true, false, true, false, true, false,
    true,
];

/// Idle table: indexed by pixel & 0x0F.
/// true = ULA is idle (floating bus returns 0xFF).
pub const IDLE_TABLE: [bool; 16] = [
    true, true, true, true, true, true, true, true, false, false, false, false, false, false,
    false, false,
];

/// 48K/128K contention delay table.
pub const DELAY_TABLE_48K: [bool; 16] = [
    false, false, false, true, true, true, true, true, true, true, true, true, true, true, true,
    false,
];

/// +2A/+3 contention delay table — different pattern.
pub const DELAY_TABLE_PLUS2A: [bool; 16] = [
    true, false, false, false, false, false, false, false, false, false, false, false, false,
    false, true, true,
];

/// Default config for serde deserialization. The owning machine must call
/// `set_config()` immediately after deserializing to install the correct config.
fn default_config() -> &'static UlaConfig {
    &CONFIG_48K
}

impl UlaEngine {
    /// Re-install the timing config after deserialization.
    pub fn set_config(&mut self, config: &'static UlaConfig) {
        self.config = config;
    }

    /// Create with a wider framebuffer for Timex hi-res support.
    pub fn new_hires(config: &'static UlaConfig) -> Self {
        let mut e = Self::new(config);
        e.fb_width = crate::timing::SCREEN_WIDTH_HIRES;
        e
    }

    pub fn new(config: &'static UlaConfig) -> Self {
        Self {
            pixel: 0,
            scan: 0,
            border: 7,
            beeper: false,
            mic: false,
            flash_counter: 0,
            flash_active: false,
            cpu_clock: true,
            z80_clock_high: true,
            int_active: false,
            bus_data: 0xFF,
            idle: true,
            data_reg: 0,
            attr_reg: 0,
            data_latch: 0,
            attr_latch: 0,
            data_addr: 0,
            attr_addr: 0,
            video: false,
            border_active: true,
            z80_mreq_prev: false,
            z80_iorq_prev: false,
            z80_iorq_prev2: false,
            fb_width: SCREEN_WIDTH,
            scld_mode: 0,
            scld_hires_ink: 0,
            data_reg2: 0,
            data_latch2: 0,
            config,
        }
    }

    /// Compute bitmap VRAM address for the given scanline.
    #[inline]
    pub fn compute_data_addr(scan: u16) -> u16 {
        ((scan & 0x38) << 2) | ((scan & 0x07) << 8) | ((scan & 0xC0) << 5)
    }

    /// Compute attribute VRAM address for the given scanline.
    #[inline]
    pub fn compute_attr_addr(scan: u16) -> u16 {
        0x1800 | ((scan & 0xF8) << 2)
    }

    /// Run the rendering portion of a ULA tick: video fetch, pixel output,
    /// counter advance, and interrupt timing. Does NOT handle contention —
    /// that's the variant wrapper's job.
    pub fn tick_rendering(&mut self, memory: &dyn MemoryBus, framebuffer: &mut [u8]) {
        let p = self.pixel as usize;
        let phase = p & 0x0F;
        let cfg = self.config;

        // === Checkpoint events ===
        match self.pixel {
            0 => {
                self.border_active = self.scan >= 192;
            }
            x if x == cfg.fetch_start => {
                self.video = self.scan < 192;
                if self.video {
                    self.data_addr = Self::compute_data_addr(self.scan);
                    self.attr_addr = Self::compute_attr_addr(self.scan);
                }
                self.border_active = !self.video;
            }
            256 => {
                self.border_active = true;
            }
            x if x == cfg.fetch_end => {
                self.video = false;
            }
            _ => {}
        }

        // === Video fetch ===
        if self.video {
            self.idle = IDLE_TABLE[phase];
            let hires = self.scld_mode & 0x04 != 0;
            let hicolour = self.scld_mode & 0x02 != 0;
            let dual = self.scld_mode & 0x01 != 0;

            // Transfer latch → active BEFORE new fetch
            if (p & 0x07) == 4 {
                self.data_reg = self.data_latch;
                self.data_reg2 = self.data_latch2;
                self.attr_reg = self.attr_latch;
            }

            // VRAM reads at phases 8, 10, 12, 14
            if !MEM_TABLE[phase] {
                if phase & 0x02 == 0 {
                    // Bitmap fetch
                    let a = self.data_addr;
                    self.data_addr = self.data_addr.wrapping_add(1);
                    let base = if dual && !hires { 0x6000u16 } else { 0x4000u16 };
                    let addr = base | (a & 0x1FFF);
                    self.bus_data = memory.read_screen(addr);
                    self.data_latch = self.bus_data;

                    // Hi-res: also fetch from the other screen for odd columns
                    if hires {
                        self.data_latch2 = memory.read_screen(0x6000 | (a & 0x1FFF));
                    }
                } else {
                    // Attribute fetch
                    let a = self.attr_addr;
                    self.attr_addr = self.attr_addr.wrapping_add(1);

                    if hires {
                        // Hi-res: no attributes — colour from port $FF bits 3-5
                        self.bus_data = 0xFF;
                        self.attr_latch = 0xFF;
                    } else if hicolour {
                        // Hi-colour: attribute from $6000 + bitmap_offset
                        // (each pixel row has its own attribute, not 8×8 cells)
                        let bitmap_offset = Self::compute_data_addr(self.scan);
                        let attr_addr = 0x6000 | (bitmap_offset & 0x1FFF) | (a & 0x1F); // column
                        self.bus_data = memory.read_screen(attr_addr);
                        self.attr_latch = self.bus_data;
                    } else {
                        let base = if dual { 0x7800u16 } else { 0x5800u16 };
                        // For dual-screen without hi-colour: attrs from second screen area
                        let addr = if dual {
                            base | (a & 0x02FF)
                        } else {
                            0x4000 | (a & 0x1FFF)
                        };
                        self.bus_data = memory.read_screen(addr);
                        self.attr_latch = self.bus_data;
                    }
                }
            }
        } else {
            self.idle = true;
            self.bus_data = 0xFF;
        }

        // === Pixel output ===
        // The scale factor: 2 for hi-res framebuffers (704 wide), 1 for standard (352).
        let hscale = if self.fb_width > SCREEN_WIDTH { 2 } else { 1 };
        let hires = self.scld_mode & 0x04 != 0;

        let fb_x: i32 = if self.pixel >= cfg.fb_x_wrap_start {
            ((p as i32) - cfg.fb_x_wrap_offset) * hscale
        } else if self.pixel < cfg.fb_x_hblank_start {
            ((p as i32) + cfg.fb_x_offset) * hscale
        } else {
            -1
        };

        let fb_y: i32 = if self.scan < cfg.vsync_start {
            self.scan as i32 + 48
        } else if self.scan < cfg.vsync_end {
            -1
        } else {
            (self.scan as i32) - cfg.vsync_end as i32
        };

        let fw = self.fb_width as i32;
        if fb_x >= 0 && fb_x < fw && fb_y >= 0 && fb_y < SCREEN_HEIGHT as i32 {
            let in_screen =
                self.scan < 192 && self.pixel >= cfg.screen_start && self.pixel < cfg.screen_end;

            if in_screen && hires && hscale == 2 {
                // Hi-res on wide framebuffer: 2 pixels per ULA clock
                let bit1 = (self.data_reg >> 7) & 1;
                let bit2 = (self.data_reg2 >> 7) & 1;
                self.data_reg <<= 1;
                self.data_reg2 <<= 1;
                let ink = self.scld_hires_ink;
                let px_even = if bit1 != 0 { ink } else { 0 };
                let px_odd = if bit2 != 0 { ink } else { 0 };
                let off = fb_y as usize * self.fb_width + fb_x as usize;
                if off + 1 < framebuffer.len() {
                    framebuffer[off] = px_even;
                    framebuffer[off + 1] = px_odd;
                }
            } else if in_screen {
                // Standard / hi-colour: attribute-based rendering
                let attr = self.attr_reg;
                let (ink, paper) = palette::attr_to_indices(attr);
                let flash = palette::attr_flash(attr) && self.flash_active;
                let bit = (self.data_reg >> 7) & 1;
                self.data_reg <<= 1;
                let pixel_idx = if (bit != 0) ^ flash { ink } else { paper };
                let off = fb_y as usize * self.fb_width + fb_x as usize;
                if off < framebuffer.len() {
                    framebuffer[off] = pixel_idx;
                    // On wide framebuffer, double each standard pixel
                    if hscale == 2 && off + 1 < framebuffer.len() {
                        framebuffer[off + 1] = pixel_idx;
                    }
                }
            } else {
                // Border
                let off = fb_y as usize * self.fb_width + fb_x as usize;
                if off < framebuffer.len() {
                    framebuffer[off] = self.border;
                    if hscale == 2 && off + 1 < framebuffer.len() {
                        framebuffer[off + 1] = self.border;
                    }
                }
            }
        } else if self.scan < 192 && self.pixel >= cfg.screen_start && self.pixel < cfg.screen_end {
            self.data_reg <<= 1;
            if hires {
                self.data_reg2 <<= 1;
            }
        }

        // === Advance pixel counter ===
        self.pixel += 1;
        if self.pixel >= cfg.pixels_per_line {
            self.pixel = 0;
            self.scan += 1;
            if self.scan >= cfg.lines_per_frame {
                self.scan = 0;
            }
        }

        // === Interrupt timing ===
        if self.scan == cfg.int_scan {
            if self.pixel == cfg.int_start_pixel {
                self.int_active = true;
            } else if self.pixel == cfg.int_end_pixel {
                self.int_active = false;
            }
        }
    }

    /// Track Z80 clock phase (called after contention decision).
    pub fn track_z80_clock(&mut self, cpu_iorq: bool, cpu_mreq: bool) {
        if self.cpu_clock {
            self.z80_iorq_prev2 = self.z80_iorq_prev;
            self.z80_iorq_prev = cpu_iorq;
            self.z80_mreq_prev = cpu_mreq;
            self.z80_clock_high = !self.z80_clock_high;
        }
    }

    /// End-of-frame housekeeping.
    pub fn end_frame(&mut self) {
        self.flash_counter = (self.flash_counter + 1) & 0x1F;
        self.flash_active = self.flash_counter >= 16;
        self.pixel = 0;
        self.scan = 0;
    }

    /// Write port 0xFE.
    pub fn write_fe(&mut self, val: u8) {
        self.border = val & 0x07;
        self.mic = val & 0x08 != 0;
        self.beeper = val & 0x10 != 0;
    }

    /// Read port 0xFE (keyboard + EAR).
    pub fn read_fe(&self, port: u16, keyboard: &[u8; 8]) -> u8 {
        let mut result = 0xBF;
        let high = (port >> 8) as u8;
        let mut keys = 0x1F;
        for (row, row_state) in keyboard.iter().enumerate().take(8) {
            if high & (1 << row) == 0 {
                keys &= *row_state;
            }
        }
        result = (result & 0xE0) | (keys & 0x1F);
        result |= 0x40; // EAR bit (no tape → high)
        result
    }
}
