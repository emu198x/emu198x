//! Atari TIA (Television Interface Adapter).
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

mod palette;

pub use palette::{NTSC_PALETTE, PAL_PALETTE};

/// Framebuffer width: 160 visible colour clocks per line.
pub const FB_WIDTH: u32 = 160;

/// Number of colour clocks per scanline (68 hblank + 160 visible).
pub const CLOCKS_PER_LINE: u16 = 228;

/// Horizontal blank duration in colour clocks.
pub const HBLANK_CLOCKS: u16 = 68;

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
    cxm0p: u8,  // CXM0P:  M0-P1 (bit 7), M0-P0 (bit 6)
    cxm1p: u8,  // CXM1P:  M1-P0 (bit 7), M1-P1 (bit 6)
    cxp0fb: u8, // CXP0FB: P0-PF (bit 7), P0-BL (bit 6)
    cxp1fb: u8, // CXP1FB: P1-PF (bit 7), P1-BL (bit 6)
    cxm0fb: u8, // CXM0FB: M0-PF (bit 7), M0-BL (bit 6)
    cxm1fb: u8, // CXM1FB: M1-PF (bit 7), M1-BL (bit 6)
    cxblpf: u8, // CXBLPF: BL-PF (bit 7)
    cxppmm: u8, // CXPPMM: P0-P1 (bit 7), M0-M1 (bit 6)

    // --- Input latches ---
    /// INPT4: Player 0 fire button (bit 7, active low).
    inpt4: u8,
    /// INPT5: Player 1 fire button (bit 7, active low).
    inpt5: u8,

    // --- Framebuffer ---
    /// ARGB32 framebuffer.
    framebuffer: Vec<u32>,
    /// Maximum lines for this region (used for framebuffer sizing).
    max_lines: u16,

    /// Frame complete flag — set when VSYNC is detected.
    frame_complete: bool,
    /// Whether we're in a VSYNC period.
    in_vsync: bool,
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
            framebuffer: vec![0; fb_size],
            max_lines,
            frame_complete: false,
            in_vsync: false,
        }
    }

    /// Advance the TIA by one colour clock.
    ///
    /// This is the master clock tick. The CPU ticks every 3rd colour clock.
    pub fn tick(&mut self) {
        // Render pixel if in visible region.
        if self.hpos >= HBLANK_CLOCKS && self.vpos < self.max_lines {
            let pixel_x = self.hpos - HBLANK_CLOCKS;

            let colour = if self.vblank {
                0 // Black during VBLANK
            } else {
                self.compose_pixel(pixel_x)
            };

            let palette = match self.region {
                TiaRegion::Ntsc => &NTSC_PALETTE,
                TiaRegion::Pal => &PAL_PALETTE,
            };
            let argb = palette[(colour >> 1) as usize];

            // Update collision latches for every visible pixel.
            if !self.vblank {
                self.update_collisions(pixel_x);
            }

            let fb_idx = self.vpos as usize * FB_WIDTH as usize + pixel_x as usize;
            if fb_idx < self.framebuffer.len() {
                self.framebuffer[fb_idx] = argb;
            }
        }

        // Advance horizontal counter.
        self.hpos += 1;
        if self.hpos >= CLOCKS_PER_LINE {
            self.hpos = 0;
            self.wsync_halt = false;
            self.hmove_pending = false;
            self.vpos += 1;

            // VSYNC detection: when VSYNC is deasserted, start new frame.
            if self.in_vsync && !self.vsync {
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
        let p0 = self.player_pixel(x, self.pos_p0, self.effective_grp0(), self.refp0, self.nusiz0);
        let p1 = self.player_pixel(x, self.pos_p1, self.effective_grp1(), self.refp1, self.nusiz1);
        let m0 = self.missile_pixel(x, self.pos_m0, self.enam0, self.nusiz0, self.resmp0, self.pos_p0);
        let m1 = self.missile_pixel(x, self.pos_m1, self.enam1, self.nusiz1, self.resmp1, self.pos_p1);
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
        if self.vdelp0 { self.grp0_old } else { self.grp0 }
    }

    /// Effective GRP1 value (accounts for VDELP1).
    fn effective_grp1(&self) -> u8 {
        if self.vdelp1 { self.grp1_old } else { self.grp1 }
    }

    /// Check if a player sprite is active at pixel position x.
    #[allow(clippy::unused_self)]
    fn player_pixel(&self, x: u16, pos: u16, grp: u8, reflect: bool, nusiz: u8) -> bool {
        if grp == 0 {
            return false;
        }

        let size = nusiz & 0x07;
        let width = match (nusiz >> 4) & 0x03 {
            0 => 1, // 1x
            1 => 2, // 2x
            2 => 4, // 4x (quad)
            _ => 1,
        };

        // Check each copy position.
        let copies: &[(u16, bool)] = match size {
            0x00 => &[(0, true)],                           // One copy
            0x01 => &[(0, true), (16, true)],               // Two copies close
            0x02 => &[(0, true), (32, true)],               // Two copies medium
            0x03 => &[(0, true), (16, true), (32, true)],   // Three copies close
            0x04 => &[(0, true), (64, true)],               // Two copies wide
            0x05 => &[(0, true)],                           // Double-size player
            0x06 => &[(0, true), (32, true), (64, true)],   // Three copies medium
            0x07 => &[(0, true)],                           // Quad-size player
            _ => &[(0, true)],
        };

        let effective_width = match size {
            0x05 => 2,
            0x07 => 4,
            _ => width.min(1),
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
    fn missile_pixel(&self, x: u16, pos: u16, enabled: bool, nusiz: u8, locked: bool, _player_pos: u16) -> bool {
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
    fn ball_pixel(&self, x: u16) -> bool {
        let enabled = if self.vdelbl { self.enabl_old } else { self.enabl };
        if !enabled {
            return false;
        }

        let width: u16 = match (self.ctrlpf >> 4) & 0x03 {
            0 => 1,
            1 => 2,
            2 => 4,
            3 => 8,
            _ => 1,
        };

        let rel = (x + 160 - self.pos_bl) % 160;
        rel < width
    }

    /// Update collision latches for the current pixel.
    fn update_collisions(&mut self, x: u16) {
        let pf = self.playfield_bit(x);
        let p0 = self.player_pixel(x, self.pos_p0, self.effective_grp0(), self.refp0, self.nusiz0);
        let p1 = self.player_pixel(x, self.pos_p1, self.effective_grp1(), self.refp1, self.nusiz1);
        let m0 = self.missile_pixel(x, self.pos_m0, self.enam0, self.nusiz0, self.resmp0, self.pos_p0);
        let m1 = self.missile_pixel(x, self.pos_m1, self.enam1, self.nusiz1, self.resmp1, self.pos_p1);
        let bl = self.ball_pixel(x);

        // M0-P1, M0-P0
        if m0 && p1 { self.cxm0p |= 0x80; }
        if m0 && p0 { self.cxm0p |= 0x40; }
        // M1-P0, M1-P1
        if m1 && p0 { self.cxm1p |= 0x80; }
        if m1 && p1 { self.cxm1p |= 0x40; }
        // P0-PF, P0-BL
        if p0 && pf { self.cxp0fb |= 0x80; }
        if p0 && bl { self.cxp0fb |= 0x40; }
        // P1-PF, P1-BL
        if p1 && pf { self.cxp1fb |= 0x80; }
        if p1 && bl { self.cxp1fb |= 0x40; }
        // M0-PF, M0-BL
        if m0 && pf { self.cxm0fb |= 0x80; }
        if m0 && bl { self.cxm0fb |= 0x40; }
        // M1-PF, M1-BL
        if m1 && pf { self.cxm1fb |= 0x80; }
        if m1 && bl { self.cxm1fb |= 0x40; }
        // BL-PF
        if bl && pf { self.cxblpf |= 0x80; }
        // P0-P1, M0-M1
        if p0 && p1 { self.cxppmm |= 0x80; }
        if m0 && m1 { self.cxppmm |= 0x40; }
    }

    /// Write a TIA register.
    ///
    /// Address is masked to 6 bits ($00-$3F).
    pub fn write(&mut self, addr: u8, value: u8) {
        match addr & 0x3F {
            0x00 => { // VSYNC
                self.vsync = value & 0x02 != 0;
            }
            0x01 => { // VBLANK
                self.vblank = value & 0x02 != 0;
                // Bit 6: dump paddle ports to ground.
                // Bit 7: latch input ports. (TODO for input phase)
            }
            0x02 => { // WSYNC
                self.wsync_halt = true;
            }
            0x03 => { // RSYNC
                self.hpos = 0;
            }
            0x04 => self.nusiz0 = value, // NUSIZ0
            0x05 => self.nusiz1 = value, // NUSIZ1
            0x06 => self.colup0 = value, // COLUP0
            0x07 => self.colup1 = value, // COLUP1
            0x08 => self.colupf = value, // COLUPF
            0x09 => self.colubk = value, // COLUBK
            0x0A => self.ctrlpf = value, // CTRLPF
            0x0B => self.refp0 = value & 0x08 != 0, // REFP0
            0x0C => self.refp1 = value & 0x08 != 0, // REFP1
            0x0D => self.pf0 = value, // PF0
            0x0E => self.pf1 = value, // PF1
            0x0F => self.pf2 = value, // PF2
            0x10 => self.pos_p0 = self.hpos.saturating_sub(HBLANK_CLOCKS), // RESP0
            0x11 => self.pos_p1 = self.hpos.saturating_sub(HBLANK_CLOCKS), // RESP1
            0x12 => self.pos_m0 = self.hpos.saturating_sub(HBLANK_CLOCKS), // RESM0
            0x13 => self.pos_m1 = self.hpos.saturating_sub(HBLANK_CLOCKS), // RESM1
            0x14 => self.pos_bl = self.hpos.saturating_sub(HBLANK_CLOCKS), // RESBL
            0x15 => {} // AUDC0 (audio — future phase)
            0x16 => {} // AUDC1
            0x17 => {} // AUDF0
            0x18 => {} // AUDF1
            0x19 => {} // AUDV0
            0x1A => {} // AUDV1
            0x1B => { // GRP0
                self.grp0_old = self.grp0;
                self.grp0 = value;
                // Writing GRP0 copies GRP1 to old GRP1.
                self.grp1_old = self.grp1;
            }
            0x1C => { // GRP1
                self.grp1_old = self.grp1;
                self.grp1 = value;
                // Writing GRP1 copies GRP0 to old GRP0.
                self.grp0_old = self.grp0;
            }
            0x1D => self.enam0 = value & 0x02 != 0, // ENAM0
            0x1E => self.enam1 = value & 0x02 != 0, // ENAM1
            0x1F => { // ENABL
                self.enabl_old = self.enabl;
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
            0x2A => { // HMOVE
                self.apply_hmove();
                self.hmove_pending = true;
            }
            0x2B => { // HMCLR
                self.hmp0 = 0;
                self.hmp1 = 0;
                self.hmm0 = 0;
                self.hmm1 = 0;
                self.hmbl = 0;
            }
            0x2C => { // CXCLR
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
            0x00 => self.cxm0p,   // CXM0P
            0x01 => self.cxm1p,   // CXM1P
            0x02 => self.cxp0fb,  // CXP0FB
            0x03 => self.cxp1fb,  // CXP1FB
            0x04 => self.cxm0fb,  // CXM0FB
            0x05 => self.cxm1fb,  // CXM1FB
            0x06 => self.cxblpf,  // CXBLPF
            0x07 => self.cxppmm,  // CXPPMM
            0x08 => 0,            // INPT0 (paddle — TODO)
            0x09 => 0,            // INPT1
            0x0A => 0,            // INPT2
            0x0B => 0,            // INPT3
            0x0C => self.inpt4,   // INPT4 (P0 fire)
            0x0D => self.inpt5,   // INPT5 (P1 fire)
            _ => 0,
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
    pub fn set_inpt4(&mut self, pressed: bool) {
        self.inpt4 = if pressed { 0x00 } else { 0x80 };
    }

    /// Set INPT5 (player 1 fire button). Active low: bit 7 = 0 when pressed.
    pub fn set_inpt5(&mut self, pressed: bool) {
        self.inpt5 = if pressed { 0x00 } else { 0x80 };
    }
}

/// Decode a 4-bit signed HMOVE value from the high nybble.
///
/// Bits 4-7 encode a 4-bit two's complement offset. Positive values move
/// left (subtract from position), negative values move right (add to
/// position). We negate so that positive result = move left in screen
/// coordinates.
///
/// Examples: $00 = 0, $10 = -1, $70 = -7, $80 = +8, $F0 = +1.
fn decode_hmove(value: u8) -> i8 {
    // Extract high nybble as 4-bit two's complement, then negate.
    // Sign-extend from 4 bits: if bit 3 is set, OR with 0xF0.
    let nibble = (value >> 4) & 0x0F;
    let signed = if nibble & 0x08 != 0 {
        nibble as i8 | -16 // sign-extend: 0x08..0x0F → -8..-1
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

        // Check that visible pixels got the background colour.
        let expected = NTSC_PALETTE[0x9A >> 1];
        assert_eq!(tia.framebuffer()[0], expected);
        assert_eq!(tia.framebuffer()[159], expected);
    }

    #[test]
    fn vblank_produces_black() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        tia.write(0x09, 0x9A); // Set background
        tia.write(0x01, 0x02); // VBLANK on

        for _ in 0..228 {
            tia.tick();
        }

        // Pixels should be black (palette index 0)
        assert_eq!(tia.framebuffer()[0], NTSC_PALETTE[0]);
    }

    #[test]
    fn framebuffer_size_ntsc() {
        let tia = Tia::new(TiaRegion::Ntsc);
        assert_eq!(tia.framebuffer_width(), 160);
        assert_eq!(tia.framebuffer_height(), 262);
        assert_eq!(tia.framebuffer().len(), 160 * 262);
    }

    #[test]
    fn framebuffer_size_pal() {
        let tia = Tia::new(TiaRegion::Pal);
        assert_eq!(tia.framebuffer_width(), 160);
        assert_eq!(tia.framebuffer_height(), 312);
        assert_eq!(tia.framebuffer().len(), 160 * 312);
    }

    #[test]
    fn playfield_reflect_vs_copy() {
        let mut tia = Tia::new(TiaRegion::Ntsc);
        tia.write(0x0D, 0x10); // PF0: bit 4 set → leftmost column
        tia.write(0x0E, 0x00); // PF1: empty
        tia.write(0x0F, 0x00); // PF2: empty

        // Copy mode (default)
        tia.write(0x0A, 0x00);
        assert!(tia.playfield_bit(0));   // Left half, bit 0
        assert!(tia.playfield_bit(80));  // Right half, copy
        assert!(!tia.playfield_bit(4));  // Left half, bit 1

        // Reflect mode
        tia.write(0x0A, 0x01);
        assert!(tia.playfield_bit(0));   // Left half, bit 0
        // In reflect mode, right half is mirrored: rightmost column maps to PF0 bit 4
        assert!(tia.playfield_bit(156)); // Reflected position
    }

    #[test]
    fn hmove_decode() {
        // $00 = no motion
        assert_eq!(decode_hmove(0x00), 0);
        // $10 = -1 (move right)
        assert_eq!(decode_hmove(0x10), -1);
        // $70 = -7
        assert_eq!(decode_hmove(0x70), -7);
        // $80 = +8 (max left)
        assert_eq!(decode_hmove(0x80), 8);
        // $F0 = +1 (move left)
        assert_eq!(decode_hmove(0xF0), 1);
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
    fn ntsc_palette_has_128_entries() {
        assert_eq!(NTSC_PALETTE.len(), 128);
    }

    #[test]
    fn pal_palette_has_128_entries() {
        assert_eq!(PAL_PALETTE.len(), 128);
    }
}
