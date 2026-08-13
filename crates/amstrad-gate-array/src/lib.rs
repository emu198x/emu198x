//! Amstrad Gate Array — the CPC's custom video, palette and ROM-paging chip.
//!
//! Part 40007 on the CPC464 (confirmed as IC116 in the 1985 service manual's
//! parts list), 40010 on the CPC664 and CPC6128. The CPC Plus range replaces it
//! with the AMS40489 ASIC, which is a different chip and not modelled here — see
//! `Emu198x/docs/plans/2026-08-13-amstrad-cpc-plan.md`.
//!
//! # What this crate covers
//!
//! The Gate Array has four jobs. This crate implements the two that are pure
//! register state and combinational decode:
//!
//! - **Video mode and pixel decode** — turning a display byte into pen numbers.
//! - **The palette** — 16 pens plus a border, each holding a 5-bit hardware
//!   colour code.
//!
//! It also holds the **ROM-paging** and **interrupt-counter-reset** bits,
//! because they live in the same register as the video mode and the chip does
//! nothing with them but drive lines the machine reads. Acting on them is the
//! machine's job.
//!
//! The two jobs deliberately *not* here are the interrupt counter (which needs
//! the CRTC's HSYNC) and `/WAIT` generation. Both are machine-level wiring
//! rather than register state, and both are timing work that wants its own
//! validation.
//!
//! # Register decode
//!
//! One write-only port, decoded on **bits 7-6** of the data byte. Verified
//! against MAME's `amstrad_GateArray_write` (`mame/amstrad/amstrad_m.cpp:1577`),
//! whose comment blocks document the bit layouts reproduced here.
//!
//! | Bits 7-6 | Register | Payload |
//! |---|---|---|
//! | `00` | `PENR` | bit 4 set selects the border; otherwise bits 3-0 select pen 0-15 |
//! | `01` | `INKR` | bits 4-0 are the colour code for the selected pen |
//! | `10` | `RMR`  | bit 4 interrupt-counter reset, bit 3 upper-ROM *disable*, bit 2 lower-ROM *disable*, bits 1-0 video mode |
//! | `11` | — | RAM banking, which is the PAL on the 6128 and not this chip |
//!
//! The ROM bits are **active-low in the sense that 1 disables**: this is the
//! one place the register layout inverts against intuition, so the accessors
//! here are named for what is *enabled*.
//!
//! # Colours
//!
//! Each pen holds a 5-bit code, so 32 codes address a 27-colour palette: every
//! channel takes one of three levels (`0x00`, `0x60`, `0xFF`), giving 3³ = 27
//! distinct colours, and five of the 32 codes are duplicates. The table is
//! taken from MAME's `amstrad_palette` (`amstrad_m.cpp:129-192`).

use serde::{Deserialize, Serialize};

/// Pen index of the border. The border is a seventeenth pen, selected by
/// `PENR` bit 4 rather than by a pen number.
pub const BORDER_PEN: u8 = 16;

/// Number of addressable pens including the border.
pub const PEN_COUNT: usize = 17;

/// The 32 hardware colour codes as `0xAARRGGBB`.
///
/// Every channel is one of three levels, which is why 32 codes yield only 27
/// distinct colours — codes 0/1, 4/16, 2/17, 3/9 and 5/8 are duplicate pairs.
/// From MAME's `amstrad_palette`.
pub const HARDWARE_PALETTE: [u32; 32] = [
    0xFF60_6060, // 0  white
    0xFF60_6060, // 1  white (duplicate of 0)
    0xFF00_FF60, // 2  sea green
    0xFFFF_FF60, // 3  pastel yellow
    0xFF00_0060, // 4  blue
    0xFFFF_0060, // 5  purple
    0xFF00_6060, // 6  cyan
    0xFFFF_6060, // 7  pink
    0xFFFF_0060, // 8  purple (duplicate of 5)
    0xFFFF_FF60, // 9  pastel yellow (duplicate of 3)
    0xFFFF_FF00, // 10 bright yellow
    0xFFFF_FFFF, // 11 bright white
    0xFFFF_0000, // 12 bright red
    0xFFFF_00FF, // 13 bright magenta
    0xFFFF_6000, // 14 orange
    0xFFFF_60FF, // 15 pastel magenta
    0xFF00_0060, // 16 blue (duplicate of 4)
    0xFF00_FF60, // 17 sea green (duplicate of 2)
    0xFF00_FF00, // 18 bright green
    0xFF00_FFFF, // 19 bright cyan
    0xFF00_0000, // 20 black
    0xFF00_00FF, // 21 bright blue
    0xFF00_6000, // 22 green
    0xFF00_60FF, // 23 sky blue
    0xFF60_0060, // 24 magenta
    0xFF60_FF60, // 25 pastel green
    0xFF60_FF00, // 26 lime
    0xFF60_FFFF, // 27 pastel cyan
    0xFF60_0000, // 28 red
    0xFF60_00FF, // 29 mauve
    0xFF60_6000, // 30 yellow
    0xFF60_60FF, // 31 pastel blue
];

/// Video mode, from `RMR` bits 1-0.
///
/// A CPC display line is 80 bytes wide in every mode; the mode decides how many
/// pixels each byte carries, which is what makes the stated resolutions come
/// out (80 × pixels-per-byte).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoMode {
    /// 160×200, 16 colours — 2 pixels per byte, 4 bits each.
    Mode0,
    /// 320×200, 4 colours — 4 pixels per byte, 2 bits each.
    Mode1,
    /// 640×200, 2 colours — 8 pixels per byte, 1 bit each.
    Mode2,
    /// 160×200, 4 colours — the undocumented mode. Two pixels per byte at
    /// mode 0's width, but taking mode 1's two-bit pen pattern, so only pens
    /// 0-3 are reachable. Follows MAME, which selects `mode1_lookup` with
    /// mode 0's pixel width (`amstrad_m.cpp`, `amstrad_vh_update_mode`).
    Mode3,
}

impl VideoMode {
    /// Decode from `RMR` bits 1-0.
    #[must_use]
    pub fn from_bits(bits: u8) -> Self {
        match bits & 0x03 {
            0 => Self::Mode0,
            1 => Self::Mode1,
            2 => Self::Mode2,
            _ => Self::Mode3,
        }
    }

    /// Pixels carried by one display byte.
    #[must_use]
    pub fn pixels_per_byte(self) -> usize {
        match self {
            Self::Mode0 | Self::Mode3 => 2,
            Self::Mode1 => 4,
            Self::Mode2 => 8,
        }
    }

    /// Horizontal resolution across the standard 80-byte line.
    #[must_use]
    pub fn width(self) -> usize {
        self.pixels_per_byte() * 80
    }

    /// Number of pens reachable in this mode.
    #[must_use]
    pub fn pen_count(self) -> usize {
        match self {
            Self::Mode0 => 16,
            Self::Mode1 | Self::Mode3 => 4,
            Self::Mode2 => 2,
        }
    }

    /// The leftmost pixel's pen number for `byte`.
    ///
    /// The CPC interleaves a pixel's bits across the byte rather than keeping
    /// them adjacent, so each mode is a gather rather than a shift-and-mask.
    /// Successive pixels come from applying this to `byte << 1`, which is why
    /// one expression serves every pixel in the byte. Masks are MAME's
    /// `amstrad_init_lookups`.
    #[must_use]
    pub fn leftmost_pen(self, byte: u8) -> u8 {
        match self {
            // bit7 → pen0, bit3 → pen1, bit5 → pen2, bit1 → pen3
            Self::Mode0 => {
                ((byte & 0x80) >> 7)
                    | ((byte & 0x20) >> 3)
                    | ((byte & 0x08) >> 2)
                    | ((byte & 0x02) << 2)
            }
            // bit7 → pen0, bit3 → pen1
            Self::Mode1 | Self::Mode3 => ((byte & 0x80) >> 7) | ((byte & 0x08) >> 2),
            // bit7 only
            Self::Mode2 => (byte & 0x80) >> 7,
        }
    }
}

/// The Amstrad Gate Array.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateArray {
    /// Hardware colour code (0-31) per pen; index [`BORDER_PEN`] is the border.
    pens: [u8; PEN_COUNT],
    /// Pen that `INKR` writes will land on.
    selected_pen: u8,
    mode: VideoMode,
    lower_rom_enabled: bool,
    upper_rom_enabled: bool,
    /// Latched `RMR` bit 4, consumed by the machine driving the interrupt
    /// counter.
    interrupt_reset_pending: bool,
}

impl Default for GateArray {
    fn default() -> Self {
        Self::new()
    }
}

impl GateArray {
    /// A Gate Array out of reset: mode 0, both ROMs paged in, every pen black.
    ///
    /// The ROMs being enabled is what puts the firmware at `$0000` for the Z80
    /// to boot from, so it is the only reset state that lets a machine start.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pens: [0; PEN_COUNT],
            selected_pen: 0,
            mode: VideoMode::Mode0,
            lower_rom_enabled: true,
            upper_rom_enabled: true,
            interrupt_reset_pending: false,
        }
    }

    /// Write to the Gate Array's port, decoded on bits 7-6.
    ///
    /// The `11` case is RAM banking, which on the 6128 is the PAL rather than
    /// this chip, so it is ignored here rather than silently mishandled.
    pub fn write(&mut self, value: u8) {
        match value >> 6 {
            0b00 => {
                self.selected_pen = if value & 0x10 != 0 {
                    BORDER_PEN
                } else {
                    value & 0x0F
                };
            }
            0b01 => {
                let pen = usize::from(self.selected_pen.min(BORDER_PEN));
                self.pens[pen] = value & 0x1F;
            }
            0b10 => {
                self.mode = VideoMode::from_bits(value);
                // Bits 2 and 3 *disable* when set.
                self.lower_rom_enabled = value & 0x04 == 0;
                self.upper_rom_enabled = value & 0x08 == 0;
                if value & 0x10 != 0 {
                    self.interrupt_reset_pending = true;
                }
            }
            _ => {}
        }
    }

    /// The pen `INKR` writes currently target.
    #[must_use]
    pub fn selected_pen(&self) -> u8 {
        self.selected_pen
    }

    /// Hardware colour code (0-31) held by `pen`. Out-of-range pens clamp to
    /// the border, matching the 5-bit selection the hardware can express.
    #[must_use]
    pub fn pen_code(&self, pen: u8) -> u8 {
        self.pens[usize::from(pen.min(BORDER_PEN))]
    }

    /// `pen`'s colour as `0xAARRGGBB`.
    #[must_use]
    pub fn pen_rgb(&self, pen: u8) -> u32 {
        HARDWARE_PALETTE[usize::from(self.pen_code(pen))]
    }

    /// The border's colour as `0xAARRGGBB`.
    #[must_use]
    pub fn border_rgb(&self) -> u32 {
        self.pen_rgb(BORDER_PEN)
    }

    /// Current video mode.
    #[must_use]
    pub fn mode(&self) -> VideoMode {
        self.mode
    }

    /// Whether the lower ROM is paged in at `$0000-$3FFF`.
    #[must_use]
    pub fn lower_rom_enabled(&self) -> bool {
        self.lower_rom_enabled
    }

    /// Whether the upper ROM is paged in at `$C000-$FFFF`.
    #[must_use]
    pub fn upper_rom_enabled(&self) -> bool {
        self.upper_rom_enabled
    }

    /// Take the pending interrupt-counter reset, clearing it.
    ///
    /// `RMR` bit 4 is a one-shot request rather than a level, so the machine
    /// consumes it once and the flag drops.
    pub fn take_interrupt_reset(&mut self) -> bool {
        core::mem::replace(&mut self.interrupt_reset_pending, false)
    }

    /// Decode one display byte into pen numbers, writing into `out` and
    /// returning how many pixels were produced.
    ///
    /// Writes nothing and returns 0 if `out` is too short for the current
    /// mode, so a caller cannot half-fill a scanline without noticing.
    pub fn decode_byte(&self, byte: u8, out: &mut [u8]) -> usize {
        let count = self.mode.pixels_per_byte();
        if out.len() < count {
            return 0;
        }
        let mut shifted = byte;
        for slot in out.iter_mut().take(count) {
            *slot = self.mode.leftmost_pen(shifted);
            shifted <<= 1;
        }
        count
    }

    /// Decode one display byte straight to `0xAARRGGBB`, resolving each pen
    /// through the palette. Same contract as [`Self::decode_byte`].
    pub fn decode_byte_rgb(&self, byte: u8, out: &mut [u32]) -> usize {
        let count = self.mode.pixels_per_byte();
        if out.len() < count {
            return 0;
        }
        let mut shifted = byte;
        for slot in out.iter_mut().take(count) {
            *slot = self.pen_rgb(self.mode.leftmost_pen(shifted));
            shifted <<= 1;
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_pages_both_roms_in() {
        // Without this the Z80 has no firmware at $0000 and cannot boot.
        let ga = GateArray::new();
        assert!(ga.lower_rom_enabled());
        assert!(ga.upper_rom_enabled());
        assert_eq!(ga.mode(), VideoMode::Mode0);
    }

    #[test]
    fn penr_selects_a_pen_and_inkr_colours_it() {
        let mut ga = GateArray::new();
        ga.write(0b0000_0101); // PENR: pen 5
        assert_eq!(ga.selected_pen(), 5);
        ga.write(0b0100_1011); // INKR: colour 11
        assert_eq!(ga.pen_code(5), 11);
        assert_eq!(ga.pen_rgb(5), 0xFFFF_FFFF, "code 11 is bright white");
    }

    #[test]
    fn penr_bit4_selects_the_border_not_pen_16() {
        // The border is reached by a flag, not by counting past pen 15.
        let mut ga = GateArray::new();
        ga.write(0b0001_0000);
        assert_eq!(ga.selected_pen(), BORDER_PEN);
        ga.write(0b0100_1100); // colour 12, bright red
        assert_eq!(ga.border_rgb(), 0xFFFF_0000);
    }

    #[test]
    fn inkr_keeps_only_five_bits() {
        let mut ga = GateArray::new();
        ga.write(0b0000_0000);
        ga.write(0b0111_1111); // upper bits are not part of the colour code
        assert_eq!(ga.pen_code(0), 0x1F);
    }

    #[test]
    fn rmr_rom_bits_disable_when_set() {
        // The one inverted layout in the register set.
        let mut ga = GateArray::new();
        ga.write(0b1000_1100); // both ROM bits set
        assert!(!ga.lower_rom_enabled());
        assert!(!ga.upper_rom_enabled());
        ga.write(0b1000_0000); // both clear
        assert!(ga.lower_rom_enabled());
        assert!(ga.upper_rom_enabled());
    }

    #[test]
    fn rmr_selects_each_mode() {
        let mut ga = GateArray::new();
        for (bits, expected) in [
            (0, VideoMode::Mode0),
            (1, VideoMode::Mode1),
            (2, VideoMode::Mode2),
            (3, VideoMode::Mode3),
        ] {
            ga.write(0b1000_0000 | bits);
            assert_eq!(ga.mode(), expected);
        }
    }

    #[test]
    fn interrupt_reset_is_a_one_shot() {
        let mut ga = GateArray::new();
        assert!(!ga.take_interrupt_reset());
        ga.write(0b1001_0000); // RMR bit 4
        assert!(ga.take_interrupt_reset());
        assert!(!ga.take_interrupt_reset(), "consumed, not latched");
    }

    #[test]
    fn ram_banking_writes_are_not_this_chip() {
        // `11` is the PAL on the 6128; it must not disturb Gate Array state.
        let mut ga = GateArray::new();
        ga.write(0b1000_0010); // mode 2
        ga.write(0b1100_0101); // RAM banking
        assert_eq!(ga.mode(), VideoMode::Mode2);
    }

    #[test]
    fn mode_geometry_matches_the_stated_resolutions() {
        // A line is 80 bytes in every mode, so pixels-per-byte is what makes
        // 160/320/640 come out. If these disagree the decode is wrong.
        assert_eq!(VideoMode::Mode0.width(), 160);
        assert_eq!(VideoMode::Mode1.width(), 320);
        assert_eq!(VideoMode::Mode2.width(), 640);
        assert_eq!(VideoMode::Mode3.width(), 160);
    }

    #[test]
    fn mode0_gathers_pen_bits_from_across_the_byte() {
        // Pen 0 bits are 7,3,5,1 in that order — the interleave is the whole
        // reason this is a gather and not a mask.
        let mut out = [0u8; 2];
        let ga = GateArray::new();
        assert_eq!(ga.decode_byte(0b1000_0000, &mut out), 2);
        assert_eq!(out[0], 0b0001, "bit7 is pen bit 0");
        assert_eq!(ga.decode_byte(0b0000_1000, &mut out), 2);
        assert_eq!(out[0], 0b0010, "bit3 is pen bit 1");
        assert_eq!(ga.decode_byte(0b0010_0000, &mut out), 2);
        assert_eq!(out[0], 0b0100, "bit5 is pen bit 2");
        assert_eq!(ga.decode_byte(0b0000_0010, &mut out), 2);
        assert_eq!(out[0], 0b1000, "bit1 is pen bit 3");
    }

    #[test]
    fn mode0_second_pixel_uses_the_odd_bits() {
        // The two pixels interleave: even bit positions for the first, odd for
        // the second.
        let mut ga = GateArray::new();
        ga.write(0b1000_0000);
        let mut out = [0u8; 2];
        assert_eq!(ga.decode_byte(0b0100_0000, &mut out), 2);
        assert_eq!(out[0], 0, "bit6 belongs to the second pixel");
        assert_eq!(out[1], 0b0001);
    }

    #[test]
    fn mode1_yields_four_pixels_of_two_bits() {
        let mut ga = GateArray::new();
        ga.write(0b1000_0001);
        let mut out = [0u8; 4];
        assert_eq!(ga.decode_byte(0b1000_1000, &mut out), 4);
        assert_eq!(out[0], 0b11, "bits 7 and 3 both set");
        assert_eq!(out[1..], [0, 0, 0]);
    }

    #[test]
    fn mode2_yields_eight_single_bit_pixels() {
        let mut ga = GateArray::new();
        ga.write(0b1000_0010);
        let mut out = [0u8; 8];
        assert_eq!(ga.decode_byte(0b1010_0001, &mut out), 8);
        assert_eq!(out, [1, 0, 1, 0, 0, 0, 0, 1]);
    }

    #[test]
    fn mode3_is_mode0_width_with_mode1_pens() {
        // The undocumented mode: two pixels like mode 0, but only pens 0-3.
        let mut ga = GateArray::new();
        ga.write(0b1000_0011);
        let mut out = [0u8; 2];
        assert_eq!(ga.decode_byte(0b1000_1000, &mut out), 2);
        assert_eq!(out[0], 0b11);
        assert!(
            out.iter().all(|&p| p < 4),
            "mode 3 cannot reach pens above 3"
        );
    }

    #[test]
    fn a_short_buffer_decodes_nothing() {
        // Better to refuse than to half-fill a scanline.
        let mut ga = GateArray::new();
        ga.write(0b1000_0010); // mode 2 wants 8
        let mut out = [0u8; 4];
        assert_eq!(ga.decode_byte(0xFF, &mut out), 0);
        assert_eq!(out, [0; 4]);
    }

    #[test]
    fn decode_to_rgb_resolves_through_the_palette() {
        let mut ga = GateArray::new();
        ga.write(0b1000_0010); // mode 2
        ga.write(0b0000_0000); // pen 0
        ga.write(0b0100_1000); // colour 8, purple
        ga.write(0b0000_0001); // pen 1
        ga.write(0b0100_1011); // colour 11, bright white
        let mut out = [0u32; 8];
        // Only bit 7 is set, so the first pixel is pen 1 and the rest pen 0.
        assert_eq!(ga.decode_byte_rgb(0b1000_0000, &mut out), 8);
        assert_eq!(out[0], HARDWARE_PALETTE[11], "pen 1 → bright white");
        assert_eq!(out[1], HARDWARE_PALETTE[8], "pen 0 → purple");
        assert!(out[2..].iter().all(|&c| c == HARDWARE_PALETTE[8]));
    }

    #[test]
    fn the_palette_holds_twenty_seven_distinct_colours() {
        // 32 codes, three levels per channel: 3^3 = 27, five duplicates.
        let mut seen: Vec<u32> = HARDWARE_PALETTE.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 27);
    }

    #[test]
    fn every_palette_channel_is_one_of_three_levels() {
        for (code, argb) in HARDWARE_PALETTE.iter().enumerate() {
            for shift in [16, 8, 0] {
                let level = (argb >> shift) & 0xFF;
                assert!(
                    matches!(level, 0x00 | 0x60 | 0xFF),
                    "code {code} has an off-scale channel {level:#04x}"
                );
            }
        }
    }
}
