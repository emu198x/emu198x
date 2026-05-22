//! Shared Spectrum-family ULA rendering engine.
//!
//! Source references:
//! - `knowledge/systems/spectrum/overview.md`
//! - `knowledge/systems/spectrum/contention.md`
//! - Adapted from `../Emu198x-Older/crates/common-sinclair-zx-spectrum/src/ula_engine.rs`
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

/// Frame routing version. Bumped when the rendering path through this
/// engine (fetch ordering, shifter pipeline depth, palette mapping,
/// border timing) changes in a way that invalidates previously-captured
/// frame hashes in the catalogue. The catalogue manifest carries the
/// version each hash was captured against; a mismatch fails loud with
/// a re-capture instruction.
///
/// **Version 1** (2026-05-19): single-latch shifter model with fetches
/// at pixels 8/10/12/14 and `fetch_start: 8`. Palette derived from
/// BT.601-style values in `palette.rs`. Border updates immediately on
/// `write_fe`. The pre-Seam-1 rendering described in
/// `knowledge/decisions/spectrum-architecture-review.md`.
///
/// **Version 2** (2026-05-19): two-stage shifter (Seam 1)
/// landed alongside the Smith Chapter 16 palette refinement.
/// `MEM_TABLE` and `IDLE_TABLE` shifted four entries left so fetches
/// happen at phases 4/6/8/10 instead of 8/10/12/14; `fetch_start: 4`
/// in all configs; pipeline depth between first fetch and first visible
/// pixel is now 8 pixels (4 T-states) per Smith Chapter 12 Figure 12-2.
/// Float48K strict-mode target: T=14338 for 48K, T=14364 for 128K.
/// Border still updates immediately on `write_fe`.
///
/// **Version 3** (2026-05-19, current): AOLatch border timing
/// (Smith Chapter 14 p. 134, `/AOLatch = /(/C0 + C1 + /C2)`).
/// `write_fe` still updates `border` (BorderLatch) immediately, but
/// the rendered border colour now reads from `border_aolatch`, which
/// samples `border` every 8 pixels at `(p & 0x07) == 4` un-gated by
/// VidEN. This is the silicon basis for the 8-pixel border-write
/// granularity that border-effect demos exploit. Frame hashes that
/// cover the border region during multi-colour effects will change.
///
/// Bumps planned: further versions per substantive rendering change.
/// See the architecture review's Seam 4 for the re-capture discipline
/// this constant enforces.
pub const FRAME_ROUTING_VERSION: u32 = 3;

/// Timing and layout constants for a specific ULA variant.
#[derive(Clone, Debug)]
pub struct UlaConfig {
    /// ULA clocks per scanline (448 for 48K, 456 for 128K).
    pub pixels_per_line: u16,
    /// Scanlines per frame (312 for 48K, 311 for 128K).
    pub lines_per_frame: u16,

    /// Pixel position where video fetch begins (always 4 since Seam 1 —
    /// prefetch cell; first CAS at phase 4 within the first 16-pixel cycle).
    pub fetch_start: u16,
    /// Pixel position where video fetch ends (always 260 since Seam 1 —
    /// last CAS at phase 10 within the final 16-pixel cycle).
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
    fetch_start: 4,
    fetch_end: 260,
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
    fetch_start: 4,
    fetch_end: 260,
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
///
/// NTSC values are from Timex documentation, NOT Smith Chapter 11.
/// Smith covers the unreleased Sinclair 6C011 NTSC ULA (264 lines,
/// 63.5 μs/line) which Timex did not use; the TS2068 uses Timex's own
/// video controller with these constants. See `knowledge/decisions/
/// spectrum-architecture-review.md` verified-non-issue entry.
pub const CONFIG_TS2068: UlaConfig = UlaConfig {
    pixels_per_line: 448,
    lines_per_frame: 262,
    fetch_start: 4,
    fetch_end: 260,
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
    fetch_start: 4,
    fetch_end: 260,
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
    /// Border colour (0-7) — the BorderLatch / OutB. Holds the value
    /// most recently written by `write_fe` and reflects CPU writes
    /// immediately. The rendered border colour is `border_aolatch`,
    /// which samples this through the AOLatch every 8 pixels (Smith
    /// Chapter 14 `/AOLatch = /(/C0 + C1 + /C2)`, un-gated by VidEN).
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
    // Video fetch state.
    //
    // Smith Chapter 12 Figure 12-2 documents a two-stage double buffer
    // per stream: memory → DataLatch → ShiftRegister, clocked by two
    // distinct signals (`DataLatch` and `SLoad`). The `*_pending` fields
    // are the memory-side stage; `data_latch`/`attr_latch` are the
    // DataLatch stage; `data_reg`/`attr_reg` are the ShiftRegister.
    // Transfer points are described in `tick_rendering`.
    //
    // `#[serde(skip)]` on the pending fields keeps snapshot byte-encoding
    // backward-compatible with snapshots taken before Seam 1 landed. The
    // pipeline drains every 16-pixel cycle, so omitting it from save
    // state at most loses a sub-cycle of rendering on the resume
    // scanline — acceptable transient state.
    pub data_reg: u8,
    pub attr_reg: u8,
    pub data_latch: u8,
    pub attr_latch: u8,
    #[serde(skip)]
    pub data_latch_pending: u8,
    #[serde(skip)]
    pub attr_latch_pending: u8,
    pub data_addr: u16,
    pub attr_addr: u16,
    /// Active video (scan < 192 and within fetch range).
    pub video: bool,
    /// Border area flag.
    pub border_active: bool,
    /// AOLatch — the latched border colour driving the output mux.
    /// Smith Chapter 14 p. 134: `/AOLatch = /(/C0 + C1 + /C2)`, fires
    /// every 8 CLK7 cycles across the full scanline including border
    /// (un-gated by VidEN). The CPU writes to `border` (BorderLatch /
    /// OutB) instantly via `write_fe`, but the *displayed* colour only
    /// catches up at the next AOLatch trigger. This is the silicon
    /// basis for the well-known 8-pixel border-write granularity that
    /// border-effect demos rely on. `#[serde(skip)]` keeps snapshot
    /// byte-encoding backward compatible — the AOLatch state drains
    /// every 8 pixels so resume reseeds from `border` within at most
    /// one character cell.
    #[serde(skip)]
    pub border_aolatch: u8,

    /// VidEN: `/Border` delayed by one character cell (8 CLK7 = 4 T-states).
    /// Per Smith Chapter 12 p. 134, `SLoad` is gated on `/VidEN` rather
    /// than `/Border`. In the current implementation we gate the SLoad
    /// transfer on `scan < 192` (the rendering scanline window) rather
    /// than tracking VidEN as a separate signal — equivalent for the
    /// 48K timing model because the SLoad/DataLatch transfers happen
    /// uniformly on every active scanline, and the visual offset from
    /// VidEN's one-character delay is already accounted for by
    /// `fetch_start` / `fetch_end`. Field reserved for future fidelity
    /// work (e.g. AOLatch border timing); `#[serde(skip)]` keeps
    /// snapshot byte-encoding backward compatible.
    #[serde(skip)]
    pub vid_en: bool,
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
    /// Hi-res pending-latch slot — mirrors `data_latch_pending` for the
    /// $6000-screen path used by Timex SCLD hi-res mode. Routed through
    /// the same two-stage pipeline so the two streams emerge in lock-step.
    #[serde(skip)]
    pub data_latch_pending2: u8,

    /// Timing configuration for this variant.
    /// Skipped during serialization — the owning machine must re-set this
    /// after deserialization via `set_config()`.
    #[serde(skip, default = "default_config")]
    config: &'static UlaConfig,
}

/// VRAM read pattern: indexed by pixel & 0x0F.
/// false = ULA reads from VRAM this clock.
///
/// Fetches happen at phases 4, 6, 8, 10 — four reads per 16-pixel cycle.
/// Phases 4/8 read display bytes (phase & 0x02 == 0), phases 6/10 read
/// attribute bytes (phase & 0x02 != 0). Per Smith Chapter 13 this matches
/// the canonical two-RAS-CAS-pair continuous-fetch pattern: phases 0-3
/// are the first RAS-CAS pair (display+attr byte N), phases 4-7 the
/// second pair (display+attr byte N+1), in Smith's 8-phase numbering.
/// In our 16-pixel-clock indexing, each Smith phase spans two of our
/// pixels and the fetch happens at the first pixel of the CAS phase.
///
/// The pre-Seam-1 model fetched at phases 8, 10, 12, 14 — same
/// continuous-fetch shape, but offset four pixels (two T-states) too
/// late, producing the +4 T-state first-fetch offset documented in
/// `knowledge/decisions/ula-first-fetch-tstate-offset.md`.
pub const MEM_TABLE: [bool; 16] = [
    true, true, true, true, false, true, false, true, false, true, false, true, true, true, true,
    true,
];

/// Idle table: indexed by pixel & 0x0F.
/// true = ULA is idle (floating bus returns 0xFF).
///
/// The bus carries the most-recently-latched byte from phase 4 (first
/// CAS) through phase 11 (latch settles after second-pair attr fetch).
/// Outside that window the bus floats and returns 0xFF via the
/// pull-ups, per Smith Chapter 19. Mirrors the four-pixel left-shift
/// applied to `MEM_TABLE` and `fetch_start` for Seam 1.
pub const IDLE_TABLE: [bool; 16] = [
    true, true, true, true, false, false, false, false, false, false, false, false, true, true,
    true, true,
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
            data_latch_pending: 0,
            attr_latch_pending: 0,
            data_addr: 0,
            attr_addr: 0,
            video: false,
            border_active: true,
            border_aolatch: 7,
            vid_en: false,
            z80_mreq_prev: false,
            z80_iorq_prev: false,
            z80_iorq_prev2: false,
            fb_width: SCREEN_WIDTH,
            scld_mode: 0,
            scld_hires_ink: 0,
            data_reg2: 0,
            data_latch2: 0,
            data_latch_pending2: 0,
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

        // === Two-stage shifter pipeline transfers (Seam 1, Smith Ch 12 Fig 12-2) ===
        // memory → pending → latch → reg, with three transfer points per
        // 16-pixel cycle:
        //   - p & 0x07 == 0 (pixels 0, 8): pending → latch (DataLatch)
        //   - p & 0x07 == 4 (pixels 4, 12): latch → reg (SLoad)
        //   - At each fetch (phases 4/6/8/10, gated by `video`): memory → pending
        //
        // The promote and SLoad transfers fire on every active scanline
        // (scan < 192), even after `fetch_end` flips `video` false. This
        // lets the byte fetched just before `fetch_end` propagate through
        // the pipeline so the last visible character (column 31, pixels
        // 260-267 of the screen) renders correctly. Fetches stay gated by
        // `video` so no phantom reads happen past `fetch_end`.
        if self.scan < 192 {
            if (p & 0x07) == 0 {
                self.data_latch = self.data_latch_pending;
                self.data_latch2 = self.data_latch_pending2;
                self.attr_latch = self.attr_latch_pending;
            }
            if (p & 0x07) == 4 {
                self.data_reg = self.data_latch;
                self.data_reg2 = self.data_latch2;
                self.attr_reg = self.attr_latch;
            }
        }

        // === AOLatch transfer (Smith Ch 14 p. 134) ===
        // `/AOLatch = /(/C0 + C1 + /C2)` fires every 8 CLK7 cycles, un-gated
        // by VidEN — across the full scanline including border. The latched
        // colour is what the screen output mux passes to the video DAC. In
        // our 16-pixel-cell indexing, AOLatch fires at `(p & 0x07) == 4`,
        // immediately following SLoad in each cycle (Smith: "AOLatch goes
        // low while SLoad is low. This presents the attribute byte to the
        // colour output multiplexer at the exact moment the display byte
        // is loaded into the shift register."). The result is the
        // well-known 8-pixel border-write granularity that border-effect
        // demos exploit. Unlike SLoad, this trigger is unconditional:
        // border colour can change anywhere on the scanline.
        if (p & 0x07) == 4 {
            self.border_aolatch = self.border;
        }

        // === Video fetch ===
        if self.video {
            self.idle = IDLE_TABLE[phase];
            let hires = self.scld_mode & 0x04 != 0;
            let hicolour = self.scld_mode & 0x02 != 0;
            let dual = self.scld_mode & 0x01 != 0;

            // VRAM reads at phases 4, 6, 8, 10 (two RAS-CAS fetch pairs
            // per 16-pixel cycle). bus_data is set at the moment of
            // each CAS strobe — this is the value `IN A,($FF)` samples
            // via the floating-bus path on the ULA.
            if !MEM_TABLE[phase] {
                if phase & 0x02 == 0 {
                    // Bitmap fetch (CAS-A or CAS-C falling)
                    let a = self.data_addr;
                    self.data_addr = self.data_addr.wrapping_add(1);
                    let base = if dual && !hires { 0x6000u16 } else { 0x4000u16 };
                    let addr = base | (a & 0x1FFF);
                    self.bus_data = memory.read_screen(addr);
                    self.data_latch_pending = self.bus_data;

                    // Hi-res: also fetch from the other screen for odd columns
                    if hires {
                        self.data_latch_pending2 = memory.read_screen(0x6000 | (a & 0x1FFF));
                    }
                } else {
                    // Attribute fetch (CAS-B or CAS-D falling)
                    let a = self.attr_addr;
                    self.attr_addr = self.attr_addr.wrapping_add(1);

                    if hires {
                        // Hi-res: no attributes — colour from port $FF bits 3-5
                        self.bus_data = 0xFF;
                        self.attr_latch_pending = 0xFF;
                    } else if hicolour {
                        // Hi-colour: attribute from $6000 + bitmap_offset
                        // (each pixel row has its own attribute, not 8×8 cells)
                        let bitmap_offset = Self::compute_data_addr(self.scan);
                        let attr_addr = 0x6000 | (bitmap_offset & 0x1FFF) | (a & 0x1F); // column
                        self.bus_data = memory.read_screen(attr_addr);
                        self.attr_latch_pending = self.bus_data;
                    } else {
                        let base = if dual { 0x7800u16 } else { 0x5800u16 };
                        // For dual-screen without hi-colour: attrs from second screen area
                        let addr = if dual {
                            base | (a & 0x02FF)
                        } else {
                            0x4000 | (a & 0x1FFF)
                        };
                        self.bus_data = memory.read_screen(addr);
                        self.attr_latch_pending = self.bus_data;
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
                // Border — driven by the AOLatch, not the BorderLatch.
                // 8-pixel granularity per Smith Ch 14 (see AOLatch
                // transfer comment above).
                let off = fb_y as usize * self.fb_width + fb_x as usize;
                if off < framebuffer.len() {
                    framebuffer[off] = self.border_aolatch;
                    if hscale == 2 && off + 1 < framebuffer.len() {
                        framebuffer[off + 1] = self.border_aolatch;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryBus;

    /// Minimal MemoryBus stub returning 0 for every read — sufficient
    /// for border-rendering tests that don't depend on bitmap or
    /// attribute content.
    struct ZeroMemory;
    impl MemoryBus for ZeroMemory {
        fn read(&self, _addr: u16) -> u8 {
            0
        }
        fn write(&mut self, _addr: u16, _value: u8) {}
        fn is_contended(&self, _addr: u16) -> bool {
            false
        }
    }

    /// AOLatch behaviour: `write_fe` updates `border` (BorderLatch)
    /// immediately, but `border_aolatch` (the rendered colour) only
    /// catches up at the next `(p & 0x07) == 4` boundary. This is the
    /// silicon basis for the well-known 8-pixel border-write
    /// granularity (Smith Ch 14 p. 134, `/AOLatch = /(/C0 + C1 + /C2)`,
    /// un-gated by VidEN).
    #[test]
    fn aolatch_defers_border_writes_to_next_8_pixel_boundary() {
        let mut e = UlaEngine::new(&CONFIG_48K);
        let mem = ZeroMemory;
        let mut fb = vec![0u8; SCREEN_WIDTH * SCREEN_HEIGHT];

        // Position the engine just past an AOLatch trigger. AOLatch
        // samples on a tick that *starts* with pixel ≡ 4 (mod 8), so
        // starting at pixel 5 the next trigger fires on the tick where
        // pixel = 12 at entry. The counter increments at the end of
        // each tick.
        e.pixel = 5;
        e.scan = 0;
        e.border_aolatch = 7;
        e.border = 7;

        // CPU writes border = 2 (red). BorderLatch updates instantly,
        // AOLatch does not.
        e.write_fe(0x02);
        assert_eq!(e.border, 2, "BorderLatch reflects CPU write instantly");
        assert_eq!(
            e.border_aolatch, 7,
            "AOLatch still holds the previous colour"
        );

        // Tick 7 times: pixel 5→12 at end, none of the entry-pixels
        // (5..=11) match p&7==4.
        for _ in 0..7 {
            e.tick_rendering(&mem, &mut fb);
        }
        assert_eq!(e.pixel, 12);
        assert_eq!(
            e.border_aolatch, 7,
            "AOLatch unchanged before the boundary"
        );

        // Eighth tick: entry pixel = 12 (matches p&0x07==4). AOLatch
        // samples BorderLatch. Pixel increments to 13.
        e.tick_rendering(&mem, &mut fb);
        assert_eq!(e.pixel, 13);
        assert_eq!(
            e.border_aolatch, 2,
            "AOLatch sampled BorderLatch on its trigger"
        );
    }

    /// AOLatch fires regardless of scan position — Smith Ch 14
    /// confirms `/AOLatch` has no VidEN gate, so the trigger is
    /// equally active during border scanlines (scan >= 192) and
    /// active video.
    #[test]
    fn aolatch_fires_during_border_scanlines() {
        let mut e = UlaEngine::new(&CONFIG_48K);
        let mem = ZeroMemory;
        let mut fb = vec![0u8; SCREEN_WIDTH * SCREEN_HEIGHT];

        // Place the engine in a bottom-border scanline at pixel 4 —
        // entry pixel matches p&7==4, so the next tick fires AOLatch.
        e.pixel = 4;
        e.scan = 250; // well into bottom border
        e.border_aolatch = 7;
        e.write_fe(0x04); // border = green

        e.tick_rendering(&mem, &mut fb);
        assert_eq!(e.pixel, 5);
        assert_eq!(
            e.border_aolatch, 4,
            "AOLatch fired during border-only scanline"
        );
    }
}
