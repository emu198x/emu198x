//! Atari GTIA (George's Television Interface Adapter) emulator.
//!
//! Adapted from `Emu198x-Oldest/crates/atari-gtia` (port 2026-06-01) for
//! the Atari 5200 / 800XL / 130XE / XEGS family. Self-contained, no
//! external chip dependencies.
//!
//! The GTIA receives playfield pixel data from ANTIC and overlays
//! player/missile graphics to produce final ARGB32 video output.
//! Used in the Atari 5200 and 8-bit computer line (400/800/XL/XE).

pub mod palette;

use palette::{NTSC_PALETTE, PAL_PALETTE};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// The **normal** playfield: 160 colour clocks, 320 pixels at hires.
///
/// Not the width GTIA composites. A narrow playfield is smaller than this and
/// a wide one is larger, and players and missiles reach either side of all
/// three — the line runs the width of the window, and this is one span within
/// it. Sizing the compositor to this constant is what clipped a wide playfield
/// (#1086); it survives because the window is still defined as this plus a
/// border either side.
pub const ACTIVE_WIDTH: u32 = 320;

/// Pixel clock of the NTSC part: twice the 3.579545 MHz colour clock, because
/// the hires modes put two pixels in each. Gives 6:7 pixels — taller than
/// they are wide, the Atari 8-bit's published ratio.
pub const NTSC_PIXEL_CLOCK_HZ: f64 = 7_159_090.0;

/// The same on PAL, from the 3.546894 MHz colour clock.
pub const PAL_PIXEL_CLOCK_HZ: f64 = 7_093_788.0;

/// Active playfield height in scan lines — ANTIC's maximum, the same on both
/// regions. What differs is how much field is left around it.
pub const ACTIVE_HEIGHT: u32 = 240;

/// Which television standard the chip is feeding.
///
/// GTIA renders the same 240 active lines either way; the region decides how
/// tall the field around them is, and so where the active window sits in it.
/// A single height cannot serve both — 288 lines on NTSC is a fifth more
/// raster than a set displays, and 240 on PAL is a sixth less.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GtiaRegion {
    /// 240 visible scan lines. ANTIC's 240 fill the field, leaving no border.
    Ntsc,
    /// 288 visible scan lines, so 48 of border around the active 240.
    Pal,
}

impl GtiaRegion {
    /// Scan lines a set displays, which is what the framebuffer holds.
    ///
    /// Per `knowledge/decisions/the-framebuffer-is-the-sets-window.md`.
    #[must_use]
    pub const fn framebuffer_height(self) -> u32 {
        match self {
            Self::Ntsc => 240,
            Self::Pal => 288,
        }
    }

    /// Scan lines of border above the active playfield.
    ///
    /// Halving what the field has left over is right on NTSC and a line or two
    /// out on PAL, and the difference is where vertical sync falls. The
    /// Altirra Hardware Reference Manual (ch. 6, "Vertical sync occurs over
    /// three scan lines. In NTSC, it occupies lines 251-253, and in PAL,
    /// 275-277") pins it, with ANTIC's display fixed at scan lines 8-247 in
    /// both regions.
    ///
    /// On NTSC that leaves exactly 22 lines outside the display — 251 to 261
    /// and 0 to 7 — and 262 less the 240 a set shows is exactly 22. The
    /// Atari's display *is* the NTSC field, so there is no border to place and
    /// nothing to get wrong.
    ///
    /// PAL moves sync 24 lines later and adds 50 to the frame, so 72 lines sit
    /// outside the display and a set hides only 24 of them. Blanking runs from
    /// about two and a half lines before the broad pulses to line 23 of the
    /// field, which puts the display 23 lines below the top of what a set
    /// shows. The same arithmetic on NTSC lands within a line of the zero that
    /// case reaches exactly, which is the check on it.
    #[must_use]
    pub const fn border_top(self) -> u32 {
        match self {
            Self::Ntsc => 0,
            Self::Pal => 23,
        }
    }

    /// Pixels a set displays along a line, which is the framebuffer's width.
    ///
    /// `pixel_clock x active_line_seconds`: 7.15909 MHz over 52.148 µs is 373
    /// on NTSC, and 7.093788 MHz over 52.0 µs is 369 on PAL, rounded to leave
    /// a whole border either side of the active 320.
    ///
    /// This used to be a fixed 32 pixels of border either side, giving 384 for
    /// both regions — 103% of an NTSC window and 104% of a PAL one, which is
    /// raster a set hides.
    #[must_use]
    pub const fn framebuffer_width(self) -> u32 {
        ACTIVE_WIDTH + 2 * self.border_left()
    }

    /// Pixels of border left of the active area — what the line has left over.
    ///
    /// Centring is exact here rather than merely close, which is worth saying
    /// because it is not exact on every chip. Altirra's GTIA chapter gives the
    /// 228-colour-clock line a visible range of `$22`-`$DD` — 188 colour
    /// clocks — with the normal playfield at `$30`-`$CF`, so 14 colour clocks
    /// of visible border sit either side of it and the visible range's
    /// midpoint falls on the playfield centre at the `$7F`/`$80` boundary. The
    /// chip's picture is centred in its own line, so ours is centred in the
    /// window. 188 colour clocks is 376 pixels, two more than the 374 a set
    /// shows.
    #[must_use]
    pub const fn border_left(self) -> u32 {
        match self {
            Self::Ntsc => 27,
            Self::Pal => 24,
        }
    }

    /// The half colour clock the framebuffer's first pixel sits on.
    ///
    /// The scan line is 228 colour clocks and the framebuffer holds the part
    /// of it a set shows, so the two need a shared origin — otherwise every
    /// question about where something lands has to be asked in a coordinate
    /// space that only covers the normal playfield, which is how a wide one
    /// came to be clipped to 320 pixels (#1086).
    ///
    /// A pixel is half a colour clock. The normal playfield runs `$30`-`$CF`
    /// and its first pixel sits [`border_left`](Self::border_left) into the
    /// window, so the window opens `border_left` half-clocks before colour
    /// clock 48: 69 on NTSC and 72 on PAL. Both fall inside the `$22`-`$DD`
    /// Altirra gives as the visible range, 68 to 444 in half-clocks.
    #[must_use]
    pub const fn first_half_clock(self) -> u16 {
        PF_LEFT_CC * 2 - self.border_left() as u16
    }
}

/// First visible colour clock in the normal playfield (160 clocks wide).
const PF_LEFT_CC: u16 = 48;

/// The colour clock every playfield width is centred on.
///
/// Altirra: "The first color clock just to the right of the center is at
/// `$80`." All three widths share it, so a wider playfield reaches further out
/// on both sides rather than starting further left.
const PF_CENTRE_CC: u16 = 128;

/// The colour clock a playfield of `width_cc` maps its first data pixel to.
const fn playfield_origin_cc(width_cc: u16) -> u16 {
    PF_CENTRE_CC - width_cc / 2
}

/// The colour clocks a playfield of `width_cc` actually reaches the screen at,
/// as a half-open range.
///
/// Not the same as where ANTIC maps it, and that is the whole point. Altirra's
/// GTIA chapter: narrow playfields "are displayed at color clocks `$40`-`$BF`",
/// normal "at `$30`-`$CF`", and wide are "mapped to positions `$20`-`$DF` (192
/// color clocks), but clipped to display only within `$2C`-`$DD` (178 color
/// clocks). The left edge is clipped by ANTIC by 10 color clocks, and displays
/// background color even in hires modes."
///
/// So a wide playfield loses 12 colour clocks off its left edge to ANTIC and
/// two off its right to horizontal blank — but the 178 that remain are 356
/// pixels, and the window holds 374. Clipping it to the normal playfield's 320
/// lost 36 more and shifted what was left, because the leftmost 320 data
/// pixels are not the 320 the hardware shows.
const fn playfield_display_cc(width_cc: u16) -> (u16, u16) {
    match width_cc {
        128 => (64, 192),
        192 => (44, 222),
        _ => (48, 208),
    }
}

/// Number of players.
const NUM_PLAYERS: usize = 4;

/// Number of missiles.
const NUM_MISSILES: usize = 4;

// ---------------------------------------------------------------------------
// ANTIC mode enum
// ---------------------------------------------------------------------------

/// ANTIC display mode, passed to GTIA so it knows how to interpret playfield
/// pixel data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnticMode {
    /// Blank scan line (no playfield data).
    Blank,
    /// Mode 2 — 40-column text, 1.5 colour.
    Mode2,
    /// Mode 3 — 40-column text (descenders).
    Mode3,
    /// Mode 4 — 40-column multi-colour text.
    Mode4,
    /// Mode 5 — 40-column multi-colour text (double height).
    Mode5,
    /// Mode 6 — 20-column text, 5 colours.
    Mode6,
    /// Mode 7 — 20-column text, 5 colours (double height).
    Mode7,
    /// Mode 8 — 40-pixel wide, 4-colour graphics.
    Mode8,
    /// Mode 9 — 80-pixel wide, 2-colour graphics.
    Mode9,
    /// Mode A — 80-pixel wide, 4-colour graphics.
    ModeA,
    /// Mode B — 160-pixel wide, 2-colour graphics.
    ModeB,
    /// Mode C — 160-pixel wide, 2-colour (single scan-line height).
    ModeC,
    /// Mode D — 160-pixel wide, 4-colour (Graphics 7).
    ModeD,
    /// Mode E — 160-pixel wide, 4-colour (single height, Graphics 15).
    ModeE,
    /// Mode F — 320-pixel wide, 2-colour (Graphics 8, hires).
    ModeF,
}

impl AnticMode {
    /// Whether the mode sends two pixels per colour clock: modes 2, 3 and F.
    #[must_use]
    pub const fn is_hires(self) -> bool {
        matches!(self, Self::Mode2 | Self::Mode3 | Self::ModeF)
    }
}

// ---------------------------------------------------------------------------
// GTIA chip
// ---------------------------------------------------------------------------

/// Atari GTIA graphics chip.
#[derive(Serialize, Deserialize)]
pub struct Gtia {
    /// PAL register, read-only at `$D014`/`$C014`. Software reads it to
    /// find out which television standard it is running on: `$0F` for
    /// NTSC, `$01` for PAL (MAME `src/mame/atari/gtia.cpp:270`,
    /// `m_r.pal = is_ntsc() ? 0x0f : 0x01`).
    ///
    /// It is not decoration. Joust reads `$C014`, compares against `$0F`,
    /// and picks an all-zero palette when it does not match — a machine
    /// that answers neither value renders the whole game black.
    pal: u8,

    // -- Colour registers --
    colpm: [u8; 4], // COLPM0-3: player/missile colours
    colpf: [u8; 4], // COLPF0-3: playfield colours
    colbk: u8,      // COLBK: background

    // -- Player/missile position --
    hposp: [u8; 4], // HPOSPx: horizontal position of players
    hposm: [u8; 4], // HPOSMx: horizontal position of missiles

    // -- Player/missile size --
    sizep: [u8; 4], // SIZEPx: player size (bits 0-1)
    sizem: u8,      // SIZEM: missile sizes (2 bits each)

    // -- Player/missile graphics --
    grafp: [u8; 4], // GRAFPx: 8-bit player graphic patterns
    /// Last DMA bytes held back for objects whose VDELAY bit is set. In the
    /// two-line display GTIA latches a delayed object one line later, so the
    /// previous byte stays on screen for the extra line.
    #[serde(default)]
    vdelay_pending_grafp: [u8; 4],
    #[serde(default)]
    vdelay_pending_grafm: u8,
    grafm: u8, // GRAFM: 2-bit missile graphic patterns

    // -- Control --
    prior: u8,  // PRIOR: priority and GTIA mode select
    vdelay: u8, // VDELAY: vertical delay
    gractl: u8, // GRACTL: graphics control

    // -- Collision registers (active-high bit flags) --
    m_pf: [u8; 4], // M0PF-M3PF: missile-to-playfield
    p_pf: [u8; 4], // P0PF-P3PF: player-to-playfield
    m_pl: [u8; 4], // M0PL-M3PL: missile-to-player
    p_pl: [u8; 4], // P0PL-P3PL: player-to-player

    // -- Trigger inputs --
    trig: [u8; 4], // TRIG0-TRIG3: 1=released, 0=pressed
    /// Triggers held at "pressed" by GRACTL bit 2. Latching is all-or-nothing
    /// across the four inputs and is released only by clearing that bit.
    #[serde(default)]
    trig_latched: [bool; 4],

    // -- Console --
    // CONSOL is split: writes drive an output latch (bit 3 = speaker), while
    // reads return the switch inputs. The OS pulses the speaker bit every VBI,
    // so a single shared byte would corrupt the switch read (and, on the XL,
    // make the OPTION switch look held — disabling BASIC).
    consol_out: u8,       // CONSOL write latch (output; bit 3 = speaker)
    console_switches: u8, // START/SELECT/OPTION inputs (active low, bits 0-2)

    // -- Per-scanline beam-compositing state --
    // The beam composites the active line left-to-right. `begin_scanline`
    // precomputes the playfield colour-register *index* buffer (stable for the
    // line — ANTIC DMA's the bitmap at line start); `composite_playfield` then
    // resolves those indices to colours using the *live* colour registers as
    // the beam reaches each pixel, so a mid-line COLBK/COLPF write changes only
    // the pixels drawn after it.
    sl_visible: bool,           // false when the line is off-screen
    sl_fb_offset: usize,        // framebuffer index of this line's first active pixel
    sl_mode: AnticMode,         // ANTIC mode for the line
    sl_pf_span: (usize, usize), // active-x [start, end) the playfield occupies
    sl_line_buf: Vec<u8>,       // per-pixel playfield colour-register indices, one per window pixel
    sl_x: usize,                // compositing cursor: next active-x to draw

    // -- Framebuffer --
    framebuffer: Vec<u32>,
    /// Width of that framebuffer, which the region decides.
    fb_width: u32,
    /// Scan lines of border above the active playfield, which the region also
    /// decides. Carried rather than re-derived from the height: the two
    /// borders are not equal on PAL, so halving the leftover would move the
    /// picture a line.
    fb_border_top: u32,
    /// The half colour clock framebuffer pixel 0 sits on, so a line position
    /// in the chip's own coordinates can be turned into a pixel.
    fb_first_half_clock: u16,
}

impl Gtia {
    /// Create a new GTIA in its power-on state, feeding `region`'s field.
    #[must_use]
    pub fn new(region: GtiaRegion) -> Self {
        Self {
            pal: match region {
                GtiaRegion::Ntsc => 0x0F,
                GtiaRegion::Pal => 0x01,
            },
            colpm: [0; 4],
            colpf: [0; 4],
            colbk: 0,
            hposp: [0; 4],
            hposm: [0; 4],
            sizep: [0; 4],
            sizem: 0,
            grafp: [0; 4],
            vdelay_pending_grafp: [0; 4],
            vdelay_pending_grafm: 0,
            grafm: 0,
            prior: 0,
            vdelay: 0,
            gractl: 0,
            m_pf: [0; 4],
            p_pf: [0; 4],
            m_pl: [0; 4],
            p_pl: [0; 4],
            trig: [1; 4], // all released
            trig_latched: [false; 4],
            consol_out: 0x00,
            console_switches: 0x07, // all buttons released (active low)
            sl_visible: false,
            sl_fb_offset: 0,
            sl_mode: AnticMode::Blank,
            sl_pf_span: (0, 0),
            sl_line_buf: vec![0; region.framebuffer_width() as usize],
            sl_x: 0,
            framebuffer: vec![
                0xFF00_0000;
                (region.framebuffer_width() * region.framebuffer_height()) as usize
            ],
            fb_width: region.framebuffer_width(),
            fb_border_top: region.border_top(),
            fb_first_half_clock: region.first_half_clock(),
        }
    }

    // -----------------------------------------------------------------------
    // Register access
    // -----------------------------------------------------------------------

    /// Write a GTIA register. `addr` is masked to 5 bits ($00-$1F).
    pub fn write(&mut self, addr: u8, value: u8) {
        let reg = addr & 0x1F;
        match reg {
            0x00..=0x03 => self.hposp[(reg) as usize] = value,
            0x04..=0x07 => self.hposm[(reg - 0x04) as usize] = value,
            0x08..=0x0B => self.sizep[(reg - 0x08) as usize] = value,
            0x0C => self.sizem = value,
            0x0D..=0x10 => self.grafp[(reg - 0x0D) as usize] = value,
            0x11 => self.grafm = value,
            0x12..=0x15 => self.colpm[(reg - 0x12) as usize] = value,
            0x16..=0x19 => self.colpf[(reg - 0x16) as usize] = value,
            0x1A => self.colbk = value,
            0x1B => self.prior = value,
            0x1C => self.vdelay = value,
            0x1D => {
                self.gractl = value;
                // "all TRIG BITs 0 are latched when the button is pressed and
                // are only reset to one when BIT 2 of GRACTL is reset to zero"
                // — Mapping the Atari, TRIG0. Latching is all-or-nothing: the
                // book notes you cannot set it for individual triggers.
                if self.trigger_latching() {
                    for i in 0..NUM_PLAYERS {
                        if self.trig[i] == 0 {
                            self.trig_latched[i] = true;
                        }
                    }
                } else {
                    self.trig_latched = [false; 4];
                }
            }
            0x1E => {
                // HITCLR — clear all collision registers
                self.m_pf = [0; 4];
                self.p_pf = [0; 4];
                self.m_pl = [0; 4];
                self.p_pl = [0; 4];
            }
            0x1F => {
                // CONSOL write — output latch (bit 3 = speaker). Does not
                // affect the switch read; the OS pulses this every VBI.
                self.consol_out = value & 0x0F;
            }
            _ => {}
        }
    }

    /// Read a GTIA register. `addr` is masked to 5 bits ($00-$1F).
    #[must_use]
    pub fn read(&self, addr: u8) -> u8 {
        let reg = addr & 0x1F;
        match reg {
            // Collision registers
            0x00..=0x03 => self.m_pf[reg as usize],
            0x04..=0x07 => self.p_pf[(reg - 0x04) as usize],
            0x08..=0x0B => self.m_pl[(reg - 0x08) as usize],
            0x0C..=0x0F => self.p_pl[(reg - 0x0C) as usize],
            // Triggers
            0x10..=0x13 => {
                let i = (reg - 0x10) as usize;
                if self.trig_latched[i] {
                    0
                } else {
                    self.trig[i]
                }
            }
            // PAL register — which television standard this chip feeds.
            0x14 => self.pal,
            // CONSOL — switch inputs (bits 0-2, active low); bit 3 reads back
            // the speaker output latch.
            0x1F => (self.consol_out & 0x08) | (self.console_switches & 0x07),
            // All other read addresses return $FF (open bus)
            _ => 0xFF,
        }
    }

    /// Current COLBK value (write-only register; debug accessor for
    /// tests and MCP-style chip inspection).
    #[must_use]
    pub const fn colbk_value(&self) -> u8 {
        self.colbk
    }
    /// Current COLPF[0..4] values (write-only; debug accessor).
    #[must_use]
    pub const fn colpf_values(&self) -> [u8; 4] {
        self.colpf
    }
    /// Current PRIOR value (write-only; debug accessor).
    #[must_use]
    pub const fn prior_value(&self) -> u8 {
        self.prior
    }
    /// Current COLPM[0..4] player/missile colours (write-only; debug accessor).
    #[must_use]
    pub const fn colpm_values(&self) -> [u8; 4] {
        self.colpm
    }
    /// Accept one line of player/missile DMA from ANTIC.
    ///
    /// GRACTL decides whether the DMA data reaches the graphics registers at
    /// all: bit 0 admits missiles, bit 1 admits players. With a bit clear the
    /// register keeps whatever the CPU last wrote, which is how software drives
    /// players with DMA switched off. Source: Master Memory Map (1982), GRACTL
    /// — "For Missile DMA, add 1. For Player DMA, add 2."
    ///
    /// VDELAY then shifts an object down one TV line, but only in the two-line
    /// display: "Used to give one-line resolution movement capability in the
    /// vertical positioning of an object when the two line resolution display
    /// is enabled" (Mapping the Atari, $D01C). Bits 4-7 are players 0-3, bits
    /// 0-3 missiles 0-3. A delayed object latches the previous line's byte and
    /// holds this one back, which is the shift.
    pub fn accept_pm_dma(&mut self, players: [u8; 4], missiles: u8, single_line: bool) {
        let delay = !single_line;
        if self.gractl & 0x02 != 0 {
            for (p, &incoming) in players.iter().enumerate() {
                let delayed = delay && (self.vdelay & (0x10 << p)) != 0;
                self.grafp[p] = if delayed {
                    self.vdelay_pending_grafp[p]
                } else {
                    incoming
                };
                self.vdelay_pending_grafp[p] = incoming;
            }
        }
        if self.gractl & 0x01 != 0 {
            // GRAFM packs all four missiles, two bits each, so a delayed
            // missile takes its own pair from the held byte.
            let mut out = 0u8;
            for m in 0..4 {
                let mask = 0x03 << (m * 2);
                let delayed = delay && (self.vdelay & (1 << m)) != 0;
                let source = if delayed {
                    self.vdelay_pending_grafm
                } else {
                    missiles
                };
                out |= source & mask;
            }
            self.grafm = out;
            self.vdelay_pending_grafm = missiles;
        }
    }

    /// Current GRAFPx pattern (write-only; debug accessor).
    #[must_use]
    pub const fn grafp_value(&self, player: usize) -> u8 {
        self.grafp[player]
    }

    /// Current GRACTL value (write-only; debug accessor).
    #[must_use]
    pub const fn gractl_value(&self) -> u8 {
        self.gractl
    }
    /// Current console-switch state (CONSOL read: START/SELECT/OPTION in bits
    /// 0-2, active low). Debug accessor.
    #[must_use]
    pub const fn console_switches(&self) -> u8 {
        self.console_switches
    }

    // -----------------------------------------------------------------------
    // Trigger inputs
    // -----------------------------------------------------------------------

    /// Set trigger input state. `index` 0-3, `pressed` true = button down.
    pub fn set_trigger(&mut self, index: u8, pressed: bool) {
        if (index as usize) < NUM_PLAYERS {
            self.trig[index as usize] = u8::from(!pressed);
            if pressed && self.trigger_latching() {
                self.trig_latched[index as usize] = true;
            }
        }
    }

    /// Whether GRACTL bit 2 is holding pressed triggers.
    const fn trigger_latching(&self) -> bool {
        self.gractl & 0x04 != 0
    }

    /// Set the console-switch inputs read via CONSOL ($D01F), bits 0-2 =
    /// START / SELECT / OPTION (active low: a clear bit means pressed). This
    /// is the read path; CPU writes to CONSOL drive the speaker latch only.
    pub fn set_console_switches(&mut self, switches: u8) {
        self.console_switches = switches & 0x07;
    }

    // -----------------------------------------------------------------------
    // Framebuffer access
    // -----------------------------------------------------------------------

    /// The ARGB32 framebuffer.
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    /// Framebuffer width in pixels.
    #[must_use]
    pub const fn framebuffer_width(&self) -> u32 {
        self.fb_width
    }

    /// Framebuffer height in pixels.
    ///
    /// Read back off the buffer rather than stated a second time, so the
    /// height a caller sees is always the height that was allocated.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        (self.framebuffer.len() / self.fb_width as usize) as u32
    }

    /// Scan lines of border above the active playfield, as the region placed
    /// it.
    #[must_use]
    pub const fn border_top(&self) -> u32 {
        self.fb_border_top
    }

    /// Pixels between the window's left edge and the **normal** playfield's,
    /// from the line width.
    ///
    /// A position, not a boundary: narrow playfields start to the right of it,
    /// wide ones and player/missile objects to the left. Compositing runs the
    /// full width of the window, so nothing is clipped to it.
    #[must_use]
    pub const fn border_left(&self) -> u32 {
        (self.fb_width - ACTIVE_WIDTH) / 2
    }

    // -----------------------------------------------------------------------
    // Rendering
    // -----------------------------------------------------------------------

    /// Fill the entire framebuffer with the current backdrop colour
    /// (COLBK / colour register 0). Called by the machine at frame start.
    ///
    /// This now only has to cover the scan lines GTIA never composites — the
    /// ones outside ANTIC's 240, which is PAL's vertical border. Within those
    /// 240 the beam composites the whole width of the window, side borders
    /// included, so a mid-line COLBK write splits the border on the same frame
    /// rather than the next. That is what an Atari raster bar in the border
    /// does, and it was not possible while compositing stopped at the normal
    /// playfield's edge (#1086).
    pub fn fill_border(&mut self) {
        let argb = self.colour_to_argb32(self.colbk);
        self.framebuffer.fill(argb);
    }

    /// Render one scan line from ANTIC playfield data.
    ///
    /// - `line`: scan line number (0-239 within the visible region)
    /// - `playfield`: pixel index values from ANTIC
    /// - `pf_width`: playfield width in colour clocks (128, 160, or 192)
    /// - `mode`: the ANTIC display mode, controls colour interpretation
    pub fn render_line(&mut self, line: u16, playfield: &[u8], pf_width: u16, mode: AnticMode) {
        self.begin_scanline(line, playfield, pf_width, mode);
        self.finish_scanline();
    }

    /// Start a new active scan line: precompute the playfield colour-register
    /// *index* buffer (stable for the whole line — ANTIC DMA's the bitmap at
    /// line start) and reset the compositing cursor. The colours themselves are
    /// resolved later, per pixel, from the live registers (see
    /// `composite_playfield`) so mid-line register writes land at the beam.
    pub fn begin_scanline(&mut self, line: u16, playfield: &[u8], pf_width: u16, mode: AnticMode) {
        self.sl_x = 0;
        if line >= ACTIVE_HEIGHT as u16 {
            self.sl_visible = false;
            return;
        }
        self.sl_visible = true;
        let fb_row = self.border_top() as usize + line as usize;
        self.sl_fb_offset = fb_row * self.fb_width as usize;
        self.sl_mode = mode;

        // Build the window-wide line of playfield colour-register indices.
        let mut line_buf = std::mem::take(&mut self.sl_line_buf);
        line_buf.fill(0);
        self.sl_pf_span = if mode == AnticMode::Blank {
            (0, 0)
        } else {
            self.fill_playfield_line(&mut line_buf, playfield, pf_width, mode)
        };
        self.sl_line_buf = line_buf;
    }

    /// Composite pixels from the cursor up to (but not including) active-x
    /// `end`, and advance the cursor. Each pixel resolves the playfield index
    /// and the player/missile coverage from the *live* registers at that pixel's
    /// beam colour-clock, applies the live PRIOR scheme, and records collisions
    /// — so mid-line colour, priority, HPOS/GRAFP and HITCLR writes all land at
    /// the beam. Calling with `end == ACTIVE_WIDTH` finishes the line; the
    /// beam-driven path calls it repeatedly with the beam position.
    pub fn composite_playfield(&mut self, end: usize) {
        if !self.sl_visible {
            return;
        }
        let end = end.min(self.fb_width as usize);

        // Hi-res 1.5-colour modes (2, 3, and F): the playfield background is
        // COLPF2 and lit pixels take COLPF2's hue with COLPF1's luminance.
        // Anything outside the playfield is COLBK border.
        let hires = self.sl_mode.is_hires();

        while self.sl_x < end {
            let x = self.sl_x;
            let pf_col_idx = self.sl_line_buf[x];

            // Players/missiles at this pixel's beam colour-clock, from the
            // *live* registers — so a mid-line HPOS/GRAFP rewrite (sprite
            // multiplexing) and per-pixel collision timing land at the beam.
            let cc = (self.fb_first_half_clock + x as u16) / 2;
            let pm_bits = self.pm_bits_at_cc(cc);

            let colour = match (self.prior >> 6) & 0x03 {
                0 => {
                    // Collisions (independent of the final priority):
                    // PM-vs-playfield where playfield is present, PM-vs-PM
                    // wherever objects overlap.
                    if pm_bits != 0 {
                        self.record_collisions(pm_bits, pf_col_idx);
                    }

                    let in_pf = x >= self.sl_pf_span.0 && x < self.sl_pf_span.1;
                    let playfield_colour = if hires && in_pf {
                        if pf_col_idx != 0 {
                            Some((self.colpf[2] & 0xF0) | (self.colpf[1] & 0x0F))
                        } else {
                            Some(self.colpf[2])
                        }
                    } else if pf_col_idx != 0 {
                        Some(self.playfield_register(pf_col_idx))
                    } else {
                        None
                    };
                    self.priority_colour(pm_bits, pf_col_idx, playfield_colour)
                        .map_or(self.colbk, |(colour, _)| colour)
                }
                gtia_mode => self.gtia_mode_colour(gtia_mode, cc, pm_bits),
            };

            self.framebuffer[self.sl_fb_offset + x] = self.colour_to_argb32(colour);
            self.sl_x += 1;
        }
    }

    /// The colour at colour clock `cc` in GTIA mode 9, 10 or 11 (`gtia_mode`
    /// 1, 2 or 3: PRIOR bits 6-7), with `pm_bits` the objects covering it.
    ///
    /// These modes read ANTIC's output two colour clocks at a time: the two
    /// AN values form a nibble, and the nibble is the pixel. The pairing is
    /// fixed to the line, not to the playfield, so a pixel is the two colour
    /// clocks from an even one — and it runs through the border and blank
    /// lines too, where AN is zero and the nibble is 0. Atari800 `antic.c`
    /// (`draw_an_gtia9`, `draw_an_gtia10`, `draw_an_gtia11`) and Altirra
    /// `gtiarenderer.cpp` (`RenderMode9`, `RenderMode10`, `RenderMode11`)
    /// agree on what each mode makes of the nibble:
    ///
    /// - Mode 9 turns the playfield off and ORs the nibble into COLBK as
    ///   luminance.
    /// - Mode 10 pairs one colour clock later than the others, so its
    ///   picture sits one clock to the right. Nibbles 0-3 put COLPM0-3 on
    ///   screen as if that player were there, so a real player in front of
    ///   it wins or overlaps by the usual rules but no collision registers;
    ///   4-7 and 12-15 are the playfield colours COLPF0-3 and collide as
    ///   playfield; 8-11 are COLBK.
    /// - Mode 11 turns the playfield off and ORs the nibble into COLBK as
    ///   hue; nibble 0 has no luminance at all, whatever COLBK says.
    ///
    /// Where the fifth player shows over a mode 9 or 11 pixel, the nibble is
    /// ORed into COLPF3 the same way; over a real player it is not.
    fn gtia_mode_colour(&mut self, gtia_mode: u8, cc: u16, pm_bits: u8) -> u8 {
        let pair = if gtia_mode == 2 {
            cc.wrapping_sub(1) & !1
        } else {
            cc & !1
        };
        let nibble = (self.an_at(pair) << 2) | self.an_at(pair.wrapping_add(1));

        if gtia_mode == 2 {
            let (pf_col_idx, base, as_player) = match nibble {
                0..=3 => (0, self.colpm[usize::from(nibble)], 1u8 << nibble),
                4..=7 | 12..=15 => {
                    let pf = nibble & 3;
                    (pf + 1, self.colpf[usize::from(pf)], 0)
                }
                _ => (0, self.colbk, 0),
            };
            if pm_bits != 0 {
                self.record_collisions(pm_bits, pf_col_idx);
            }
            let playfield_colour = (pf_col_idx != 0).then_some(base);
            return self
                .priority_colour(pm_bits | as_player, pf_col_idx, playfield_colour)
                .map_or(base, |(colour, _)| colour);
        }

        // Modes 9 and 11: no playfield, so only object collisions register,
        // and the nibble colours whatever the objects leave uncovered.
        if pm_bits != 0 {
            self.record_collisions(pm_bits, 0);
        }
        let shade = |register: u8| {
            if gtia_mode == 1 {
                register | nibble
            } else if nibble == 0 {
                register & 0xF0
            } else {
                register | (nibble << 4)
            }
        };
        match self.priority_colour(pm_bits, 0, None) {
            None => shade(self.colbk),
            Some((_, true)) => shade(self.colpf[3]),
            Some((colour, false)) => colour,
        }
    }

    /// ANTIC's AN0-1 output at colour clock `cc` for the current line, as
    /// GTIA's nibble logic sees it: the two hi-res bits of a hi-res mode,
    /// or the playfield number (PF0-3 as 0-3, background as 0) of any other.
    /// Zero outside the picture.
    fn an_at(&self, cc: u16) -> u8 {
        let Some(x) = (cc * 2).checked_sub(self.fb_first_half_clock) else {
            return 0;
        };
        let x = usize::from(x);
        if self.sl_mode.is_hires() {
            let bit = |x: usize| self.sl_line_buf.get(x).copied().unwrap_or(0) & 1;
            (bit(x) << 1) | bit(x + 1)
        } else {
            self.sl_line_buf
                .get(x)
                .copied()
                .unwrap_or(0)
                .saturating_sub(1)
        }
    }

    /// Object-bit mask for every player and missile covering colour-clock `cc`.
    fn pm_bits_at_cc(&self, cc: u16) -> u8 {
        let mut bits = 0u8;
        for m in 0..NUM_MISSILES {
            if self.missile_covers(m, cc) {
                bits |= 1 << (m + 4);
            }
        }
        for p in 0..NUM_PLAYERS {
            if self.player_covers(p, cc) {
                bits |= 1 << p;
            }
        }
        bits
    }

    /// Resolve the live PRIOR scheme for one playfield/PM overlap.
    ///
    /// Ranks are the hardware's front-to-back order. Normal missiles share
    /// their associated player's colour and rank; combined missiles occupy the
    /// PF3/fifth-player layer. Colour zero retains the existing transparent-PM
    /// behaviour, while a conflicting PRIOR selection returns visible black.
    ///
    /// Returns the winning colour and whether the fifth player supplied it,
    /// which the GTIA modes need because they shade it like the background.
    fn priority_colour(
        &self,
        pm_bits: u8,
        pf_col_idx: u8,
        playfield_colour: Option<u8>,
    ) -> Option<(u8, bool)> {
        let schemes = self.prior & 0x0F;
        let player_ranks = [[0u8, 1, 2, 3], [0, 1, 6, 7], [4, 5, 6, 7], [2, 3, 4, 5]];
        let playfield_ranks = [[4u8, 5, 6, 7], [2, 3, 4, 5], [0, 1, 2, 3], [0, 1, 6, 7]];

        let front_pm_rank = |scheme: usize| {
            let player = player_ranks[scheme]
                .iter()
                .enumerate()
                .filter(|&(player, _)| {
                    self.colpm[player] != 0
                        && ((pm_bits & (1 << player)) != 0
                            || ((self.prior & 0x10) == 0 && (pm_bits & (1 << (player + 4))) != 0))
                })
                .map(|(_, &rank)| rank)
                .min();
            let fifth = ((self.prior & 0x10) != 0 && (pm_bits & 0xF0) != 0 && self.colpf[3] != 0)
                .then_some(playfield_ranks[scheme][3]);
            player.into_iter().chain(fifth).min()
        };

        if schemes.count_ones() > 1 && playfield_colour.is_some() {
            let pf = usize::from(pf_col_idx.saturating_sub(1).min(3));
            let mut outcome = None;
            for (scheme, pf_ranks) in playfield_ranks.iter().enumerate() {
                if (schemes & (1 << scheme)) == 0 {
                    continue;
                }
                let pm_wins = front_pm_rank(scheme).is_some_and(|rank| rank < pf_ranks[pf]);
                if outcome.is_some_and(|previous| previous != pm_wins) {
                    return Some((0, false));
                }
                outcome = Some(pm_wins);
            }
        }

        // PRIOR=0 retains the chip's default PM-over-playfield order.
        let scheme = if schemes == 0 {
            0
        } else {
            schemes.trailing_zeros() as usize
        };

        let mut winner = playfield_colour.map(|colour| {
            let pf = usize::from(pf_col_idx.saturating_sub(1).min(3));
            (playfield_ranks[scheme][pf], colour)
        });

        // Which players are showing here. A missile takes its own player's
        // colour unless PRIOR bit 4 has combined the four into a fifth player.
        let mut active = 0u8;
        for player in 0..NUM_PLAYERS {
            let player_or_missile = (pm_bits & (1 << player)) != 0
                || ((self.prior & 0x10) == 0 && (pm_bits & (1 << (player + 4))) != 0);
            if player_or_missile && self.colpm[player] != 0 {
                active |= 1 << player;
            }
        }

        let mut player_won = false;
        for (player, &player_rank) in player_ranks[scheme].iter().enumerate() {
            if active & (1 << player) != 0 && winner.is_none_or(|(rank, _)| player_rank < rank) {
                winner = Some((player_rank, self.colpm[player]));
                player_won = true;
            }
        }

        // Where players overlap, the front one does not simply win.
        //
        // Mapping the Atari, PRIOR: "The Atari performs a logical OR to colors
        // of players 0/1 and 2/3 when they overlap. Only the 0/1, 2/3
        // combinations are allowed; you will not get a third color when players
        // 1 and 3 overlap, for example (you will get black instead) ... If you
        // don't enable the overlap option, the area of overlap for all players
        // will be black."
        //
        // So bit 5 buys the third colour for the two sanctioned pairs, and
        // every other overlap — including all of them with the bit clear — is
        // black rather than the front player's colour.
        if player_won && active.count_ones() > 1 {
            let multi_colour = self.prior & 0x20 != 0;
            let overlap = match active {
                0b0011 if multi_colour => self.colpm[0] | self.colpm[1],
                0b1100 if multi_colour => self.colpm[2] | self.colpm[3],
                _ => 0,
            };
            winner = winner.map(|(rank, _)| (rank, overlap));
        }

        let mut fifth_won = false;
        if (self.prior & 0x10) != 0 && (pm_bits & 0xF0) != 0 {
            let rank = playfield_ranks[scheme][3];
            if winner.is_none_or(|(winner_rank, _)| rank < winner_rank) {
                winner = Some((rank, self.colpf[3]));
                fifth_won = true;
            }
        }

        winner.map(|(_, colour)| (colour, fifth_won))
    }

    /// Whether player `p`'s graphic covers beam colour-clock `cc`, from the
    /// live HPOS / SIZE / GRAFP. A player spans `8 × width` colour clocks from
    /// its HPOS, each of its 8 graphic bits `width` clocks wide.
    fn player_covers(&self, p: usize, cc: u16) -> bool {
        let pattern = self.grafp[p];
        if pattern == 0 {
            return false;
        }
        let hpos = u16::from(self.hposp[p]);
        if cc < hpos {
            return false;
        }
        let width = player_pixel_width(self.sizep[p] & 0x03);
        let offset = cc - hpos;
        if offset >= 8 * width {
            return false;
        }
        let bit = (offset / width) as u8; // 0..8, MSB first
        pattern & (1 << (7 - bit)) != 0
    }

    /// Whether missile `m`'s graphic covers beam colour-clock `cc`, from the
    /// live HPOS / SIZE / GRAFM. A missile is a 2-bit pattern, each bit
    /// `width` colour clocks wide.
    fn missile_covers(&self, m: usize, cc: u16) -> bool {
        let pattern = (self.grafm >> (m * 2)) & 0x03;
        if pattern == 0 {
            return false;
        }
        let hpos = u16::from(self.hposm[m]);
        if cc < hpos {
            return false;
        }
        let width = missile_width((self.sizem >> (m * 2)) & 0x03);
        let offset = cc - hpos;
        if offset >= 2 * width {
            return false;
        }
        let bit = (offset / width) as u8; // 0 or 1, MSB first
        pattern & (1 << (1 - bit)) != 0
    }

    /// Composite the playfield up to the beam's current line colour-clock.
    ///
    /// `line_cc` is the beam position within the 228-colour-clock scan line
    /// (0 at the line's left edge). It is mapped to an active-x through the
    /// playfield's left margin (`PF_LEFT_CC`) — the same origin players use —
    /// so a colour-register write at a given beam cc recolours exactly the
    /// pixels from that cc onward. Left of the active window it draws nothing;
    /// past the right edge it finishes the line. The machine calls this every
    /// colour clock to drive beam-ordered compositing.
    pub fn composite_to_beam(&mut self, line_cc: u16) {
        let target = usize::from((line_cc * 2).saturating_sub(self.fb_first_half_clock))
            .min(self.fb_width as usize);
        self.composite_playfield(target);
    }

    /// Finish the scan line by compositing any remaining pixels to the right
    /// edge. Players/missiles and collisions are folded into
    /// `composite_playfield` at beam time (see `pm_at_cc`), so this just
    /// flushes the cursor — the machine calls it once the line completes.
    pub fn finish_scanline(&mut self) {
        if !self.sl_visible {
            return;
        }
        self.composite_playfield(self.fb_width as usize);
    }

    /// Fill the window-wide line buffer with playfield colour register indices.
    ///
    /// Returns the `[start, end)` framebuffer-x span that the playfield
    /// occupies, so the caller can tell in-playfield background from border.
    ///
    /// The span comes from the chip's line, not from the buffer's middle. Each
    /// width is centred on colour clock [`PF_CENTRE_CC`] and displayed over
    /// the range [`playfield_display_cc`] gives it; the window keeps whatever
    /// part of that it reaches. Centring the data in a fixed 320-pixel active
    /// area is what clipped a wide playfield to the normal one's width and
    /// shifted the part that survived (#1086).
    fn fill_playfield_line(
        &self,
        line_buf: &mut [u8],
        playfield: &[u8],
        pf_width: u16,
        mode: AnticMode,
    ) -> (usize, usize) {
        // Hi-res modes carry one data entry per half colour clock; the rest
        // carry one per colour clock and each covers two pixels.
        let hires = mode.is_hires();

        let first = self.fb_first_half_clock;
        let (display_start_cc, display_end_cc) = playfield_display_cc(pf_width);
        let origin_h = playfield_origin_cc(pf_width) * 2;

        let end = usize::from((display_end_cc * 2).saturating_sub(first)).min(line_buf.len());
        let start = usize::from((display_start_cc * 2).saturating_sub(first)).min(end);

        for (x, slot) in line_buf.iter_mut().enumerate().take(end).skip(start) {
            let offset = usize::from((first + x as u16).saturating_sub(origin_h));
            let index = if hires { offset } else { offset / 2 };
            if let Some(&pixel) = playfield.get(index) {
                *slot = pixel;
            }
        }

        (start, end)
    }

    /// Record collisions for a pixel covered by the players/missiles in
    /// `pm_bits`. `pf_idx` is the playfield colour-register index there (0 =
    /// background / no playfield). PM-to-playfield collisions only register
    /// where a playfield colour (1-4) is present; PM-to-PM collisions register
    /// wherever the objects overlap, **independent of the playfield** — the
    /// hardware compares the object signals directly, so two players collide
    /// over bare background too.
    fn record_collisions(&mut self, pm_bits: u8, pf_idx: u8) {
        // PM vs playfield — only where a playfield colour is present.
        if (1..=4).contains(&pf_idx) {
            let pf_bit = 1u8 << (pf_idx - 1);
            for p in 0..NUM_PLAYERS {
                if pm_bits & (1 << p) != 0 {
                    self.p_pf[p] |= pf_bit;
                }
            }
            for m in 0..NUM_MISSILES {
                if pm_bits & (1 << (m + 4)) != 0 {
                    self.m_pf[m] |= pf_bit;
                }
            }
        }

        // Player-to-player collisions (independent of the playfield).
        for p in 0..NUM_PLAYERS {
            if pm_bits & (1 << p) == 0 {
                continue;
            }
            for q in 0..NUM_PLAYERS {
                if p != q && pm_bits & (1 << q) != 0 {
                    self.p_pl[p] |= 1 << q;
                }
            }
        }

        // Missile-to-player collisions (independent of the playfield).
        for m in 0..NUM_MISSILES {
            if pm_bits & (1 << (m + 4)) == 0 {
                continue;
            }
            for p in 0..NUM_PLAYERS {
                if pm_bits & (1 << p) != 0 {
                    self.m_pl[m] |= 1 << p;
                }
            }
        }
    }

    /// The colour register a playfield index names: 1-4 are COLPF0-3,
    /// anything else the background.
    fn playfield_register(&self, pf_idx: u8) -> u8 {
        match pf_idx {
            1..=4 => self.colpf[usize::from(pf_idx - 1)],
            _ => self.colbk,
        }
    }
}

impl Gtia {
    /// Serialize GTIA register state for save states.
    #[must_use]
    pub fn save_state(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(64);
        data.extend_from_slice(&self.colpm);
        data.extend_from_slice(&self.colpf);
        data.push(self.colbk);
        data.extend_from_slice(&self.hposp);
        data.extend_from_slice(&self.hposm);
        data.extend_from_slice(&self.sizep);
        data.push(self.sizem);
        data.extend_from_slice(&self.grafp);
        data.extend_from_slice(&self.vdelay_pending_grafp);
        data.push(self.vdelay_pending_grafm);
        data.push(self.grafm);
        data.push(self.prior);
        data.push(self.vdelay);
        data.push(self.gractl);
        data.extend_from_slice(&self.m_pf);
        data.extend_from_slice(&self.p_pf);
        data.extend_from_slice(&self.m_pl);
        data.extend_from_slice(&self.p_pl);
        data.extend_from_slice(&self.trig);
        for latched in self.trig_latched {
            data.push(u8::from(latched));
        }
        data.push(self.consol_out);
        data.push(self.console_switches);
        data
    }

    /// Restore GTIA state from a save state.
    ///
    /// # Errors
    ///
    /// Returns an error if the data is too short.
    pub fn load_state(&mut self, data: &[u8]) -> Result<usize, String> {
        if data.len() < 51 {
            return Err("GTIA state truncated".into());
        }
        let mut p = 0;
        self.colpm.copy_from_slice(&data[p..p + 4]);
        p += 4;
        self.colpf.copy_from_slice(&data[p..p + 4]);
        p += 4;
        self.colbk = data[p];
        p += 1;
        self.hposp.copy_from_slice(&data[p..p + 4]);
        p += 4;
        self.hposm.copy_from_slice(&data[p..p + 4]);
        p += 4;
        self.sizep.copy_from_slice(&data[p..p + 4]);
        p += 4;
        self.sizem = data[p];
        p += 1;
        self.grafp.copy_from_slice(&data[p..p + 4]);
        p += 4;
        self.vdelay_pending_grafp.copy_from_slice(&data[p..p + 4]);
        p += 4;
        self.vdelay_pending_grafm = data[p];
        p += 1;
        self.grafm = data[p];
        p += 1;
        self.prior = data[p];
        p += 1;
        self.vdelay = data[p];
        p += 1;
        self.gractl = data[p];
        p += 1;
        self.m_pf.copy_from_slice(&data[p..p + 4]);
        p += 4;
        self.p_pf.copy_from_slice(&data[p..p + 4]);
        p += 4;
        self.m_pl.copy_from_slice(&data[p..p + 4]);
        p += 4;
        self.p_pl.copy_from_slice(&data[p..p + 4]);
        p += 4;
        self.trig.copy_from_slice(&data[p..p + 4]);
        p += 4;
        for i in 0..NUM_PLAYERS {
            self.trig_latched[i] = data[p + i] != 0;
        }
        p += 4;
        self.consol_out = data[p];
        p += 1;
        self.console_switches = data[p];
        p += 1;
        Ok(p)
    }
}

// No `Default`. There is no default television standard, and a chip that
// guessed one would size its framebuffer wrong for half the machines that use
// it — which is the bug this region parameter exists to fix.

/// Player pixel width for a given size value (bits 0-1 of `SIZEPx`).
const fn player_pixel_width(size_bits: u8) -> u16 {
    match size_bits & 0x03 {
        0x00 => 1, // normal
        0x01 => 2, // double
        0x03 => 4, // quad
        _ => 1,    // $02 = normal
    }
}

/// Missile width in colour clocks for a given 2-bit size value.
const fn missile_width(size_bits: u8) -> u16 {
    match size_bits & 0x03 {
        0x00 => 1, // normal (2 px = 1 cc)
        0x01 => 2, // double
        0x03 => 4, // quad
        _ => 1,    // $02 = normal
    }
}

impl Gtia {
    /// Convert an Atari colour register value to ARGB32 using the palette for
    /// the television standard reported by PAL ($D014).
    #[must_use]
    pub fn colour_to_argb32(&self, colour: u8) -> u32 {
        let palette = if self.pal == 0x01 {
            &PAL_PALETTE
        } else {
            &NTSC_PALETTE
        };
        palette[usize::from(colour)]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    /// Software reads `$D014`/`$C014` to find out which television
    /// standard it is on. MAME: `m_r.pal = is_ntsc() ? 0x0f : 0x01`
    /// (`src/mame/atari/gtia.cpp:270`).
    ///
    /// Answering `$00` — neither value — made Joust load an all-zero
    /// palette and render the whole game black on an NTSC machine.
    #[test]
    fn pal_register_reports_the_television_standard() {
        assert_eq!(Gtia::new(GtiaRegion::Ntsc).read(0x14), 0x0F, "NTSC");
        assert_eq!(Gtia::new(GtiaRegion::Pal).read(0x14), 0x01, "PAL");
    }

    #[test]
    fn colour_conversion_uses_the_television_standard_palette() {
        let ntsc = Gtia::new(GtiaRegion::Ntsc);
        let pal = Gtia::new(GtiaRegion::Pal);

        assert_eq!(ntsc.colour_to_argb32(0x20), 0xFF1C_0800);
        assert_eq!(pal.colour_to_argb32(0x20), 0xFF38_0000);
        assert_ne!(
            ntsc.colour_to_argb32(0x20),
            pal.colour_to_argb32(0x20),
            "PAL must not silently render through the NTSC table"
        );
    }

    /// GTIA has sixteen luminances, not the TIA's eight: the odd values
    /// mode 9 produces must each be their own shade.
    #[test]
    fn every_luminance_is_a_distinct_shade() {
        for gtia in [Gtia::new(GtiaRegion::Ntsc), Gtia::new(GtiaRegion::Pal)] {
            let shades: Vec<u32> = (0x20..0x30).map(|c| gtia.colour_to_argb32(c)).collect();
            let mut distinct = shades.clone();
            distinct.dedup();
            assert_eq!(distinct.len(), 16, "hue 2 luminances: {shades:08X?}");
            assert!(
                shades.windows(2).all(|w| w[0] < w[1]),
                "luminance climbs: {shades:08X?}"
            );
        }
    }

    /// The register is mirrored across GTIA's address space like every
    /// other one, so a read of `$C034` must answer too.
    #[test]
    fn pal_register_is_mirrored() {
        let gtia = Gtia::new(GtiaRegion::Ntsc);
        assert_eq!(gtia.read(0x34), 0x0F);
    }

    #[test]
    fn each_region_holds_exactly_the_field_a_set_shows() {
        // 240 lines on NTSC, 288 on PAL — `Display::Television`'s
        // `lines_per_tv_height`, and the rule in
        // `the-framebuffer-is-the-sets-window.md`.
        //
        // This held one height for both. It was 288, which is right for PAL
        // and a fifth more raster than an NTSC set displays; the #1054 audit
        // read the NTSC profiles as 120%.
        for (region, field) in [(GtiaRegion::Ntsc, 240), (GtiaRegion::Pal, 288)] {
            let gtia = Gtia::new(region);
            assert_eq!(gtia.framebuffer_height(), field, "{region:?}");
            assert_eq!(
                gtia.framebuffer().len(),
                (region.framebuffer_width() * field) as usize,
                "{region:?} allocated a buffer of the wrong size"
            );
        }
    }

    #[test]
    fn the_active_playfield_fits_the_field_with_the_border_around_it() {
        // The border is what the field has left over once the active
        // playfield is placed, so it cannot be a constant: NTSC has nothing
        // left over. Adding a fixed 24 to a field that was already full is
        // how the old height reached 288.
        for region in [GtiaRegion::Ntsc, GtiaRegion::Pal] {
            assert!(
                region.border_top() + ACTIVE_HEIGHT <= region.framebuffer_height(),
                "{region:?} places the active playfield past the end of its field"
            );
        }
        assert_eq!(GtiaRegion::Ntsc.border_top(), 0);
        assert_eq!(GtiaRegion::Pal.border_top(), 23);
    }

    #[test]
    fn the_last_active_line_lands_inside_the_ntsc_field() {
        // The narrow field is the one that can overflow: ANTIC's last line
        // has to reach the last row of the buffer and no further.
        let mut gtia = Gtia::new(GtiaRegion::Ntsc);
        let playfield = vec![0u8; ACTIVE_WIDTH as usize];
        gtia.render_line(ACTIVE_HEIGHT as u16 - 1, &playfield, 160, AnticMode::Mode2);

        assert_eq!(
            gtia.framebuffer().len() / GtiaRegion::Ntsc.framebuffer_width() as usize,
            GtiaRegion::Ntsc.framebuffer_height() as usize
        );
    }

    use super::*;

    #[test]
    fn colour_register_write_read() {
        let mut gtia = Gtia::new(GtiaRegion::Pal);
        // Write COLBK ($1A) and verify it's stored
        gtia.write(0x1A, 0x94);
        assert_eq!(gtia.colbk, 0x94);

        // Write COLPF0 ($16) and verify
        gtia.write(0x16, 0x28);
        assert_eq!(gtia.colpf[0], 0x28);

        // Write COLPM2 ($14) and verify
        gtia.write(0x14, 0x46);
        assert_eq!(gtia.colpm[2], 0x46);
    }

    #[test]
    fn mid_line_colbk_change_affects_only_later_pixels() {
        // The beam seam: compositing resolves colour indices to colours from
        // the *live* registers as the cursor advances. Writing COLBK between
        // two `composite_playfield` calls must recolour only the pixels drawn
        // after the write — the mechanism Phase 2 drives from the machine to
        // make rainbow / gradient kernels appear. A blank line is all index 0
        // (background), so every pixel takes COLBK.
        let mut gtia = Gtia::new(GtiaRegion::Pal);
        let width = GtiaRegion::Pal.framebuffer_width() as usize;
        gtia.write(0x1A, 0x0A); // COLBK = A
        gtia.begin_scanline(0, &[], 160, AnticMode::Blank);
        gtia.composite_playfield(width / 2); // left half at A
        gtia.write(0x1A, 0x0C); // COLBK = B
        gtia.composite_playfield(width); // right half at B

        let fb = gtia.framebuffer();
        let row = GtiaRegion::Pal.border_top() as usize * width;
        let colour_a = gtia.colour_to_argb32(0x0A);
        let colour_b = gtia.colour_to_argb32(0x0C);
        assert_ne!(colour_a, colour_b);
        // Pixel 10 is border, not playfield: the beam composites the whole
        // window now, so a mid-line COLBK write splits the border too — which
        // is what an Atari raster bar in the border actually does.
        assert_eq!(fb[row + 10], colour_a, "left border keeps the first COLBK");
        assert_eq!(
            fb[row + width / 2 - 1],
            colour_a,
            "last pixel before the seam"
        );
        assert_eq!(fb[row + width / 2], colour_b, "first pixel after the seam");
        assert_eq!(
            fb[row + width - 1],
            colour_b,
            "right edge takes the new COLBK"
        );
    }

    #[test]
    fn player_position_and_graphics() {
        let mut gtia = Gtia::new(GtiaRegion::Pal);
        // Place player 0 at HPOS=80, give it a solid 8-pixel pattern
        gtia.write(0x00, 80); // HPOSP0
        gtia.write(0x0D, 0xFF); // GRAFP0: all 8 bits set
        gtia.write(0x12, 0x38); // COLPM0: some colour

        // Render a blank line — player should appear
        let playfield = vec![0u8; 160];
        gtia.render_line(0, &playfield, 160, AnticMode::ModeD);

        // Player at HPOS=80, PF_LEFT_CC=48, so active-region x = (80-48)*2 = 64.
        // 8 pixels wide at normal size, each 1 cc = 2 fb pixels.
        // The active region starts at (GtiaRegion::Pal.border_left(), GtiaRegion::Pal.border_top()) within the
        // 384 x 288 TV-visible framebuffer.
        let fb = gtia.framebuffer();
        let active_start = GtiaRegion::Pal.border_top() as usize
            * GtiaRegion::Pal.framebuffer_width() as usize
            + GtiaRegion::Pal.border_left() as usize;
        let player_argb = gtia.colour_to_argb32(0x38);
        assert_eq!(fb[active_start + 64], player_argb);
        assert_eq!(fb[active_start + 65], player_argb);
    }

    #[test]
    fn player_multiplexes_within_a_scanline() {
        // Phase 3: players are evaluated per pixel from the *live* HPOS as the
        // beam advances, so rewriting HPOSP0 partway across the line draws the
        // same player object at two X positions — sprite multiplexing. The old
        // whole-line overlay sampled HPOS once (the final value), so the early
        // copy could not exist.
        let mut gtia = Gtia::new(GtiaRegion::Pal);
        gtia.write(0x0D, 0xFF); // GRAFP0: solid 8-pixel pattern
        gtia.write(0x12, 0x3A); // COLPM0: a visible colour
        // Blank line — no playfield, so any drawn pixel is the player.
        gtia.begin_scanline(0, &[], 160, AnticMode::Blank);

        gtia.write(0x00, 50); // HPOSP0 = 50 → covers cc 50..58 → active-x 4..20
        gtia.composite_playfield(40); // beam crosses the left copy with HPOS 50
        gtia.write(0x00, 180); // HPOSP0 = 180 → covers cc 180..188 → active-x 264..280
        gtia.composite_playfield(GtiaRegion::Pal.framebuffer_width() as usize); // right copy

        let fb = gtia.framebuffer();
        let base = GtiaRegion::Pal.border_top() as usize
            * GtiaRegion::Pal.framebuffer_width() as usize
            + GtiaRegion::Pal.border_left() as usize;
        let player_argb = gtia.colour_to_argb32(0x3A);
        let bg_argb = gtia.colour_to_argb32(0x00);
        // Left copy (HPOS 50): cc 52 → active-x 8.
        assert_eq!(fb[base + 8], player_argb, "left copy at the first HPOS");
        // Right copy (HPOS 180): cc 183 → active-x 270.
        assert_eq!(fb[base + 270], player_argb, "right copy at the second HPOS");
        // Between the two copies the player is absent (background shows).
        assert_eq!(fb[base + 140], bg_argb, "no player between the two copies");
    }

    #[test]
    fn collision_detection_player_playfield() {
        let mut gtia = Gtia::new(GtiaRegion::Pal);
        // Place player 0 at HPOS=60 (fb_x = (60-48)*2 = 24)
        gtia.write(0x00, 60); // HPOSP0
        gtia.write(0x0D, 0x80); // GRAFP0: leftmost bit only
        gtia.write(0x12, 0x0E); // COLPM0

        // Playfield with colour index 1 (COLPF0) at the overlap position
        let mut playfield = vec![0u8; 160];
        playfield[12] = 1; // pixel at position 12 → fb_x=24 (centred 160cc PF)

        gtia.render_line(0, &playfield, 160, AnticMode::ModeD);

        // P0PF should have bit 0 set (hit PF0)
        let p0pf = gtia.read(0x04);
        assert_ne!(p0pf & 0x01, 0, "Player 0 should collide with PF0");
    }

    #[test]
    fn players_collide_over_bare_background() {
        // Player-to-player collisions register wherever the objects overlap,
        // even over background with no playfield. The old overlay only recorded
        // collisions where playfield was present, so two players over bare
        // background never registered.
        let mut gtia = Gtia::new(GtiaRegion::Pal);
        // Players 0 and 1 both at HPOS=60 with a solid pattern → they overlap.
        gtia.write(0x00, 60); // HPOSP0
        gtia.write(0x01, 60); // HPOSP1
        gtia.write(0x0D, 0xFF); // GRAFP0
        gtia.write(0x0E, 0xFF); // GRAFP1
        gtia.write(0x12, 0x0E); // COLPM0
        gtia.write(0x13, 0x2A); // COLPM1

        // Blank line — no playfield anywhere.
        gtia.render_line(0, &[], 160, AnticMode::Blank);

        assert_ne!(gtia.read(0x0C) & 0x02, 0, "P0PL should record a hit on P1");
        assert_ne!(gtia.read(0x0D) & 0x01, 0, "P1PL should record a hit on P0");
    }

    #[test]
    fn collisions_accumulate_at_beam_time() {
        // Phase 3: collisions are recorded per pixel as the beam advances, not
        // in one pass at line end. So a collision is visible after compositing
        // only the left part of the line (the old whole-line overlay would read
        // zero until the line finished), and a mid-line HITCLR wipes only what
        // has been drawn so far — the next pixels re-accumulate it.
        let mut gtia = Gtia::new(GtiaRegion::Pal);
        gtia.write(0x00, 60); // HPOSP0 = 60 → active-x 24..
        gtia.write(0x08, 0x03); // SIZEP0 = quad → 8×4 = 32 cc wide (active-x 24..88)
        gtia.write(0x0D, 0xFF); // GRAFP0: solid
        gtia.write(0x12, 0x0E); // COLPM0: visible
        // Playfield (PF0) under the whole player span (fb-x 24..88 → bytes 12..44).
        let mut playfield = vec![0u8; 160];
        for b in playfield.iter_mut().take(44).skip(12) {
            *b = 1; // colour index 1 = COLPF0
        }
        gtia.begin_scanline(0, &playfield, 160, AnticMode::ModeD);

        // Composite only as far as active-x 50 — the beam has crossed the left
        // part of the player/playfield overlap.
        gtia.composite_playfield(50);
        assert_ne!(
            gtia.read(0x04) & 0x01,
            0,
            "collision already recorded mid-line (beam-time, not line-end)"
        );

        // HITCLR mid-line clears what has been drawn so far…
        gtia.write(0x1E, 0x00);
        assert_eq!(gtia.read(0x04) & 0x01, 0, "HITCLR cleared the collision");

        // …and the rest of the line re-accumulates it as the beam continues.
        gtia.finish_scanline();
        assert_ne!(
            gtia.read(0x04) & 0x01,
            0,
            "right of the line re-records the collision after HITCLR"
        );
    }

    #[test]
    fn collision_clear() {
        let mut gtia = Gtia::new(GtiaRegion::Pal);
        // Set up a collision
        gtia.p_pf[0] = 0x03;
        gtia.m_pf[1] = 0x05;

        // Write HITCLR
        gtia.write(0x1E, 0x00);

        assert_eq!(gtia.read(0x04), 0, "P0PF should be cleared");
        assert_eq!(gtia.read(0x01), 0, "M1PF should be cleared");
    }

    #[test]
    fn trigger_inputs() {
        let mut gtia = Gtia::new(GtiaRegion::Pal);
        // Default: all released (1)
        assert_eq!(gtia.read(0x10), 1);
        assert_eq!(gtia.read(0x11), 1);

        // Press trigger 0
        gtia.set_trigger(0, true);
        assert_eq!(gtia.read(0x10), 0);

        // Release trigger 0
        gtia.set_trigger(0, false);
        assert_eq!(gtia.read(0x10), 1);
    }

    #[test]
    fn consol_register() {
        let mut gtia = Gtia::new(GtiaRegion::Pal);
        // Default: all buttons released (bits 0-2 = 1)
        assert_eq!(gtia.read(0x1F) & 0x07, 0x07);

        // Switch input — simulates pressing START (bit 0 = 0)
        gtia.set_console_switches(0x06);
        assert_eq!(gtia.read(0x1F) & 0x07, 0x06);

        // A CONSOL write (speaker pulse) must NOT corrupt the switch read.
        gtia.write(0x1F, 0x00);
        assert_eq!(
            gtia.read(0x1F) & 0x07,
            0x06,
            "speaker write clobbered the console-switch read"
        );
    }

    #[test]
    fn framebuffer_size() {
        let gtia = Gtia::new(GtiaRegion::Pal);
        assert_eq!(
            gtia.framebuffer_width(),
            GtiaRegion::Pal.framebuffer_width()
        );
        assert_eq!(
            gtia.framebuffer_height(),
            GtiaRegion::Pal.framebuffer_height()
        );
        assert_eq!(
            gtia.framebuffer().len(),
            (GtiaRegion::Pal.framebuffer_width() * GtiaRegion::Pal.framebuffer_height()) as usize
        );
    }

    /// GRACTL: bit 0 admits missile DMA, bit 1 player DMA (Master Memory Map).
    const GRACTL_MISSILES: u8 = 0x01;
    const GRACTL_PLAYERS: u8 = 0x02;
    const TWO_LINE: bool = false;
    const ONE_LINE: bool = true;

    /// GRACTL bit 2 latches the trigger inputs.
    const GRACTL_LATCH: u8 = 0x04;

    #[test]
    fn a_latched_trigger_reads_pressed_until_gractl_releases_it() {
        // "all TRIG BITs 0 are latched when the button is pressed and are only
        // reset to one when BIT 2 of GRACTL is reset to zero" — Mapping the
        // Atari, TRIG0.
        let mut gtia = Gtia::new(GtiaRegion::Ntsc);
        gtia.write(0x1D, GRACTL_LATCH);

        gtia.set_trigger(0, true);
        assert_eq!(gtia.read(0x10), 0, "pressed");
        gtia.set_trigger(0, false);
        assert_eq!(gtia.read(0x10), 0, "still reads pressed while latched");

        gtia.write(0x1D, 0x00);
        assert_eq!(gtia.read(0x10), 1, "clearing bit 2 releases the latch");
    }

    #[test]
    fn triggers_follow_the_button_when_latching_is_off() {
        let mut gtia = Gtia::new(GtiaRegion::Ntsc);
        gtia.set_trigger(1, true);
        assert_eq!(gtia.read(0x11), 0);
        gtia.set_trigger(1, false);
        assert_eq!(gtia.read(0x11), 1, "no latch, so the release shows");
    }

    #[test]
    fn enabling_the_latch_catches_a_button_already_held() {
        let mut gtia = Gtia::new(GtiaRegion::Ntsc);
        gtia.set_trigger(2, true);
        gtia.write(0x1D, GRACTL_LATCH);
        gtia.set_trigger(2, false);
        assert_eq!(gtia.read(0x12), 0, "the held button latched on enable");
    }

    #[test]
    fn latching_is_all_four_triggers_or_none() {
        // "you cannot set the latch mode for individual triggers" — Mapping
        // the Atari, GRACTL.
        let mut gtia = Gtia::new(GtiaRegion::Ntsc);
        gtia.write(0x1D, GRACTL_LATCH);
        for i in 0..4u8 {
            gtia.set_trigger(i, true);
            gtia.set_trigger(i, false);
            assert_eq!(gtia.read(0x10 + i), 0, "trigger {i} latched");
        }
        gtia.write(0x1D, 0x00);
        for i in 0..4u8 {
            assert_eq!(gtia.read(0x10 + i), 1, "trigger {i} released together");
        }
    }

    #[test]
    fn dma_is_ignored_until_gractl_admits_it() {
        let mut gtia = Gtia::new(GtiaRegion::Ntsc);
        // The CPU has written a pattern directly; DMA is off, so it stands.
        gtia.write(0x0D, 0xAA);
        gtia.write(0x11, 0x55);
        gtia.accept_pm_dma([0xFF; 4], 0xFF, ONE_LINE);
        assert_eq!(gtia.grafp[0], 0xAA, "players need GRACTL bit 1");
        assert_eq!(gtia.grafm, 0x55, "missiles need GRACTL bit 0");

        gtia.write(0x1D, GRACTL_PLAYERS);
        gtia.accept_pm_dma([0x3C; 4], 0xFF, ONE_LINE);
        assert_eq!(gtia.grafp[0], 0x3C, "players admitted");
        assert_eq!(gtia.grafm, 0x55, "missiles still not admitted");

        gtia.write(0x1D, GRACTL_PLAYERS | GRACTL_MISSILES);
        gtia.accept_pm_dma([0x3C; 4], 0x99, ONE_LINE);
        assert_eq!(gtia.grafm, 0x99, "missiles admitted");
    }

    #[test]
    fn vdelay_holds_a_player_back_one_line() {
        let mut gtia = Gtia::new(GtiaRegion::Ntsc);
        gtia.write(0x1D, GRACTL_PLAYERS);
        gtia.write(0x1C, 0x20); // VDELAY bit 5 = player 1

        gtia.accept_pm_dma([0x11, 0x11, 0x11, 0x11], 0, TWO_LINE);
        assert_eq!(gtia.grafp[0], 0x11, "player 0 is not delayed");
        assert_eq!(gtia.grafp[1], 0x00, "player 1 shows the previous line");

        gtia.accept_pm_dma([0x22, 0x22, 0x22, 0x22], 0, TWO_LINE);
        assert_eq!(gtia.grafp[0], 0x22, "player 0 keeps up");
        assert_eq!(gtia.grafp[1], 0x11, "player 1 trails by one line");
    }

    #[test]
    fn vdelay_does_nothing_in_the_one_line_display() {
        // "when the two line resolution display is enabled" — Mapping the
        // Atari, $D01C. One-line P/M already has per-line positioning.
        let mut gtia = Gtia::new(GtiaRegion::Ntsc);
        gtia.write(0x1D, GRACTL_PLAYERS);
        gtia.write(0x1C, 0xFF); // every object delayed
        gtia.accept_pm_dma([0x11; 4], 0, ONE_LINE);
        assert_eq!(gtia.grafp, [0x11; 4], "no delay at one-line resolution");
    }

    #[test]
    fn vdelay_delays_one_missile_without_disturbing_its_neighbours() {
        // GRAFM packs four missiles at two bits each, so a delayed missile
        // must take only its own pair from the held byte.
        let mut gtia = Gtia::new(GtiaRegion::Ntsc);
        gtia.write(0x1D, GRACTL_MISSILES);
        gtia.write(0x1C, 0x02); // VDELAY bit 1 = missile 1

        gtia.accept_pm_dma([0; 4], 0b1111_1111, TWO_LINE);
        assert_eq!(
            gtia.grafm, 0b1111_0011,
            "missile 1's pair still shows the previous line's zeros"
        );

        gtia.accept_pm_dma([0; 4], 0b0000_0000, TWO_LINE);
        assert_eq!(
            gtia.grafm, 0b0000_1100,
            "missile 1's pair trails by one line while the others keep up"
        );
    }

    #[test]
    fn priority_default_players_over_playfield() {
        let mut gtia = Gtia::new(GtiaRegion::Pal);
        // Player 0 at position overlapping a playfield pixel
        gtia.write(0x00, 60); // HPOSP0 = 60
        gtia.write(0x0D, 0x80); // GRAFP0: leftmost bit
        gtia.write(0x12, 0x38); // COLPM0 colour
        gtia.write(0x16, 0x94); // COLPF0 colour

        // PF pixel at the same position
        let mut playfield = vec![0u8; 160];
        playfield[12] = 1; // PF0 at overlap position

        gtia.render_line(0, &playfield, 160, AnticMode::ModeD);

        // With default priority, player should win
        let fb = gtia.framebuffer();
        let player_argb = gtia.colour_to_argb32(0x38);
        let active_x = ((60 - PF_LEFT_CC) * 2) as usize;
        let fb_idx = GtiaRegion::Pal.border_top() as usize
            * GtiaRegion::Pal.framebuffer_width() as usize
            + GtiaRegion::Pal.border_left() as usize
            + active_x;
        assert_eq!(
            fb[fb_idx], player_argb,
            "Player should be on top at default priority"
        );
    }

    /// Put two players at the same colour clock and return the pixel there.
    /// `colours` are the raw COLPM values for the two players.
    fn player_overlap_colour(prior: u8, players: (u8, u8), colours: (u8, u8)) -> u32 {
        let mut gtia = Gtia::new(GtiaRegion::Pal);
        for (player, colour) in [(players.0, colours.0), (players.1, colours.1)] {
            gtia.write(player, 60); // HPOSPx — same position, so they overlap
            gtia.write(0x0D + player, 0x80); // GRAFPx — leftmost bit
            gtia.write(0x12 + player, colour); // COLPMx
        }
        gtia.write(0x1B, prior);

        let pixels = vec![0u8; 160];
        gtia.render_line(0, &pixels, 160, AnticMode::ModeD);

        let active_x = ((60 - PF_LEFT_CC) * 2) as usize;
        let fb_idx = GtiaRegion::Pal.border_top() as usize
            * GtiaRegion::Pal.framebuffer_width() as usize
            + GtiaRegion::Pal.border_left() as usize
            + active_x;
        gtia.framebuffer()[fb_idx]
    }

    /// PRIOR bit 5 — the multi-colour player enable.
    const PRIOR_MULTI_COLOUR: u8 = 0x20;

    #[test]
    fn overlapping_players_zero_and_one_make_a_third_colour() {
        // "The Atari performs a logical OR to colors of players 0/1 and 2/3
        // when they overlap ... If player one is pink and player 0 is blue, the
        // overlap is green." — Mapping the Atari, PRIOR.
        let expected = palette::PAL_PALETTE[0x46 | 0x82];
        assert_eq!(
            player_overlap_colour(PRIOR_MULTI_COLOUR | 0x01, (0, 1), (0x46, 0x82)),
            expected,
            "P0/P1 overlap should take the ORed colour"
        );
    }

    #[test]
    fn overlapping_players_two_and_three_make_a_third_colour() {
        let expected = palette::PAL_PALETTE[0x24 | 0x90];
        assert_eq!(
            player_overlap_colour(PRIOR_MULTI_COLOUR | 0x01, (2, 3), (0x24, 0x90)),
            expected,
            "P2/P3 is the other sanctioned pair"
        );
    }

    #[test]
    fn an_unsanctioned_pair_overlaps_to_black() {
        // "you will not get a third color when players 1 and 3 overlap, for
        // example (you will get black instead)".
        let black = palette::PAL_PALETTE[0];
        assert_eq!(
            player_overlap_colour(PRIOR_MULTI_COLOUR | 0x01, (1, 3), (0x46, 0x82)),
            black,
            "only 0/1 and 2/3 combine"
        );
    }

    #[test]
    fn players_overlap_to_black_when_the_option_is_off() {
        // "If you don't enable the overlap option, the area of overlap for all
        // players will be black." Not the front player's colour, which is what
        // a plain priority sort would give.
        let black = palette::PAL_PALETTE[0];
        assert_eq!(
            player_overlap_colour(0x01, (0, 1), (0x46, 0x82)),
            black,
            "no bit 5, so the overlap is black rather than player 0"
        );
    }

    #[test]
    fn a_lone_player_keeps_its_own_colour_either_way() {
        for prior in [0x01, PRIOR_MULTI_COLOUR | 0x01] {
            let mut gtia = Gtia::new(GtiaRegion::Pal);
            gtia.write(0x00, 60);
            gtia.write(0x0D, 0x80);
            gtia.write(0x12, 0x46);
            gtia.write(0x1B, prior);
            let pixels = vec![0u8; 160];
            gtia.render_line(0, &pixels, 160, AnticMode::ModeD);
            let active_x = ((60 - PF_LEFT_CC) * 2) as usize;
            let fb_idx = GtiaRegion::Pal.border_top() as usize
                * GtiaRegion::Pal.framebuffer_width() as usize
                + GtiaRegion::Pal.border_left() as usize
                + active_x;
            assert_eq!(
                gtia.framebuffer()[fb_idx],
                palette::PAL_PALETTE[0x46],
                "one player is not an overlap (PRIOR ${prior:02X})"
            );
        }
    }

    fn priority_overlap_colour(prior: u8, player: u8, playfield: u8) -> u32 {
        let mut gtia = Gtia::new(GtiaRegion::Pal);
        gtia.write(player, 60);
        gtia.write(0x0D + player, 0x80);
        gtia.write(0x12 + player, 0x20 + player * 0x10 + 0x08);
        gtia.write(0x15 + playfield, 0x80 + playfield * 0x10 + 0x04);
        gtia.write(0x1B, prior);

        let mut pixels = vec![0u8; 160];
        pixels[12] = playfield;
        gtia.render_line(0, &pixels, 160, AnticMode::ModeD);

        let active_x = ((60 - PF_LEFT_CC) * 2) as usize;
        let fb_idx = GtiaRegion::Pal.border_top() as usize
            * GtiaRegion::Pal.framebuffer_width() as usize
            + GtiaRegion::Pal.border_left() as usize
            + active_x;
        gtia.framebuffer()[fb_idx]
    }

    #[test]
    fn prior_scheme_one_puts_all_players_over_playfield() {
        let expected = Gtia::new(GtiaRegion::Pal).colour_to_argb32(0x48);
        assert_eq!(priority_overlap_colour(0x01, 2, 1), expected);
    }

    #[test]
    fn prior_scheme_two_splits_players_around_playfield() {
        let gtia = Gtia::new(GtiaRegion::Pal);
        assert_eq!(
            priority_overlap_colour(0x02, 0, 1),
            gtia.colour_to_argb32(0x28)
        );
        assert_eq!(
            priority_overlap_colour(0x02, 2, 1),
            gtia.colour_to_argb32(0x94)
        );
    }

    #[test]
    fn prior_scheme_three_puts_all_playfield_over_players() {
        let expected = Gtia::new(GtiaRegion::Pal).colour_to_argb32(0x94);
        assert_eq!(priority_overlap_colour(0x04, 0, 1), expected);
    }

    #[test]
    fn prior_scheme_four_splits_playfield_around_players() {
        let gtia = Gtia::new(GtiaRegion::Pal);
        assert_eq!(
            priority_overlap_colour(0x08, 0, 1),
            gtia.colour_to_argb32(0x94)
        );
        assert_eq!(
            priority_overlap_colour(0x08, 0, 3),
            gtia.colour_to_argb32(0x28)
        );
    }

    #[test]
    fn conflicting_prior_schemes_render_overlap_black() {
        let gtia = Gtia::new(GtiaRegion::Pal);
        assert_eq!(
            priority_overlap_colour(0x05, 0, 1),
            gtia.colour_to_argb32(0)
        );
        assert_eq!(
            priority_overlap_colour(0x03, 0, 1),
            gtia.colour_to_argb32(0x28),
            "schemes which agree on this overlap should retain their winner"
        );
    }

    #[test]
    fn prior_changes_at_the_beam() {
        let mut gtia = Gtia::new(GtiaRegion::Pal);
        gtia.write(0x00, 60);
        gtia.write(0x08, 0x03); // quadruple-width player covers both samples
        gtia.write(0x0D, 0xFF);
        gtia.write(0x12, 0x38);
        gtia.write(0x16, 0x94);
        gtia.write(0x1B, 0x01);
        gtia.begin_scanline(0, &[1u8; 160], 160, AnticMode::ModeD);
        gtia.composite_playfield(72);
        gtia.write(0x1B, 0x04);
        gtia.composite_playfield(GtiaRegion::Pal.framebuffer_width() as usize);

        let row =
            GtiaRegion::Pal.border_top() as usize * GtiaRegion::Pal.framebuffer_width() as usize;
        assert_eq!(
            gtia.framebuffer()[row + 52],
            gtia.colour_to_argb32(0x38),
            "left side should retain player-first PRIOR"
        );
        assert_eq!(
            gtia.framebuffer()[row + 80],
            gtia.colour_to_argb32(0x94),
            "right side should use the live playfield-first PRIOR"
        );
    }

    /// #1086: a wide playfield used to be clipped to the normal playfield's
    /// 320 pixels, losing 32 a side — and worse, keeping the *leftmost* 320
    /// data pixels rather than the ones the hardware displays.
    #[test]
    fn each_playfield_width_lands_where_the_chip_displays_it() {
        // Altirra: narrow at $40-$BF, normal at $30-$CF, wide mapped to
        // $20-$DF but displayed only within $2C-$DD. In half colour clocks
        // that is 128..384, 96..416 and 88..444, and a framebuffer pixel is
        // one half colour clock from the window's own origin.
        for region in [GtiaRegion::Ntsc, GtiaRegion::Pal] {
            let first = region.first_half_clock();
            let width = region.framebuffer_width() as usize;
            for (pf_width, data_len, first_h, last_h) in [
                (128u16, 128usize, 128u16, 384u16),
                (160, 160, 96, 416),
                (192, 192, 88, 444),
            ] {
                let mut gtia = Gtia::new(region);
                gtia.begin_scanline(0, &vec![1u8; data_len], pf_width, AnticMode::ModeD);
                let (start, end) = gtia.sl_pf_span;
                assert_eq!(
                    start,
                    (first_h - first) as usize,
                    "{region:?} {pf_width}cc starts in the wrong place"
                );
                assert_eq!(
                    end,
                    usize::min((last_h - first) as usize, width),
                    "{region:?} {pf_width}cc ends in the wrong place"
                );
            }
        }
    }

    #[test]
    fn a_wide_playfield_shows_the_slice_antic_displays_not_the_leftmost() {
        // Wide data pixel 0 maps to colour clock $20, but ANTIC clips the left
        // edge and display starts at $2C — 12 colour clocks in, so 12 entries
        // of data never reach the screen. Marking exactly the first displayed
        // one proves the slice is aligned rather than merely wide: clipping to
        // 320 used to put data pixel 0 at the playfield's left edge, so this
        // pixel landed 12 colour clocks too far right.
        let region = GtiaRegion::Ntsc;
        let mut playfield = vec![0u8; 192]; // one entry per colour clock
        playfield[12] = 1; // colour clock $20 + 12 = $2C

        let mut gtia = Gtia::new(region);
        gtia.write(0x16, 0x94); // COLPF0
        gtia.write(0x1A, 0x00); // COLBK
        gtia.render_line(0, &playfield, 192, AnticMode::ModeD);

        let row = gtia.border_top() as usize * region.framebuffer_width() as usize;
        let x = (88 - region.first_half_clock()) as usize;
        assert_eq!(
            x, 19,
            "the first displayed wide pixel is 19 into the window"
        );
        assert_eq!(
            gtia.framebuffer()[row + x],
            gtia.colour_to_argb32(0x94),
            "the first displayed data pixel must land on the first displayed clock"
        );
        assert_eq!(
            gtia.framebuffer()[row + x - 1],
            gtia.colour_to_argb32(0x00),
            "and the clock before it is still border"
        );
    }

    #[test]
    fn a_wide_playfield_fills_more_of_the_window_than_a_normal_one() {
        // The point of the fix, stated in pixels: 178 displayed colour clocks
        // is 356 pixels of a 374-pixel window, against the normal playfield's
        // 320. Clipping to ACTIVE_WIDTH threw away 36 of them.
        let region = GtiaRegion::Ntsc;
        let mut gtia = Gtia::new(region);
        gtia.begin_scanline(0, &[1u8; 192], 192, AnticMode::ModeD);
        let (start, end) = gtia.sl_pf_span;
        assert_eq!(
            end - start,
            355,
            "wide reaches all but the last window pixel"
        );
        assert!(
            end - start > ACTIVE_WIDTH as usize,
            "a wide playfield must be wider than the normal one it was clipped to"
        );
    }

    #[test]
    fn a_player_in_the_border_is_drawn() {
        // Altirra, on the range ANTIC clips out of a wide playfield: "P/M
        // graphics can still display within this range $22-$2B." Compositing
        // over the normal playfield's 320 pixels could not draw them at all —
        // colour clocks below $30 had no active-x to land on.
        let region = GtiaRegion::Pal;
        let mut gtia = Gtia::new(region);
        gtia.write(0x00, 36); // HPOSP0 = $24, inside the border
        gtia.write(0x0D, 0xFF); // GRAFP0: solid
        gtia.write(0x12, 0x38); // COLPM0

        gtia.render_line(0, &[], 160, AnticMode::Blank);

        let row = gtia.border_top() as usize * region.framebuffer_width() as usize;
        let x = (36 * 2 - region.first_half_clock()) as usize;
        assert!(
            x < gtia.border_left() as usize,
            "the test position is border"
        );
        assert_eq!(
            gtia.framebuffer()[row + x],
            gtia.colour_to_argb32(0x38),
            "a player left of the playfield must still reach the screen"
        );
    }

    #[test]
    fn gtia_mode_selection() {
        let mut gtia = Gtia::new(GtiaRegion::Pal);

        // Default: mode 0
        assert_eq!((gtia.prior >> 6) & 0x03, 0);

        // Set mode 9 (PRIOR bits 6-7 = 01)
        gtia.write(0x1B, 0x40);
        assert_eq!((gtia.prior >> 6) & 0x03, 1);

        // Set mode 10 (PRIOR bits 6-7 = 10)
        gtia.write(0x1B, 0x80);
        assert_eq!((gtia.prior >> 6) & 0x03, 2);

        // Set mode 11 (PRIOR bits 6-7 = 11)
        gtia.write(0x1B, 0xC0);
        assert_eq!((gtia.prior >> 6) & 0x03, 3);
    }

    /// The GTIA modes transform whatever ANTIC sends, border and blank
    /// lines included, where the nibble is 0: mode 10 shows COLPM0 there,
    /// mode 11 shows COLBK's hue with no luminance, and mode 9 shows COLBK.
    #[test]
    fn gtia_modes_colour_a_blank_line_from_nibble_zero() {
        let mut gtia = Gtia::new(GtiaRegion::Ntsc);
        gtia.write(0x12, 0x38); // COLPM0
        gtia.write(0x1A, 0x94); // COLBK, deliberately different
        let row = gtia.border_top() as usize * gtia.fb_width as usize;
        let width = gtia.fb_width as usize;

        for (prior, colour) in [(0x40, 0x94), (0x80, 0x38), (0xC0, 0x90)] {
            gtia.write(0x1B, prior);
            gtia.render_line(0, &[], 160, AnticMode::Blank);
            let want = gtia.colour_to_argb32(colour);
            assert!(
                gtia.framebuffer()[row..row + width]
                    .iter()
                    .all(|&px| px == want),
                "PRIOR ${prior:02X}: blank line should be ${colour:02X} across"
            );
        }
    }
}
