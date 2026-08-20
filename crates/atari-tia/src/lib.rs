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
use serde::{Deserialize, Serialize};

/// Framebuffer width: 160 visible colour clocks per line.
/// Width of the visible playfield region (TIA renders `tile` + sprite +
/// playfield + ball pixels into here).
pub const ACTIVE_WIDTH: u32 = 160;

/// Full colour-clocks per line including HBLANK. The framebuffer keeps
/// the canonical 228-clock line width; the 68-clock HBLANK region is
/// rendered black, because the TIA holds its output in blanking during
/// horizontal retrace (COLUBK only appears in the 160 visible clocks).
/// Colour clock of the NTSC TIA. The framebuffer is one pixel per colour
/// clock, so this is its pixel clock too — and it is slow enough that a 2600
/// pixel is 12:7, nearly twice as wide as it is tall.
pub const NTSC_COLOUR_CLOCK_HZ: f64 = 3_579_545.0;

/// Colour clock of the PAL TIA, four times the PAL subcarrier over five.
/// Gives 25:12.
pub const PAL_COLOUR_CLOCK_HZ: f64 = 3_546_894.0;

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

/// Colour clocks the HMOVE strobe is delayed before the movement engine starts.
/// The delay is why a strobe issued at the very start of a line's HBLANK lands
/// after that line's hctr-0 reset, so the extended HBLANK applies to the correct
/// line. It also sets the end-of-line boundary: a strobe whose fire lands before
/// the HSync wrap has its comb-latch cleared by the wrap (comb suppressed — the
/// "CPU cycle 74" trick); one clock later is an ordinary end-of-line HMOVE.
/// Calibrated to 7 so that boundary matches hardware — an hpos-222 strobe stays
/// normal — validated against Gopher2600 on the Pole Position HUD (#581). At 6
/// it suppressed one clock too early and flung the HUD's missiles/ball to the
/// left edge (the sliver).
const DELAY_HMOVE: u8 = 7;

/// Extra colour clocks the HMOVE comb holds the beam in HBLANK (the visible
/// region starts 8 clocks later, at hctr 76 instead of 68).
const HMOVE_COMB_CLOCKS: u16 = 8;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Serialize, Deserialize)]
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

    // --- Player per-clock render pipeline (Stella counter model, #406 phase 1b) ---
    // Indexed [0] = player 0, [1] = player 1. Like the ball, the counter advances
    // once per *visible* colour clock and the pipeline is seeded from the
    // canonical position (`pos_pN`) at the start of each visible line — and on a
    // visible-region RESPx/HMOVE/NUSIZ strobe. Decode counter and render offset
    // (−5) are picked so the first pixel of each copy lands at `pos + offset` for
    // every NUSIZ size, reproducing the position-formula `player_pixel` output.
    /// Player free-running render counters (0-159).
    counter_p: [u16; 2],
    /// Player render pipeline active — latched at a copy's decode counter.
    p_is_rendering: [bool; 2],
    /// Player render counters (Stella `myRenderCounter`, −5 at decode). The
    /// pattern is sampled while this is at or past the divider trip point.
    p_render_counter: [i8; 2],
    /// Player graphics sample index (0-7) — which GRP bit the render is on.
    p_sample_counter: [u8; 2],
    /// Player display signal for the current colour clock (already includes the
    /// GRP pattern bit), latched once per visible tick and shared by compose +
    /// collisions.
    p_signal: [bool; 2],

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

    // --- Missile per-clock render pipeline (Stella counter model, #406 phase 1b) ---
    // Indexed [0] = missile 0, [1] = missile 1. Structurally the ball: a single
    // copy, decode at counter 156, render offset −4, width from NUSIZ bits 5:4.
    // Seeded from `pos_mN` at the start of each visible line and on a
    // visible-region RESMx/HMOVE strobe. Enable (`enamN && !resmpN`) is applied
    // at signal time, matching the position-formula `missile_pixel`.
    /// Missile free-running render counters (0-159).
    counter_m: [u16; 2],
    /// Missile render pipelines active — latched at the decode counter (156).
    m_is_rendering: [bool; 2],
    /// Missile render counters (Stella `myRenderCounter`, −4 at decode); the
    /// signal is active while this is in `[0, width)`.
    m_render_counter: [i8; 2],
    /// Missile display signal (geometry only) for the current colour clock,
    /// latched once per visible tick and shared by compose + collisions.
    m_signal: [bool; 2],
    /// Missile width used while moving — modulated by the starfield effect
    /// (Stella `myEffectiveWidth`); equals the configured width otherwise.
    m_effective_width: [i8; 2],

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
    /// Counter value at the ball's last movement tick — the phase reference for
    /// the starfield width modulation (Stella `myLastMovementTick`).
    ball_last_movement_tick: u16,
    /// Ball width used while moving — modulated by the starfield effect (Stella
    /// `myEffectiveWidth`); equals the configured width otherwise.
    ball_effective_width: i8,

    // --- Horizontal motion (HMOVE movement engine, #406 phase 2) ---
    // Each object's HM register decoded as Stella's `hmmClocks = (value>>4) ^ 8`
    // (0..15): the number of extra counter ticks injected during the extended
    // HBLANK. Net leftward motion is `hmmClocks − 8` (8 = no motion). Indexed
    // [0],[1] for the two players/missiles.
    hmm_p: [u8; 2],
    hmm_m: [u8; 2],
    hmm_bl: u8,
    /// Per-object "still moving" latch — true from an HMOVE strobe until the
    /// object's movement clock reaches its `hmmClocks`.
    moving_p: [bool; 2],
    moving_m: [bool; 2],
    moving_bl: bool,
    /// Movement-engine clock (0..15+), incremented every 4th colour clock while
    /// movement is in progress.
    movement_clock: u8,
    /// Any object still moving — gates the movement engine.
    movement_active: bool,
    /// Extended HBLANK: set by HMOVE, holds the beam in blanking for 8 extra
    /// clocks (to hctr 75) so the first visible pixel is column 8 — the 8-pixel
    /// HMOVE comb. Cleared at the start of each line.
    extended_hblank: bool,
    /// Colour clocks until a strobed HMOVE takes effect (Stella `Delay::hmove`
    /// = 6). `None` when no HMOVE is pending.
    hmove_delay: Option<u8>,

    // --- HMOVE TIA-revision quirks (#406 phase 3, default off = baseline) ---
    // These emulate idiosyncrasies of particular TIA revisions; with all three
    // off the behaviour is the canonical one. See `set_hmove_quirks`.
    /// Inverted movement clock phase: a movement tick outside HBLANK suppresses
    /// the following ordinary tick (the phase shift behind e.g. the Cool Aid Man
    /// bug on some Jr. models).
    quirk_inverted_phase_clock: bool,
    /// Short late HMOVE: a movement tick at hctr 0 is skipped.
    quirk_short_late_hmove: bool,
    /// Late RESPx: a RESPx strobed in HBLANK just as movement starts lands one
    /// clock later.
    quirk_late_respx: bool,
    /// Per-object "movement tick outside HBLANK pending" latch for the inverted
    /// phase clock quirk (Stella `myInvertedPhaseClock`). Set in the movement
    /// engine, consumed (suppressing one tick) by the next `advance_*`.
    inverted_phase_p: [bool; 2],
    inverted_phase_m: [bool; 2],
    inverted_phase_bl: bool,

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
        let mut tia = Self {
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
            counter_p: [0; 2],
            p_is_rendering: [false; 2],
            p_render_counter: [0; 2],
            p_sample_counter: [0; 2],
            p_signal: [false; 2],
            enam0: false,
            enam1: false,
            pos_m0: 0,
            pos_m1: 0,
            resmp0: false,
            resmp1: false,
            counter_m: [0; 2],
            m_is_rendering: [false; 2],
            m_render_counter: [0; 2],
            m_signal: [false; 2],
            m_effective_width: [1; 2],
            enabl: false,
            enabl_old: false,
            pos_bl: 0,
            vdelbl: false,
            counter_bl: 0,
            ball_is_rendering: false,
            ball_render_counter: 0,
            ball_signal: false,
            ball_last_movement_tick: 0,
            ball_effective_width: 1,
            // HM registers default to 0 → hmmClocks = (0>>4)^8 = 8 = no motion.
            hmm_p: [8; 2],
            hmm_m: [8; 2],
            hmm_bl: 8,
            moving_p: [false; 2],
            moving_m: [false; 2],
            moving_bl: false,
            movement_clock: 0,
            movement_active: false,
            extended_hblank: false,
            hmove_delay: None,
            quirk_inverted_phase_clock: false,
            quirk_short_late_hmove: false,
            quirk_late_respx: false,
            inverted_phase_p: [false; 2],
            inverted_phase_m: [false; 2],
            inverted_phase_bl: false,
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
        };
        // Seed every free-running counter to position 0 so a never-positioned
        // object behaves like the old `pos_*=0` default rather than drifting.
        tia.set_ball_position(0);
        tia.set_player_position(0, 0);
        tia.set_player_position(1, 0);
        tia.set_missile_position(0, 0);
        tia.set_missile_position(1, 0);
        tia
    }

    /// Advance the TIA by one colour clock.
    ///
    /// This is the master clock tick. The CPU ticks every 3rd colour clock.
    pub fn tick(&mut self) {
        // Audio advances every colour clock (phase clocks fire at fixed
        // positions within the scanline; see TiaAudio::tick).
        self.audio.tick();

        // A strobed HMOVE takes effect after a fixed delay; fire it before the
        // movement engine runs this clock.
        if let Some(d) = self.hmove_delay {
            let next = d - 1;
            if next == 0 {
                self.fire_hmove();
                self.hmove_delay = None;
            } else {
                self.hmove_delay = Some(next);
            }
        }

        // The movement engine injects extra counter ticks (every 4th clock,
        // during HBLANK) — this is how HMOVE moves objects now.
        self.tick_movement();

        let palette = match self.region {
            TiaRegion::Ntsc => &NTSC_PALETTE,
            TiaRegion::Pal => &PAL_PALETTE,
        };

        if self.vpos < self.max_lines {
            let line_offset = self.vpos as usize * FB_WIDTH as usize;
            let fb_idx = line_offset + self.hpos as usize;
            // Under an extended HBLANK the visible region starts 8 clocks late;
            // those 8 columns render as HBLANK black — the HMOVE comb.
            if self.hpos >= self.first_visible_hctr() {
                let pixel_x = self.hpos - HBLANK_CLOCKS;

                // Latch this clock's geometry signal before compose/collisions
                // read it, mirroring Stella's `Ball::tick` ordering (signal is
                // computed from the pipeline state *before* it advances). The
                // counters free-run across lines now (#406 phase 2) — they are
                // seeded only by RESPx/HMOVE/position writes, not per line.
                self.ball_signal = self.ball_is_rendering && self.ball_render_counter >= 0;
                self.p_signal[0] = self.player_signal(0);
                self.p_signal[1] = self.player_signal(1);
                self.m_signal[0] = self.missile_visible(0, self.hpos, true);
                self.m_signal[1] = self.missile_visible(1, self.hpos, true);

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

                // Advance the per-clock pipelines for the next colour clock (a
                // regular visible tick — `is_regular = true` — which is what
                // drives the starfield modulation while an object is moving).
                self.advance_ball(true);
                self.advance_player(0);
                self.advance_player(1);
                self.advance_missile(0, self.hpos, true);
                self.advance_missile(1, self.hpos, true);
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
            // A new line starts un-extended; re-sync each shadow position from
            // its (possibly HMOVE-moved) free-running counter.
            self.extended_hblank = false;
            self.sync_position_shadows();
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
        let p0 = self.p_signal[0];
        let p1 = self.p_signal[1];
        let m0 = self.missile_signal(0);
        let m1 = self.missile_signal(1);
        let bl = self.ball_pixel(x);

        // (The HMOVE comb is now the extended HBLANK in `tick`, not a per-pixel
        // blank here — see #406 phase 2.)

        // Update collision latches (conceptually — we do it in compose for simplicity).
        // In a real implementation these would be accumulated; since we're called
        // per pixel, the caller's mutable self handles this via tick().
        // For now we just use the bits for rendering priority.

        let pf_priority = self.ctrlpf & 0x04 != 0;
        // Score mode (recolour the playfield from COLUP0/COLUP1) applies only
        // when SCORE is set *and* PFP is clear: Stella gates it on
        // `(CTRLPF & 0x06) == 0x02`. With playfield priority on, the playfield
        // keeps COLUPF. Pole Position sets both bits (CTRLPF $3f) so its red
        // speed bar is a COLUPF playfield, not a white score-mode one.
        let score_mode = self.ctrlpf & 0x06 == 0x02;

        // Score mode (CTRLPF bit 1) recolours only the *playfield* — the left
        // half takes COLUP0, the right half COLUP1. The ball is never affected:
        // it always uses COLUPF. Pole Position's HUD relies on this — its red
        // speed bar is the ball (COLUPF) drawn over the white score-mode
        // playfield. Resolve the ball before the playfield so it shows through.
        let ball_colour = self.colupf;
        let pf_colour = if score_mode && x < 80 {
            self.colup0
        } else if score_mode {
            self.colup1
        } else {
            self.colupf
        };

        if pf_priority {
            // Playfield/ball have priority over players/missiles.
            if bl {
                return ball_colour;
            }
            if pf {
                return pf_colour;
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
            if bl {
                return ball_colour;
            }
            if pf {
                return pf_colour;
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

    /// Reference (position-formula) player renderer: is a player sprite active
    /// at pixel `x`? Since the #406 phase-1b rewrite, production rendering runs
    /// through the per-clock pipeline (`player_signal`); this stays as the
    /// executable spec that the pipeline is validated against
    /// (`player_counter_model_matches_the_position_formula_everywhere`) and that
    /// the NUSIZ-width test pins. Test-only.
    #[cfg(test)]
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

    /// Player `idx`'s canonical position, effective GRP (VDELP), reflect, and
    /// NUSIZ — the register inputs the per-clock pipeline reads live each clock.
    fn player_inputs(&self, idx: usize) -> (u16, u8, bool, u8) {
        if idx == 0 {
            (self.pos_p0, self.effective_grp0(), self.refp0, self.nusiz0)
        } else {
            (self.pos_p1, self.effective_grp1(), self.refp1, self.nusiz1)
        }
    }

    /// Copy offsets and the size divider for a NUSIZ value (player bits 2:0).
    /// Multi-copy modes are always 1× (divider 1); the 2×/4× stretch modes are
    /// always a single copy.
    fn player_copies(nusiz: u8) -> (&'static [u16], u16) {
        match nusiz & 0x07 {
            0x00 => (&[0], 1),
            0x01 => (&[0, 16], 1),
            0x02 => (&[0, 32], 1),
            0x03 => (&[0, 16, 32], 1),
            0x04 => (&[0, 64], 1),
            0x05 => (&[0], 2),
            0x06 => (&[0, 32, 64], 1),
            0x07 => (&[0], 4),
            _ => (&[0], 1),
        }
    }

    /// The player's display signal for the current colour clock: rendering, at
    /// or past the divider trip point, with the sampled GRP bit set. Reflection
    /// chooses bit order (MSB-first normally, LSB-first when reflected), matching
    /// `player_pixel`.
    fn player_signal(&self, idx: usize) -> bool {
        let (_, grp, reflect, nusiz) = self.player_inputs(idx);
        let (_, divider) = Self::player_copies(nusiz);
        let trip = if divider == 1 { 0 } else { 1 };
        let rc = self.p_render_counter[idx];
        let sc = self.p_sample_counter[idx];
        if !self.p_is_rendering[idx] || rc < trip || sc > 7 {
            return false;
        }
        let bit = if reflect {
            grp & (1 << sc)
        } else {
            grp & (0x80 >> sc)
        };
        bit != 0
    }

    /// Seed player `idx`'s pipeline so that, run forward from visible column `x`,
    /// it reproduces `player_pixel`'s output. The counter is phase-locked to the
    /// position; a copy whose decode happened before `x` (carried from the
    /// previous line, or wrapped past column 159) is recovered as an in-progress
    /// render. See the field block and #406 phase 1b.
    fn seed_player(&mut self, idx: usize, pos: u16, x: u16) {
        let (_, _, _, nusiz) = self.player_inputs(idx);
        let pos = pos % 160;
        let (copies, divider) = Self::player_copies(nusiz);
        let trip = i32::from(if divider == 1 { 0u8 } else { 1 });
        self.counter_p[idx] = (x + 160 - pos + 2) % 160;
        self.p_is_rendering[idx] = false;
        self.p_render_counter[idx] = 0;
        self.p_sample_counter[idx] = 0;
        // Render-window length in clocks since decode: the (6 + trip) ramp plus
        // 8 samples × divider.
        let last_dd = (6 + trip as u16) + 8 * divider - 1;
        for &off in copies {
            let s = (pos + off) % 160;
            let dd = (x + 160 - s + 6 + trip as u16) % 160;
            if dd >= 1 && dd <= last_dd {
                let rc = i32::from(dd) - 6;
                self.p_is_rendering[idx] = true;
                self.p_render_counter[idx] = i8::try_from(rc).unwrap_or(0);
                self.p_sample_counter[idx] = if rc < trip {
                    0
                } else {
                    u8::try_from((rc - trip) / i32::from(divider)).unwrap_or(0)
                };
                break;
            }
        }
    }

    /// The visible column the beam is about to render — where a position-setting
    /// strobe takes effect. In HBLANK there is no current column, so the seed
    /// targets column 0 (the start of the upcoming visible region).
    fn seed_x(&self) -> u16 {
        self.hpos.saturating_sub(HBLANK_CLOCKS)
    }

    /// Re-seed player `idx`'s pipeline from its shadow position (`pos_pN`) at the
    /// current beam — used when a register change (RESPx/HMOVE/NUSIZ) must take
    /// effect without moving the object. With the counters free-running (#406
    /// phase 2) this is the only re-seed path; there is no per-line re-seed.
    fn reseed_player(&mut self, idx: usize) {
        let pos = if idx == 0 { self.pos_p0 } else { self.pos_p1 };
        self.seed_player(idx, pos, self.seed_x());
    }

    /// Set player `idx`'s position: update the shadow and seed the pipeline at
    /// the current beam. The shadow (`pos_pN`) stays the canonical position for
    /// reads and save-state; the free-running counter is the rendering truth.
    pub fn set_player_position(&mut self, idx: usize, pos: u16) {
        let pos = pos % 160;
        if idx == 0 {
            self.pos_p0 = pos;
        } else {
            self.pos_p1 = pos;
        }
        self.seed_player(idx, pos, self.seed_x());
    }

    /// Advance player `idx`'s pipeline by one visible colour clock (Stella
    /// `Player::tick`, without the movement/divider-change/quirk paths). A copy's
    /// decode counter is `(offset − 4 − trip) mod 160`, chosen so the first pixel
    /// lands at `pos + offset` for every size. Call after the clock's signal is
    /// latched.
    fn advance_player(&mut self, idx: usize) {
        if self.quirk_inverted_phase_clock && self.inverted_phase_p[idx] {
            self.inverted_phase_p[idx] = false;
            return;
        }
        let (_, _, _, nusiz) = self.player_inputs(idx);
        let (copies, divider) = Self::player_copies(nusiz);
        let trip = if divider == 1 { 0i8 } else { 1 };
        let c = self.counter_p[idx];
        let decoded = copies
            .iter()
            .any(|&off| c == (off + 160 - 4 - trip as u16) % 160);
        if decoded {
            self.p_is_rendering[idx] = true;
            self.p_sample_counter[idx] = 0;
            self.p_render_counter[idx] = -5;
        } else if self.p_is_rendering[idx] {
            self.p_render_counter[idx] += 1;
            let rc = self.p_render_counter[idx];
            let advance_sample = if divider == 1 {
                rc > 0
            } else {
                rc > 1 && (((rc - 1) as u16) & (divider - 1)) == 0
            };
            if advance_sample {
                self.p_sample_counter[idx] += 1;
            }
            if self.p_sample_counter[idx] > 7 {
                self.p_is_rendering[idx] = false;
            }
        }
        self.counter_p[idx] = (c + 1) % 160;
    }

    /// Reference (position-formula) missile renderer. Since the #406 phase-1b
    /// rewrite, production rendering runs through the per-clock pipeline
    /// (`missile_signal`); this stays as the executable spec the pipeline is
    /// validated against. A locked (RESMP) missile reads off here — matching the
    /// pre-rewrite behaviour, which Phase 1b must preserve. Test-only.
    #[cfg(test)]
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

        let rel = (x + 160 - pos) % 160;
        rel < missile_width(nusiz)
    }

    /// Missile `idx`'s position, enable (`enamN && !resmpN`), and width — the
    /// register inputs the per-clock pipeline reads live each clock.
    fn missile_inputs(&self, idx: usize) -> (u16, bool, u16) {
        if idx == 0 {
            (
                self.pos_m0,
                self.enam0 && !self.resmp0,
                missile_width(self.nusiz0),
            )
        } else {
            (
                self.pos_m1,
                self.enam1 && !self.resmp1,
                missile_width(self.nusiz1),
            )
        }
    }

    /// Missile `idx`'s display signal for the current colour clock: the latched
    /// geometry visibility (`m_signal`, includes the starfield case) gated by
    /// enable.
    fn missile_signal(&self, idx: usize) -> bool {
        let (_, enabled, _) = self.missile_inputs(idx);
        enabled && self.m_signal[idx]
    }

    /// Seed missile `idx`'s pipeline from its position so that, run forward from
    /// visible column `x`, it reproduces `missile_pixel`'s geometry — identical
    /// in structure to [`Self::seed_ball`] (decode 156, render offset −4).
    fn seed_missile(&mut self, idx: usize, pos: u16, x: u16) {
        let (_, _, w) = self.missile_inputs(idx);
        let pos = pos % 160;
        self.counter_m[idx] = (x + 160 - pos + 1) % 160;
        let d = (x + 160 - pos + 4) % 160;
        if d < 4 + w {
            self.m_is_rendering[idx] = true;
            self.m_render_counter[idx] = i8::try_from(i32::from(d) - 4).unwrap_or(0);
        } else {
            self.m_is_rendering[idx] = false;
            self.m_render_counter[idx] = 0;
        }
    }

    /// Re-seed missile `idx`'s pipeline from its shadow position at the current
    /// beam (free-running counters; no per-line re-seed — see [`Self::seed_x`]).
    fn reseed_missile(&mut self, idx: usize) {
        let pos = if idx == 0 { self.pos_m0 } else { self.pos_m1 };
        self.seed_missile(idx, pos, self.seed_x());
    }

    /// Set missile `idx`'s position: update the shadow and seed the pipeline at
    /// the current beam.
    pub fn set_missile_position(&mut self, idx: usize, pos: u16) {
        let pos = pos % 160;
        if idx == 0 {
            self.pos_m0 = pos;
        } else {
            self.pos_m1 = pos;
        }
        self.seed_missile(idx, pos, self.seed_x());
    }

    /// Advance missile `idx`'s pipeline by one colour clock (Stella
    /// `Missile::tick`). `is_regular` is true for an ordinary visible-clock tick,
    /// false for a movement-engine injection. As with the ball, a regular tick
    /// while the missile is still moving is the starfield effect — but the
    /// missile keys its width modulation off the beam phase `(hclock + 1) mod 4`
    /// at the first render-counter step (−1). Call after the signal is latched.
    fn advance_missile(&mut self, idx: usize, hclock: u16, is_regular: bool) {
        if self.quirk_inverted_phase_clock && self.inverted_phase_m[idx] {
            self.inverted_phase_m[idx] = false;
            return;
        }
        let (_, _, w) = self.missile_inputs(idx);
        let w = i8::try_from(w).unwrap_or(1);
        if self.counter_m[idx] == 156 && !self.resmp(idx) {
            self.m_is_rendering[idx] = true;
            self.m_render_counter[idx] = -4;
        } else if self.m_is_rendering[idx] {
            if self.m_render_counter[idx] == -1 {
                if self.moving_m[idx] && is_regular {
                    match (hclock + 1) % 4 {
                        3 => {
                            self.m_effective_width[idx] = if w == 1 { 2 } else { w };
                            if w < 4 {
                                self.m_render_counter[idx] += 1;
                            }
                        }
                        2 => self.m_effective_width[idx] = 0,
                        _ => self.m_effective_width[idx] = w,
                    }
                } else {
                    self.m_effective_width[idx] = w;
                }
            }
            self.m_render_counter[idx] += 1;
            let limit = if self.moving_m[idx] {
                self.m_effective_width[idx]
            } else {
                w
            };
            if self.m_render_counter[idx] >= limit {
                self.m_is_rendering[idx] = false;
            }
        }
        self.counter_m[idx] = (self.counter_m[idx] + 1) % 160;
    }

    /// RESMP (lock missile to player) state for missile `idx`.
    fn resmp(&self, idx: usize) -> bool {
        if idx == 0 { self.resmp0 } else { self.resmp1 }
    }

    /// Missile `idx`'s geometry visibility this clock, including the starfield
    /// 1-pixel visibility special case (Stella `myIsVisible`): normally `render
    /// counter ≥ 0`, but while moving a regular tick at render counter −1 with
    /// width < 4 and beam phase `(hclock+1) mod 4 == 3` also shows.
    fn missile_visible(&self, idx: usize, hclock: u16, is_regular: bool) -> bool {
        let (_, _, w) = self.missile_inputs(idx);
        let rc = self.m_render_counter[idx];
        self.m_is_rendering[idx]
            && (rc >= 0
                || (self.moving_m[idx] && is_regular && rc == -1 && w < 4 && (hclock + 1) % 4 == 3))
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
    fn seed_ball(&mut self, pos: u16, x: u16) {
        let pos = pos % 160;
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

    /// Re-seed the ball pipeline from its shadow position (`pos_bl`) at the
    /// current beam (free-running counters; no per-line re-seed).
    fn reseed_ball(&mut self) {
        self.seed_ball(self.pos_bl, self.seed_x());
    }

    /// Set the ball's position: update the shadow and seed the pipeline at the
    /// current beam.
    pub fn set_ball_position(&mut self, pos: u16) {
        self.pos_bl = pos % 160;
        self.seed_ball(self.pos_bl, self.seed_x());
    }

    /// Advance the ball's render pipeline and counter by one colour clock
    /// (Stella `Ball::tick`). `is_regular` is true for an ordinary visible-clock
    /// tick, false for a movement-engine injection. A regular tick that lands
    /// while the ball is still moving is the *starfield* effect: the ball's
    /// effective width is modulated by the counter's phase against the last
    /// movement tick (mod 4) — case 2 hides it, case 3 widens it — which is how
    /// the Cosmic Ark stars are drawn. Call after the clock's signal is latched.
    fn advance_ball(&mut self, is_regular: bool) {
        if self.quirk_inverted_phase_clock && self.inverted_phase_bl {
            self.inverted_phase_bl = false;
            return;
        }
        let w = i8::try_from(self.ball_width()).unwrap_or(1);
        let starfield = self.moving_bl && is_regular;
        if self.counter_bl == 156 {
            self.ball_is_rendering = true;
            self.ball_render_counter = -4;
            let delta = (self.counter_bl + 160 - self.ball_last_movement_tick) % 4;
            if starfield && delta == 3 && w < 4 {
                self.ball_render_counter += 1;
            }
            self.ball_effective_width = match delta {
                3 => {
                    if w == 1 {
                        2
                    } else {
                        w
                    }
                }
                2 => 0,
                _ => w,
            };
        } else if self.ball_is_rendering {
            self.ball_render_counter += 1;
            let limit = if starfield {
                self.ball_effective_width
            } else {
                w
            };
            if self.ball_render_counter >= limit {
                self.ball_is_rendering = false;
            }
        }
        self.counter_bl = (self.counter_bl + 1) % 160;
    }

    /// Update collision latches for the current pixel.
    fn update_collisions(&mut self, x: u16) {
        let pf = self.playfield_bit(x);
        let p0 = self.p_signal[0];
        let p1 = self.p_signal[1];
        let m0 = self.missile_signal(0);
        let m1 = self.missile_signal(1);
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
            0x04 => {
                // NUSIZ0: copy layout / size for player 0. A change reshapes the
                // render pipeline, so re-seed from the shadow position.
                self.nusiz0 = value;
                self.reseed_player(0);
            }
            0x05 => {
                self.nusiz1 = value;
                self.reseed_player(1);
            }
            0x06 => self.colup0 = value,            // COLUP0
            0x07 => self.colup1 = value,            // COLUP1
            0x08 => self.colupf = value,            // COLUPF
            0x09 => self.colubk = value,            // COLUBK
            0x0A => self.ctrlpf = value,            // CTRLPF
            0x0B => self.refp0 = value & 0x08 != 0, // REFP0
            0x0C => self.refp1 = value & 0x08 != 0, // REFP1
            0x0D => self.pf0 = value,               // PF0
            0x0E => self.pf1 = value,               // PF1
            0x0F => self.pf2 = value,               // PF2
            0x10 => {
                // RESP0: reset player 0 to the strobe column (beam + pipeline
                // delay, with the late-RESPx quirk applied), seeding the
                // free-running counter there.
                let pos = self.late_respx_adjust(self.resx_reset_position());
                self.set_player_position(0, pos);
            }
            0x11 => {
                let pos = self.late_respx_adjust(self.resx_reset_position());
                self.set_player_position(1, pos);
            }
            0x12 => {
                // RESM0: reset missile 0 to the strobe column.
                let pos = self.late_respx_adjust(self.resx_reset_position());
                self.set_missile_position(0, pos);
            }
            0x13 => {
                let pos = self.late_respx_adjust(self.resx_reset_position());
                self.set_missile_position(1, pos);
            }
            0x14 => {
                // RESBL: reset the ball to the strobe column.
                let pos = self.late_respx_adjust(self.resx_reset_position());
                self.set_ball_position(pos);
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
            0x20 => self.hmm_p[0] = hmm_clocks(value), // HMP0
            0x21 => self.hmm_p[1] = hmm_clocks(value), // HMP1
            0x22 => self.hmm_m[0] = hmm_clocks(value), // HMM0
            0x23 => self.hmm_m[1] = hmm_clocks(value), // HMM1
            0x24 => self.hmm_bl = hmm_clocks(value),   // HMBL
            0x25 => self.vdelp0 = value & 0x01 != 0,   // VDELP0
            0x26 => self.vdelp1 = value & 0x01 != 0,   // VDELP1
            0x27 => self.vdelbl = value & 0x01 != 0,   // VDELBL
            0x28 => self.resmp0 = value & 0x02 != 0,   // RESMP0
            0x29 => self.resmp1 = value & 0x02 != 0,   // RESMP1
            0x2A => {
                // HMOVE: arm the movement engine. Stella delays the strobe by 6
                // colour clocks; the engine then injects each object's extra
                // counter ticks during an extended HBLANK (the 8-pixel comb).
                self.hmove_delay = Some(DELAY_HMOVE);
            }
            0x2B => {
                // HMCLR: clear the HM registers (value 0 → hmmClocks 8 = no move).
                self.hmm_p = [8; 2];
                self.hmm_m = [8; 2];
                self.hmm_bl = 8;
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

    /// Enable or disable the three HMOVE TIA-revision quirks (all default off,
    /// which is the canonical baseline). A particular silicon revision turns on
    /// some combination; wiring a revision selector to this is left to the
    /// machine. See the quirk fields for what each does.
    pub fn set_hmove_quirks(
        &mut self,
        inverted_phase_clock: bool,
        short_late: bool,
        late_respx: bool,
    ) {
        self.quirk_inverted_phase_clock = inverted_phase_clock;
        self.quirk_short_late_hmove = short_late;
        self.quirk_late_respx = late_respx;
    }

    /// Whether a RESPx strobed right now meets Stella's "late RESPx" condition:
    /// in HBLANK with movement just started (`movement_clock == 0`).
    fn late_respx_condition(&self) -> bool {
        self.hpos < HBLANK_CLOCKS && self.movement_active && self.movement_clock == 0
    }

    /// Apply the late-RESPx quirk to a reset position if it is enabled and the
    /// condition holds: the object lands one clock later (Stella shifts the reset
    /// counter by −1, which is +1 in position terms).
    fn late_respx_adjust(&self, pos: u16) -> u16 {
        if self.quirk_late_respx && self.late_respx_condition() {
            (pos + 1) % 160
        } else {
            pos
        }
    }

    /// Fire a (delayed) HMOVE strobe: start the movement engine, extend the
    /// HBLANK for the comb, and mark every object as moving.
    fn fire_hmove(&mut self) {
        self.movement_clock = 0;
        self.movement_active = true;
        self.extended_hblank = true;
        self.moving_p = [true; 2];
        self.moving_m = [true; 2];
        self.moving_bl = true;
    }

    /// The first visible hctr this line: 8 clocks later under an extended HBLANK
    /// (the HMOVE comb), otherwise the usual end of HBLANK.
    fn first_visible_hctr(&self) -> u16 {
        if self.extended_hblank {
            HBLANK_CLOCKS + HMOVE_COMB_CLOCKS
        } else {
            HBLANK_CLOCKS
        }
    }

    /// The movement engine. Once every 4th colour clock, while movement is in
    /// progress, advance each still-moving object's counter by one extra tick
    /// (only during HBLANK — Stella masks/merges injections that fall in the
    /// visible region). An object stops when its movement clock reaches its
    /// `hmmClocks`; movement ends when every object has stopped.
    fn tick_movement(&mut self) {
        if !self.movement_active || self.hpos & 0x03 != 0 {
            return;
        }
        let clock = if self.movement_clock > 15 {
            0
        } else {
            self.movement_clock
        };
        let hblank = self.hpos < self.first_visible_hctr();
        let hclock = self.hpos;
        // The ball keys its starfield phase off the counter at the last movement
        // tick — recorded every 4th clock while moving, like Stella.
        self.ball_last_movement_tick = self.counter_bl;

        // Short-late-HMOVE quirk: a movement tick at hctr 0 is skipped.
        let process = !self.quirk_short_late_hmove || hclock != 0;

        if self.moving_bl {
            if clock == self.hmm_bl {
                self.moving_bl = false;
            } else if process {
                if hblank {
                    self.advance_ball(false);
                }
                // Inverted phase clock quirk: a movement tick outside HBLANK is
                // latched to suppress the following ordinary tick.
                self.inverted_phase_bl = !hblank;
            }
        }
        for idx in 0..2 {
            if self.moving_p[idx] {
                if clock == self.hmm_p[idx] {
                    self.moving_p[idx] = false;
                } else if process {
                    if hblank {
                        self.advance_player(idx);
                    }
                    self.inverted_phase_p[idx] = !hblank;
                }
            }
            if self.moving_m[idx] {
                if clock == self.hmm_m[idx] {
                    self.moving_m[idx] = false;
                } else if process {
                    if hblank {
                        self.advance_missile(idx, hclock, false);
                    }
                    self.inverted_phase_m[idx] = !hblank;
                }
            }
        }

        self.movement_active =
            self.moving_bl || self.moving_p.iter().any(|&m| m) || self.moving_m.iter().any(|&m| m);
        self.movement_clock += 1;
    }

    /// Re-derive each `pos_*` shadow from its free-running counter at the start
    /// of a line. The movement engine moves an object by injecting counter ticks
    /// (not by writing the shadow), so the shadow must be re-synced or a later
    /// re-seed (e.g. a NUSIZ write) would snap the object back. The inverse of
    /// the per-object seed at column 0: ball/missile counter `(1 − pos)`,
    /// player `(2 − pos)`.
    fn sync_position_shadows(&mut self) {
        self.pos_bl = (160 + 1 - self.counter_bl) % 160;
        self.pos_p0 = (160 + 2 - self.counter_p[0]) % 160;
        self.pos_p1 = (160 + 2 - self.counter_p[1]) % 160;
        self.pos_m0 = (160 + 1 - self.counter_m[0]) % 160;
        self.pos_m1 = (160 + 1 - self.counter_m[1]) % 160;
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
        // Horizontal motion / movement engine
        data.push(self.hmm_p[0]);
        data.push(self.hmm_p[1]);
        data.push(self.hmm_m[0]);
        data.push(self.hmm_m[1]);
        data.push(self.hmm_bl);
        data.push(u8::from(self.moving_p[0]));
        data.push(u8::from(self.moving_p[1]));
        data.push(u8::from(self.moving_m[0]));
        data.push(u8::from(self.moving_m[1]));
        data.push(u8::from(self.moving_bl));
        data.push(self.movement_clock);
        data.push(u8::from(self.movement_active));
        data.push(u8::from(self.extended_hblank));
        data.push(self.hmove_delay.unwrap_or(0));
        data.push(u8::from(self.hmove_delay.is_some()));
        // HMOVE revision quirks (config)
        data.push(u8::from(self.quirk_inverted_phase_clock));
        data.push(u8::from(self.quirk_short_late_hmove));
        data.push(u8::from(self.quirk_late_respx));
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
        self.hmm_p[0] = r8!();
        self.hmm_p[1] = r8!();
        self.hmm_m[0] = r8!();
        self.hmm_m[1] = r8!();
        self.hmm_bl = r8!();
        self.moving_p[0] = r8!() != 0;
        self.moving_p[1] = r8!() != 0;
        self.moving_m[0] = r8!() != 0;
        self.moving_m[1] = r8!() != 0;
        self.moving_bl = r8!() != 0;
        self.movement_clock = r8!();
        self.movement_active = r8!() != 0;
        self.extended_hblank = r8!() != 0;
        let hmove_delay_val = r8!();
        self.hmove_delay = if r8!() != 0 {
            Some(hmove_delay_val)
        } else {
            None
        };
        self.quirk_inverted_phase_clock = r8!() != 0;
        self.quirk_short_late_hmove = r8!() != 0;
        self.quirk_late_respx = r8!() != 0;
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
        // The render pipelines are derived, not serialized — re-seed each
        // free-running counter from its restored shadow position.
        self.reseed_ball();
        self.reseed_player(0);
        self.reseed_player(1);
        self.reseed_missile(0);
        self.reseed_missile(1);
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

/// Decode an HMxx register write into the movement engine's `hmmClocks`: the
/// number of extra counter ticks injected during the extended HBLANK
/// (`(value >> 4) ^ 0x08`, 0..15). Net leftward motion is `hmmClocks − 8`, so
/// `$00` → 8 (no motion), `$70` → 15 (left 7), `$80` → 0 (right 8),
/// `$F0` → 7 (right 1).
fn hmm_clocks(value: u8) -> u8 {
    (value >> 4) ^ 0x08
}

/// Decode an HMxx register write into a position delta for [`apply_motion`].
/// Test-only since the movement engine replaced the instant offset — kept to
/// document and cross-check the net motion (`-decode_hmove == hmmClocks - 8`).
///
/// Bits 7:4 are a signed 4-bit value (−8..+7). A **positive** nybble moves the
/// object **left**, a **negative** nybble **right** (Stella / real hardware;
/// the prose `tia-reference.md` table has this inverted). `apply_motion` treats
/// a smaller position as further left, so we return the negated nybble:
///
/// - `$70` (+7) → `-7` → left 7
/// - `$80` (−8) → `+8` → right 8
#[cfg(test)]
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

/// Missile width in colour clocks from NUSIZ bits 5:4 (1/2/4/8).
fn missile_width(nusiz: u8) -> u16 {
    match (nusiz >> 4) & 0x03 {
        0 => 1,
        1 => 2,
        2 => 4,
        3 => 8,
        _ => 1,
    }
}

/// Apply a motion offset to a position, wrapping within 0-159. Test-only — see
/// [`decode_hmove`].
#[cfg(test)]
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
    fn vdelp0_renders_the_delayed_pattern_not_the_new_one() {
        // Output-level VDELP proof. The latch tests above assert internal
        // state (`effective_grp0`); this asserts the delayed pattern actually
        // reaches *pixels*. A state-only check can pass while nothing draws.
        let colup0 = NTSC_PALETTE[0x44 >> 1];

        // Position one player mid-line, render a clean following line, and
        // count its COLUP0 pixels. `setup` programs the GRP/VDELP state.
        fn lit_pixels(colup0: u32, setup: impl FnOnce(&mut Tia)) -> usize {
            let mut tia = Tia::new(TiaRegion::Ntsc);
            tia.write(0x01, 0x00); // VBLANK off
            tia.write(0x09, 0x00); // COLUBK black
            tia.write(0x06, 0x44); // COLUP0
            tia.write(0x04, 0x00); // NUSIZ0 — one copy, 1× width (8 px)
            setup(&mut tia);
            // Line 0: strobe RESP0 at a fixed column, then finish the line.
            for _ in 0..120 {
                tia.tick();
            }
            tia.write(0x10, 0); // RESP0
            for _ in 0..(CLOCKS_PER_LINE - 120) {
                tia.tick();
            }
            // Line 1: the player holds its column, no strobe artifacts.
            for _ in 0..CLOCKS_PER_LINE {
                tia.tick();
            }
            let row1 = (FB_WIDTH as usize)..(2 * FB_WIDTH as usize);
            tia.framebuffer()[row1]
                .iter()
                .filter(|&&p| p == colup0)
                .count()
        }

        // Reference: VDELP0 off draws the new pattern directly.
        let undelayed = lit_pixels(colup0, |tia| {
            tia.write(0x25, 0x00); // VDELP0 off
            tia.write(0x1B, 0xFF); // GRP0 = solid
        });
        assert!(undelayed > 0, "a solid player must draw some pixels");

        // VDELP0 on, delayed copy latched solid, newest GRP0 empty → the
        // *old* (solid) pattern must render, identical to the direct draw.
        let delayed_solid = lit_pixels(colup0, |tia| {
            tia.write(0x25, 0x01); // VDELP0 on
            tia.write(0x1B, 0xFF); // GRP0 new = solid
            tia.write(0x1C, 0x00); // GRP1 write → grp0_old = solid
            tia.write(0x1B, 0x00); // GRP0 new = empty (must NOT show yet)
        });
        assert_eq!(
            delayed_solid, undelayed,
            "VDELP0 on must render the delayed solid pattern, not the new empty one"
        );

        // VDELP0 on, delayed copy latched empty, newest GRP0 solid → the new
        // solid pattern must stay hidden until the next GRP1 latch.
        let delayed_empty = lit_pixels(colup0, |tia| {
            tia.write(0x25, 0x01); // VDELP0 on
            tia.write(0x1B, 0x00); // GRP0 new = empty
            tia.write(0x1C, 0x00); // GRP1 write → grp0_old = empty
            tia.write(0x1B, 0xFF); // GRP0 new = solid (delayed, must NOT show)
        });
        assert_eq!(
            delayed_empty, 0,
            "VDELP0 on must hide the not-yet-latched new pattern"
        );
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
        tia.grp0 = 0xFF;
        tia.nusiz0 = 0x00;
        tia.set_player_position(0, 40);

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
        tia.grp0 = 0xFF;
        tia.nusiz0 = 0x05; // double width → 16px
        tia.set_player_position(0, 40);

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
        tia.grp0 = 0xFF;
        tia.nusiz0 = 0x01; // two copies, 16px apart (8px gap)
        tia.set_player_position(0, 40);

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
        tia.enam0 = true;
        tia.nusiz0 = 0x20; // missile width 4
        tia.set_missile_position(0, 50);

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
        tia.enabl = true;
        tia.ctrlpf = 0x20; // ball width 4
        tia.set_ball_position(60);

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
                tia.set_ball_position(pos);

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
    fn player_counter_model_matches_the_position_formula_everywhere() {
        // Exhaustive output-equivalence guard for the #406 phase-1b player
        // rewrite: across every NUSIZ size, both reflect states, a spread of GRP
        // patterns, and every position, the counter-driven pipeline must paint
        // exactly the columns the reference `player_pixel` formula does — checked
        // on a *settled* line (line 1), since a render carried over the very
        // first scanline has no predecessor line to decode it.
        for nusiz in 0u8..8 {
            for &reflect in &[false, true] {
                for &grp in &[0xFFu8, 0x80, 0x01, 0xA5, 0x3C] {
                    // A representative spread of positions incl. left-edge
                    // straddle and right-edge wrap, kept small so the whole
                    // sweep stays fast.
                    for pos in [0u16, 1, 3, 5, 7, 20, 40, 80, 120, 150, 156, 159] {
                        let mut tia = Tia::new(TiaRegion::Ntsc);
                        tia.colubk = 0x00;
                        tia.colup0 = 0x1E;
                        tia.grp0 = grp;
                        tia.nusiz0 = nusiz;
                        tia.refp0 = reflect;
                        tia.set_player_position(0, pos);

                        // Render two lines; assert the second (settled) one.
                        for _ in 0..(2 * CLOCKS_PER_LINE) {
                            tia.tick();
                        }
                        let base = FB_WIDTH as usize + HBLANK_CLOCKS as usize;
                        let line = &tia.framebuffer()[base..base + 160];

                        let (bg, fg) = (colour(0x00), colour(0x1E));
                        for (x, &px) in line.iter().enumerate() {
                            let on = tia.player_pixel(x as u16, pos, grp, reflect, nusiz);
                            let want = if on { fg } else { bg };
                            assert_eq!(
                                px, want,
                                "nusiz={nusiz:#04x} reflect={reflect} grp={grp:#04x} pos={pos} x={x}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn missile_counter_model_matches_the_position_formula_everywhere() {
        // Exhaustive output-equivalence guard for the #406 phase-1b missile
        // rewrite: for every width, both lock states, and every position the
        // counter-driven renderer must paint exactly the columns the reference
        // `missile_pixel` formula does — including the left-edge straddle and the
        // right-edge wrap back onto the same line.
        for &(nusiz, _width) in &[(0x00u8, 1u16), (0x10, 2), (0x20, 4), (0x30, 8)] {
            for &locked in &[false, true] {
                for pos in 0u16..160 {
                    let mut tia = Tia::new(TiaRegion::Ntsc);
                    tia.colubk = 0x00;
                    tia.colup0 = 0x1E; // missile 0 uses player 0's colour
                    tia.nusiz0 = nusiz;
                    tia.enam0 = true;
                    tia.resmp0 = locked;
                    tia.set_missile_position(0, pos);

                    let prev_line = tia.vpos() as usize;
                    for _ in 0..CLOCKS_PER_LINE {
                        tia.tick();
                    }
                    let base = prev_line * FB_WIDTH as usize + HBLANK_CLOCKS as usize;
                    let line = &tia.framebuffer()[base..base + 160];

                    let (bg, fg) = (colour(0x00), colour(0x1E));
                    for (x, &px) in line.iter().enumerate() {
                        let on = tia.missile_pixel(x as u16, pos, true, nusiz, locked, 0);
                        let want = if on { fg } else { bg };
                        assert_eq!(
                            px, want,
                            "nusiz={nusiz:#04x} locked={locked} pos={pos} x={x}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn hmove_blanks_the_first_eight_pixels_and_applies_net_motion() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        tia.colubk = 0x00;
        tia.colup0 = 0x1E;
        tia.grp0 = 0xFF;
        tia.nusiz0 = 0x00;
        tia.set_player_position(0, 12);
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

    /// Leftmost lit column of a rendered line (the start of a solid sprite).
    fn leftmost_lit(line: &[u32], fg: u32) -> Option<usize> {
        line.iter().position(|&px| px == fg)
    }

    #[test]
    fn hmove_engine_net_motion_matches_the_decode_table_for_every_hm_value() {
        // The movement engine moves objects by injecting counter ticks during
        // the extended HBLANK; the *net* in-HBLANK-HMOVE motion must still match
        // the canonical table (`hmmClocks − 8` = `−decode_hmove`). Sweep all 16
        // HM nibbles from a fixed start clear of the 8px comb, and check where a
        // solid player lands.
        let start = 80u16;
        for nibble in 0u8..16 {
            let value = nibble << 4;
            let mut tia = Tia::new(TiaRegion::Ntsc);
            tia.colubk = 0x00;
            tia.colup0 = 0x1E;
            tia.grp0 = 0xFF;
            tia.nusiz0 = 0x00;
            tia.set_player_position(0, start);
            tia.write(0x20, value); // HMP0
            tia.write(0x2A, 0x00); // HMOVE in HBLANK

            let line = render_visible_line(&mut tia);
            let want = apply_motion(start, decode_hmove(value));
            assert_eq!(
                leftmost_lit(&line, colour(0x1E)),
                Some(want as usize),
                "HMOVE {value:#04x}: player should land at {want}"
            );
        }
    }

    #[test]
    fn hmove_motion_persists_on_the_following_line() {
        // A single HMOVE shifts the free-running counter permanently: the object
        // stays moved on later lines (no comb, no re-move) until the next
        // HMOVE/RESPx — proving the counter free-runs and the shadow re-syncs.
        let mut tia = Tia::new(TiaRegion::Ntsc);
        tia.colubk = 0x00;
        tia.colup0 = 0x1E;
        tia.grp0 = 0xFF;
        tia.nusiz0 = 0x00;
        tia.set_player_position(0, 80);
        tia.write(0x20, 0x70); // HMP0 = left 7
        tia.write(0x2A, 0x00); // HMOVE

        let _hmove_line = render_visible_line(&mut tia);
        let next_line = render_visible_line(&mut tia);
        // 80 − 7 = 73, and the following line has no comb to blank it.
        assert_eq!(leftmost_lit(&next_line, colour(0x1E)), Some(73));
    }

    #[test]
    fn late_hmove_moves_less_than_a_full_hblank_hmove() {
        // Regression baseline for *late* HMOVE (strobed in the visible region):
        // the injections that fall outside HBLANK are masked, so the object
        // moves less than the full `hmmClocks − 8`. This pins the engine's
        // behaviour; the exact pixels still want a Stella 7.0 cross-check.
        let strobe_at = HBLANK_CLOCKS + 30; // mid visible region

        let mut full = Tia::new(TiaRegion::Ntsc);
        full.colubk = 0x00;
        full.colup0 = 0x1E;
        full.grp0 = 0xFF;
        full.set_player_position(0, 100);
        full.write(0x20, 0x70); // left 7
        full.write(0x2A, 0x00); // HMOVE in HBLANK → full move
        let full_line = render_visible_line(&mut full);
        let full_pos = leftmost_lit(&full_line, colour(0x1E)).expect("full HMOVE renders");

        let mut late = Tia::new(TiaRegion::Ntsc);
        late.colubk = 0x00;
        late.colup0 = 0x1E;
        late.grp0 = 0xFF;
        late.set_player_position(0, 100);
        late.write(0x20, 0x70);
        for _ in 0..strobe_at {
            late.tick();
        }
        late.write(0x2A, 0x00); // HMOVE mid-line → late, partially masked
        for _ in 0..(CLOCKS_PER_LINE - strobe_at) {
            late.tick();
        }
        // The next line shows the (smaller) net move the late strobe applied.
        let late_line = render_visible_line(&mut late);
        let late_pos = leftmost_lit(&late_line, colour(0x1E)).expect("late HMOVE renders");

        assert_eq!(full_pos, 93, "full in-HBLANK HMOVE moves left 7");
        assert!(
            late_pos > full_pos,
            "late HMOVE moves less than the full 7 (full={full_pos}, late={late_pos})"
        );
    }

    /// Where a width-1 ball lands on the line *after* an HMOVE strobed at hpos
    /// 221 (late enough that the HMOVE latch is set then cleared by the HSync
    /// wrap — the CPU-cycle-74 trick), for one HMBL nibble. Ball starts at 30.
    fn ball_after_cycle74_hmove(nibble: u8) -> Option<usize> {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        tia.colubk = 0x00;
        tia.colupf = 0x1E;
        tia.ctrlpf = 0x00; // width 1
        tia.enabl = true;
        tia.set_ball_position(30);
        tia.write(0x24, nibble << 4); // HMBL
        while tia.hpos() != 221 {
            tia.tick();
        }
        tia.write(0x2A, 0x00); // HMOVE in the comb-suppressed window
        while tia.hpos() != 0 {
            tia.tick();
        }
        let line = render_visible_line(&mut tia);
        leftmost_lit(&line, colour(0x1E))
    }

    #[test]
    fn cycle74_late_hmove_moves_8_plus_value_left_without_comb() {
        // Towers TIA Hardware Notes: an HMOVE strobed late enough that the
        // latch is set then cleared by the HSync wrap (hpos 220–221 here)
        // suppresses the comb, so objects move the full hmmClocks LEFT — i.e.
        // (8 + value) pixels, *not* the comb-offset `hmmClocks − 8`. A strobe one
        // clock later (hpos 222) is a *normal* end-of-line HMOVE (comb present) —
        // that boundary is exactly what the Pole Position HUD relies on (#581),
        // and getting it one clock early put the HUD's missiles/ball at the left
        // edge (the sliver). Cross-checked against Gopher2600. If the comb were
        // wrongly applied here, every result would sit 8px right (30 − (hmm − 8)).
        for nibble in 0u8..16 {
            let hmm = (nibble ^ 8) as usize; // hmmClocks = (8 + signed value)
            let want = 30 - hmm; // full left move, no comb
            assert_eq!(
                ball_after_cycle74_hmove(nibble),
                Some(want),
                "HMBL nibble {nibble:#x}: cycle-74 HMOVE should move {hmm}px left to {want}"
            );
        }
    }

    /// Lit columns of a moving ball: width-1 ball at 30, HMBL left-7, HMOVE
    /// strobed at colour clock `strobe` of the line, rendered and scanned.
    fn moving_ball_lit(strobe: usize) -> Vec<usize> {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        tia.colubk = 0x00;
        tia.colupf = 0x1E;
        tia.ctrlpf = 0x00; // ball width 1
        tia.enabl = true;
        tia.set_ball_position(30);
        tia.write(0x24, 0x70); // HMBL = left 7
        for _ in 0..strobe {
            tia.tick();
        }
        tia.write(0x2A, 0x00); // HMOVE
        for _ in 0..(CLOCKS_PER_LINE as usize - strobe) {
            tia.tick();
        }
        let base = HBLANK_CLOCKS as usize;
        tia.framebuffer()[base..base + 160]
            .iter()
            .enumerate()
            .filter(|&(_, &p)| p == colour(0x1E))
            .map(|(i, _)| i)
            .collect()
    }

    #[test]
    fn starfield_modulates_a_moving_ball_width() {
        // The starfield effect: when a regular (visible) clock lands while the
        // ball is still moving — a late HMOVE — its effective width is modulated
        // by the movement phase. Same ball + HMBL, only the strobe column moves.
        // These pin the engine's behaviour; exact pixels still want a Stella 7.0
        // cross-check (GUI-driven). See #406 phase 3.
        assert_eq!(
            moving_ball_lit(0),
            vec![23],
            "in-HBLANK HMOVE: clean 1px ball"
        );
        assert_eq!(
            moving_ball_lit(30),
            vec![27, 28],
            "starfield widens the moving ball to 2px"
        );
        assert!(
            moving_ball_lit(40).is_empty(),
            "starfield hides the ball at this phase"
        );
    }

    /// Lit columns of a moving missile: width-1 missile at 30, HMM0 left-7,
    /// HMOVE strobed at colour clock `strobe`.
    fn moving_missile_lit(strobe: usize) -> Vec<usize> {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        tia.colubk = 0x00;
        tia.colup0 = 0x1E;
        tia.enam0 = true;
        tia.nusiz0 = 0x00; // missile width 1
        tia.set_missile_position(0, 30);
        tia.write(0x22, 0x70); // HMM0 = left 7
        for _ in 0..strobe {
            tia.tick();
        }
        tia.write(0x2A, 0x00); // HMOVE
        for _ in 0..(CLOCKS_PER_LINE as usize - strobe) {
            tia.tick();
        }
        let base = HBLANK_CLOCKS as usize;
        tia.framebuffer()[base..base + 160]
            .iter()
            .enumerate()
            .filter(|&(_, &p)| p == colour(0x1E))
            .map(|(i, _)| i)
            .collect()
    }

    #[test]
    fn starfield_modulates_a_moving_missile_width() {
        // The missile keys its starfield width off the beam phase `(hclock+1)%4`,
        // so its modulation lands at a different strobe than the ball's. (This is
        // the object Cosmic Ark actually uses for its stars.)
        assert_eq!(
            moving_missile_lit(0),
            vec![23],
            "in-HBLANK HMOVE: clean 1px missile"
        );
        assert_eq!(
            moving_missile_lit(40),
            vec![30, 31],
            "starfield widens the moving missile to 2px"
        );
    }

    /// Lit columns of a ball, configurable HMOVE strobe column, optional second
    /// strobe, and quirk flags — used to probe the phase-3 corner cases.
    fn probe_ball(strobe: usize, second: Option<usize>, quirks: (bool, bool, bool)) -> Vec<usize> {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        tia.colubk = 0x00;
        tia.colupf = 0x1E;
        tia.ctrlpf = 0x00;
        tia.enabl = true;
        tia.set_hmove_quirks(quirks.0, quirks.1, quirks.2);
        tia.set_ball_position(80);
        tia.write(0x24, 0x70); // HMBL left 7
        let mut clk = 0usize;
        let tick_to = |tia: &mut Tia, clk: &mut usize, target: usize| {
            while *clk < target {
                tia.tick();
                *clk += 1;
            }
        };
        tick_to(&mut tia, &mut clk, strobe);
        tia.write(0x2A, 0x00);
        if let Some(s) = second {
            tick_to(&mut tia, &mut clk, s);
            tia.write(0x2A, 0x00);
        }
        tick_to(&mut tia, &mut clk, CLOCKS_PER_LINE as usize);
        let base = HBLANK_CLOCKS as usize;
        tia.framebuffer()[base..base + 160]
            .iter()
            .enumerate()
            .filter(|&(_, &p)| p == colour(0x1E))
            .map(|(i, _)| i)
            .collect()
    }

    #[test]
    fn multiple_hmoves_on_one_line_both_move_the_object() {
        // A second HMOVE strobe on the same line restarts the movement engine, so
        // the object moves further than a single strobe would. (Baseline, no
        // quirks.)
        let off = (false, false, false);
        assert_eq!(probe_ball(0, None, off), vec![73], "single HMOVE: left 7");
        assert_eq!(
            probe_ball(0, Some(30), off),
            vec![71],
            "a second HMOVE moves the ball further left"
        );
    }

    #[test]
    fn inverted_phase_clock_quirk_changes_late_hmove() {
        // The inverted-phase-clock quirk only bites when movement reaches the
        // visible region (a late HMOVE), where it suppresses ordinary ticks —
        // shifting the result. Off is the baseline. (Exact pixels want a Stella
        // cross-check; this pins that the flag is wired and has an effect.)
        assert_eq!(probe_ball(40, None, (false, false, false)), vec![81]);
        assert_eq!(
            probe_ball(40, None, (true, false, false)),
            vec![89],
            "inverted phase clock shifts the late-HMOVE landing"
        );
    }

    /// Leftmost lit column after HMOVE (no net move) then a RESP0 `resp_at`
    /// clocks later, with the late-RESPx quirk `late`.
    fn respx_after_hmove(late: bool, resp_at: usize) -> Vec<usize> {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        tia.colubk = 0x00;
        tia.colup0 = 0x1E;
        tia.grp0 = 0xFF;
        tia.set_hmove_quirks(false, false, late);
        tia.set_player_position(0, 80);
        tia.write(0x2A, 0x00); // HMOVE — movement runs (clock 0 in early HBLANK)
        for _ in 0..resp_at {
            tia.tick();
        }
        tia.write(0x10, 0x00); // RESP0 while movement_clock == 0
        for _ in 0..(CLOCKS_PER_LINE as usize - resp_at) {
            tia.tick();
        }
        let base = HBLANK_CLOCKS as usize;
        tia.framebuffer()[base..base + 160]
            .iter()
            .position(|&p| p == colour(0x1E))
            .into_iter()
            .collect()
    }

    #[test]
    fn late_respx_quirk_shifts_the_reset_by_one() {
        // With movement just started (clock 0) and the beam in HBLANK, the
        // late-RESPx quirk lands the object one clock later. Here that lifts the
        // ball's left edge out of the 8px comb so a pixel appears.
        assert!(
            respx_after_hmove(false, 7).is_empty(),
            "baseline: reset lands inside the comb (hidden)"
        );
        assert_eq!(
            respx_after_hmove(true, 7),
            vec![8],
            "late RESPx shifts +1, so one pixel clears the comb"
        );
    }

    /// Lit columns of the line *after* a line-boundary-spanning HMOVE, with the
    /// short-late quirk `sl` — the only setup where a movement tick lands at
    /// hctr 0.
    fn ball_after_boundary_hmove(sl: bool) -> Vec<usize> {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        tia.colubk = 0x00;
        tia.colupf = 0x1E;
        tia.ctrlpf = 0x00;
        tia.enabl = true;
        tia.set_hmove_quirks(false, sl, false);
        tia.set_ball_position(80);
        tia.write(0x24, 0x70); // left 7
        for _ in 0..220 {
            tia.tick();
        }
        tia.write(0x2A, 0x00); // HMOVE late — movement spans into the next line
        for _ in 0..(2 * CLOCKS_PER_LINE as usize - 220) {
            tia.tick();
        }
        let base = FB_WIDTH as usize + HBLANK_CLOCKS as usize;
        tia.framebuffer()[base..base + 160]
            .iter()
            .enumerate()
            .filter(|&(_, &p)| p == colour(0x1E))
            .map(|(i, _)| i)
            .collect()
    }

    #[test]
    fn short_late_hmove_quirk_skips_the_hctr0_tick() {
        // When a movement tick falls on hctr 0 (only possible once movement spans
        // a line boundary), the short-late quirk skips it, so the object moves one
        // clock less.
        assert_eq!(ball_after_boundary_hmove(false), vec![65], "baseline");
        assert_eq!(
            ball_after_boundary_hmove(true),
            vec![66],
            "short-late skips the hctr-0 tick → one pixel less movement"
        );
    }
}
