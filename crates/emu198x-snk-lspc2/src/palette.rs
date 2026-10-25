//! Palette RAM colours to RGB.
//!
//! Each colour word drives three 5-bit resistor DACs. Bits 11-8, 7-4 and 3-0
//! are the high four bits of red, green and blue; bits 14, 13 and 12 are
//! their low bits. The DAC resistors are 220, 470, 1k, 2.2k and 3.9k ohms,
//! high bit to low. Bit 15, the "dark" bit, adds an 8.2k pull-down to all
//! three outputs, and the board's shadow line adds a 150 ohm one.
//!
//! The levels are the voltages that network produces, scaled so that full
//! intensity with neither pull-down is 255, following MAME's Neo Geo driver
//! (`neogeo_v.cpp`, `create_rgb_lookups`, BSD-3-Clause).

/// Ohms, low bit first.
const RESISTORS: [f64; 5] = [3900.0, 2200.0, 1000.0, 470.0, 220.0];
const DARK_PULLDOWN: f64 = 8200.0;
const SHADOW_PULLDOWN: f64 = 150.0;

/// One DAC level table for a pull-down conductance (0 for none).
pub(crate) const fn levels(pulldown: f64) -> [u8; 32] {
    let mut total = 0.0;
    let mut i = 0;
    while i < 5 {
        total += 1.0 / RESISTORS[i];
        i += 1;
    }
    let mut table = [0; 32];
    let mut value = 0;
    while value < 32 {
        let mut sum = 0.0;
        let mut bit = 0;
        while bit < 5 {
            if value & (1 << bit) != 0 {
                sum += 255.0 * (1.0 / RESISTORS[bit]) / (total + pulldown);
            }
            bit += 1;
        }
        // Round to nearest, as MAME's combine_weights does.
        table[value] = (sum + 0.5) as u8;
        value += 1;
    }
    table
}

/// Levels indexed by `[dark + 2 * shadow][5-bit intensity]`.
pub(crate) const LEVELS: [[u8; 32]; 4] = [
    levels(0.0),
    levels(1.0 / DARK_PULLDOWN),
    levels(1.0 / SHADOW_PULLDOWN),
    levels(1.0 / DARK_PULLDOWN + 1.0 / SHADOW_PULLDOWN),
];

/// The ARGB colour a palette word shows, with or without the shadow line.
#[must_use]
pub fn argb(word: u16, shadow: bool) -> u32 {
    let table = &LEVELS[usize::from(word >> 15) + 2 * usize::from(shadow)];
    let r = ((word >> 14) & 1) | ((word >> 7) & 0x1e);
    let g = ((word >> 13) & 1) | ((word >> 3) & 0x1e);
    let b = ((word >> 12) & 1) | ((word << 1) & 0x1e);
    0xff00_0000
        | (u32::from(table[usize::from(r)]) << 16)
        | (u32::from(table[usize::from(g)]) << 8)
        | u32::from(table[usize::from(b)])
}
