//! Atari ST Shifter.
//!
//! The Shifter turns the bitplane words the MMU fetches into pixels. It holds
//! the 16-colour palette (`$FF8240`–`$FF825E`, three bits per gun) and the
//! resolution register (`$FF8260`):
//!
//! | Resolution | Planes | Pixels per line | Pixel clock |
//! |---|---|---|---|
//! | 0, low | 4 | 320 | 8 MHz, one per CPU cycle |
//! | 1, medium | 2 | 640 | 16 MHz, two per cycle |
//! | 2, high | 1 | 640 | 32 MHz, four per cycle |
//!
//! The MMU fetches one 16-bit word every four CPU cycles while the display is
//! enabled, in plane order: words 0–3 of a 16-pixel group in low resolution,
//! 0–1 in medium, one word in high. The Shifter collects a group's words in
//! its input registers. When the last plane arrives it moves them into its
//! shift registers, which shift one bit out of each plane per pixel. The four
//! fetches of every 16 cycles thus make 16 low, 32 medium or 64 high pixels,
//! exactly what the pixel clock consumes in those 16 cycles.
//!
//! The host calls [`Shifter::load`] with each fetched word and
//! [`Shifter::tick`] once per CPU cycle with the display-enable line. A tick
//! gives one, two or four pixels as `0x00RRGGBB`. A group's pixels start
//! once its last word has arrived, so the picture runs behind display
//! enable: in low resolution by the 12 cycles its fourth word takes to come.
//! The last group of a line shifts out after display enable has dropped. With no group to shift the Shifter shows the border:
//! palette entry 0 in the colour modes, black in high resolution.
//!
//! In high resolution only palette entry 0's lowest bit matters. It inverts
//! the picture: with it set, as TOS leaves it, a 0 pixel is white and a 1
//! pixel black.
//!
//! # Not modelled
//!
//! - The Shifter's few cycles of pipeline latency beyond the group delay
//!   above, which position the picture a few pixels later on hardware.
//! - Resolution changes mid-line, as overscan and "sync scrolling" tricks use.
//!   A change takes effect at the next group.
//!
//! # Reference
//!
//! Atari ST technical documentation in `198x/reference/by-system/atari-st/`;
//! facts cross-checked against Hatari's `video.c` (GPL-2, not used as a code
//! source).

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// A screen resolution, as `$FF8260` selects it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Resolution {
    Low,
    Medium,
    High,
}

impl Resolution {
    /// The resolution for register value `value`. The reserved value 3
    /// behaves as high resolution.
    #[must_use]
    pub fn from_register(value: u8) -> Self {
        match value & 3 {
            0 => Self::Low,
            1 => Self::Medium,
            _ => Self::High,
        }
    }

    /// Bitplanes per pixel.
    #[must_use]
    pub fn planes(self) -> usize {
        match self {
            Self::Low => 4,
            Self::Medium => 2,
            Self::High => 1,
        }
    }

    /// Pixels the Shifter outputs per CPU cycle.
    #[must_use]
    pub fn pixels_per_cycle(self) -> usize {
        match self {
            Self::Low => 1,
            Self::Medium => 2,
            Self::High => 4,
        }
    }
}

/// The pixels one tick produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pixels {
    pub count: usize,
    pub colours: [u32; 4],
}

impl Pixels {
    /// The produced colours as a slice.
    #[must_use]
    pub fn as_slice(&self) -> &[u32] {
        &self.colours[..self.count]
    }
}

/// The ST Shifter.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Shifter {
    palette: [u16; 16],
    resolution: u8,
    /// Input registers, filled in plane order.
    input: [u16; 4],
    loaded: usize,
    /// Shift registers, one per plane, shifted left a bit per pixel.
    shift: [u16; 4],
    /// Pixels left in the shift registers.
    remaining: u8,
}

impl Default for Shifter {
    fn default() -> Self {
        Self::new()
    }
}

impl Shifter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            palette: [0; 16],
            resolution: 0,
            input: [0; 4],
            loaded: 0,
            shift: [0; 4],
            remaining: 0,
        }
    }

    /// The current resolution.
    #[must_use]
    pub fn resolution(&self) -> Resolution {
        Resolution::from_register(self.resolution)
    }

    /// Read `$FF8260`.
    #[must_use]
    pub fn read_resolution(&self) -> u8 {
        self.resolution & 3
    }

    /// Write `$FF8260`.
    pub fn write_resolution(&mut self, value: u8) {
        self.resolution = value & 3;
    }

    /// Read palette entry `index` (`$FF8240 + 2 × index`). Only the three
    /// bits of each gun exist.
    #[must_use]
    pub fn read_palette(&self, index: usize) -> u16 {
        self.palette[index & 15]
    }

    /// Write palette entry `index`.
    pub fn write_palette(&mut self, index: usize, value: u16) {
        self.palette[index & 15] = value & 0x777;
    }

    /// One byte of a palette entry: `high` selects the even address.
    pub fn write_palette_byte(&mut self, index: usize, high: bool, value: u8) {
        let entry = &mut self.palette[index & 15];
        *entry = if high {
            (*entry & 0x00FF) | (u16::from(value) << 8)
        } else {
            (*entry & 0xFF00) | u16::from(value)
        } & 0x777;
    }

    /// A video word from the MMU.
    pub fn load(&mut self, word: u16) {
        let planes = self.resolution().planes();
        self.input[self.loaded % 4] = word;
        self.loaded += 1;
        if self.loaded >= planes {
            self.shift = self.input;
            self.loaded = 0;
            self.remaining = 16;
        }
    }

    /// One CPU cycle. `display_enable` is the GLUE's DE line.
    pub fn tick(&mut self, display_enable: bool) -> Pixels {
        let resolution = self.resolution();
        let count = resolution.pixels_per_cycle();
        let mut colours = [0; 4];
        for colour in colours.iter_mut().take(count) {
            *colour = if self.remaining > 0 {
                self.next_pixel(resolution)
            } else {
                self.border(resolution)
            };
        }
        if !display_enable {
            // Between lines the input registers start a fresh group. A group
            // already loaded still shifts out: the picture runs a group's
            // length behind display enable.
            self.loaded = 0;
        }
        Pixels { count, colours }
    }

    fn next_pixel(&mut self, resolution: Resolution) -> u32 {
        let planes = resolution.planes();
        let mut index = 0;
        for plane in (0..planes).rev() {
            index = index << 1 | usize::from(self.shift[plane] >> 15);
        }
        for register in self.shift.iter_mut().take(planes) {
            *register <<= 1;
        }
        self.remaining -= 1;
        match resolution {
            Resolution::High => self.mono(index as u16),
            _ => rgb(self.palette[index]),
        }
    }

    fn border(&self, resolution: Resolution) -> u32 {
        match resolution {
            Resolution::High => 0,
            _ => rgb(self.palette[0]),
        }
    }

    fn mono(&self, pixel: u16) -> u32 {
        if (pixel ^ self.palette[0]) & 1 != 0 {
            0x00FF_FFFF
        } else {
            0
        }
    }
}

/// An ST palette entry as `0x00RRGGBB`, each three-bit gun scaled to eight.
#[must_use]
pub fn rgb(entry: u16) -> u32 {
    let scale = |gun: u16| u32::from(gun & 7) * 255 / 7;
    scale(entry >> 8) << 16 | scale(entry >> 4) << 8 | scale(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(shifter: &mut Shifter, cycles: usize) -> Vec<u32> {
        (0..cycles)
            .flat_map(|_| shifter.tick(true).as_slice().to_vec())
            .collect()
    }

    #[test]
    fn low_resolution_reads_four_planes_msb_first() {
        let mut s = Shifter::new();
        s.write_palette(0, 0x000);
        s.write_palette(1, 0x700);
        s.write_palette(2, 0x070);
        s.write_palette(8, 0x007);
        s.write_palette(15, 0x777);
        s.load(0x8001); // plane 0: pixels 0 and 15
        s.load(0x4001); // plane 1: pixels 1 and 15
        s.load(0x0001); // plane 2: pixel 15
        s.load(0x2001); // plane 3: pixels 2 and 15
        let px = run(&mut s, 16);
        assert_eq!(px[0], rgb(0x700));
        assert_eq!(px[1], rgb(0x070));
        assert_eq!(px[2], rgb(0x007));
        assert_eq!(px[3], 0);
        assert_eq!(px[15], 0xFF_FFFF);
    }

    #[test]
    fn medium_resolution_makes_two_pixels_a_cycle_from_two_planes() {
        let mut s = Shifter::new();
        s.write_resolution(1);
        s.write_palette(3, 0x777);
        s.load(0xC000);
        s.load(0x8000);
        let px = run(&mut s, 8);
        assert_eq!(px.len(), 16);
        assert_eq!(px[0], 0xFF_FFFF, "both planes set: colour 3");
        assert_eq!(px[1], rgb(0), "plane 0 only: colour 1, unset");
    }

    #[test]
    fn high_resolution_inverts_with_palette_bit_zero() {
        let mut s = Shifter::new();
        s.write_resolution(2);
        s.write_palette(0, 0x777);
        s.load(0x8000);
        let px = run(&mut s, 4);
        assert_eq!(px.len(), 16);
        assert_eq!(px[0], 0, "a 1 pixel is black on TOS's white paper");
        assert_eq!(px[1], 0xFF_FFFF);
        s.write_palette(0, 0x000);
        s.load(0x8000);
        assert_eq!(run(&mut s, 1)[0], 0xFF_FFFF);
    }

    #[test]
    fn the_border_is_palette_zero_and_black_in_high() {
        let mut s = Shifter::new();
        s.write_palette(0, 0x123);
        assert_eq!(s.tick(false).as_slice(), &[rgb(0x123)]);
        s.write_resolution(2);
        assert_eq!(s.tick(false).as_slice(), &[0, 0, 0, 0]);
    }

    #[test]
    fn a_loaded_group_finishes_after_display_enable_drops() {
        let mut s = Shifter::new();
        s.write_palette(0, 0x000);
        s.write_palette(1, 0x700);
        for word in [0xFFFF, 0, 0, 0] {
            s.load(word);
        }
        let pixels: Vec<u32> = (0..17).map(|_| s.tick(false).as_slice()[0]).collect();
        assert!(pixels[..16].iter().all(|&p| p == rgb(0x700)));
        assert_eq!(pixels[16], rgb(0), "then the border");
    }

    #[test]
    fn a_partial_group_is_discarded_at_the_end_of_the_line() {
        let mut s = Shifter::new();
        s.write_palette(1, 0x700);
        s.load(0xFFFF);
        s.tick(false);
        s.load(0x0000);
        s.load(0x0000);
        s.load(0x0000);
        s.load(0x0000);
        assert_eq!(run(&mut s, 1)[0], rgb(0), "the stale plane 0 was dropped");
    }

    #[test]
    fn palette_registers_keep_three_bits_per_gun() {
        let mut s = Shifter::new();
        s.write_palette(4, 0xFFFF);
        assert_eq!(s.read_palette(4), 0x777);
        s.write_palette_byte(5, true, 0x07);
        s.write_palette_byte(5, false, 0x5F);
        assert_eq!(s.read_palette(5), 0x757);
        assert_eq!(rgb(0x700), 0xFF_0000);
        assert_eq!(rgb(0x007), 0x00_00FF);
        s.write_resolution(0xFF);
        assert_eq!(s.read_resolution(), 3);
        assert_eq!(s.resolution(), Resolution::High);
    }

    #[test]
    fn state_round_trips_through_serde() {
        let mut s = Shifter::new();
        s.write_palette(1, 0x777);
        s.load(0x8000);
        let json = serde_json::to_string(&s).expect("serde round trip");
        let mut back: Shifter = serde_json::from_str(&json).expect("serde round trip");
        back.load(0);
        back.load(0);
        back.load(0);
        assert_eq!(back.tick(true).as_slice(), &[0xFF_FFFF]);
    }
}
