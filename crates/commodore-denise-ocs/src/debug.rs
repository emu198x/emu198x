//! Debug instrumentation types exposed by `DeniseOcs` for unit tests and
//! the live machine's pixel-tracing tools.
//!
//! These structs are pure data carriers — every field is `pub` so that
//! tests can pattern-match on individual lanes without bouncing through
//! accessors. None of the values is on the cycle-accurate hot path; they
//! are populated by inspection helpers (`last_shift_load_debug`,
//! `output_pixel_with_beam`, ...) and serialised for snapshot testing.

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeniseSourcePixelDebug {
    pub raw_color_idx: u8,
    pub pf1_code: u8,
    pub pf2_code: u8,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeniseOutputPixelDebug {
    pub called: bool,
    pub beam_x: u32,
    pub beam_y: u32,
    pub requested_x: u32,
    pub requested_y: u32,
    pub hires: bool,
    pub source_pixels_per_fb_pixel: u8,
    pub quad_samples: [DeniseSourcePixelDebug; 4],
    pub plane_bits_mask: u8,
    pub final_color_idx: u8,
    /// Independently-composed color indices for source pixels shifted out
    /// during this output call. SuperHires: 4 unique entries. Hires: [c0, c1,
    /// c1, c1]. Lores: all identical (`final_color_idx`).
    pub quad_color_idx: [u8; 4],
    /// Whether each quad_color_idx entry came from a sprite (true) or
    /// bitplane/background (false). Needed so AGA palette lookup can apply
    /// BPLAM XOR only to bitplane colours, not sprites.
    pub quad_is_sprite: [bool; 4],
    pub playfield_visible_gate: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeniseShiftLoadPlaneDebug {
    pub raw: u16,
    pub prev: u16,
    pub scroll: u8,
    pub combined_hi: u16,
    pub combined_lo: u16,
    pub shift_loaded: u16,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeniseShiftLoadDebug {
    pub hires: bool,
    pub odd_scroll: u8,
    pub even_scroll: u8,
    pub num_bitplanes: u8,
    pub planes: [DeniseShiftLoadPlaneDebug; 3],
}
