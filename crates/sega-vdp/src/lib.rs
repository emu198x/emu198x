//! Sega Master System / Game Gear VDP (315-5124 / 315-5246).
//!
//! Adapted from `Emu198x-Oldest/crates/sega-vdp` (port 2026-06-01) as
//! commit 1 of 3 unlocking Sega Master System. Self-contained port
//! with no external dependencies; first major new chip needed by SMS
//! beyond what ColecoVision + SG-1000 + MSX1 + Sord M5 already brought
//! in (TMS9918 + SN76489 + AY-3-8910 + 8255 PPI). The TMS9918 chip is
//! adjacent silicon — register I/O is mostly compatible at the
//! TMS9918-mode level — but the new Mode 4 tile pipeline, dual 16-colour
//! palettes, scroll registers, line-interrupt counter, and H/V counter
//! readback make this a substantial step beyond `ti-tms9918`.
//!
//! Extends the TMS9918A with Mode 4: 4bpp tiles with per-tile flip,
//! priority, and palette select; two 16-color palettes from 64 colors
//! (6-bit RGB); horizontal and vertical scrolling; 8 sprites per line;
//! and a line interrupt counter.
//!
//! All four TMS9918A legacy modes (Graphics I/II, Text, Multicolor) are
//! retained for SG-1000 backward compatibility.
//!
//! The Game Gear variant extends CRAM to 12-bit RGB (4096 colors) and
//! displays a 160×144 viewport from the centre of the 256×192 active
//! area, with no border — an LCD has no overscan to hide. Its framebuffer
//! is sized to the LCD, so what it reports and what it holds agree.

#![allow(clippy::cast_possible_truncation)]

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

// ---------------------------------------------------------------------------
// Region and variant
// ---------------------------------------------------------------------------

/// VDP region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VdpRegion {
    Ntsc,
    Pal,
}

impl VdpRegion {
    /// Scan lines a set displays, which is what a television framebuffer
    /// holds.
    ///
    /// Per `knowledge/decisions/the-framebuffer-is-the-sets-window.md`. One
    /// height served both regions and it was NTSC's, so a PAL Master System
    /// showed 240 lines of a 288-line field — 83%, which is what the #1054
    /// audit read across this chip and the TMS9918 family alike.
    #[must_use]
    pub const fn framebuffer_height(self) -> u32 {
        match self {
            Self::Ntsc => 240,
            Self::Pal => 288,
        }
    }

    /// Border lines the chip *scans* above and below the active area.
    ///
    /// MAME's `315_5124.h` tables the frame for every display height. The
    /// blanking is a constant 19 lines in all six cases — 3 of sync and 13
    /// before the picture, 3 after — so the border shrinks by exactly what
    /// the picture grows, and each row here checks against its region's
    /// frame: 27+192+24+19 = 262, 38+224+32+19 = 313, and so on.
    ///
    /// The TMS9918A manual's Table 3-3 gives the same 27 and 24 for the chip
    /// this one descends from, so the 192-line NTSC pair is doubly attested.
    ///
    /// NTSC at 240 lines is the one MAME does not table, and the arithmetic
    /// says why: 262 less 19 blanked less 240 active leaves three lines of
    /// border for the whole frame. There is nowhere to put a picture that
    /// size on a 60 Hz set, which is why the mode is documented as unusable
    /// there. The 11:8 ratio of the 224-line row splits those three as 2 and
    /// 1, and the set then crops both away.
    const fn scanned_borders(self, active_height: u32) -> (u32, u32) {
        match (self, active_height) {
            (Self::Ntsc, 192) => (27, 24),
            (Self::Ntsc, 224) => (11, 8),
            (Self::Ntsc, _) => (2, 1),
            (Self::Pal, 192) => (54, 48),
            (Self::Pal, 224) => (38, 32),
            (Self::Pal, _) => (30, 24),
        }
    }

    /// Scan lines of border above the active area, as a set shows them.
    ///
    /// Halving what the field has left over would put the active area in the
    /// middle of the window, and the chip does not put it there. The chip
    /// scans more lines than a set displays — 243 of 262 on NTSC, 294 of 313
    /// on PAL — so the difference is cropped, split as evenly as the count
    /// allows with the odd line coming off the larger top border. NTSC loses
    /// three lines, two of them off the top; PAL loses six, three each end.
    ///
    /// At 192 lines that gives 25 and 51, and the picture sits a line and a
    /// half below the middle of the window because that is where the chip
    /// scans it. At 240 lines on NTSC it gives zero: the picture is the
    /// window.
    #[must_use]
    pub const fn border_top(self, active_height: u32) -> u32 {
        let (top, bottom) = self.scanned_borders(active_height);
        let scanned = top + active_height + bottom;
        let cropped = scanned - self.framebuffer_height();
        top - cropped.div_ceil(2)
    }

    /// Scan lines of border below the active area, as a set shows them.
    #[must_use]
    pub const fn border_bottom(self, active_height: u32) -> u32 {
        self.framebuffer_height() - active_height - self.border_top(active_height)
    }

    /// Pixels a set displays along a line, which is a television framebuffer's
    /// width.
    ///
    /// `dot_clock x active_line_seconds`: 5.369318 MHz over 52.148 µs is 280
    /// on NTSC, and 5.320342 MHz over 52.0 µs is 277 on PAL. This used to be a
    /// fixed 16 pixels of border either side of the active 256 — 288 for both
    /// regions, which is 103% and 104% of their windows.
    #[must_use]
    pub const fn framebuffer_width(self) -> u32 {
        match self {
            Self::Ntsc => 280,
            Self::Pal => 277,
        }
    }

    /// Pixels of border left of the active area.
    ///
    /// Centring is right here, which had to be checked rather than assumed
    /// after the vertical case turned out not to be. MAME gives the line as 13
    /// pixels of left border, 256 active and 15 of right, with 58 of sync,
    /// burst and blanking — so the picture is *not* centred in the 284 the
    /// chip scans. It is all but centred in what a set shows: measured from
    /// the leading edge of sync the active area's midpoint lands 35.57 µs into
    /// the line, against a broadcast picture centre of 35.5 to 35.7 depending
    /// on which back-porch figure you take. Under a pixel either way, and less
    /// than the porch figures disagree among themselves.
    #[must_use]
    pub const fn border_left(self) -> u32 {
        (self.framebuffer_width() - ACTIVE_WIDTH) / 2
    }
}

/// VDP variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VdpVariant {
    /// SMS1 (315-5124): no 224/240-line modes, sprite zoom bug.
    Sms1,
    /// SMS2 / Game Gear (315-5246): 224/240-line modes, fixed sprite zoom.
    Sms2,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Active display area dimensions (the pixels the SMS VDP actually
/// draws tiles + sprites into).
pub const ACTIVE_WIDTH: u32 = 256;

/// Pixels the chip scans per line — 256 active and 86 of border, sync and
/// blanking.
pub const DOTS_PER_LINE: u16 = 342;
pub const ACTIVE_HEIGHT: u32 = 192;

/// Dot clock of the NTSC VDP: half a 10.738635 MHz crystal, three times the
/// colour subcarrier. Inherited from the TMS9918 the chip descends from, and
/// the reason a Master System's pixels come out at 8:7 like an MSX's.
pub const NTSC_DOT_CLOCK_HZ: f64 = 5_369_318.0;

/// Dot clock of the PAL VDP: a 53.203424 MHz master clock divided by ten.
///
/// This held 5.34375 MHz — half a 10.6875 MHz crystal, which is the PAL
/// *MSX's* figure and not this machine's. A PAL Master System runs from
/// twelve times the PAL colour subcarrier (12 x 4.43361875 MHz), the VDP
/// takes master ÷ 10 and the Z80 master ÷ 15. MAME's `sms.cpp` states the
/// master clock and both divisors; Genesis Plus GX's `system.c` gives the
/// same 53203424, and `reference/by-topic/vdp-sms/vdp-sms-reference.md`
/// reaches 5.320 MHz from the other direction. The machine's own
/// `PAL_PSG_CLOCK_HZ` has been 3546893 — master ÷ 15 — all along, so this
/// constant disagreed with its neighbour by 0.44%.
pub const PAL_DOT_CLOCK_HZ: f64 = 5_320_342.0;

/// The Game Gear's LCD, which shows a window cut from the centre of the
/// active display rather than the whole of it.
pub const GG_WIDTH: u32 = 160;
pub const GG_HEIGHT: u32 = 144;

/// Where that window sits inside the 256x192 active area. The handheld has
/// no border at all: a border emulates the overscan a television hides, and
/// an LCD has none.
const GG_ORIGIN_X: u32 = (ACTIVE_WIDTH - GG_WIDTH) / 2;
const GG_ORIGIN_Y: u32 = (ACTIVE_HEIGHT - GG_HEIGHT) / 2;

// ---------------------------------------------------------------------------
// VDP
// ---------------------------------------------------------------------------

/// Sega VDP.
#[derive(Serialize, Deserialize)]
pub struct SegaVdp {
    // VRAM: 16 KB
    #[serde(with = "BigArray")]
    vram: [u8; 16384],
    // CRAM: 32 bytes (SMS) or 64 bytes (GG)
    #[serde(with = "BigArray")]
    cram: [u8; 64],
    cram_latch: u8,
    is_game_gear: bool,

    // Registers (0-10)
    regs: [u8; 11],

    // Status register
    status: u8,

    // I/O state
    read_buffer: u8,
    address: u16,
    code: u8,
    latch_first: bool,
    latch_value: u8,

    // Counters
    v_counter: u16,
    h_counter: u8,
    line_counter: u8,
    line_irq_pending: bool,
    /// R9 as it stood when the active display began. The chip samples the
    /// vertical scroll once a frame, so this is the value the whole frame
    /// renders with, whatever a game writes to R9 part-way down it.
    vscroll: u8,
    /// Active display height for this frame — 192, 224 or 240. Latched at the
    /// top of the frame for the same reason as `vscroll`.
    active_height: u32,

    // Rendering
    scanline: u16,
    /// Current dot within the scanline (0-341), for per-dot rendering.
    dot: u16,
    region: VdpRegion,
    variant: VdpVariant,
    framebuffer: Vec<u32>,
    /// Per-line sprite colour-index buffer (0 = no sprite pixel), evaluated at
    /// the start of each active line and overlaid per pixel. Transient — not
    /// part of the saved state.
    #[serde(with = "BigArray")]
    sprite_buf: [u8; 256],

    /// Interrupt output (directly drives Z80 INT).
    pub interrupt: bool,
    /// Frame counter.
    pub frame_count: u64,
}

impl SegaVdp {
    /// Create a new SMS VDP.
    #[must_use]
    pub fn new(region: VdpRegion, variant: VdpVariant) -> Self {
        Self::new_inner(region, variant, false)
    }

    /// Create a new Game Gear VDP.
    #[must_use]
    pub fn new_game_gear() -> Self {
        Self::new_inner(VdpRegion::Ntsc, VdpVariant::Sms2, true)
    }

    fn new_inner(region: VdpRegion, variant: VdpVariant, is_game_gear: bool) -> Self {
        Self {
            vram: [0; 16384],
            cram: [0; 64],
            cram_latch: 0,
            is_game_gear,
            regs: [0; 11],
            status: 0,
            read_buffer: 0,
            address: 0,
            code: 0,
            latch_first: true,
            latch_value: 0,
            v_counter: 0,
            h_counter: 0,
            line_counter: 0,
            line_irq_pending: false,
            vscroll: 0,
            active_height: ACTIVE_HEIGHT,
            scanline: 0,
            dot: 0,
            region,
            variant,
            // Sized to what the machine displays, so the buffer and the
            // dimensions reported alongside it can never disagree.
            framebuffer: if is_game_gear {
                vec![0; (GG_WIDTH * GG_HEIGHT) as usize]
            } else {
                vec![0; (region.framebuffer_width() * region.framebuffer_height()) as usize]
            },
            sprite_buf: [0; 256],
            interrupt: false,
            frame_count: 0,
        }
    }

    /// The current framebuffer, ARGB32, `framebuffer_width()` by
    /// `framebuffer_height()`.
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    /// Width of what the machine displays: the television envelope for a
    /// Master System, the LCD for a Game Gear.
    #[must_use]
    pub const fn framebuffer_width(&self) -> u32 {
        if self.is_game_gear {
            GG_WIDTH
        } else {
            self.region.framebuffer_width()
        }
    }
    /// Height of what the machine displays: the LCD for a Game Gear, and
    /// otherwise the region's own field — 240 lines on NTSC, 288 on PAL.
    #[must_use]
    pub const fn framebuffer_height(&self) -> u32 {
        if self.is_game_gear {
            GG_HEIGHT
        } else {
            self.region.framebuffer_height()
        }
    }

    fn lines_per_frame(&self) -> u16 {
        match self.region {
            VdpRegion::Ntsc => 262,
            VdpRegion::Pal => 313,
        }
    }

    fn mode4_active(&self) -> bool {
        self.regs[0] & 0x04 != 0
    }

    /// Active display height for whatever the four mode bits currently say.
    ///
    /// M2 and M4 are R0 bits 1 and 2; M3 and M1 are R1 bits 3 and 4. Both
    /// extended heights need M2 as well as M4, and the decode is an exact
    /// match rather than "M1 implies 224" — setting M1 and M3 together, or
    /// either without M2, is ordinary 192-line Mode 4. Genesis Plus GX builds
    /// the same four-bit word and tests it for equality against $0E and $16.
    ///
    /// Both are 315-5246 modes. The 315-5124 ignores the bits, which is the
    /// same `system_hw > SYSTEM_SMS` gate as its address-bus masks.
    ///
    /// A Game Gear is held at 192 as well, which is a decision rather than a
    /// fact about the silicon: it carries a 315-5246, so the bits presumably
    /// do something. But its 160x144 window is a physical panel rather than a
    /// television's, and where that panel would sit on a taller raster is not
    /// something any source here answers. No Game Gear software is known to
    /// ask. Modelling it would mean either moving the panel on a guess or
    /// breaking this crate's rule that the window is the centre of the active
    /// area, so it stays at the height the panel was built around.
    fn mode_height(&self) -> u32 {
        if self.is_sms1() || self.is_game_gear {
            return ACTIVE_HEIGHT;
        }
        match (self.regs[0] & 0x06) | (self.regs[1] & 0x18) {
            0x0E => 240,
            0x16 => 224,
            _ => ACTIVE_HEIGHT,
        }
    }

    /// The height this frame is being scanned at.
    ///
    /// Latched, like the vertical scroll: a mode change part-way down a frame
    /// takes effect on the next one. Genesis Plus GX says so in as many
    /// words — "viewport changes should be applied on next frame".
    #[must_use]
    pub const fn active_height(&self) -> u32 {
        self.active_height
    }

    fn display_enabled(&self) -> bool {
        self.regs[1] & 0x40 != 0
    }

    /// R0 bit 5 — draw the leftmost eight pixels as border instead of picture.
    fn left_column_hidden(&self) -> bool {
        self.regs[0] & 0x20 != 0
    }

    /// Whether this chip is the 315-5124.
    ///
    /// SMS Power puts the difference as a logic gate on the VRAM address bus:
    /// several register bits that look unused are ANDed with an address bit,
    /// so clearing one forces that bit to 0 for every fetch of its kind. On
    /// the 315-5246 the gate always gets a 1 from the register and nothing is
    /// masked. Well-behaved software sets all of them, which is why the
    /// difference stays invisible until something leans on it deliberately —
    /// tilemap mirroring being the usual reason.
    const fn is_sms1(&self) -> bool {
        matches!(self.variant, VdpVariant::Sms1)
    }

    fn backdrop_color(&self) -> u32 {
        // Backdrop from sprite palette (palette 1), entry from reg 7 low nibble
        let idx = (self.regs[7] & 0x0F) as usize + 16;
        self.cram_to_argb(idx)
    }

    /// Read-only access to CRAM (Colour RAM).
    ///
    /// SMS: 32 bytes (6-bit RGB), Game Gear: 64 bytes (12-bit RGB).
    pub fn cram(&self) -> &[u8] {
        if self.is_game_gear {
            &self.cram[..64]
        } else {
            &self.cram[..32]
        }
    }

    /// Whether this VDP is in Game Gear mode.
    pub fn is_game_gear(&self) -> bool {
        self.is_game_gear
    }

    /// Which revision of the chip this is.
    ///
    /// The two differ in the mask bits documented on [`is_sms1`](Self::is_sms1)
    /// and in how sprite magnification behaves, so a host that builds one from
    /// a machine profile wants to be able to check it got the one it asked for.
    #[must_use]
    pub const fn variant(&self) -> VdpVariant {
        self.variant
    }

    fn cram_to_argb(&self, index: usize) -> u32 {
        if self.is_game_gear {
            // 12-bit RGB: low byte = xxxxGGGGRRRR, high byte = xxxxBBBB
            let lo = self.cram[(index * 2) & 0x3F] as u32;
            let hi = self.cram[(index * 2 + 1) & 0x3F] as u32;
            let r = (lo & 0x0F) * 17;
            let g = ((lo >> 4) & 0x0F) * 17;
            let b = (hi & 0x0F) * 17;
            0xFF00_0000 | (r << 16) | (g << 8) | b
        } else {
            // 6-bit RGB: %00BBGGRR
            let c = self.cram[index & 0x1F] as u32;
            let r = (c & 0x03) * 85;
            let g = ((c >> 2) & 0x03) * 85;
            let b = ((c >> 4) & 0x03) * 85;
            0xFF00_0000 | (r << 16) | (g << 8) | b
        }
    }

    // -----------------------------------------------------------------------
    // I/O
    // -----------------------------------------------------------------------

    /// Read VDP data port ($BE).
    pub fn read_data(&mut self) -> u8 {
        self.latch_first = true;
        let result = self.read_buffer;
        self.read_buffer = self.vram[self.address as usize & 0x3FFF];
        self.address = (self.address + 1) & 0x3FFF;
        result
    }

    /// Write VDP data port ($BE).
    pub fn write_data(&mut self, value: u8) {
        self.latch_first = true;

        match self.code {
            3 => {
                // CRAM write
                if self.is_game_gear {
                    let addr = self.address as usize & 0x3F;
                    if addr & 1 == 0 {
                        self.cram_latch = value;
                    } else {
                        self.cram[addr & 0xFE] = self.cram_latch;
                        self.cram[addr] = value;
                    }
                } else {
                    self.cram[self.address as usize & 0x1F] = value;
                }
            }
            _ => {
                // VRAM write
                self.vram[self.address as usize & 0x3FFF] = value;
            }
        }
        self.read_buffer = value;
        self.address = (self.address + 1) & 0x3FFF;
    }

    /// Read VDP control/status port ($BF).
    pub fn read_status(&mut self) -> u8 {
        self.latch_first = true;
        // Status bits 4-0 are the fifth-sprite number in the TMS9918 modes
        // this chip inherited, but Mode 4 has no such field and reads them
        // back as ones. Genesis Plus GX does the same and names the title
        // that proves it — "Mode 4 unused bits (fixes PGA Tour Golf)" — so
        // this is not a cosmetic difference; a game reads them and expects
        // them set.
        let result = if self.mode4_active() {
            self.status | 0x1F
        } else {
            self.status
        };
        self.status = 0;
        self.line_irq_pending = false;
        self.interrupt = false;
        result
    }

    /// Write VDP control port ($BF).
    pub fn write_control(&mut self, value: u8) {
        if self.latch_first {
            self.latch_value = value;
            self.latch_first = false;
            // Update address low byte immediately
            self.address = (self.address & 0x3F00) | u16::from(value);
            return;
        }

        self.latch_first = true;
        self.address = u16::from(self.latch_value) | (u16::from(value & 0x3F) << 8);
        self.code = (value >> 6) & 0x03;

        match self.code {
            0 => {
                // VRAM read setup — pre-fetch
                self.read_buffer = self.vram[self.address as usize & 0x3FFF];
                self.address = (self.address + 1) & 0x3FFF;
            }
            2 => {
                // Register write
                let reg = (value & 0x0F) as usize;
                if reg < self.regs.len() {
                    self.regs[reg] = self.latch_value;
                }
                self.update_interrupt();
            }
            _ => {} // Code 1 (VRAM write) or 3 (CRAM write) — just set code
        }
    }

    /// Read V counter ($7E).
    #[must_use]
    pub fn read_v_counter(&self) -> u8 {
        self.v_counter as u8
    }

    /// The H counter's value at the dot the beam is on.
    ///
    /// The counter advances once every two pixels of a 342-pixel line, so it
    /// has 171 distinct values rather than 256: it counts $00 up to $93, then
    /// jumps to $E9 and runs to $FF. MAME states the relation as
    /// `((hpos - 1 - 46) >> 1) & 0xFF` over a 342-pixel raster whose active
    /// display starts at pixel 63, which makes the jump fall out of the
    /// arithmetic rather than needing a table: the pixels before the counter's
    /// origin give a negative difference, and an arithmetic shift of that
    /// truncated to a byte is the $E9-$FF run.
    ///
    /// Our `dot` counts from the first active pixel, so it sits 63 along
    /// MAME's raster and the counter reads $08 on the first active pixel and
    /// $87 on the last. The reference's "roughly $00-$7F" is the right span
    /// and eight counts off on where it starts.
    fn hcount(&self) -> u8 {
        let hclock = i32::from((self.dot + 62) % DOTS_PER_LINE);
        ((hclock - 46) >> 1) as u8
    }

    /// Latch the H counter, as a low-to-high transition on a controller
    /// port's TH pin does.
    ///
    /// This is the whole of the light gun's position sense and the only way
    /// to read a horizontal raster position on this machine: the counter free
    /// runs and the CPU never sees it directly, so what port $7F returns is
    /// whatever was captured here.
    pub fn latch_h_counter(&mut self) {
        self.h_counter = self.hcount();
    }

    /// Read H counter ($7F) — the value latched by the last TH transition.
    #[must_use]
    pub fn read_h_counter(&self) -> u8 {
        self.h_counter
    }

    /// Direct VRAM access.
    #[must_use]
    pub fn vram(&self) -> &[u8; 16384] {
        &self.vram
    }

    /// Direct VRAM write.
    pub fn write_vram(&mut self, addr: u16, value: u8) {
        self.vram[addr as usize & 0x3FFF] = value;
    }

    // -----------------------------------------------------------------------
    // Timing
    // -----------------------------------------------------------------------

    /// Fill the entire framebuffer with the current backdrop colour.
    /// Called at frame start so top + bottom border regions plus the
    /// left + right columns of each active row carry the border
    /// colour. Mid-frame backdrop changes affect the *next* frame —
    /// a v1 simplification, matches the TMS9918 family's treatment.
    fn fill_border(&mut self) {
        let backdrop = self.backdrop_color();
        self.framebuffer.fill(backdrop);
    }

    /// Sample R9 for the frame about to be scanned.
    ///
    /// The vertical scroll is latched once, as the active display starts, so
    /// a write part-way down a frame lands on the *next* one. This is why the
    /// Master System has no vertical raster-scroll trick — a split has to be
    /// built from R8 and the line counter instead. Genesis Plus GX does the
    /// same, latching `vscroll` from `reg[9]` on the line before the active
    /// display and reading it unchanged for every line after.
    fn latch_frame_registers(&mut self) {
        self.vscroll = self.regs[9];
        self.active_height = self.mode_height();
    }

    /// Tick one dot (pixel clock, ~5.37 MHz). Renders the active pixel scanned
    /// out at this dot, and processes the per-line events (line/frame interrupt,
    /// V counter) at line end. Returns true at frame end.
    ///
    /// Per dot, the line interrupt is flagged at the *end* of the line it
    /// belongs to, so a host that interleaves the CPU per dot sees it at the
    /// right scanline — the timing that makes Mode-4 raster splits land
    /// correctly. For a static frame the framebuffer is identical to the old
    /// scanline-batched render (both route every pixel through `bg_pixel`).
    pub fn tick(&mut self) -> bool {
        if self.scanline == 0 && self.dot == 0 {
            self.fill_border();
            self.latch_frame_registers();
        }
        let active_lines = self.active_height as u16;
        if self.scanline < active_lines && self.dot == 0 {
            self.prepare_line_sprites(self.scanline as usize);
        }
        if self.scanline < active_lines && self.dot < ACTIVE_WIDTH as u16 {
            self.render_pixel(self.scanline as usize, self.dot as usize);
        }
        self.dot += 1;
        if self.dot >= DOTS_PER_LINE {
            self.dot = 0;
            return self.advance_line();
        }
        false
    }

    /// Tick one whole scanline (batch render). Kept for tests and any per-line
    /// host; produces identical output to the per-dot path for a static frame.
    pub fn tick_scanline(&mut self) -> bool {
        if self.scanline == 0 {
            self.fill_border();
            self.latch_frame_registers();
        }
        if self.scanline < self.active_height as u16 {
            self.render_scanline(self.scanline);
        }
        self.advance_line()
    }

    /// End-of-line events: line counter / frame interrupt, V counter, interrupt
    /// recompute, and the scanline advance. Returns true at frame end. Shared by
    /// [`tick`](Self::tick) and [`tick_scanline`](Self::tick_scanline).
    fn advance_line(&mut self) -> bool {
        let active_lines = self.active_height as u16;
        if self.scanline < active_lines {
            if self.line_counter == 0 {
                self.line_counter = self.regs[10];
                self.line_irq_pending = true;
            } else {
                self.line_counter -= 1;
            }
        } else if self.scanline == active_lines {
            self.status |= 0x80;
            self.line_counter = self.regs[10];
            self.frame_count += 1;
        } else {
            self.line_counter = self.regs[10];
        }

        // The V counter has to report a scanline in a byte, so past a
        // per-mode threshold it jumps back far enough that the last line of
        // the frame reads $FF. The thresholds are Genesis Plus GX's
        // `vc_table`; the jump is however much the frame overruns 256 lines,
        // which is 6 on NTSC and 57 on PAL whatever the height.
        let vc_max = match (self.region, self.active_height) {
            (VdpRegion::Ntsc, 192) => 0x00DA,
            (VdpRegion::Ntsc, 224) => 0x00EA,
            (VdpRegion::Ntsc, _) => 0x0106,
            (VdpRegion::Pal, 192) => 0x00F2,
            (VdpRegion::Pal, 224) => 0x0102,
            (VdpRegion::Pal, _) => 0x010A,
        };
        let jump = self.lines_per_frame() - 256;
        self.v_counter = if self.scanline <= vc_max {
            self.scanline
        } else {
            self.scanline.wrapping_sub(jump)
        };

        self.update_interrupt();

        self.scanline += 1;
        if self.scanline >= self.lines_per_frame() {
            self.scanline = 0;
            return true;
        }
        false
    }

    fn update_interrupt(&mut self) {
        let frame_irq = self.status & 0x80 != 0 && self.regs[1] & 0x20 != 0;
        let line_irq = self.line_irq_pending && self.regs[0] & 0x10 != 0;
        self.interrupt = frame_irq || line_irq;
    }

    // -----------------------------------------------------------------------
    // Rendering
    // -----------------------------------------------------------------------

    fn active_offset(&self, line: usize) -> usize {
        (self.region.border_top(self.active_height) as usize + line)
            * self.framebuffer_width() as usize
            + self.region.border_left() as usize
    }

    /// Where active-area pixel (`line`, `x`) lands in the framebuffer, or
    /// `None` when this machine does not display it.
    ///
    /// The Master System displays every active pixel inside a border. The
    /// Game Gear displays a 160x144 window from the middle and nothing
    /// else — the rest is rendered by the VDP and never reaches the LCD.
    fn plot_index(&self, line: usize, x: usize) -> Option<usize> {
        if !self.is_game_gear {
            return Some(self.active_offset(line) + x);
        }
        let column = x.checked_sub(GG_ORIGIN_X as usize)?;
        let row = line.checked_sub(GG_ORIGIN_Y as usize)?;
        if column >= GG_WIDTH as usize || row >= GG_HEIGHT as usize {
            return None;
        }
        Some(row * GG_WIDTH as usize + column)
    }

    fn render_scanline(&mut self, line: u16) {
        let line = line as usize;
        self.prepare_line_sprites(line);
        for x in 0..ACTIVE_WIDTH as usize {
            self.render_pixel(line, x);
        }
    }

    /// Draw the active pixel at column `x` of `line`. In Mode 4 the background
    /// and this line's sprite pixel are arbitrated by the SMS priority rule:
    /// the sprite is shown **unless** an *opaque* background pixel
    /// (`color_idx != 0`) belongs to a tile whose priority bit is set — then the
    /// foreground background tile occludes the sprite (status bars, HUD layers).
    /// Background `color_idx` 0 is transparent for this comparison, so sprites
    /// always show through it regardless of the priority bit.
    fn render_pixel(&mut self, line: usize, x: usize) {
        let sprite = self.sprite_buf[x];
        let argb = if self.display_enabled() && self.mode4_active() {
            if self.left_column_hidden() && x < 8 {
                // R0 bit 5 draws the leftmost eight pixels as border, and the
                // reference's rendering order applies that *after* sprites are
                // composited. That ordering is the point of the bit: SMS Power
                // notes that showing the column is what stops sprites
                // scrolling smoothly off either edge, so what the mask has to
                // hide is sprites, not background alone.
                self.backdrop_color()
            } else {
                let (bg_idx, bg_priority, palette) = self.mode4_bg_lookup(line, x);
                let bg_opaque = bg_idx != 0;
                if sprite != 0 && !(bg_priority && bg_opaque) {
                    self.cram_to_argb(16 + sprite as usize)
                } else if bg_opaque || bg_priority {
                    self.cram_to_argb(palette + bg_idx as usize)
                } else {
                    self.backdrop_color()
                }
            }
        } else if sprite != 0 {
            self.cram_to_argb(16 + sprite as usize)
        } else {
            self.bg_pixel(line, x)
        };
        if let Some(index) = self.plot_index(line, x) {
            self.framebuffer[index] = argb;
        }
    }

    /// Background colour when active Mode-4 rendering is not in effect (display
    /// blanked or a placeholder legacy TMS9918 mode) — both render as backdrop.
    fn bg_pixel(&self, _line: usize, _x: usize) -> u32 {
        self.backdrop_color()
    }

    /// Background colour-index, tile priority bit, and palette base at column
    /// `pixel_x` of `line` in Mode 4. `color_idx` 0 is transparent for sprite
    /// priority; `priority` is the tile's foreground bit. The colour itself is
    /// resolved by the caller so it can arbitrate against the sprite pixel.
    fn mode4_bg_lookup(&self, line: usize, pixel_x: usize) -> (u8, bool, usize) {
        // R2 in the 192-line mode is bits 3-1 times $800. In the tall modes
        // only bits 3-2 count, times $1000, with $700 added — so the same
        // R2 = $FF that gives $3800 in one gives $3700 in the other.
        let name_base = if self.active_height > ACTIVE_HEIGHT {
            (self.regs[2] as usize & 0x0C) * 0x400 + 0x700
        } else {
            (self.regs[2] as usize & 0x0E) * 0x400
        };
        let scroll_x = self.regs[8] as usize;
        let hscroll_lock = self.regs[0] & 0x40 != 0;
        let vscroll_lock = self.regs[0] & 0x80 != 0;

        // R0 bit 7 holds screen columns 24-31 — pixels 192 and up — at
        // vertical scroll 0, which is how a vertically scrolling game gives
        // itself a fixed status panel down the right-hand side. The lock is
        // keyed to the screen column, before horizontal scrolling moves the
        // background under it.
        let scroll_y = if vscroll_lock && pixel_x >= 192 {
            0
        } else {
            self.vscroll as usize
        };

        // The name table is 28 rows in the 192 and 224-line modes and 32 in
        // the 240-line one, so the scroll wraps at 224 pixels or 256.
        let wrap = if self.active_height > 224 { 256 } else { 224 };
        let effective_line = (line + scroll_y) % wrap;
        let tile_row = effective_line / 8;
        let fine_y = effective_line & 7;

        // Horizontal scroll (disabled for the top 2 rows if hscroll_lock).
        let scrolled_x = if hscroll_lock && line < 16 {
            pixel_x
        } else {
            (pixel_x + (256 - scroll_x)) & 0xFF
        };
        let tile_col = scrolled_x / 8;
        let fine_x = scrolled_x & 7;

        // Name table entry (2 bytes, little-endian).
        let mut nt_addr = name_base + (tile_row * 32 + tile_col) * 2;
        // R2 bit 0 is ANDed with the high bit of the row, so on a 315-5124
        // with it clear the bottom half of the tilemap mirrors the top.
        if self.is_sms1() && self.regs[2] & 0x01 == 0 {
            nt_addr &= !0x0400;
        }
        let nt_lo = self.vram[nt_addr & 0x3FFF] as u16;
        let nt_hi = self.vram[(nt_addr + 1) & 0x3FFF] as u16;
        let nt_entry = nt_lo | (nt_hi << 8);

        let pattern_idx = (nt_entry & 0x01FF) as usize;
        let h_flip = nt_entry & 0x0200 != 0;
        let v_flip = nt_entry & 0x0400 != 0;
        let palette = if nt_entry & 0x0800 != 0 { 16 } else { 0 };
        let priority = nt_entry & 0x1000 != 0;

        let row = if v_flip { 7 - fine_y } else { fine_y };
        let col = if h_flip { fine_x } else { 7 - fine_x };

        // 4bpp planar: 4 bytes per row, 32 bytes per tile.
        //
        // On a 315-5124 the tile index is masked differently for each half of
        // the bitplanes: R3's eight bits gate index bits 8-1 when fetching
        // planes 0 and 1, R4's low three gate bits 8-6 when fetching planes 2
        // and 3. The two halves of a pixel's colour can therefore come from
        // two different tiles.
        let (low_idx, high_idx) = if self.is_sms1() {
            (
                pattern_idx & (0x001 | (usize::from(self.regs[3]) << 1)),
                pattern_idx & (0x03F | (usize::from(self.regs[4] & 0x07) << 6)),
            )
        } else {
            (pattern_idx, pattern_idx)
        };
        let low_addr = low_idx * 32 + row * 4;
        let high_addr = high_idx * 32 + row * 4;
        let b0 = self.vram[low_addr & 0x3FFF];
        let b1 = self.vram[(low_addr + 1) & 0x3FFF];
        let b2 = self.vram[(high_addr + 2) & 0x3FFF];
        let b3 = self.vram[(high_addr + 3) & 0x3FFF];

        let color_idx = ((b0 >> col) & 1)
            | (((b1 >> col) & 1) << 1)
            | (((b2 >> col) & 1) << 2)
            | (((b3 >> col) & 1) << 3);

        (color_idx, priority, palette)
    }

    /// Prepare this line's sprite overlay: clear `sprite_buf`, then (when the
    /// display is on and Mode 4 is active) evaluate the sprite table into it and
    /// set the overflow / collision status flags.
    fn prepare_line_sprites(&mut self, line: usize) {
        self.sprite_buf = [0u8; 256];
        if self.display_enabled() && self.mode4_active() {
            self.evaluate_sprites(line);
        }
    }

    fn evaluate_sprites(&mut self, line: usize) {
        let sat_base = (self.regs[5] as usize & 0x7E) * 0x80;
        let spg_base = if self.regs[6] & 0x04 != 0 {
            0x2000
        } else {
            0x0000
        };
        let tall_sprites = self.regs[1] & 0x02 != 0;
        let pattern_height: usize = if tall_sprites { 16 } else { 8 };
        // R1 bit 0 magnifies every sprite pixel to 2x2. The pattern is the
        // same size either way — one tile for an 8x8, a pair for an 8x16 —
        // so what doubles is the ground it covers, and a magnified sprite is
        // on twice as many lines as its pattern has rows.
        let zoom = usize::from(self.regs[1] & 0x01 != 0);
        let sprite_height = pattern_height << zoom;
        let shift_left = self.regs[0] & 0x08 != 0;

        let mut sprite_buffer = [0u8; 256]; // Color index per pixel
        let mut collision = false;

        // Evaluation first, drawing second. The 315-5124's magnification rule
        // is stated in terms of how many sprites are on the line, so the
        // count has to be known before the first one is drawn.
        let mut chosen = [(0usize, 0usize); 8]; // (sprite index, first line)
        let mut count = 0usize;
        for sprite in 0..64 {
            let y_raw = self.vram[(sat_base + sprite) & 0x3FFF];

            // $D0 ends the list, but only in the 192-line mode. In the tall
            // modes it is an ordinary Y coordinate, and a chip that still
            // treated it as a terminator would truncate every sprite list.
            if y_raw == 0xD0 && self.active_height == ACTIVE_HEIGHT {
                break;
            }

            let y = y_raw as usize + 1;
            if line < y || line >= y + sprite_height {
                continue;
            }

            if count == 8 {
                self.status |= 0x40;
                break;
            }
            chosen[count] = (sprite, y);
            count += 1;
        }

        for (slot, &(sprite, y)) in chosen[..count].iter().enumerate() {
            // On the 315-5124 magnification stretches every sprite
            // vertically but only the first N-4 of the N sprites on the line
            // horizontally, so how many widen depends on how crowded the line
            // is — and on a line of four or fewer, none of them do. Genesis
            // Plus GX has the same rule from the other end: "last 4 sprites
            // can not be zoomed".
            let h_zoom = if self.is_sms1() && slot + 4 >= count {
                0
            } else {
                zoom
            };

            // X and pattern from second half of SAT. R5 bit 0 is ANDed with
            // address bit 7, so on a 315-5124 with it clear both fold back
            // into the Y half and a sprite reads its position out of the
            // coordinate bytes.
            let mut offset = 0x80 + sprite * 2;
            if self.is_sms1() && self.regs[5] & 0x01 == 0 {
                offset &= !0x80;
            }
            let x_addr = sat_base + offset;
            let mut x = self.vram[x_addr & 0x3FFF] as i16;
            let mut pattern = self.vram[(x_addr + 1) & 0x3FFF] as usize;

            if shift_left {
                x -= 8;
            }
            if tall_sprites {
                pattern &= 0xFE;
            }
            // R6's low two bits gate tile-number bits 7 and 6, cutting the
            // sprite tile set to 128 or 64 on a 315-5124.
            if self.is_sms1() {
                pattern &= 0x3F | (usize::from(self.regs[6] & 0x03) << 6);
            }

            let sprite_row = (line - y) >> zoom;
            let pattern_addr = spg_base + pattern * 32 + sprite_row * 4;

            let b0 = self.vram[(pattern_addr) & 0x3FFF];
            let b1 = self.vram[(pattern_addr + 1) & 0x3FFF];
            let b2 = self.vram[(pattern_addr + 2) & 0x3FFF];
            let b3 = self.vram[(pattern_addr + 3) & 0x3FFF];

            for bit in 0..8usize {
                let col = 7 - bit;
                let color_idx = ((b0 >> col) & 1)
                    | (((b1 >> col) & 1) << 1)
                    | (((b2 >> col) & 1) << 2)
                    | (((b3 >> col) & 1) << 3);

                if color_idx == 0 {
                    continue;
                }

                // One screen pixel per pattern bit, or two side by side when
                // the sprite is magnified.
                for step in 0..(1usize << h_zoom) {
                    let px = x + ((bit << h_zoom) + step) as i16;
                    if !(0..256).contains(&px) {
                        continue;
                    }
                    let px = px as usize;

                    if sprite_buffer[px] != 0 {
                        collision = true;
                    } else {
                        sprite_buffer[px] = color_idx;
                    }
                }
            }
        }

        if collision {
            self.status |= 0x20;
        }

        // Publish the line's sprite pixels; `render_pixel` overlays them.
        self.sprite_buf = sprite_buffer;
    }

    // -----------------------------------------------------------------------
    // Save/load state
    // -----------------------------------------------------------------------

    /// Serialize VDP state to a byte vector.
    ///
    /// Layout: regs (11) + status (1) + read_buffer (1) + address (2) +
    /// code (1) + latch_first (1) + latch_value (1) + cram_latch (1) +
    /// v_counter (2) + h_counter (1) + line_counter (1) + line_irq_pending (1) +
    /// vscroll (1) + active_height (1) + scanline (2) + interrupt (1) +
    /// frame_count (8) + vram (16384) + cram (64) = 16485 bytes.
    pub fn save_state(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.regs);
        out.push(self.status);
        out.push(self.read_buffer);
        out.extend_from_slice(&self.address.to_le_bytes());
        out.push(self.code);
        out.push(u8::from(self.latch_first));
        out.push(self.latch_value);
        out.push(self.cram_latch);
        out.extend_from_slice(&self.v_counter.to_le_bytes());
        out.push(self.h_counter);
        out.push(self.line_counter);
        out.push(u8::from(self.line_irq_pending));
        out.push(self.vscroll);
        // 192, 224 or 240 — a byte holds any of them.
        out.push(self.active_height as u8);
        out.extend_from_slice(&self.scanline.to_le_bytes());
        out.push(u8::from(self.interrupt));
        out.extend_from_slice(&self.frame_count.to_le_bytes());
        out.extend_from_slice(&self.vram);
        out.extend_from_slice(&self.cram);
    }

    /// Restore VDP state from a byte slice. Returns bytes consumed or error.
    pub fn load_state(&mut self, data: &[u8]) -> Result<usize, String> {
        let needed =
            11 + 1 + 1 + 2 + 1 + 1 + 1 + 1 + 2 + 1 + 1 + 1 + 1 + 1 + 2 + 1 + 8 + 16384 + 64;
        if data.len() < needed {
            return Err("SegaVdp state truncated".into());
        }
        let mut p = 0;
        self.regs.copy_from_slice(&data[p..p + 11]);
        p += 11;
        self.status = data[p];
        p += 1;
        self.read_buffer = data[p];
        p += 1;
        self.address = u16::from_le_bytes([data[p], data[p + 1]]);
        p += 2;
        self.code = data[p];
        p += 1;
        self.latch_first = data[p] != 0;
        p += 1;
        self.latch_value = data[p];
        p += 1;
        self.cram_latch = data[p];
        p += 1;
        self.v_counter = u16::from_le_bytes([data[p], data[p + 1]]);
        p += 2;
        self.h_counter = data[p];
        p += 1;
        self.line_counter = data[p];
        p += 1;
        self.line_irq_pending = data[p] != 0;
        p += 1;
        self.vscroll = data[p];
        p += 1;
        self.active_height = u32::from(data[p]);
        p += 1;
        self.scanline = u16::from_le_bytes([data[p], data[p + 1]]);
        p += 2;
        self.interrupt = data[p] != 0;
        p += 1;
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&data[p..p + 8]);
        p += 8;
        self.frame_count = u64::from_le_bytes(bytes);
        self.vram.copy_from_slice(&data[p..p + 16384]);
        p += 16384;
        self.cram.copy_from_slice(&data[p..p + 64]);
        p += 64;
        Ok(p)
    }

    /// Read-only access to registers.
    #[must_use]
    pub fn registers(&self) -> &[u8; 11] {
        &self.regs
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #[test]
    fn a_television_holds_exactly_the_field_its_region_shows() {
        // 240 lines on NTSC, 288 on PAL. One height served both and it was
        // NTSC's, so a PAL Master System showed 240 lines of a 288-line field
        // — the 83% the #1054 audit read across this chip and the TMS9918
        // family alike.
        for (region, field, border) in [(VdpRegion::Ntsc, 240, 25), (VdpRegion::Pal, 288, 51)] {
            let vdp = SegaVdp::new(region, VdpVariant::Sms2);
            assert_eq!(vdp.framebuffer_height(), field, "{region:?}");
            assert_eq!(
                vdp.framebuffer().len(),
                (region.framebuffer_width() * field) as usize,
                "{region:?} allocated a buffer of the wrong size"
            );
            assert_eq!(region.border_top(ACTIVE_HEIGHT), border, "{region:?}");
            assert_eq!(
                region.border_top(ACTIVE_HEIGHT)
                    + ACTIVE_HEIGHT
                    + region.border_bottom(ACTIVE_HEIGHT),
                field,
                "{region:?} does not account for every line of its field"
            );
        }
    }

    #[test]
    fn the_game_gear_keeps_its_lcd_whatever_the_region() {
        // A panel is not a field. The handheld shows 160x144 of the active
        // area and has no border at all, so the region's television geometry
        // must not reach it.
        let gg = SegaVdp::new_game_gear();
        assert_eq!(gg.framebuffer_width(), GG_WIDTH);
        assert_eq!(gg.framebuffer_height(), GG_HEIGHT);
        assert_eq!(gg.framebuffer().len(), (GG_WIDTH * GG_HEIGHT) as usize);
    }

    use super::*;

    #[test]
    fn new_vdp_has_blank_framebuffer() {
        let vdp = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2);
        assert_eq!(
            vdp.framebuffer().len(),
            (VdpRegion::Ntsc.framebuffer_width() * VdpRegion::Ntsc.framebuffer_height()) as usize
        );
    }

    #[test]
    fn control_port_register_write() {
        let mut vdp = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2);
        vdp.write_control(0x44); // value
        vdp.write_control(0x81); // register 1
        assert_eq!(vdp.regs[1], 0x44);
    }

    #[test]
    fn vram_write_and_read() {
        let mut vdp = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2);
        // Set write address $0000 (code 01)
        vdp.write_control(0x00);
        vdp.write_control(0x40);
        vdp.write_data(0xAB);
        vdp.write_data(0xCD);
        assert_eq!(vdp.vram[0], 0xAB);
        assert_eq!(vdp.vram[1], 0xCD);
    }

    #[test]
    fn cram_write_sms() {
        let mut vdp = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2);
        // Set CRAM write address $00 (code 11 = $C0)
        vdp.write_control(0x00);
        vdp.write_control(0xC0);
        vdp.write_data(0x3F); // White-ish (R=3, G=3, B=3)
        assert_eq!(vdp.cram[0], 0x3F);
    }

    #[test]
    fn cram_write_game_gear() {
        let mut vdp = SegaVdp::new_game_gear();
        // Set CRAM write address $00
        vdp.write_control(0x00);
        vdp.write_control(0xC0);
        vdp.write_data(0xF0); // Even byte: GG=F, RR=0
        vdp.write_data(0x0F); // Odd byte: BB=F
        // Should write to CRAM[0] and CRAM[1]
        assert_eq!(vdp.cram[0], 0xF0);
        assert_eq!(vdp.cram[1], 0x0F);
    }

    #[test]
    fn status_clears_on_read() {
        let mut vdp = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2);
        vdp.status = 0xE0; // All flags set
        let s = vdp.read_status();
        assert_eq!(s, 0xE0);
        assert_eq!(vdp.status, 0);
    }

    #[test]
    fn ntsc_frame_is_262_lines() {
        let mut vdp = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2);
        let mut frames = 0;
        for _ in 0..262 {
            if vdp.tick_scanline() {
                frames += 1;
            }
        }
        assert_eq!(frames, 1);
    }

    #[test]
    fn pal_frame_is_313_lines() {
        let mut vdp = SegaVdp::new(VdpRegion::Pal, VdpVariant::Sms2);
        let mut frames = 0;
        for _ in 0..313 {
            if vdp.tick_scanline() {
                frames += 1;
            }
        }
        assert_eq!(frames, 1);
    }

    #[test]
    fn mode4_detection() {
        let mut vdp = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2);
        assert!(!vdp.mode4_active());
        vdp.regs[0] = 0x04;
        assert!(vdp.mode4_active());
    }

    #[test]
    fn mode4_priority_tile_occludes_sprite() {
        // SMS BG-over-sprite priority: a sprite shows unless an opaque
        // background pixel belongs to a tile whose priority bit is set.
        fn setup(priority: bool, bg_opaque: bool) -> SegaVdp {
            let mut vdp = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2);
            vdp.regs[0] = 0x04; // Mode 4; no column-0 blank, no hscroll lock
            vdp.regs[1] = 0x40; // display on
            vdp.regs[2] = 0x00; // name table base $0000
            // Tile (0,0): pattern 1, palette 0, priority optional.
            let nt_entry: u16 = 0x0001 | if priority { 0x1000 } else { 0 };
            vdp.vram[0] = (nt_entry & 0xFF) as u8;
            vdp.vram[1] = (nt_entry >> 8) as u8;
            // Pattern 1, row 0: leftmost pixel (col 7) = colour index 1 if opaque.
            vdp.vram[32] = if bg_opaque { 0x80 } else { 0x00 };
            vdp.cram[1] = 0x3F; // BG colour 1 = white
            vdp.cram[16 + 5] = 0x03; // sprite colour 5 = red
            vdp.sprite_buf[0] = 5; // a sprite pixel at column 0
            vdp
        }
        let fb = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2).active_offset(0);

        // Opaque, high-priority background occludes the sprite.
        let mut vdp = setup(true, true);
        vdp.render_pixel(0, 0);
        assert_eq!(
            vdp.framebuffer[fb],
            vdp.cram_to_argb(1),
            "opaque priority tile should occlude the sprite"
        );

        // Without the priority bit, the sprite shows over the same tile.
        let mut vdp = setup(false, true);
        vdp.render_pixel(0, 0);
        assert_eq!(
            vdp.framebuffer[fb],
            vdp.cram_to_argb(16 + 5),
            "non-priority tile must not occlude the sprite"
        );

        // Priority bit set but the background pixel is transparent (index 0):
        // a transparent BG pixel is never in front, so the sprite shows.
        let mut vdp = setup(true, false);
        vdp.render_pixel(0, 0);
        assert_eq!(
            vdp.framebuffer[fb],
            vdp.cram_to_argb(16 + 5),
            "transparent background (index 0) never occludes, even with priority"
        );
    }

    #[test]
    fn sms_palette_conversion() {
        let mut vdp = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2);
        // White: R=3, G=3, B=3 = $3F
        vdp.cram[0] = 0x3F;
        let argb = vdp.cram_to_argb(0);
        assert_eq!(argb, 0xFF_FF_FF_FF);

        // Black: $00
        vdp.cram[1] = 0x00;
        let argb = vdp.cram_to_argb(1);
        assert_eq!(argb, 0xFF_00_00_00);
    }

    #[test]
    fn gg_palette_conversion() {
        let mut vdp = SegaVdp::new_game_gear();
        // White: R=F, G=F, B=F
        vdp.cram[0] = 0xFF; // GGRR = FF
        vdp.cram[1] = 0x0F; // BB = F
        let argb = vdp.cram_to_argb(0);
        assert_eq!(argb, 0xFF_FF_FF_FF);
    }

    #[test]
    fn line_interrupt_counter() {
        let mut vdp = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2);
        vdp.regs[1] = 0x40; // Display on
        vdp.regs[0] = 0x14; // Mode 4 + line IRQ enable
        vdp.regs[10] = 5; // Fire every 5 lines

        // Tick 6 scanlines — counter should reach 0 and fire
        for _ in 0..6 {
            vdp.tick_scanline();
        }
        assert!(vdp.line_irq_pending);
        assert!(vdp.interrupt);
    }

    /// #1003: both machines reported 288x240, so nothing downstream could
    /// tell a Game Gear frame from a Master System one. The dimensions must
    /// differ, and they must match the buffer that carries them.
    #[test]
    fn the_two_machines_display_different_sized_screens() {
        let sms = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2);
        let gg = SegaVdp::new_game_gear();

        // 280 x 240 is the NTSC window: 5.369318 MHz over 52.148 µs, and 240
        // lines. It was 288 x 240 while the horizontal border was a fixed 16
        // either side of the active 256.
        assert_eq!(
            (sms.framebuffer_width(), sms.framebuffer_height()),
            (280, 240)
        );
        assert_eq!(
            (gg.framebuffer_width(), gg.framebuffer_height()),
            (160, 144)
        );
        assert_ne!(
            (sms.framebuffer_width(), sms.framebuffer_height()),
            (gg.framebuffer_width(), gg.framebuffer_height()),
            "a Game Gear frame must not be mistakable for a Master System one"
        );

        for vdp in [&sms, &gg] {
            assert_eq!(
                vdp.framebuffer().len(),
                (vdp.framebuffer_width() * vdp.framebuffer_height()) as usize,
                "the buffer and the dimensions reported for it must agree"
            );
        }
    }

    /// The window is cut from the centre, so an active pixel outside it is
    /// rendered and discarded rather than wrapping into the visible area.
    #[test]
    fn the_game_gear_window_is_the_centre_of_the_active_area() {
        let gg = SegaVdp::new_game_gear();
        let gg_origin_y = GG_ORIGIN_Y;

        assert_eq!(
            gg.plot_index(0, 0),
            None,
            "top-left of the active area is off-LCD"
        );
        assert_eq!(
            gg.plot_index(gg_origin_y as usize, GG_ORIGIN_X as usize),
            Some(0),
            "the window's first pixel is the buffer's first pixel"
        );
        let last_row = (gg_origin_y + GG_HEIGHT - 1) as usize;
        let last_column = (GG_ORIGIN_X + GG_WIDTH - 1) as usize;
        assert_eq!(
            gg.plot_index(last_row, last_column),
            Some((GG_WIDTH * GG_HEIGHT - 1) as usize),
            "the window's last pixel is the buffer's last pixel"
        );
        assert_eq!(
            gg.plot_index(last_row, last_column + 1),
            None,
            "one column past the window must not wrap onto the next row"
        );
        assert_eq!(
            gg.plot_index(last_row + 1, last_column),
            None,
            "one row past the window must not run off the buffer"
        );
    }

    /// The Master System keeps its border, and the active area still starts
    /// inside it.
    #[test]
    fn the_master_system_keeps_its_border() {
        let sms = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2);
        assert_eq!(
            sms.plot_index(0, 0),
            Some(
                (VdpRegion::Ntsc.border_top(ACTIVE_HEIGHT) * VdpRegion::Ntsc.framebuffer_width()
                    + VdpRegion::Ntsc.border_left()) as usize
            )
        );
    }
}
