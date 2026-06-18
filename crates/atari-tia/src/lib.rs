//! Atari TIA (Television Interface Adapter).
//!
//! Adapted from `Emu198x-Oldest/crates/atari-tia` (port 2026-06-01) as
//! the iconic / accuracy-hard chip of the Atari 2600 family. Self-
//! contained, no external dependencies. The TIA is the famous
//! racing-the-beam chip: this initial port preserves the donor's
//! pixel-level rendering model; refining cycle-perfect timing
//! (HMOVE quirks, RESP starfield-collision edge cases, audio mixing)
//! is in the accuracy backlog.
//!
//! The TIA generates the video signal for the Atari 2600. Unlike later
//! systems with framebuffers, the TIA renders one colour clock at a time
//! — the CPU must "race the beam" to update TIA registers before each
//! scanline is drawn.
//!
//! # Timing
//!
//! Each colour clock is one crystal tick:
//! - NTSC: 3,579,545 Hz crystal, 228 colour clocks per line, 262 lines per frame.
//! - PAL: 3,546,894 Hz crystal, 228 colour clocks per line, 312 lines per frame.
//!
//! The CPU runs at crystal/3 (1 CPU cycle = 3 colour clocks).
//!
//! # Visible region
//!
//! Of the 228 colour clocks per line, the first 68 are horizontal blank.
//! The visible region is colour clocks 68-227 (160 pixels). Vertical
//! timing is software-controlled via VSYNC/VBLANK.
//!
//! # Register map (active bits: A0-A5)
//!
//! | Addr  | Name    | Description                          |
//! |-------|---------|--------------------------------------|
//! | $00   | VSYNC   | Vertical sync (bit 1: start/stop)    |
//! | $01   | VBLANK  | Vertical blank control               |
//! | $02   | WSYNC   | Halt CPU until end of line            |
//! | $03   | RSYNC   | Reset horizontal counter             |
//! | $04   | NUSIZ0  | Player 0 / missile 0 size            |
//! | $05   | NUSIZ1  | Player 1 / missile 1 size            |
//! | $06   | COLUP0  | Player 0 colour                      |
//! | $07   | COLUP1  | Player 1 colour                      |
//! | $08   | COLUPF  | Playfield colour                     |
//! | $09   | COLUBK  | Background colour                    |
//! | $0A   | CTRLPF  | Playfield control                    |
//! | $0B   | REFP0   | Player 0 reflect                     |
//! | $0C   | REFP1   | Player 1 reflect                     |
//! | $0D   | PF0     | Playfield 0 (bits 4-7)               |
//! | $0E   | PF1     | Playfield 1 (bits 0-7)               |
//! | $0F   | PF2     | Playfield 2 (bits 0-7)               |
//! | $10   | RESP0   | Reset player 0 position              |
//! | $11   | RESP1   | Reset player 1 position              |
//! | $12   | RESM0   | Reset missile 0 position             |
//! | $13   | RESM1   | Reset missile 1 position             |
//! | $14   | RESBL   | Reset ball position                  |
//! | $15   | AUDC0   | Audio control 0                      |
//! | $16   | AUDC1   | Audio control 1                      |
//! | $17   | AUDF0   | Audio frequency 0                    |
//! | $18   | AUDF1   | Audio frequency 1                    |
//! | $19   | AUDV0   | Audio volume 0                       |
//! | $1A   | AUDV1   | Audio volume 1                       |
//! | $1B   | GRP0    | Player 0 graphics                    |
//! | $1C   | GRP1    | Player 1 graphics                    |
//! | $1D   | ENAM0   | Enable missile 0                     |
//! | $1E   | ENAM1   | Enable missile 1                     |
//! | $1F   | ENABL   | Enable ball                          |
//! | $20   | HMP0    | Horizontal motion player 0           |
//! | $21   | HMP1    | Horizontal motion player 1           |
//! | $22   | HMM0    | Horizontal motion missile 0          |
//! | $23   | HMM1    | Horizontal motion missile 1          |
//! | $24   | HMBL    | Horizontal motion ball               |
//! | $25   | VDELP0  | Vertical delay player 0              |
//! | $26   | VDELP1  | Vertical delay player 1              |
//! | $27   | VDELBL  | Vertical delay ball                  |
//! | $28   | RESMP0  | Reset missile 0 to player 0          |
//! | $29   | RESMP1  | Reset missile 1 to player 1          |
//! | $2A   | HMOVE   | Apply horizontal motion              |
//! | $2B   | HMCLR   | Clear horizontal motion registers    |
//! | $2C   | CXCLR   | Clear collision latches               |

mod audio;
mod palette;

use audio::TiaAudio;
pub use palette::{NTSC_PALETTE, PAL_PALETTE};

/// Framebuffer width: 160 visible colour clocks per line.
/// Width of the visible playfield region (TIA renders `tile` + sprite +
/// playfield + ball pixels into here).
pub const ACTIVE_WIDTH: u32 = 160;

/// Full colour-clocks per line including HBLANK. The framebuffer keeps
/// the canonical 228-clock line width; the 68-clock HBLANK region is
/// rendered black, because the TIA holds its output in blanking during
/// horizontal retrace (COLUBK only appears in the 160 visible clocks).
pub const FB_WIDTH: u32 = 228;

/// Number of colour clocks per scanline (68 hblank + 160 visible).
pub const CLOCKS_PER_LINE: u16 = 228;

/// Horizontal blank duration in colour clocks.
pub const HBLANK_CLOCKS: u16 = 68;

/// Colour-clock pipeline delay between a RESP/RESM/RESBL strobe and the
/// object's reset position taking effect. The TIA does not latch the position
/// at the beam's current clock — there is a small pipeline lag, commonly
/// described as 5 clocks (TIA reference § Horizontal positioning, note 3). The
/// exact value in the HBLANK region / under an active HMOVE is finer-grained
/// and left approximate here.
const RESX_PIPELINE_DELAY: u16 = 5;

/// Paddle capacitor base charge time at zero pot resistance, in nanoseconds
/// ×100. From the Atari7800 MiSTer paddle-LUT generator (`paddle_lut.c`).
const PADDLE_BASE_TIME_NS100: u64 = 6_034_284;
/// Extra paddle charge time per kΩ of pot resistance, in nanoseconds ×100.
const PADDLE_TIME_PER_KOHM_NS100: u64 = 4_532_214;
/// NTSC colour-clock frequency (Hz): one TIA `tick`.
const TIA_COLOUR_CLOCK_HZ: u64 = 3_579_545;

/// Colour clocks the paddle capacitor takes to charge past the INPT trigger at
/// position `pos`. The 8-bit position maps to the CX-30's ≈0..1 MΩ pot
/// (`pos × 4` kΩ); the charge time follows the MiSTer LUT model
/// (`BASE + resistance × T_PER_KO`), converted from nanoseconds to NTSC colour
/// clocks (`ns ×100 × freq / 1e11`).
fn paddle_threshold(pos: u8) -> u32 {
    let resistance_kohm = u64::from(pos) * 4;
    let time_ns100 = PADDLE_BASE_TIME_NS100 + resistance_kohm * PADDLE_TIME_PER_KOHM_NS100;
    let clocks = time_ns100 * TIA_COLOUR_CLOCK_HZ / 100_000_000_000;
    u32::try_from(clocks).unwrap_or(u32::MAX)
}

/// Video region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TiaRegion {
    /// NTSC: 262 lines, 3,579,545 Hz.
    Ntsc,
    /// PAL: 312 lines, 3,546,894 Hz.
    Pal,
}

impl TiaRegion {
    /// Lines per frame (approximate — software-controlled in reality).
    #[must_use]
    pub const fn lines_per_frame(self) -> u16 {
        match self {
            Self::Ntsc => 262,
            Self::Pal => 312,
        }
    }

    /// Crystal frequency in Hz.
    #[must_use]
    pub const fn crystal_hz(self) -> u32 {
        match self {
            Self::Ntsc => 3_579_545,
            Self::Pal => 3_546_894,
        }
    }
}

/// Atari TIA chip.
pub struct Tia {
    /// Video region.
    region: TiaRegion,

    /// Horizontal position counter (0-227).
    hpos: u16,
    /// Vertical line counter.
    vpos: u16,

    // --- Sync and blank ---
    /// VSYNC register (bit 1 active).
    vsync: bool,
    /// VBLANK register (bit 1 active).
    vblank: bool,
    /// WSYNC halt flag — CPU should stop until hpos wraps to 0.
    pub wsync_halt: bool,

    // --- Colour registers ---
    /// Player 0 colour (COLUP0).
    colup0: u8,
    /// Player 1 colour (COLUP1).
    colup1: u8,
    /// Playfield colour (COLUPF).
    colupf: u8,
    /// Background colour (COLUBK).
    colubk: u8,

    // --- Playfield ---
    /// PF0 register (only bits 4-7 used).
    pf0: u8,
    /// PF1 register.
    pf1: u8,
    /// PF2 register.
    pf2: u8,
    /// CTRLPF register.
    /// Bit 0: reflect playfield (vs copy).
    /// Bit 1: score mode (PF uses player colours).
    /// Bit 2: playfield priority over players.
    /// Bits 4-5: ball size (1/2/4/8 clocks).
    ctrlpf: u8,

    // --- Players ---
    /// Player 0 graphics register (GRP0).
    grp0: u8,
    /// Player 1 graphics register (GRP1).
    grp1: u8,
    /// Old GRP0 (for VDELP0).
    grp0_old: u8,
    /// Old GRP1 (for VDELP1).
    grp1_old: u8,
    /// Player 0 reflect (REFP0 bit 3).
    refp0: bool,
    /// Player 1 reflect (REFP1 bit 3).
    refp1: bool,
    /// Player 0 position counter.
    pos_p0: u16,
    /// Player 1 position counter.
    pos_p1: u16,
    /// NUSIZ0 register.
    nusiz0: u8,
    /// NUSIZ1 register.
    nusiz1: u8,
    /// Vertical delay player 0 (VDELP0).
    vdelp0: bool,
    /// Vertical delay player 1 (VDELP1).
    vdelp1: bool,

    // --- Missiles ---
    /// Missile 0 enable (ENAM0 bit 1).
    enam0: bool,
    /// Missile 1 enable (ENAM1 bit 1).
    enam1: bool,
    /// Missile 0 position counter.
    pos_m0: u16,
    /// Missile 1 position counter.
    pos_m1: u16,
    /// Lock missile 0 to player 0 (RESMP0 bit 1).
    resmp0: bool,
    /// Lock missile 1 to player 1 (RESMP1 bit 1).
    resmp1: bool,

    // --- Ball ---
    /// Ball enable (ENABL bit 1).
    enabl: bool,
    /// Old ball enable (for VDELBL).
    enabl_old: bool,
    /// Ball position counter.
    pos_bl: u16,
    /// Vertical delay ball (VDELBL).
    vdelbl: bool,

    // --- Ball per-clock render pipeline (Stella counter model, #406 phase 1b) ---
    /// Ball free-running render counter (0-159). Advances once per *visible*
    /// colour clock (Stella clocks objects only in the frame region, which is
    /// what keeps the 160-wide counter phase-stable against the 228-clock line).
    /// Phase 1b keeps `pos_bl` canonical and reseeds this from it at the start of
    /// each visible line; Phase 2 makes it free-run across lines so the HMOVE
    /// movement engine can inject extra ticks during the extended HBLANK.
    counter_bl: u16,
    /// Ball render pipeline active — latched when the counter hits the decode
    /// value (156) or by the line-start seed for a render carried over the line
    /// boundary. Cleared once the render counter passes the ball width.
    ball_is_rendering: bool,
    /// Ball render counter (Stella `myRenderCounter`, offset −4 at decode). The
    /// display signal is active while this is in `[0, width)`.
    ball_render_counter: i8,
    /// Ball display signal (geometry only) for the current colour clock. Latched
    /// once per visible tick from the render pipeline and shared by both
    /// `compose_pixel` and `update_collisions`; the enable gate (ENABL/VDELBL)
    /// is applied separately in `ball_pixel`.
    ball_signal: bool,

    // --- Horizontal motion ---
    hmp0: i8,
    hmp1: i8,
    hmm0: i8,
    hmm1: i8,
    hmbl: i8,
    /// HMOVE was triggered this line — blanks first 8 visible pixels.
    hmove_pending: bool,

    // --- Collision latches ---
    /// 15 collision flags, packed into CXM0P..CXPPMM registers.
    /// Bit layout matches hardware read registers.
    cxm0p: u8, // CXM0P:  M0-P1 (bit 7), M0-P0 (bit 6)
    cxm1p: u8,  // CXM1P:  M1-P0 (bit 7), M1-P1 (bit 6)
    cxp0fb: u8, // CXP0FB: P0-PF (bit 7), P0-BL (bit 6)
    cxp1fb: u8, // CXP1FB: P1-PF (bit 7), P1-BL (bit 6)
    cxm0fb: u8, // CXM0FB: M0-PF (bit 7), M0-BL (bit 6)
    cxm1fb: u8, // CXM1FB: M1-PF (bit 7), M1-BL (bit 6)
    cxblpf: u8, // CXBLPF: BL-PF (bit 7)
    cxppmm: u8, // CXPPMM: P0-P1 (bit 7), M0-M1 (bit 6)

    // --- Input latches ---
    /// INPT4: Player 0 fire button read value (bit 7, active low).
    inpt4: u8,
    /// INPT5: Player 1 fire button read value (bit 7, active low).
    inpt5: u8,
    /// Raw INPT4/INPT5 pin levels (`true` = button pressed = line low), kept
    /// separate from the read value so latch mode can hold a release.
    inpt4_pin: bool,
    inpt5_pin: bool,
    /// VBLANK bit 6: when set, INPT4/INPT5 latch a press low until the bit is
    /// cleared (which re-opens the latch). When clear, the reads follow the pin.
    inpt_latch: bool,

    // --- Paddle pots (INPT0-3) ---
    /// Paddle positions, one per INPT0-3 line (`0..=255`). Higher = more
    /// resistance = slower capacitor charge. Set by the host; INPT0/1 are the
    /// two paddles on the left jack, INPT2/3 the right.
    paddle_pos: [u8; 4],
    /// Colour clocks each paddle capacitor has charged since the dump (VBLANK
    /// bit 7) was last released. Saturating; reset to 0 while the dump is held.
    paddle_charge: [u32; 4],
    /// VBLANK bit 7 — when set, the INPT0-3 capacitors are grounded (dumped),
    /// so they read 0 and stop charging.
    paddle_dump: bool,
    /// Digital override for INPT0-3, one per line. `Some(high)` forces the read
    /// (bit 7 = `high`), bypassing the paddle capacitor model; `None` leaves the
    /// analog pot path in charge. Driven by digital controllers — the keypad
    /// ties INPT0/1 to ground or Vcc per its scanned matrix column.
    inpt_digital: [Option<bool>; 4],

    // --- Framebuffer ---
    /// ARGB32 framebuffer.
    framebuffer: Vec<u32>,
    /// Maximum lines for this region (used for framebuffer sizing).
    max_lines: u16,

    /// Frame complete flag — set when VSYNC is detected.
    frame_complete: bool,
    /// Whether we're in a VSYNC period.
    in_vsync: bool,

    /// Two-channel audio (AUDC/AUDF/AUDV), clocked once per colour clock.
    audio: TiaAudio,
}

impl Tia {
    /// Create a new TIA for the given video region.
    #[must_use]
    pub fn new(region: TiaRegion) -> Self {
        let max_lines = region.lines_per_frame();
        let fb_size = FB_WIDTH as usize * max_lines as usize;
        Self {
            region,
            hpos: 0,
            vpos: 0,
            vsync: false,
            vblank: false,
            wsync_halt: false,
            colup0: 0,
            colup1: 0,
            colupf: 0,
            colubk: 0,
            pf0: 0,
            pf1: 0,
            pf2: 0,
            ctrlpf: 0,
            grp0: 0,
            grp1: 0,
            grp0_old: 0,
            grp1_old: 0,
            refp0: false,
            refp1: false,
            pos_p0: 0,
            pos_p1: 0,
            nusiz0: 0,
            nusiz1: 0,
            vdelp0: false,
            vdelp1: false,
            enam0: false,
            enam1: false,
            pos_m0: 0,
            pos_m1: 0,
            resmp0: false,
            resmp1: false,
            enabl: false,
            enabl_old: false,
            pos_bl: 0,
            vdelbl: false,
            counter_bl: 0,
            ball_is_rendering: false,
            ball_render_counter: 0,
            ball_signal: false,
            hmp0: 0,
            hmp1: 0,
            hmm0: 0,
            hmm1: 0,
            hmbl: 0,
            hmove_pending: false,
            cxm0p: 0,
            cxm1p: 0,
            cxp0fb: 0,
            cxp1fb: 0,
            cxm0fb: 0,
            cxm1fb: 0,
            cxblpf: 0,
            cxppmm: 0,
            inpt4: 0x80,
            inpt5: 0x80,
            inpt4_pin: false,
            inpt5_pin: false,
            inpt_latch: false,
            paddle_pos: [0x80; 4],
            paddle_charge: [0; 4],
            paddle_dump: false,
            inpt_digital: [None; 4],
            framebuffer: vec![0; fb_size],
            max_lines,
            frame_complete: false,
            in_vsync: false,
            audio: TiaAudio::default(),
        }
    }

    /// Advance the TIA by one colour clock.
    ///
    /// This is the master clock tick. The CPU ticks every 3rd colour clock.
    pub fn tick(&mut self) {
        // Audio advances every colour clock (phase clocks fire at fixed
        // positions within the scanline; see TiaAudio::tick).
        self.audio.tick();

        let palette = match self.region {
            TiaRegion::Ntsc => &NTSC_PALETTE,
            TiaRegion::Pal => &PAL_PALETTE,
        };

        if self.vpos < self.max_lines {
            let line_offset = self.vpos as usize * FB_WIDTH as usize;
            let fb_idx = line_offset + self.hpos as usize;
            if self.hpos >= HBLANK_CLOCKS {
                let pixel_x = self.hpos - HBLANK_CLOCKS;

                // Reseed the ball's free-running counter from its canonical
                // position at the start of the visible region (Phase 1b). Then
                // latch this clock's geometry signal before compose/collisions
                // read it, mirroring Stella's `Ball::tick` ordering (signal is
                // computed from the pipeline state *before* it advances).
                if pixel_x == 0 {
                    self.seed_ball(0);
                }
                self.ball_signal = self.ball_is_rendering && self.ball_render_counter >= 0;

                let colour = if self.vblank {
                    0 // Black during VBLANK
                } else {
                    self.compose_pixel(pixel_x)
                };

                let argb = palette[(colour >> 1) as usize];

                // Update collision latches for every visible pixel.
                if !self.vblank {
                    self.update_collisions(pixel_x);
                }

                if fb_idx < self.framebuffer.len() {
                    self.framebuffer[fb_idx] = argb;
                }

                // Advance the ball pipeline for the next colour clock.
                self.advance_ball();
            } else {
                // HBLANK region — black. During the 68-clock horizontal
                // retrace the TIA holds its output in blanking, so a real
                // TV shows black here, not COLUBK (which only appears in the
                // 160 visible clocks). The full 228-wide framebuffer keeps
                // the canonical line width while matching the VBLANK=black
                // treatment above. (Borders survey, 2026-06-01.)
                let argb = palette[0];
                if fb_idx < self.framebuffer.len() {
                    self.framebuffer[fb_idx] = argb;
                }
            }
        }

        // Advance the paddle capacitors. A held dump keeps them grounded
        // (charge 0); otherwise each charges one colour clock toward its
        // position-dependent threshold.
        for charge in &mut self.paddle_charge {
            *charge = if self.paddle_dump {
                0
            } else {
                charge.saturating_add(1)
            };
        }

        // Advance horizontal counter.
        self.hpos += 1;
        if self.hpos >= CLOCKS_PER_LINE {
            self.hpos = 0;
            self.wsync_halt = false;
            self.hmove_pending = false;
            self.vpos += 1;

            // Start a new frame on VSYNC deassert (the normal path) or as a
            // safety roll if a game never asserts VSYNC at all. A real display
            // loses vertical sync and rolls rather than scanning forever; the
            // cap sits well above any legitimate frame (2× the region's line
            // count) so correctly-synced games reset via VSYNC long before it
            // bites. Without the roll, `vpos` (a `u16` line counter) would
            // climb unbounded and overflow on such a ROM.
            let vsync_reset = self.in_vsync && !self.vsync;
            if vsync_reset || self.vpos >= self.max_lines.saturating_mul(2) {
                self.vpos = 0;
                self.frame_complete = true;
            }
            self.in_vsync = self.vsync;
        }
    }

    /// Compose the output colour for pixel position `x` (0-159).
    ///
    /// Evaluates playfield, players, missiles, ball, and applies priority.
    fn compose_pixel(&self, x: u16) -> u8 {
        let pf = self.playfield_bit(x);
        let p0 = self.player_pixel(
            x,
            self.pos_p0,
            self.effective_grp0(),
            self.refp0,
            self.nusiz0,
        );
        let p1 = self.player_pixel(
            x,
            self.pos_p1,
            self.effective_grp1(),
            self.refp1,
            self.nusiz1,
        );
        let m0 = self.missile_pixel(
            x,
            self.pos_m0,
            self.enam0,
            self.nusiz0,
            self.resmp0,
            self.pos_p0,
        );
        let m1 = self.missile_pixel(
            x,
            self.pos_m1,
            self.enam1,
            self.nusiz1,
            self.resmp1,
            self.pos_p1,
        );
        let bl = self.ball_pixel(x);

        // HMOVE blanking: first 8 pixels are black when HMOVE was triggered.
        if self.hmove_pending && x < 8 {
            return 0;
        }

        // Update collision latches (conceptually — we do it in compose for simplicity).
        // In a real implementation these would be accumulated; since we're called
        // per pixel, the caller's mutable self handles this via tick().
        // For now we just use the bits for rendering priority.

        let pf_priority = self.ctrlpf & 0x04 != 0;
        let score_mode = self.ctrlpf & 0x02 != 0;

        if pf_priority {
            // Playfield/ball have priority over players/missiles.
            if pf || bl {
                if score_mode && x < 80 {
                    return self.colup0;
                } else if score_mode {
                    return self.colup1;
                }
                return self.colupf;
            }
            if p0 || m0 {
                return self.colup0;
            }
            if p1 || m1 {
                return self.colup1;
            }
        } else {
            // Players/missiles have priority over playfield/ball.
            if p0 || m0 {
                return self.colup0;
            }
            if p1 || m1 {
                return self.colup1;
            }
            if pf || bl {
                if score_mode && x < 80 {
                    return self.colup0;
                } else if score_mode {
                    return self.colup1;
                }
                return self.colupf;
            }
        }

        self.colubk
    }

    /// Evaluate playfield bit for pixel position x (0-159).
    fn playfield_bit(&self, x: u16) -> bool {
        // Playfield is 20 bits wide, each bit = 4 colour clocks.
        // Left half (x 0-79): PF0(4-7), PF1(7-0), PF2(0-7)
        // Right half (x 80-159): copy or mirror depending on CTRLPF bit 0.
        let pf_clock = x / 4;

        if pf_clock < 20 {
            // Left half
            self.pf_bit_left(pf_clock)
        } else if self.ctrlpf & 0x01 != 0 {
            // Reflected: mirror the left half
            self.pf_bit_left(39 - pf_clock)
        } else {
            // Copy: repeat the left half
            self.pf_bit_left(pf_clock - 20)
        }
    }

    /// Get a playfield bit from the left-half 20-bit pattern.
    fn pf_bit_left(&self, index: u16) -> bool {
        match index {
            // PF0 bits 4-7 (displayed left to right as bit4, bit5, bit6, bit7)
            0..=3 => self.pf0 & (0x10 << index) != 0,
            // PF1 bits 7-0 (displayed left to right as bit7, bit6, ..., bit0)
            4..=11 => self.pf1 & (0x80 >> (index - 4)) != 0,
            // PF2 bits 0-7 (displayed left to right as bit0, bit1, ..., bit7)
            12..=19 => self.pf2 & (1 << (index - 12)) != 0,
            _ => false,
        }
    }

    /// Effective GRP0 value (accounts for VDELP0).
    fn effective_grp0(&self) -> u8 {
        if self.vdelp0 {
            self.grp0_old
        } else {
            self.grp0
        }
    }

    /// Effective GRP1 value (accounts for VDELP1).
    fn effective_grp1(&self) -> u8 {
        if self.vdelp1 {
            self.grp1_old
        } else {
            self.grp1
        }
    }

    /// Check if a player sprite is active at pixel position x.
    #[allow(clippy::unused_self)]
    fn player_pixel(&self, x: u16, pos: u16, grp: u8, reflect: bool, nusiz: u8) -> bool {
        if grp == 0 {
            return false;
        }

        // The player's copy layout AND its 2×/4× stretch are both selected by
        // NUSIZ bits 2:0. Bits 5:4 are the MISSILE size (see `missile_pixel`)
        // and have no bearing on the player — reading them here was dead,
        // misleading code (#408).
        let size = nusiz & 0x07;

        // Check each copy position.
        let copies: &[(u16, bool)] = match size {
            0x00 => &[(0, true)],                         // One copy
            0x01 => &[(0, true), (16, true)],             // Two copies close
            0x02 => &[(0, true), (32, true)],             // Two copies medium
            0x03 => &[(0, true), (16, true), (32, true)], // Three copies close
            0x04 => &[(0, true), (64, true)],             // Two copies wide
            0x05 => &[(0, true)],                         // Double-size player
            0x06 => &[(0, true), (32, true), (64, true)], // Three copies medium
            0x07 => &[(0, true)],                         // Quad-size player
            _ => &[(0, true)],
        };

        // Double-size (0x05) draws each GRP bit 2 clocks wide, quad-size (0x07)
        // 4 clocks; every other mode is 1×.
        let effective_width = match size {
            0x05 => 2,
            0x07 => 4,
            _ => 1,
        };

        for &(offset, _) in copies {
            let start = (pos + offset) % 160;
            let pixel_width = 8 * effective_width;
            let rel = (x + 160 - start) % 160;
            if rel < pixel_width {
                let bit_index = rel / effective_width;
                let bit = if reflect {
                    grp & (1 << bit_index) != 0
                } else {
                    grp & (0x80 >> bit_index) != 0
                };
                if bit {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a missile is active at pixel position x.
    #[allow(clippy::unused_self)]
    fn missile_pixel(
        &self,
        x: u16,
        pos: u16,
        enabled: bool,
        nusiz: u8,
        locked: bool,
        _player_pos: u16,
    ) -> bool {
        if !enabled || locked {
            return false;
        }

        let width: u16 = match (nusiz >> 4) & 0x03 {
            0 => 1,
            1 => 2,
            2 => 4,
            3 => 8,
            _ => 1,
        };

        let rel = (x + 160 - pos) % 160;
        rel < width
    }

    /// Check if the ball is active at pixel position x.
    ///
    /// The geometry comes from the per-clock render pipeline (`ball_signal`,
    /// latched in `tick`); this only applies the ENABL/VDELBL enable gate. `x`
    /// is unused now that rendering is counter-driven — it is kept so the
    /// per-pixel call sites (compose + collisions) read identically.
    fn ball_pixel(&self, _x: u16) -> bool {
        let enabled = if self.vdelbl {
            self.enabl_old
        } else {
            self.enabl
        };
        enabled && self.ball_signal
    }

    /// Ball width in colour clocks, from CTRLPF bits 5:4 (1/2/4/8).
    fn ball_width(&self) -> u16 {
        match (self.ctrlpf >> 4) & 0x03 {
            0 => 1,
            1 => 2,
            2 => 4,
            3 => 8,
            _ => 1,
        }
    }

    /// Seed the ball's free-running counter and render pipeline so that, run
    /// forward from visible column `x`, it reproduces the position-formula
    /// output: the leftmost pixel at `pos_bl`, `ball_width` clocks wide.
    ///
    /// Stella's ball decodes at counter 156 and offsets the render counter by
    /// −4, so the display signal (render counter in `[0, width)`) lands at
    /// columns `[pos_bl, pos_bl + width)` mod 160. Inverting that gives the
    /// counter at column `x` as `(x − pos_bl + 1) mod 160`, and the render
    /// pipeline state from the distance `d = (x − (pos_bl − 4)) mod 160` into
    /// the `4 + width`-clock render window. A render that began on the previous
    /// line (a sprite near the left edge, or one wrapping past column 159) is
    /// recovered here as an already-active pipeline. See #406 phase 1b.
    fn seed_ball(&mut self, x: u16) {
        let pos = self.pos_bl % 160;
        let w = self.ball_width();
        self.counter_bl = (x + 160 - pos + 1) % 160;
        let d = (x + 160 - pos + 4) % 160;
        if d < 4 + w {
            self.ball_is_rendering = true;
            self.ball_render_counter = i8::try_from(i32::from(d) - 4).unwrap_or(0);
        } else {
            self.ball_is_rendering = false;
            self.ball_render_counter = 0;
        }
    }

    /// Re-seed the ball pipeline from `pos_bl` when a RESBL/HMOVE strobe lands
    /// in the visible region, so the new column takes effect on the next clock
    /// (the line-start seed handles strobes during HBLANK).
    fn reseed_ball_if_visible(&mut self) {
        if self.hpos >= HBLANK_CLOCKS {
            self.seed_ball(self.hpos - HBLANK_CLOCKS);
        }
    }

    /// Advance the ball's render pipeline and counter by one visible colour
    /// clock (Stella `Ball::tick`, without the movement/starfield paths — those
    /// arrive in later #406 phases). Call after the clock's signal is latched.
    fn advance_ball(&mut self) {
        let w = i8::try_from(self.ball_width()).unwrap_or(1);
        if self.counter_bl == 156 {
            self.ball_is_rendering = true;
            self.ball_render_counter = -4;
        } else if self.ball_is_rendering {
            self.ball_render_counter += 1;
            if self.ball_render_counter >= w {
                self.ball_is_rendering = false;
            }
        }
        self.counter_bl = (self.counter_bl + 1) % 160;
    }

    /// Update collision latches for the current pixel.
    fn update_collisions(&mut self, x: u16) {
        let pf = self.playfield_bit(x);
        let p0 = self.player_pixel(
            x,
            self.pos_p0,
            self.effective_grp0(),
            self.refp0,
            self.nusiz0,
        );
        let p1 = self.player_pixel(
            x,
            self.pos_p1,
            self.effective_grp1(),
            self.refp1,
            self.nusiz1,
        );
        let m0 = self.missile_pixel(
            x,
            self.pos_m0,
            self.enam0,
            self.nusiz0,
            self.resmp0,
            self.pos_p0,
        );
        let m1 = self.missile_pixel(
            x,
            self.pos_m1,
            self.enam1,
            self.nusiz1,
            self.resmp1,
            self.pos_p1,
        );
        let bl = self.ball_pixel(x);

        // M0-P1, M0-P0
        if m0 && p1 {
            self.cxm0p |= 0x80;
        }
        if m0 && p0 {
            self.cxm0p |= 0x40;
        }
        // M1-P0, M1-P1
        if m1 && p0 {
            self.cxm1p |= 0x80;
        }
        if m1 && p1 {
            self.cxm1p |= 0x40;
        }
        // P0-PF, P0-BL
        if p0 && pf {
            self.cxp0fb |= 0x80;
        }
        if p0 && bl {
            self.cxp0fb |= 0x40;
        }
        // P1-PF, P1-BL
        if p1 && pf {
            self.cxp1fb |= 0x80;
        }
        if p1 && bl {
            self.cxp1fb |= 0x40;
        }
        // M0-PF, M0-BL
        if m0 && pf {
            self.cxm0fb |= 0x80;
        }
        if m0 && bl {
            self.cxm0fb |= 0x40;
        }
        // M1-PF, M1-BL
        if m1 && pf {
            self.cxm1fb |= 0x80;
        }
        if m1 && bl {
            self.cxm1fb |= 0x40;
        }
        // BL-PF
        if bl && pf {
            self.cxblpf |= 0x80;
        }
        // P0-P1, M0-M1
        if p0 && p1 {
            self.cxppmm |= 0x80;
        }
        if m0 && m1 {
            self.cxppmm |= 0x40;
        }
    }

    /// Visible-pixel column at which a RESx strobe lands the object: the beam's
    /// current visible column plus the [`RESX_PIPELINE_DELAY`], wrapped to the
    /// 160-pixel width. A strobe still inside HBLANK saturates near the left
    /// edge rather than wrapping back onto the previous line.
    fn resx_reset_position(&self) -> u16 {
        (self.hpos + RESX_PIPELINE_DELAY).saturating_sub(HBLANK_CLOCKS) % 160
    }

    /// Write a TIA register.
    ///
    /// Address is masked to 6 bits ($00-$3F).
    pub fn write(&mut self, addr: u8, value: u8) {
        match addr & 0x3F {
            0x00 => {
                // VSYNC
                self.vsync = value & 0x02 != 0;
            }
            0x01 => {
                // VBLANK
                self.vblank = value & 0x02 != 0;
                // Bit 7 dumps the INPT0-3 paddle capacitors to ground; releasing
                // the dump starts the caps charging.
                self.paddle_dump = value & 0x80 != 0;
                // Bit 6 enables the INPT4/INPT5 fire-button latches. Enabling
                // captures a currently-held press; clearing re-opens the latch
                // so the reads follow the pin again.
                self.inpt_latch = value & 0x40 != 0;
                self.inpt4 = latched_inpt(self.inpt_latch, self.inpt4_pin, self.inpt4);
                self.inpt5 = latched_inpt(self.inpt_latch, self.inpt5_pin, self.inpt5);
            }
            0x02 => {
                // WSYNC halts the CPU until the beam reaches the start of the
                // next scanline (RDY released at the next SHB). This is a no-op
                // only at the exact line-start boundary; in particular it still
                // holds when strobed mid-HBLANK, which the common back-to-back
                // `STA WSYNC` idiom relies on (cf. Stella TIA::onHalt, which
                // advances `(H_CLOCKS - hctr) % H_CLOCKS`).
                self.wsync_halt = true;
            }
            0x03 => {
                // RSYNC
                self.hpos = 0;
            }
            0x04 => self.nusiz0 = value,                      // NUSIZ0
            0x05 => self.nusiz1 = value,                      // NUSIZ1
            0x06 => self.colup0 = value,                      // COLUP0
            0x07 => self.colup1 = value,                      // COLUP1
            0x08 => self.colupf = value,                      // COLUPF
            0x09 => self.colubk = value,                      // COLUBK
            0x0A => self.ctrlpf = value,                      // CTRLPF
            0x0B => self.refp0 = value & 0x08 != 0,           // REFP0
            0x0C => self.refp1 = value & 0x08 != 0,           // REFP1
            0x0D => self.pf0 = value,                         // PF0
            0x0E => self.pf1 = value,                         // PF1
            0x0F => self.pf2 = value,                         // PF2
            0x10 => self.pos_p0 = self.resx_reset_position(), // RESP0
            0x11 => self.pos_p1 = self.resx_reset_position(), // RESP1
            0x12 => self.pos_m0 = self.resx_reset_position(), // RESM0
            0x13 => self.pos_m1 = self.resx_reset_position(), // RESM1
            0x14 => {
                // RESBL: reset the ball position. A strobe inside the visible
                // region must re-seed the live pipeline so the new column takes
                // effect immediately, as the old per-pixel formula did; an
                // HBLANK strobe is picked up by the line-start seed.
                self.pos_bl = self.resx_reset_position();
                self.reseed_ball_if_visible();
            }
            // AUDC0/1, AUDF0/1, AUDV0/1 — both audio channels.
            0x15..=0x1A => self.audio.write(addr & 0x3F, value),
            0x1B => {
                // GRP0: store the new player-0 pattern, and latch player 1's
                // delayed (old) pattern from its current new value. A GRP write
                // shuffles the OTHER player's buffer, never its own — modelling
                // it as "delay by one write" drifts (TIA ref note 7; Stella
                // GRP0 → player0.grp + shuffleP1).
                self.grp0 = value;
                self.grp1_old = self.grp1;
            }
            0x1C => {
                // GRP1: store the new player-1 pattern, and latch player 0's
                // delayed pattern. The ball's delayed (VDELBL) enable also
                // latches here, not on the ENABL write (Stella GRP1 →
                // player1.grp + shuffleP0 + shuffleBL).
                self.grp1 = value;
                self.grp0_old = self.grp0;
                self.enabl_old = self.enabl;
            }
            0x1D => self.enam0 = value & 0x02 != 0, // ENAM0
            0x1E => self.enam1 = value & 0x02 != 0, // ENAM1
            0x1F => {
                // ENABL: store the new ball enable only. The delayed (old) copy
                // used by VDELBL latches on the GRP1 write (see $1C above).
                self.enabl = value & 0x02 != 0;
            }
            0x20 => self.hmp0 = decode_hmove(value), // HMP0
            0x21 => self.hmp1 = decode_hmove(value), // HMP1
            0x22 => self.hmm0 = decode_hmove(value), // HMM0
            0x23 => self.hmm1 = decode_hmove(value), // HMM1
            0x24 => self.hmbl = decode_hmove(value), // HMBL
            0x25 => self.vdelp0 = value & 0x01 != 0, // VDELP0
            0x26 => self.vdelp1 = value & 0x01 != 0, // VDELP1
            0x27 => self.vdelbl = value & 0x01 != 0, // VDELBL
            0x28 => self.resmp0 = value & 0x02 != 0, // RESMP0
            0x29 => self.resmp1 = value & 0x02 != 0, // RESMP1
            0x2A => {
                // HMOVE
                self.apply_hmove();
                self.reseed_ball_if_visible();
                self.hmove_pending = true;
            }
            0x2B => {
                // HMCLR
                self.hmp0 = 0;
                self.hmp1 = 0;
                self.hmm0 = 0;
                self.hmm1 = 0;
                self.hmbl = 0;
            }
            0x2C => {
                // CXCLR
                self.cxm0p = 0;
                self.cxm1p = 0;
                self.cxp0fb = 0;
                self.cxp1fb = 0;
                self.cxm0fb = 0;
                self.cxm1fb = 0;
                self.cxblpf = 0;
                self.cxppmm = 0;
            }
            _ => {} // Unmapped
        }
    }

    /// Read a TIA register.
    ///
    /// Address is masked to 4 bits for reads ($00-$0F).
    #[must_use]
    pub fn read(&self, addr: u8) -> u8 {
        match addr & 0x0F {
            0x00 => self.cxm0p,          // CXM0P
            0x01 => self.cxm1p,          // CXM1P
            0x02 => self.cxp0fb,         // CXP0FB
            0x03 => self.cxp1fb,         // CXP1FB
            0x04 => self.cxm0fb,         // CXM0FB
            0x05 => self.cxm1fb,         // CXM1FB
            0x06 => self.cxblpf,         // CXBLPF
            0x07 => self.cxppmm,         // CXPPMM
            0x08 => self.paddle_inpt(0), // INPT0 (left jack, paddle A)
            0x09 => self.paddle_inpt(1), // INPT1 (left jack, paddle B)
            0x0A => self.paddle_inpt(2), // INPT2 (right jack, paddle A)
            0x0B => self.paddle_inpt(3), // INPT3 (right jack, paddle B)
            0x0C => self.inpt4,          // INPT4 (P0 fire)
            0x0D => self.inpt5,          // INPT5 (P1 fire)
            _ => 0,
        }
    }

    /// The INPT0-3 read value for paddle `index`: bit 7 is set once the
    /// capacitor has charged past its position-dependent threshold, and clear
    /// while it is still charging or the dump is held.
    fn paddle_inpt(&self, index: usize) -> u8 {
        if let Some(high) = self.inpt_digital[index] {
            // A digital controller (keypad) drives the line directly; the pot
            // capacitor path is bypassed.
            return if high { 0x80 } else { 0x00 };
        }
        if self.paddle_charge[index] >= paddle_threshold(self.paddle_pos[index]) {
            0x80
        } else {
            0x00
        }
    }

    /// Set a paddle position for INPT line `index` (0-3). `value` is the 8-bit
    /// host position: 0 charges fastest (minimum resistance), 255 slowest.
    /// Out-of-range indices are ignored.
    pub fn set_paddle(&mut self, index: u8, value: u8) {
        if let Some(slot) = self.paddle_pos.get_mut(index as usize) {
            *slot = value;
        }
    }

    /// Force INPT0-3 line `index` to a digital level (`Some(high)`), or release
    /// it back to the paddle pot path (`None`). Used by digital controllers
    /// (the keypad ties its column lines to ground/Vcc). Out-of-range ignored.
    pub fn set_inpt_digital(&mut self, index: u8, level: Option<bool>) {
        if let Some(slot) = self.inpt_digital.get_mut(index as usize) {
            *slot = level;
        }
    }

    /// Apply HMOVE offsets to all object positions.
    fn apply_hmove(&mut self) {
        self.pos_p0 = apply_motion(self.pos_p0, self.hmp0);
        self.pos_p1 = apply_motion(self.pos_p1, self.hmp1);
        self.pos_m0 = apply_motion(self.pos_m0, self.hmm0);
        self.pos_m1 = apply_motion(self.pos_m1, self.hmm1);
        self.pos_bl = apply_motion(self.pos_bl, self.hmbl);
    }

    /// Reference to the framebuffer (ARGB32, 160 × `max_lines`).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    /// Framebuffer width.
    #[must_use]
    pub const fn framebuffer_width(&self) -> u32 {
        FB_WIDTH
    }

    /// Framebuffer height (total lines for this region).
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        u32::from(self.max_lines)
    }

    /// Whether a frame has completed (VSYNC deasserted).
    ///
    /// Calling this clears the flag.
    pub fn take_frame_complete(&mut self) -> bool {
        let complete = self.frame_complete;
        self.frame_complete = false;
        complete
    }

    /// Drain the mono audio samples produced since the last call. Two samples
    /// are emitted per scanline (≈31.4 kHz NTSC / ≈31.2 kHz PAL); the runtime
    /// pushes them each frame and the host resamples to the output rate.
    pub fn take_audio_samples(&mut self) -> Vec<f32> {
        self.audio.take_samples()
    }

    /// Current horizontal position (0-227).
    #[must_use]
    pub fn hpos(&self) -> u16 {
        self.hpos
    }

    /// Current vertical position (line number).
    #[must_use]
    pub fn vpos(&self) -> u16 {
        self.vpos
    }

    /// Set INPT4 (player 0 fire button). Active low: bit 7 = 0 when pressed.
    /// In latch mode (VBLANK bit 6) a press is held until the latch re-opens.
    pub fn set_inpt4(&mut self, pressed: bool) {
        self.inpt4_pin = pressed;
        self.inpt4 = latched_inpt(self.inpt_latch, pressed, self.inpt4);
    }

    /// Set INPT5 (player 1 fire button). Active low: bit 7 = 0 when pressed.
    /// In latch mode (VBLANK bit 6) a press is held until the latch re-opens.
    pub fn set_inpt5(&mut self, pressed: bool) {
        self.inpt5_pin = pressed;
        self.inpt5 = latched_inpt(self.inpt_latch, pressed, self.inpt5);
    }

    /// Serialize TIA register state for save states.
    ///
    /// Captures all write registers, object positions, collision latches,
    /// and beam position. Does not include the framebuffer (derived state).
    #[must_use]
    pub fn save_state(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(64);
        // Beam position
        data.extend_from_slice(&self.hpos.to_le_bytes());
        data.extend_from_slice(&self.vpos.to_le_bytes());
        // Sync/blank
        data.push(u8::from(self.vsync));
        data.push(u8::from(self.vblank));
        data.push(u8::from(self.wsync_halt));
        // Colour registers
        data.push(self.colup0);
        data.push(self.colup1);
        data.push(self.colupf);
        data.push(self.colubk);
        // Playfield
        data.push(self.pf0);
        data.push(self.pf1);
        data.push(self.pf2);
        data.push(self.ctrlpf);
        // Players
        data.push(self.grp0);
        data.push(self.grp1);
        data.push(self.grp0_old);
        data.push(self.grp1_old);
        data.push(u8::from(self.refp0));
        data.push(u8::from(self.refp1));
        data.extend_from_slice(&self.pos_p0.to_le_bytes());
        data.extend_from_slice(&self.pos_p1.to_le_bytes());
        data.push(self.nusiz0);
        data.push(self.nusiz1);
        data.push(u8::from(self.vdelp0));
        data.push(u8::from(self.vdelp1));
        // Missiles
        data.push(u8::from(self.enam0));
        data.push(u8::from(self.enam1));
        data.extend_from_slice(&self.pos_m0.to_le_bytes());
        data.extend_from_slice(&self.pos_m1.to_le_bytes());
        data.push(u8::from(self.resmp0));
        data.push(u8::from(self.resmp1));
        // Ball
        data.push(u8::from(self.enabl));
        data.push(u8::from(self.enabl_old));
        data.extend_from_slice(&self.pos_bl.to_le_bytes());
        data.push(u8::from(self.vdelbl));
        // Horizontal motion
        data.push(self.hmp0 as u8);
        data.push(self.hmp1 as u8);
        data.push(self.hmm0 as u8);
        data.push(self.hmm1 as u8);
        data.push(self.hmbl as u8);
        data.push(u8::from(self.hmove_pending));
        // Collision latches
        data.push(self.cxm0p);
        data.push(self.cxm1p);
        data.push(self.cxp0fb);
        data.push(self.cxp1fb);
        data.push(self.cxm0fb);
        data.push(self.cxm1fb);
        data.push(self.cxblpf);
        data.push(self.cxppmm);
        // Input latches
        data.push(self.inpt4);
        data.push(self.inpt5);
        data.push(u8::from(self.inpt4_pin));
        data.push(u8::from(self.inpt5_pin));
        data.push(u8::from(self.inpt_latch));
        // Frame state
        data.push(u8::from(self.frame_complete));
        data.push(u8::from(self.in_vsync));
        data
    }

    /// Restore TIA register state from a save state.
    ///
    /// # Errors
    ///
    /// Returns an error if the data is too short.
    pub fn load_state(&mut self, data: &[u8]) -> Result<usize, String> {
        let mut p = 0usize;
        let need = |p: usize, n: usize, d: &[u8]| -> Result<(), String> {
            if p + n > d.len() {
                Err("TIA state truncated".into())
            } else {
                Ok(())
            }
        };
        macro_rules! r8 {
            () => {{
                need(p, 1, data)?;
                let v = data[p];
                p += 1;
                v
            }};
        }
        macro_rules! r16 {
            () => {{
                need(p, 2, data)?;
                let v = u16::from_le_bytes([data[p], data[p + 1]]);
                p += 2;
                v
            }};
        }

        self.hpos = r16!();
        self.vpos = r16!();
        self.vsync = r8!() != 0;
        self.vblank = r8!() != 0;
        self.wsync_halt = r8!() != 0;
        self.colup0 = r8!();
        self.colup1 = r8!();
        self.colupf = r8!();
        self.colubk = r8!();
        self.pf0 = r8!();
        self.pf1 = r8!();
        self.pf2 = r8!();
        self.ctrlpf = r8!();
        self.grp0 = r8!();
        self.grp1 = r8!();
        self.grp0_old = r8!();
        self.grp1_old = r8!();
        self.refp0 = r8!() != 0;
        self.refp1 = r8!() != 0;
        self.pos_p0 = r16!();
        self.pos_p1 = r16!();
        self.nusiz0 = r8!();
        self.nusiz1 = r8!();
        self.vdelp0 = r8!() != 0;
        self.vdelp1 = r8!() != 0;
        self.enam0 = r8!() != 0;
        self.enam1 = r8!() != 0;
        self.pos_m0 = r16!();
        self.pos_m1 = r16!();
        self.resmp0 = r8!() != 0;
        self.resmp1 = r8!() != 0;
        self.enabl = r8!() != 0;
        self.enabl_old = r8!() != 0;
        self.pos_bl = r16!();
        self.vdelbl = r8!() != 0;
        self.hmp0 = r8!() as i8;
        self.hmp1 = r8!() as i8;
        self.hmm0 = r8!() as i8;
        self.hmm1 = r8!() as i8;
        self.hmbl = r8!() as i8;
        self.hmove_pending = r8!() != 0;
        self.cxm0p = r8!();
        self.cxm1p = r8!();
        self.cxp0fb = r8!();
        self.cxp1fb = r8!();
        self.cxm0fb = r8!();
        self.cxm1fb = r8!();
        self.cxblpf = r8!();
        self.cxppmm = r8!();
        self.inpt4 = r8!();
        self.inpt5 = r8!();
        self.inpt4_pin = r8!() != 0;
        self.inpt5_pin = r8!() != 0;
        self.inpt_latch = r8!() != 0;
        self.frame_complete = r8!() != 0;
        self.in_vsync = r8!() != 0;
        Ok(p)
    }
}

/// The new INPT4/INPT5 read value (bit 7, active low) for a fire button.
///
/// Unlatched (`enabled == false`): the read follows the pin — `0x00` pressed,
/// `0x80` released. Latched (`enabled == true`): a press pulls the value low
/// and it holds there even after release; releasing the pin keeps the previous
/// value. The latch re-opens (and so starts following the pin again) when
/// VBLANK bit 6 is cleared. Per `reference/by-topic/tia/tia-reference.md` § Input.
#[must_use]
fn latched_inpt(enabled: bool, pressed: bool, prev: u8) -> u8 {
    if pressed {
        0x00
    } else if enabled {
        prev
    } else {
        0x80
    }
}

/// Decode an HMxx register write into a position delta for [`apply_motion`].
///
/// Bits 7:4 are a signed 4-bit value (−8..+7). On real hardware a **positive**
/// nybble moves the object **left**, a **negative** nybble moves it **right**
/// (Stella sets `myHmmClocks = nybble ^ 8` and ticks the object that many extra
/// times during HMOVE — more ticks advance its counter further, i.e. left).
/// `apply_motion` treats a smaller position as further left, so we return the
/// negated nybble:
///
/// - `$70` (+7) → `-7` → left 7
/// - `$10` (+1) → `-1` → left 1
/// - `$80` (−8) → `+8` → right 8
/// - `$F0` (−1) → `+1` → right 1
///
/// NB: the prose `tia-reference.md` table has this direction inverted; the
/// convention here matches Stella and real hardware.
fn decode_hmove(value: u8) -> i8 {
    // Sign-extend the high nybble from 4 bits, then negate so a positive
    // (leftward) nybble subtracts from the position.
    let nibble = (value >> 4) & 0x0F;
    let signed = if nibble & 0x08 != 0 {
        nibble as i8 | -16 // 0x08..0x0F → -8..-1
    } else {
        nibble as i8
    };
    -signed
}

/// Apply a motion offset to a position, wrapping within 0-159.
fn apply_motion(pos: u16, motion: i8) -> u16 {
    let new_pos = i32::from(pos) + i32::from(motion);
    ((new_pos % 160 + 160) % 160) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horizontal_counter_wraps_at_228() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        for _ in 0..228 {
            tia.tick();
        }
        assert_eq!(tia.hpos(), 0);
    }

    #[test]
    fn vpos_is_bounded_when_a_game_never_asserts_vsync() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        // Run several frames' worth of lines without ever writing VSYNC. The
        // safety roll must keep vpos bounded (a real display rolls), so the
        // u16 line counter never overflows.
        for _ in 0..(262 * 5 * CLOCKS_PER_LINE as usize) {
            tia.tick();
        }
        assert!(
            tia.vpos() < 262 * 2,
            "vpos rolls within 2× the frame height ({})",
            tia.vpos()
        );
    }

    #[test]
    fn vsync_resets_vpos_before_the_safety_roll() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        // Advance a few lines, then a normal VSYNC pulse (assert, hold, clear)
        // resets the frame well before the safety cap is reached.
        for _ in 0..(20 * CLOCKS_PER_LINE as usize) {
            tia.tick();
        }
        tia.write(0x00, 0x02); // VSYNC on
        for _ in 0..(3 * CLOCKS_PER_LINE as usize) {
            tia.tick();
        }
        tia.write(0x00, 0x00); // VSYNC off
        // The deassert is detected at the next line boundary.
        for _ in 0..CLOCKS_PER_LINE as usize {
            tia.tick();
        }
        assert!(
            tia.vpos() <= 1,
            "VSYNC restarts the frame (vpos={})",
            tia.vpos()
        );
    }

    #[test]
    fn respx_strobe_applies_the_pipeline_delay() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        // Advance the beam into the visible region (hpos = 108 → column 40).
        for _ in 0..(HBLANK_CLOCKS as usize + 40) {
            tia.tick();
        }
        let column = tia.hpos() - HBLANK_CLOCKS;

        // Each RESx strobe lands the object 5 colour clocks right of the beam,
        // not exactly at it.
        tia.write(0x10, 0); // RESP0
        tia.write(0x14, 0); // RESBL
        assert_eq!(
            tia.pos_p0,
            column + RESX_PIPELINE_DELAY,
            "RESP0 delayed by 5"
        );
        assert_eq!(
            tia.pos_bl,
            column + RESX_PIPELINE_DELAY,
            "RESBL delayed by 5"
        );
    }

    #[test]
    fn player_width_comes_from_size_bits_and_ignores_missile_bits() {
        let tia = Tia::new(TiaRegion::Ntsc);
        let grp = 0x80; // only the leftmost GRP bit set
        let pos = 0;

        // Size 0 (one copy, 1×): the leftmost bit covers exactly one pixel.
        assert!(tia.player_pixel(0, pos, grp, false, 0x00));
        assert!(!tia.player_pixel(1, pos, grp, false, 0x00));

        // Setting the MISSILE-size bits (5:4) must not widen the player.
        assert!(tia.player_pixel(0, pos, grp, false, 0x30));
        assert!(
            !tia.player_pixel(1, pos, grp, false, 0x30),
            "missile-size bits do not stretch the player"
        );

        // Double-size player (size 0x05): the leftmost bit is 2 clocks wide.
        assert!(tia.player_pixel(1, pos, grp, false, 0x05));
        assert!(!tia.player_pixel(2, pos, grp, false, 0x05));

        // Quad-size player (size 0x07): 4 clocks wide.
        assert!(tia.player_pixel(3, pos, grp, false, 0x07));
        assert!(!tia.player_pixel(4, pos, grp, false, 0x07));
    }

    #[test]
    fn vdelp_latches_only_the_other_player_on_a_grp_write() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        tia.write(0x25, 0x01); // VDELP0 on — displayed pattern is the delayed one.

        // A GRP1 write latches player 0's delayed buffer to the current new P0.
        tia.write(0x1B, 0x11); // GRP0 new = 0x11
        tia.write(0x1C, 0x00); // GRP1 write → grp0_old = 0x11
        assert_eq!(
            tia.effective_grp0(),
            0x11,
            "GRP1 write latched player-0's delay buffer"
        );

        // Consecutive GRP0 writes must NOT advance player 0's own delay buffer
        // (the "delay by one write" model the reference warns against). Under
        // that bug, effective_grp0 would track the previous GRP0 (0xFF).
        tia.write(0x1B, 0xFF);
        tia.write(0x1B, 0xAA);
        assert_eq!(
            tia.effective_grp0(),
            0x11,
            "GRP0 writes leave player-0's own delay buffer untouched"
        );

        // Only the next GRP1 write latches it — to the current new P0 (0xAA).
        tia.write(0x1C, 0x00);
        assert_eq!(
            tia.effective_grp0(),
            0xAA,
            "GRP1 write latches the delayed player-0 pattern"
        );
    }

    #[test]
    fn vdelbl_ball_enable_latches_on_a_grp1_write_not_on_enabl() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        tia.write(0x27, 0x01); // VDELBL on.

        // Enabling the ball sets the new value; the delayed copy must NOT move.
        tia.write(0x1F, 0x02); // ENABL on
        assert!(
            !tia.enabl_old,
            "ENABL write does not latch the delayed ball enable"
        );

        // The GRP1 write strobe is what latches the ball's delayed enable.
        tia.write(0x1C, 0x00);
        assert!(tia.enabl_old, "GRP1 write latches the delayed ball enable");
    }

    #[test]
    fn wsync_flag_cleared_at_line_end() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        tia.write(0x02, 0); // WSYNC
        assert!(tia.wsync_halt);
        // Tick to end of line
        for _ in 0..228 {
            tia.tick();
        }
        assert!(!tia.wsync_halt);
    }

    #[test]
    fn colubk_fills_visible_region() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        // Set background to colour index $1A (NTSC blue-ish)
        tia.write(0x09, 0x9A);
        // Ensure not in VBLANK
        tia.write(0x01, 0x00);

        // Tick one full line
        for _ in 0..228 {
            tia.tick();
        }

        // Visible pixels (indices 68..=227) get COLUBK; the HBLANK
        // region (0..68) is black, matching real TIA horizontal blanking.
        let expected = NTSC_PALETTE[0x9A >> 1];
        assert_eq!(tia.framebuffer()[HBLANK_CLOCKS as usize], expected);
        assert_eq!(tia.framebuffer()[(FB_WIDTH - 1) as usize], expected);
        assert_eq!(tia.framebuffer()[0], NTSC_PALETTE[0]);
        assert_eq!(
            tia.framebuffer()[(HBLANK_CLOCKS - 1) as usize],
            NTSC_PALETTE[0]
        );
    }

    #[test]
    fn vblank_produces_black_in_active_region() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        tia.write(0x09, 0x9A); // Set background
        tia.write(0x01, 0x02); // VBLANK on

        for _ in 0..228 {
            tia.tick();
        }

        // First visible pixel (after the 68-clock HBLANK) should be
        // black during VBLANK. The HBLANK region carries COLUBK.
        assert_eq!(tia.framebuffer()[HBLANK_CLOCKS as usize], NTSC_PALETTE[0]);
    }

    #[test]
    fn framebuffer_size_ntsc() {
        let tia = Tia::new(TiaRegion::Ntsc);
        assert_eq!(tia.framebuffer_width(), 228);
        assert_eq!(tia.framebuffer_height(), 262);
        assert_eq!(tia.framebuffer().len(), 228 * 262);
    }

    #[test]
    fn framebuffer_size_pal() {
        let tia = Tia::new(TiaRegion::Pal);
        assert_eq!(tia.framebuffer_width(), 228);
        assert_eq!(tia.framebuffer_height(), 312);
        assert_eq!(tia.framebuffer().len(), 228 * 312);
    }

    #[test]
    fn playfield_reflect_vs_copy() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        tia.write(0x0D, 0x10); // PF0: bit 4 set → leftmost column
        tia.write(0x0E, 0x00); // PF1: empty
        tia.write(0x0F, 0x00); // PF2: empty

        // Copy mode (default)
        tia.write(0x0A, 0x00);
        assert!(tia.playfield_bit(0)); // Left half, bit 0
        assert!(tia.playfield_bit(80)); // Right half, copy
        assert!(!tia.playfield_bit(4)); // Left half, bit 1

        // Reflect mode
        tia.write(0x0A, 0x01);
        assert!(tia.playfield_bit(0)); // Left half, bit 0
        // In reflect mode, right half is mirrored: rightmost column maps to PF0 bit 4
        assert!(tia.playfield_bit(156)); // Reflected position
    }

    #[test]
    fn hmove_decode() {
        // A positive HM nybble moves the object LEFT, a negative nybble RIGHT
        // (matches Stella / real hardware; the prose reference table is
        // inverted). decode_hmove returns the delta for apply_motion, where a
        // smaller position is further left.
        assert_eq!(decode_hmove(0x00), 0); //  0  → no motion
        assert_eq!(decode_hmove(0x10), -1); // +1 → left 1
        assert_eq!(decode_hmove(0x70), -7); // +7 → left 7 (max left)
        assert_eq!(decode_hmove(0x80), 8); // −8 → right 8 (max right)
        assert_eq!(decode_hmove(0xF0), 1); // −1 → right 1
    }

    #[test]
    fn hmove_direction_through_apply_motion() {
        // The end-to-end direction: $70 shifts a sprite left, $80 right.
        let start = 80;
        assert_eq!(apply_motion(start, decode_hmove(0x70)), 73, "$70 → 7 left");
        assert_eq!(apply_motion(start, decode_hmove(0x10)), 79, "$10 → 1 left");
        assert_eq!(apply_motion(start, decode_hmove(0x80)), 88, "$80 → 8 right");
        assert_eq!(apply_motion(start, decode_hmove(0xF0)), 81, "$F0 → 1 right");
        assert_eq!(apply_motion(start, decode_hmove(0x00)), 80, "$00 → no move");
    }

    #[test]
    fn motion_wraps_positions() {
        assert_eq!(apply_motion(0, -1), 159);
        assert_eq!(apply_motion(159, 1), 0);
        assert_eq!(apply_motion(80, 0), 80);
    }

    #[test]
    fn collision_clear() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        tia.cxm0p = 0xFF;
        tia.cxppmm = 0xFF;
        tia.write(0x2C, 0); // CXCLR
        assert_eq!(tia.cxm0p, 0);
        assert_eq!(tia.cxppmm, 0);
    }

    #[test]
    fn inpt4_fire_button() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        // Default: not pressed (bit 7 set)
        assert_eq!(tia.read(0x0C), 0x80);
        // Press fire
        tia.set_inpt4(true);
        assert_eq!(tia.read(0x0C), 0x00);
        // Release fire
        tia.set_inpt4(false);
        assert_eq!(tia.read(0x0C), 0x80);
    }

    #[test]
    fn inpt4_latch_mode_holds_a_press_until_the_latch_reopens() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        // Enable latch mode (VBLANK bit 6).
        tia.write(0x01, 0x40);

        // A momentary press latches low and holds after release.
        tia.set_inpt4(true);
        assert_eq!(tia.read(0x0C), 0x00, "press latches low");
        tia.set_inpt4(false);
        assert_eq!(tia.read(0x0C), 0x00, "release stays low while latched");

        // Clearing VBLANK bit 6 re-opens the latch — the read follows the pin
        // again (currently released → high).
        tia.write(0x01, 0x00);
        assert_eq!(tia.read(0x0C), 0x80, "clearing bit 6 re-opens the latch");
    }

    #[test]
    fn inpt5_unlatched_follows_the_pin() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        // No latch (bit 6 clear): INPT5 tracks the pin both ways.
        tia.set_inpt5(true);
        assert_eq!(tia.read(0x0D), 0x00, "pressed → low");
        tia.set_inpt5(false);
        assert_eq!(tia.read(0x0D), 0x80, "released → high (no hold)");
    }

    #[test]
    fn enabling_the_latch_captures_a_held_button() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        // Button held before the latch is armed.
        tia.set_inpt4(true);
        // Arming the latch captures the held press, so a later release holds.
        tia.write(0x01, 0x40);
        tia.set_inpt4(false);
        assert_eq!(tia.read(0x0C), 0x00, "held press captured at arm time");
    }

    #[test]
    fn paddle_capacitor_charges_after_the_dump_is_released() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        tia.set_paddle(0, 0x00); // fastest charge (minimum resistance)
        let threshold = super::paddle_threshold(0x00);

        // Hold the dump (VBLANK bit 7): INPT0 reads 0 and never charges.
        tia.write(0x01, 0x80);
        for _ in 0..(threshold + 50) {
            tia.tick();
        }
        assert_eq!(tia.read(0x08), 0x00, "dumped paddle stays discharged");

        // Release the dump: INPT0 reads 0 until the cap charges past threshold.
        tia.write(0x01, 0x00);
        for _ in 0..(threshold - 1) {
            tia.tick();
        }
        assert_eq!(tia.read(0x08), 0x00, "still charging before threshold");
        tia.tick();
        tia.tick();
        assert_eq!(tia.read(0x08), 0x80, "charged past threshold → bit 7 set");
    }

    #[test]
    fn higher_paddle_position_takes_longer_to_charge() {
        // More resistance = later trigger. INPT1 (far) should still be low
        // when INPT0 (near) has already fired.
        let mut tia = Tia::new(TiaRegion::Ntsc);
        tia.set_paddle(0, 0x10);
        tia.set_paddle(1, 0xF0);
        assert!(super::paddle_threshold(0x10) < super::paddle_threshold(0xF0));

        tia.write(0x01, 0x00); // release dump
        let near = super::paddle_threshold(0x10);
        for _ in 0..(near + 5) {
            tia.tick();
        }
        assert_eq!(tia.read(0x08), 0x80, "near paddle charged");
        assert_eq!(tia.read(0x09), 0x00, "far paddle still charging");
    }

    #[test]
    fn ntsc_palette_has_128_entries() {
        assert_eq!(NTSC_PALETTE.len(), 128);
    }

    #[test]
    fn pal_palette_has_128_entries() {
        assert_eq!(PAL_PALETTE.len(), 128);
    }

    // --- Object-rendering regression lock-in (HMOVE per-clock port, #406) ---
    //
    // These capture the *current* position-formula rendering so the phased
    // per-clock counter rewrite stays output-equivalent (phase 1) — they must
    // remain green through it. Object state is set directly (same-module test)
    // and one scanline is rendered; the 160 visible pixels are asserted.

    /// Tick one full scanline from a line boundary and return its 160 visible
    /// ARGB pixels. Call with a freshly-constructed (or line-aligned) TIA.
    fn render_visible_line(tia: &mut Tia) -> Vec<u32> {
        assert_eq!(tia.hpos(), 0, "must start at a line boundary");
        let line = tia.vpos() as usize;
        for _ in 0..CLOCKS_PER_LINE {
            tia.tick();
        }
        let base = line * FB_WIDTH as usize + HBLANK_CLOCKS as usize;
        tia.framebuffer()[base..base + 160].to_vec()
    }

    /// ARGB for a colour-register value (the palette folds bit 0).
    fn colour(value: u8) -> u32 {
        NTSC_PALETTE[(value >> 1) as usize]
    }

    #[test]
    fn player_renders_eight_pixel_block_at_its_position() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        tia.colubk = 0x00;
        tia.colup0 = 0x1E;
        tia.pos_p0 = 40;
        tia.grp0 = 0xFF;
        tia.nusiz0 = 0x00;

        let line = render_visible_line(&mut tia);
        let (bg, fg) = (colour(0x00), colour(0x1E));
        for (x, &px) in line.iter().enumerate() {
            let want = if (40..48).contains(&x) { fg } else { bg };
            assert_eq!(px, want, "pixel {x}");
        }
    }

    #[test]
    fn player_nusiz_double_size_doubles_pixel_width() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        tia.colubk = 0x00;
        tia.colup0 = 0x1E;
        tia.pos_p0 = 40;
        tia.grp0 = 0xFF;
        tia.nusiz0 = 0x05; // double width → 16px

        let line = render_visible_line(&mut tia);
        let (bg, fg) = (colour(0x00), colour(0x1E));
        assert_eq!(line[39], bg);
        for (x, px) in line.iter().enumerate().take(56).skip(40) {
            assert_eq!(*px, fg, "double-width pixel {x}");
        }
        assert_eq!(line[56], bg);
    }

    #[test]
    fn player_nusiz_two_close_copies() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        tia.colubk = 0x00;
        tia.colup0 = 0x1E;
        tia.pos_p0 = 40;
        tia.grp0 = 0xFF;
        tia.nusiz0 = 0x01; // two copies, 16px apart (8px gap)

        let line = render_visible_line(&mut tia);
        let fg = colour(0x1E);
        assert_eq!(line[40], fg, "first copy");
        assert_eq!(line[47], fg);
        assert_eq!(line[48], colour(0x00), "gap between copies");
        assert_eq!(line[56], fg, "second copy at +16");
        assert_eq!(line[63], fg);
    }

    #[test]
    fn missile_width_from_nusiz_high_bits() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        tia.colubk = 0x00;
        tia.colup0 = 0x1E;
        tia.pos_m0 = 50;
        tia.enam0 = true;
        tia.nusiz0 = 0x20; // missile width 4

        let line = render_visible_line(&mut tia);
        let (bg, fg) = (colour(0x00), colour(0x1E));
        assert_eq!(line[49], bg);
        for (x, px) in line.iter().enumerate().take(54).skip(50) {
            assert_eq!(*px, fg, "missile pixel {x}");
        }
        assert_eq!(line[54], bg);
    }

    #[test]
    fn ball_width_from_ctrlpf() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        tia.colubk = 0x00;
        tia.colupf = 0x1E; // ball uses the playfield colour
        tia.pos_bl = 60;
        tia.enabl = true;
        tia.ctrlpf = 0x20; // ball width 4

        let line = render_visible_line(&mut tia);
        let (bg, fg) = (colour(0x00), colour(0x1E));
        assert_eq!(line[59], bg);
        for (x, px) in line.iter().enumerate().take(64).skip(60) {
            assert_eq!(*px, fg, "ball pixel {x}");
        }
        assert_eq!(line[64], bg);
    }

    #[test]
    fn ball_counter_model_matches_the_position_formula_everywhere() {
        // Exhaustive output-equivalence guard for the #406 phase-1b ball
        // rewrite: for every position and width the counter-driven renderer
        // must paint exactly the columns the old `(x - pos) mod 160 < width`
        // formula did — including the left-edge straddle (pos < 4) and the
        // right-edge wrap back onto the same line (pos + width > 160).
        for &(ctrlpf, width) in &[(0x00u8, 1u16), (0x10, 2), (0x20, 4), (0x30, 8)] {
            for pos in 0u16..160 {
                let mut tia = Tia::new(TiaRegion::Ntsc);
                tia.colubk = 0x00;
                tia.colupf = 0x1E; // ball uses the playfield colour
                tia.ctrlpf = ctrlpf;
                tia.enabl = true;
                tia.pos_bl = pos;

                let prev_line = tia.vpos() as usize;
                for _ in 0..CLOCKS_PER_LINE {
                    tia.tick();
                }
                let base = prev_line * FB_WIDTH as usize + HBLANK_CLOCKS as usize;
                let line = &tia.framebuffer()[base..base + 160];

                let (bg, fg) = (colour(0x00), colour(0x1E));
                for (x, &px) in line.iter().enumerate() {
                    let on = (x as u16 + 160 - pos) % 160 < width;
                    let want = if on { fg } else { bg };
                    assert_eq!(px, want, "ctrlpf={ctrlpf:#04x} pos={pos} x={x}");
                }
            }
        }
    }

    #[test]
    fn hmove_blanks_the_first_eight_pixels_and_applies_net_motion() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        tia.colubk = 0x00;
        tia.colup0 = 0x1E;
        tia.pos_p0 = 12;
        tia.grp0 = 0xFF;
        tia.nusiz0 = 0x00;
        tia.write(0x20, 0x40); // HMP0 = move left 4
        tia.write(0x2A, 0x00); // HMOVE → pos 12 → 8, comb blanks x<8

        let line = render_visible_line(&mut tia);
        let (bg, fg) = (colour(0x00), colour(0x1E));
        for (x, px) in line.iter().enumerate().take(8) {
            assert_eq!(*px, bg, "HMOVE comb blanks pixel {x}");
        }
        for (x, px) in line.iter().enumerate().take(16).skip(8) {
            assert_eq!(*px, fg, "player moved to 8..16, pixel {x}");
        }
        assert_eq!(line[16], bg);
    }
}
