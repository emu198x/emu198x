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

use palette::NTSC_PALETTE;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Framebuffer width (hires resolution: 320 pixels).
/// Active playfield area dimensions (the pixels ANTIC + GTIA draw
/// playfield + player/missile content into).
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
}

/// First visible colour clock in the normal playfield (160 clocks wide).
const PF_LEFT_CC: u16 = 48;

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

// ---------------------------------------------------------------------------
// GTIA chip
// ---------------------------------------------------------------------------

/// Atari GTIA graphics chip.
#[derive(Serialize, Deserialize)]
pub struct Gtia {
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
    grafm: u8,      // GRAFM: 2-bit missile graphic patterns

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
    sl_gtia_mode: u8,           // PRIOR bits 6-7 (GTIA 9/10/11 modes)
    sl_pf_width: u16,           // playfield width in colour clocks
    sl_pf_span: (usize, usize), // active-x [start, end) the playfield occupies
    #[serde(with = "BigArray")]
    sl_line_buf: [u8; ACTIVE_WIDTH as usize], // per-pixel playfield colour-register indices
    sl_playfield: Vec<u8>,      // raw ANTIC playfield bytes (GTIA 9/10/11 resolve)
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
}

impl Gtia {
    /// Create a new GTIA in its power-on state, feeding `region`'s field.
    #[must_use]
    pub fn new(region: GtiaRegion) -> Self {
        Self {
            colpm: [0; 4],
            colpf: [0; 4],
            colbk: 0,
            hposp: [0; 4],
            hposm: [0; 4],
            sizep: [0; 4],
            sizem: 0,
            grafp: [0; 4],
            grafm: 0,
            prior: 0,
            vdelay: 0,
            gractl: 0,
            m_pf: [0; 4],
            p_pf: [0; 4],
            m_pl: [0; 4],
            p_pl: [0; 4],
            trig: [1; 4], // all released
            consol_out: 0x00,
            console_switches: 0x07, // all buttons released (active low)
            sl_visible: false,
            sl_fb_offset: 0,
            sl_mode: AnticMode::Blank,
            sl_gtia_mode: 0,
            sl_pf_width: 0,
            sl_pf_span: (0, 0),
            sl_line_buf: [0; ACTIVE_WIDTH as usize],
            sl_playfield: Vec::new(),
            sl_x: 0,
            framebuffer: vec![
                0xFF00_0000;
                (region.framebuffer_width() * region.framebuffer_height()) as usize
            ],
            fb_width: region.framebuffer_width(),
            fb_border_top: region.border_top(),
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
            0x1D => self.gractl = value,
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
            0x10..=0x13 => self.trig[(reg - 0x10) as usize],
            // PAL flag (always NTSC = 0 for now)
            0x14 => 0x00,
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
        }
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

    /// Pixels of border left of the active playfield, from the line width.
    #[must_use]
    pub const fn border_left(&self) -> u32 {
        (self.fb_width - ACTIVE_WIDTH) / 2
    }

    // -----------------------------------------------------------------------
    // Rendering
    // -----------------------------------------------------------------------

    /// Fill the entire framebuffer with the current backdrop colour
    /// (COLBK / colour register 0). Called by the machine at frame
    /// start so the canonical TV-visible border around the active
    /// 320 x 240 playfield carries the current backdrop colour.
    /// Mid-frame COLBK changes affect the *next* frame — v1
    /// simplification matching the TMS9918/sega-vdp treatment.
    pub fn fill_border(&mut self) {
        let argb = colour_to_argb32(self.colbk);
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
        self.sl_fb_offset = fb_row * self.fb_width as usize + self.border_left() as usize;
        self.sl_mode = mode;
        self.sl_gtia_mode = (self.prior >> 6) & 0x03;
        self.sl_pf_width = pf_width;
        self.sl_playfield.clear();
        self.sl_playfield.extend_from_slice(playfield);

        // Build the 320-pixel line of playfield colour-register indices.
        let mut line_buf = [0u8; ACTIVE_WIDTH as usize];
        self.sl_pf_span = if mode != AnticMode::Blank {
            self.fill_playfield_line(&mut line_buf, playfield, pf_width, mode, self.sl_gtia_mode)
        } else {
            (0, 0)
        };
        self.sl_line_buf = line_buf;
    }

    /// Composite pixels from the cursor up to (but not including) active-x
    /// `end`, and advance the cursor. Each pixel resolves the playfield index
    /// and the player/missile coverage from the *live* registers at that pixel's
    /// beam colour-clock, applies default priority (PM over PF over background),
    /// and records collisions — so mid-line colour, HPOS/GRAFP and HITCLR writes
    /// all land at the beam. Calling with `end == ACTIVE_WIDTH` finishes the
    /// line; the beam-driven path calls it repeatedly with the beam position.
    pub fn composite_playfield(&mut self, end: usize) {
        if !self.sl_visible {
            return;
        }
        let end = end.min(ACTIVE_WIDTH as usize);

        // Hi-res 1.5-colour modes (2, 3, and F with no GTIA override): the
        // playfield background is COLPF2 and lit pixels take COLPF2's hue with
        // COLPF1's luminance. Anything outside the playfield is COLBK border.
        let hires_text = self.sl_gtia_mode == 0
            && matches!(
                self.sl_mode,
                AnticMode::Mode2 | AnticMode::Mode3 | AnticMode::ModeF
            );

        while self.sl_x < end {
            let x = self.sl_x;
            let pf_col_idx = self.sl_line_buf[x];

            // Players/missiles at this pixel's beam colour-clock, from the
            // *live* registers — so a mid-line HPOS/GRAFP rewrite (sprite
            // multiplexing) and per-pixel collision timing land at the beam.
            let cc = PF_LEFT_CC + (x as u16) / 2;
            let (pm_colour, pm_bits) = self.pm_at_cc(cc);

            // Collisions (independent of the final priority): PM-vs-playfield
            // where playfield is present, PM-vs-PM wherever objects overlap.
            if pm_bits != 0 {
                self.record_collisions(pm_bits, pf_col_idx);
            }

            let in_pf = x >= self.sl_pf_span.0 && x < self.sl_pf_span.1;
            let colour = if pm_colour != 0 {
                pm_colour
            } else if hires_text && in_pf {
                if pf_col_idx != 0 {
                    (self.colpf[2] & 0xF0) | (self.colpf[1] & 0x0F)
                } else {
                    self.colpf[2]
                }
            } else if pf_col_idx != 0 {
                self.resolve_colour(
                    pf_col_idx,
                    self.sl_gtia_mode,
                    &self.sl_playfield,
                    x,
                    self.sl_pf_width,
                    self.sl_mode,
                )
            } else {
                self.colbk
            };

            self.framebuffer[self.sl_fb_offset + x] = colour_to_argb32(colour);
            self.sl_x += 1;
        }
    }

    /// Player/missile colour and object-bit mask covering beam colour-clock
    /// `cc`, evaluated from the live registers. Missiles paint first and only
    /// where still empty; players then overwrite the colour and OR their object
    /// bit — default priority, matching the prior whole-line overlay. A colour
    /// of 0 doubles as "no PM pixel" (a player whose COLPM is 0 still collides
    /// but shows the playfield through), preserving the original sentinel.
    fn pm_at_cc(&self, cc: u16) -> (u8, u8) {
        let fifth_player = (self.prior & 0x10) != 0;
        let mut colour = 0u8;
        let mut bits = 0u8;
        for m in 0..NUM_MISSILES {
            if self.missile_covers(m, cc) && colour == 0 {
                colour = if fifth_player {
                    self.colpf[3]
                } else {
                    self.colpm[m]
                };
                bits |= 1 << (m + 4);
            }
        }
        for p in 0..NUM_PLAYERS {
            if self.player_covers(p, cc) {
                colour = self.colpm[p];
                bits |= 1 << p;
            }
        }
        (colour, bits)
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
        let target = if line_cc <= PF_LEFT_CC {
            0
        } else {
            usize::min(((line_cc - PF_LEFT_CC) * 2) as usize, ACTIVE_WIDTH as usize)
        };
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
        self.composite_playfield(ACTIVE_WIDTH as usize);
    }

    /// Fill the 320-pixel line buffer with playfield colour register indices.
    ///
    /// Returns the `[start, end)` framebuffer-x span that the playfield
    /// occupies, so the caller can tell in-playfield background from border.
    #[allow(clippy::unused_self)] // will use colour registers for GTIA mode expansion
    fn fill_playfield_line(
        &self,
        line_buf: &mut [u8; ACTIVE_WIDTH as usize],
        playfield: &[u8],
        pf_width: u16,
        mode: AnticMode,
        _gtia_mode: u8,
    ) -> (usize, usize) {
        // Pixels per colour clock depend on mode resolution
        let (pixels_per_cc, hires) = match mode {
            AnticMode::ModeF => (2, true),                     // 320 px / 160 cc
            AnticMode::ModeD | AnticMode::ModeE => (2, false), // 160 px → 2 fb px each
            AnticMode::Mode2 | AnticMode::Mode3 => (2, true),  // text hires
            _ => (2, false),
        };

        // Centre the playfield in the 320-pixel framebuffer
        let pf_fb_width = u16::min(pf_width * pixels_per_cc, ACTIVE_WIDTH as u16);
        let fb_start = ((ACTIVE_WIDTH as u16 - pf_fb_width) / 2) as usize;
        let mut fb_end = fb_start;

        if hires {
            // Hires: each playfield byte is one pixel → 1 fb pixel
            for (i, &px) in playfield.iter().enumerate() {
                let fb_x = fb_start + i;
                if fb_x < ACTIVE_WIDTH as usize {
                    line_buf[fb_x] = px;
                    fb_end = fb_x + 1;
                }
            }
        } else {
            // Non-hires: each playfield pixel maps to 2 fb pixels
            for (i, &px) in playfield.iter().enumerate() {
                let fb_x = fb_start + i * 2;
                if fb_x + 1 < ACTIVE_WIDTH as usize {
                    line_buf[fb_x] = px;
                    line_buf[fb_x + 1] = px;
                    fb_end = fb_x + 2;
                }
            }
        }

        (fb_start, fb_end)
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

    /// Map a playfield colour register index to an actual colour value.
    fn resolve_colour(
        &self,
        pf_idx: u8,
        gtia_mode: u8,
        _playfield: &[u8],
        _x: usize,
        _pf_width: u16,
        _mode: AnticMode,
    ) -> u8 {
        match gtia_mode {
            1 => {
                // Mode 9: 16-shade. Pixel value selects luminance, COLBK hue.
                let lum = (pf_idx & 0x0F) << 1;
                (self.colbk & 0xF0) | lum
            }
            2 => {
                // Mode 10: 9-colour. Use all 9 colour registers.
                match pf_idx {
                    0 => self.colbk,
                    1 => self.colpf[0],
                    2 => self.colpf[1],
                    3 => self.colpf[2],
                    4 => self.colpf[3],
                    5 => self.colpm[0],
                    6 => self.colpm[1],
                    7 => self.colpm[2],
                    8 => self.colpm[3],
                    _ => self.colbk,
                }
            }
            3 => {
                // Mode 11: 16-hue. Pixel value selects hue, COLBK luminance.
                let hue = (pf_idx & 0x0F) << 4;
                hue | (self.colbk & 0x0F)
            }
            _ => {
                // Normal: map index to colour register
                match pf_idx {
                    1 => self.colpf[0],
                    2 => self.colpf[1],
                    3 => self.colpf[2],
                    4 => self.colpf[3],
                    _ => self.colbk,
                }
            }
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
        data.push(self.grafm);
        data.push(self.prior);
        data.push(self.vdelay);
        data.push(self.gractl);
        data.extend_from_slice(&self.m_pf);
        data.extend_from_slice(&self.p_pf);
        data.extend_from_slice(&self.m_pl);
        data.extend_from_slice(&self.p_pl);
        data.extend_from_slice(&self.trig);
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

/// Convert an Atari colour register value to ARGB32 via the NTSC palette.
fn colour_to_argb32(colour: u8) -> u32 {
    let index = (colour >> 1) as usize;
    if index < NTSC_PALETTE.len() {
        NTSC_PALETTE[index]
    } else {
        0xFF00_0000 // black fallback
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
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
        gtia.write(0x1A, 0x0A); // COLBK = A
        gtia.begin_scanline(0, &[], 160, AnticMode::Blank);
        gtia.composite_playfield(ACTIVE_WIDTH as usize / 2); // left half at A
        gtia.write(0x1A, 0x0C); // COLBK = B
        gtia.composite_playfield(ACTIVE_WIDTH as usize); // right half at B

        let fb = gtia.framebuffer();
        let base = GtiaRegion::Pal.border_top() as usize
            * GtiaRegion::Pal.framebuffer_width() as usize
            + GtiaRegion::Pal.border_left() as usize;
        let colour_a = colour_to_argb32(0x0A);
        let colour_b = colour_to_argb32(0x0C);
        assert_ne!(colour_a, colour_b);
        assert_eq!(fb[base + 10], colour_a, "left half keeps the first COLBK");
        assert_eq!(fb[base + 150], colour_a, "still left of the change");
        assert_eq!(fb[base + 200], colour_b, "right half takes the new COLBK");
        assert_eq!(fb[base + 310], colour_b, "right edge takes the new COLBK");
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
        let player_argb = colour_to_argb32(0x38);
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
        gtia.composite_playfield(ACTIVE_WIDTH as usize); // right copy with HPOS 180

        let fb = gtia.framebuffer();
        let base = GtiaRegion::Pal.border_top() as usize
            * GtiaRegion::Pal.framebuffer_width() as usize
            + GtiaRegion::Pal.border_left() as usize;
        let player_argb = colour_to_argb32(0x3A);
        let bg_argb = colour_to_argb32(0x00);
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
        let player_argb = colour_to_argb32(0x38);
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
}
