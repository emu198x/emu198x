//! Atari GTIA (George's Television Interface Adapter) emulator.
//!
//! The GTIA receives playfield pixel data from ANTIC and overlays
//! player/missile graphics to produce final ARGB32 video output.
//! Used in the Atari 5200 and 8-bit computer line (400/800/XL/XE).

pub mod palette;

use palette::NTSC_PALETTE;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Framebuffer width (hires resolution: 320 pixels).
pub const FB_WIDTH: u32 = 320;

/// Framebuffer height (240 visible scan lines).
pub const FB_HEIGHT: u32 = 240;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
pub struct Gtia {
    // -- Colour registers --
    colpm: [u8; 4],  // COLPM0-3: player/missile colours
    colpf: [u8; 4],  // COLPF0-3: playfield colours
    colbk: u8,       // COLBK: background

    // -- Player/missile position --
    hposp: [u8; 4],  // HPOSPx: horizontal position of players
    hposm: [u8; 4],  // HPOSMx: horizontal position of missiles

    // -- Player/missile size --
    sizep: [u8; 4],  // SIZEPx: player size (bits 0-1)
    sizem: u8,       // SIZEM: missile sizes (2 bits each)

    // -- Player/missile graphics --
    grafp: [u8; 4],  // GRAFPx: 8-bit player graphic patterns
    grafm: u8,       // GRAFM: 2-bit missile graphic patterns

    // -- Control --
    prior: u8,       // PRIOR: priority and GTIA mode select
    vdelay: u8,      // VDELAY: vertical delay
    gractl: u8,      // GRACTL: graphics control

    // -- Collision registers (active-high bit flags) --
    m_pf: [u8; 4],   // M0PF-M3PF: missile-to-playfield
    p_pf: [u8; 4],   // P0PF-P3PF: player-to-playfield
    m_pl: [u8; 4],   // M0PL-M3PL: missile-to-player
    p_pl: [u8; 4],   // P0PL-P3PL: player-to-player

    // -- Trigger inputs --
    trig: [u8; 4],   // TRIG0-TRIG3: 1=released, 0=pressed

    // -- Console --
    consol: u8,      // CONSOL: console button state (active low, bits 0-2)

    // -- Framebuffer --
    framebuffer: Vec<u32>,
}

impl Gtia {
    /// Create a new GTIA in its power-on state.
    #[must_use]
    pub fn new() -> Self {
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
            consol: 0x07, // all buttons released (active low)
            framebuffer: vec![0xFF00_0000; (FB_WIDTH * FB_HEIGHT) as usize],
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
                // CONSOL write — directly stores low 3 bits (active-low buttons)
                self.consol = (self.consol & 0xF8) | (value & 0x07);
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
            // CONSOL
            0x1F => self.consol,
            // All other read addresses return $FF (open bus)
            _ => 0xFF,
        }
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
        FB_WIDTH
    }

    /// Framebuffer height in pixels.
    #[must_use]
    pub const fn framebuffer_height(&self) -> u32 {
        FB_HEIGHT
    }

    // -----------------------------------------------------------------------
    // Rendering
    // -----------------------------------------------------------------------

    /// Render one scan line from ANTIC playfield data.
    ///
    /// - `line`: scan line number (0-239 within the visible region)
    /// - `playfield`: pixel index values from ANTIC
    /// - `pf_width`: playfield width in colour clocks (128, 160, or 192)
    /// - `mode`: the ANTIC display mode, controls colour interpretation
    pub fn render_line(
        &mut self,
        line: u16,
        playfield: &[u8],
        pf_width: u16,
        mode: AnticMode,
    ) {
        if line >= FB_HEIGHT as u16 {
            return;
        }

        let fb_offset = line as usize * FB_WIDTH as usize;
        let gtia_mode = (self.prior >> 6) & 0x03;

        // -- Build a 320-pixel line of colour register indices --
        let mut line_buf = [0u8; FB_WIDTH as usize];

        if mode != AnticMode::Blank {
            self.fill_playfield_line(&mut line_buf, playfield, pf_width, mode, gtia_mode);
        }

        // -- Build player/missile overlay --
        let mut pm_colour = [0u8; FB_WIDTH as usize]; // 0 = no PM pixel
        let mut pm_index = [0u8; FB_WIDTH as usize];  // which PM object (for collisions)
        self.overlay_players_missiles(&mut pm_colour, &mut pm_index);

        // -- Compose final pixels with priority and collision detection --
        for x in 0..FB_WIDTH as usize {
            let pf_col_idx = line_buf[x];

            // Collision detection: PM vs playfield
            if pm_index[x] != 0 && pf_col_idx != 0 {
                self.record_collisions(pm_index[x], pf_col_idx);
            }

            // Priority: default ($00) — PM over PF over background
            let final_colour = if pm_colour[x] != 0 {
                pm_colour[x]
            } else if pf_col_idx != 0 {
                self.resolve_colour(pf_col_idx, gtia_mode, playfield, x, pf_width, mode)
            } else {
                self.colbk
            };

            self.framebuffer[fb_offset + x] = colour_to_argb32(final_colour);
        }
    }

    /// Fill the 320-pixel line buffer with playfield colour register indices.
    #[allow(clippy::unused_self)] // will use colour registers for GTIA mode expansion
    fn fill_playfield_line(
        &self,
        line_buf: &mut [u8; FB_WIDTH as usize],
        playfield: &[u8],
        pf_width: u16,
        mode: AnticMode,
        gtia_mode: u8,
    ) {
        // Pixels per colour clock depend on mode resolution
        let (pixels_per_cc, hires) = match mode {
            AnticMode::ModeF => (2, true),         // 320 px / 160 cc
            AnticMode::ModeD | AnticMode::ModeE => (2, false), // 160 px → 2 fb px each
            AnticMode::Mode2 | AnticMode::Mode3 => (2, true),  // text hires
            _ => (2, false),
        };

        // Centre the playfield in the 320-pixel framebuffer
        let pf_fb_width = u16::min(pf_width * pixels_per_cc, FB_WIDTH as u16);
        let fb_start = (FB_WIDTH as u16 - pf_fb_width) / 2;

        if hires {
            // Hires: each playfield byte is one pixel → 1 fb pixel
            for (i, &px) in playfield.iter().enumerate() {
                let fb_x = fb_start as usize + i;
                if fb_x < FB_WIDTH as usize {
                    if gtia_mode != 0 {
                        // GTIA colour modes use the raw pixel value
                        line_buf[fb_x] = px;
                    } else {
                        line_buf[fb_x] = px;
                    }
                }
            }
        } else {
            // Non-hires: each playfield pixel maps to 2 fb pixels
            for (i, &px) in playfield.iter().enumerate() {
                let fb_x = fb_start as usize + i * 2;
                if fb_x + 1 < FB_WIDTH as usize {
                    line_buf[fb_x] = px;
                    line_buf[fb_x + 1] = px;
                }
            }
        }
    }

    /// Overlay player/missile graphics onto the PM buffers.
    fn overlay_players_missiles(
        &self,
        pm_colour: &mut [u8; FB_WIDTH as usize],
        pm_index: &mut [u8; FB_WIDTH as usize],
    ) {
        let fifth_player = (self.prior & 0x10) != 0;

        // Render missiles
        for m in 0..NUM_MISSILES {
            let pattern = (self.grafm >> (m * 2)) & 0x03;
            if pattern == 0 {
                continue;
            }
            let hpos = self.hposm[m];
            let size_bits = (self.sizem >> (m * 2)) & 0x03;
            let width = missile_width(size_bits);
            let colour = if fifth_player {
                self.colpf[3]
            } else {
                self.colpm[m]
            };

            for bit in 0..2u16 {
                if pattern & (1 << (1 - bit)) == 0 {
                    continue;
                }
                for sub in 0..width {
                    let cc = u16::from(hpos) + bit * width + sub;
                    if let Some(x) = cc_to_fb_x(cc) {
                        // Each colour clock = 2 hires pixels
                        for dx in 0..2usize {
                            let fx = x + dx;
                            if fx < FB_WIDTH as usize && pm_colour[fx] == 0 {
                                pm_colour[fx] = colour;
                                pm_index[fx] |= 1 << (m + 4);
                            }
                        }
                    }
                }
            }
        }

        // Render players (higher priority than missiles in default mode)
        for p in 0..NUM_PLAYERS {
            let pattern = self.grafp[p];
            if pattern == 0 {
                continue;
            }
            let hpos = self.hposp[p];
            let size_bits = self.sizep[p] & 0x03;
            let pixel_width = player_pixel_width(size_bits);
            let colour = self.colpm[p];

            for bit in 0..8u16 {
                if pattern & (1 << (7 - bit)) == 0 {
                    continue;
                }
                for sub in 0..pixel_width {
                    let cc = u16::from(hpos) + bit * pixel_width + sub;
                    if let Some(x) = cc_to_fb_x(cc) {
                        // Each colour clock = 2 hires pixels
                        for dx in 0..2usize {
                            let fx = x + dx;
                            if fx < FB_WIDTH as usize {
                                pm_colour[fx] = colour;
                                pm_index[fx] |= 1 << p;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Record collision flags for a pixel where PM and PF overlap.
    fn record_collisions(&mut self, pm_bits: u8, pf_idx: u8) {
        // pf_idx is a colour register index (1-4 for PF0-PF3)
        let pf_bit = if (1..=4).contains(&pf_idx) {
            1u8 << (pf_idx - 1)
        } else {
            return;
        };

        // Players (bits 0-3 of pm_bits)
        for p in 0..NUM_PLAYERS {
            if pm_bits & (1 << p) != 0 {
                self.p_pf[p] |= pf_bit;
            }
        }

        // Missiles (bits 4-7 of pm_bits)
        for m in 0..NUM_MISSILES {
            if pm_bits & (1 << (m + 4)) != 0 {
                self.m_pf[m] |= pf_bit;
            }
        }

        // Player-to-player collisions
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

        // Missile-to-player collisions
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

impl Default for Gtia {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a colour clock position to a framebuffer x coordinate.
/// Returns `None` if the position is outside the visible region.
fn cc_to_fb_x(cc: u16) -> Option<usize> {
    if cc >= PF_LEFT_CC && cc < PF_LEFT_CC + (FB_WIDTH as u16 / 2) {
        // Each colour clock = 2 hires pixels
        let x = ((cc - PF_LEFT_CC) * 2) as usize;
        if x + 1 < FB_WIDTH as usize {
            Some(x)
        } else {
            None
        }
    } else {
        None
    }
}

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
    use super::*;

    #[test]
    fn colour_register_write_read() {
        let mut gtia = Gtia::new();
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
    fn player_position_and_graphics() {
        let mut gtia = Gtia::new();
        // Place player 0 at HPOS=80, give it a solid 8-pixel pattern
        gtia.write(0x00, 80); // HPOSP0
        gtia.write(0x0D, 0xFF); // GRAFP0: all 8 bits set
        gtia.write(0x12, 0x38); // COLPM0: some colour

        // Render a blank line — player should appear
        let playfield = vec![0u8; 160];
        gtia.render_line(0, &playfield, 160, AnticMode::ModeD);

        // Player at HPOS=80, PF_LEFT_CC=48, so fb_x = (80-48)*2 = 64
        // 8 pixels wide at normal size, each 1 cc = 2 fb pixels
        let fb = gtia.framebuffer();
        let player_argb = colour_to_argb32(0x38);
        assert_eq!(fb[64], player_argb);
        assert_eq!(fb[65], player_argb);
    }

    #[test]
    fn collision_detection_player_playfield() {
        let mut gtia = Gtia::new();
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
    fn collision_clear() {
        let mut gtia = Gtia::new();
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
        let mut gtia = Gtia::new();
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
        let mut gtia = Gtia::new();
        // Default: all buttons released (bits 0-2 = 1)
        assert_eq!(gtia.read(0x1F) & 0x07, 0x07);

        // Write CONSOL — simulates pressing START (bit 0 = 0)
        gtia.write(0x1F, 0x06);
        assert_eq!(gtia.read(0x1F) & 0x07, 0x06);
    }

    #[test]
    fn framebuffer_size() {
        let gtia = Gtia::new();
        assert_eq!(gtia.framebuffer_width(), 320);
        assert_eq!(gtia.framebuffer_height(), 240);
        assert_eq!(gtia.framebuffer().len(), 320 * 240);
    }

    #[test]
    fn priority_default_players_over_playfield() {
        let mut gtia = Gtia::new();
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
        let fb_x = ((60 - PF_LEFT_CC) * 2) as usize;
        assert_eq!(fb[fb_x], player_argb, "Player should be on top at default priority");
    }

    #[test]
    fn gtia_mode_selection() {
        let mut gtia = Gtia::new();

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
