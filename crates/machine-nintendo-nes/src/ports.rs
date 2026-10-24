//! What is plugged into the controller ports: two pads, a pad and a
//! Zapper, or a Four Score with four pads.
//!
//! # Four Score
//!
//! The Four Score sits on both ports and stretches each port's serial
//! read to 24 bits: `$4016` gives pad 1, then pad 3, then a signature;
//! `$4017` gives pad 2, then pad 4, then its own signature. The
//! signatures, in read order, are `0 0 0 1 0 0 0 0` on `$4016` and
//! `0 0 1 0 0 0 0 0` on `$4017`, which is how a game knows the adapter
//! is there. Reads past the 24th return 1.
//!
//! # Zapper
//!
//! The Zapper in port 2 reads on `$4017` bit 3, low while its
//! photodiode sees light, and bit 4, high while the trigger is held. Its
//! sensor responds to a bright patch of screen for a short while after
//! the beam passes it: here the light is seen when any pixel within
//! [`ZAPPER_RADIUS`] of the aim point has been drawn bright in the last
//! [`ZAPPER_PERSISTENCE_LINES`] scanlines.
//!
//! Facts from the NESdev wiki "Four Score" and "Zapper" pages and
//! Mesen2's `FourScore.h` and `Zapper.h` (GPL-3, read for behaviour only).

use serde::{Deserialize, Serialize};

/// Pixels around the aim point the Zapper's sensor sees.
pub const ZAPPER_RADIUS: i32 = 2;

/// Scanlines after the beam passes a pixel that the sensor still sees it.
pub const ZAPPER_PERSISTENCE_LINES: i32 = 20;

/// The luma, out of 255, a pixel needs for the sensor to register it.
pub const ZAPPER_BRIGHTNESS: u32 = 85;

/// What is plugged into the controller ports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InputDevices {
    /// A standard pad in each port.
    #[default]
    Controllers,
    /// A pad in port 1 and a Zapper in port 2.
    Zapper,
    /// A Four Score with four pads.
    FourScore,
}

/// Port state beyond the two standard pads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Ports {
    pub(crate) devices: InputDevices,
    pub(crate) controller3: u8,
    pub(crate) controller4: u8,
    four_score_shift: [u32; 2],
    pub(crate) zapper_aim: Option<(u16, u16)>,
    pub(crate) zapper_trigger: bool,
}

impl Ports {
    /// Latch the Four Score's shift registers on the strobe's falling
    /// edge.
    pub(crate) fn latch(&mut self, pad1: u8, pad2: u8) {
        let tail = 0xFF00_0000;
        self.four_score_shift = [
            u32::from(pad1) | (u32::from(self.controller3) << 8) | (0x08 << 16) | tail,
            u32::from(pad2) | (u32::from(self.controller4) << 8) | (0x04 << 16) | tail,
        ];
    }

    /// One Four Score read on port 0 (`$4016`) or 1 (`$4017`). While
    /// the strobe is high the port keeps returning the first pad's A.
    pub(crate) fn read_four_score(&mut self, port: usize, strobe: bool, pad: u8) -> u8 {
        if strobe {
            return pad & 1;
        }
        let bit = (self.four_score_shift[port] & 1) as u8;
        self.four_score_shift[port] = (self.four_score_shift[port] >> 1) | 0x8000_0000;
        bit
    }
}

/// The Zapper's bits on `$4017`: bit 3 clear when it sees light, bit 4
/// set while the trigger is held.
///
/// `framebuffer` is the PPU's 256-wide ARGB picture, which holds the
/// current frame above the beam and the previous one below it; `scanline`
/// and `dot` are where the beam is.
#[must_use]
pub fn zapper_bits(
    aim: Option<(u16, u16)>,
    trigger: bool,
    framebuffer: &[u32],
    scanline: u16,
    dot: u16,
) -> u8 {
    let light = aim.is_some_and(|aim| sees_light(aim, framebuffer, scanline, dot));
    (if light { 0 } else { 0x08 }) | (if trigger { 0x10 } else { 0 })
}

fn sees_light(aim: (u16, u16), framebuffer: &[u32], scanline: u16, dot: u16) -> bool {
    const WIDTH: i32 = 256;
    let height = i32::try_from(framebuffer.len()).unwrap_or(0) / WIDTH;
    let (scanline, dot) = (i32::from(scanline), i32::from(dot));
    let (aim_x, aim_y) = (i32::from(aim.0), i32::from(aim.1));
    for y in aim_y - ZAPPER_RADIUS..=aim_y + ZAPPER_RADIUS {
        // Drawn this frame, recently enough to still glow.
        if !(0..height).contains(&y) || scanline < y || scanline - y > ZAPPER_PERSISTENCE_LINES {
            continue;
        }
        for x in aim_x - ZAPPER_RADIUS..=aim_x + ZAPPER_RADIUS {
            if !(0..WIDTH).contains(&x) || (scanline == y && dot <= x) {
                continue;
            }
            let Ok(index) = usize::try_from(y * WIDTH + x) else {
                continue;
            };
            if luma(framebuffer[index]) >= ZAPPER_BRIGHTNESS {
                return true;
            }
        }
    }
    false
}

/// Rec. 601 luma of an ARGB pixel, 0 to 255.
fn luma(pixel: u32) -> u32 {
    let r = (pixel >> 16) & 0xFF;
    let g = (pixel >> 8) & 0xFF;
    let b = pixel & 0xFF;
    (r * 299 + g * 587 + b * 114) / 1000
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(bright: (usize, usize)) -> Vec<u32> {
        let mut fb = vec![0xFF00_0000; 256 * 240];
        fb[bright.1 * 256 + bright.0] = 0xFFFF_FFFF;
        fb
    }

    #[test]
    fn the_zapper_sees_a_bright_pixel_only_soon_after_the_beam() {
        let fb = picture((100, 50));
        let aim = Some((101, 51));
        assert_eq!(zapper_bits(aim, false, &fb, 49, 0), 0x08, "not drawn yet");
        assert_eq!(
            zapper_bits(aim, false, &fb, 50, 100),
            0x08,
            "the beam is on it"
        );
        assert_eq!(zapper_bits(aim, false, &fb, 50, 101), 0x00, "just drawn");
        assert_eq!(zapper_bits(aim, false, &fb, 70, 0), 0x00, "still glowing");
        assert_eq!(zapper_bits(aim, false, &fb, 71, 0), 0x08, "faded");
        assert_eq!(zapper_bits(aim, true, &fb, 60, 0), 0x10, "trigger");
        assert_eq!(zapper_bits(None, true, &fb, 60, 0), 0x18, "aimed away");
        assert_eq!(
            zapper_bits(Some((110, 51)), false, &fb, 60, 0),
            0x08,
            "too far"
        );
    }

    #[test]
    fn dark_pixels_are_not_light() {
        let mut fb = picture((0, 0));
        fb[0] = 0xFF40_4040;
        assert_eq!(zapper_bits(Some((0, 0)), false, &fb, 5, 0), 0x08);
        assert_eq!(luma(0xFF55_5555), 85);
    }

    #[test]
    fn the_four_score_reads_two_pads_then_its_signature() {
        let mut ports = Ports {
            devices: InputDevices::FourScore,
            controller3: 0x81,
            controller4: 0x02,
            ..Ports::default()
        };
        ports.latch(0x01, 0x80);
        let read = |ports: &mut Ports, port: usize| -> Vec<u8> {
            (0..26)
                .map(|_| ports.read_four_score(port, false, 0))
                .collect()
        };
        let p0 = read(&mut ports, 0);
        assert_eq!(&p0[..8], &[1, 0, 0, 0, 0, 0, 0, 0], "pad 1: A");
        assert_eq!(&p0[8..16], &[1, 0, 0, 0, 0, 0, 0, 1], "pad 3: A and Right");
        assert_eq!(&p0[16..24], &[0, 0, 0, 1, 0, 0, 0, 0], "signature");
        assert_eq!(&p0[24..], &[1, 1]);
        let p1 = read(&mut ports, 1);
        assert_eq!(&p1[..8], &[0, 0, 0, 0, 0, 0, 0, 1], "pad 2: Right");
        assert_eq!(&p1[8..16], &[0, 1, 0, 0, 0, 0, 0, 0], "pad 4: B");
        assert_eq!(&p1[16..24], &[0, 0, 1, 0, 0, 0, 0, 0], "signature");
        assert_eq!(ports.read_four_score(0, true, 0x01), 1, "strobe high");
    }
}
