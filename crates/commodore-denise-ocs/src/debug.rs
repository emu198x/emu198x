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

/// Side-effect-free view of Denise's bitplane holding, shifting and
/// wide-fetch pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeniseBitplaneDiagnosticSnapshot {
    pub holding_data: [u16; 8],
    pub shift_data: [u16; 8],
    pub aggregate_shift_count: u8,
    pub shift_counts: [u8; 8],
    pub shift_delays: [u8; 8],
    pub previous_data: [u16; 8],
    pub pending_data: [u16; 8],
    pub pending_copy_odd_planes: bool,
    pub pending_copy_even_planes: bool,
    pub scroll_pending_line: bool,
    pub active_fifo: [[u16; 4]; 8],
    pub active_fifo_lengths: [u8; 8],
    pub staged_fetch_tails: [[u16; 3]; 8],
    pub staged_fetch_tail_lengths: [u8; 8],
    pub deferred_shift_load_source_pixels: Option<u8>,
}

/// Side-effect-free view of one Denise sprite comparator and shifter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeniseSpriteDiagnosticSnapshot {
    pub position: u16,
    pub display_position: u16,
    pub position_dirty: bool,
    pub control: u16,
    pub data: u64,
    pub data_b: u64,
    pub armed: bool,
    pub shift_data: u64,
    pub shift_data_b: u64,
    pub shift_count: u8,
    pub current_code: u8,
    pub pixels_rendered: u64,
}

/// Complete read-only snapshot of the implemented OCS Denise rendering core.
///
/// The framebuffer pixels themselves remain available through the existing
/// framebuffer surface; this snapshot reports their dimensions and length
/// while exposing all pipeline state that affects future output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeniseDiagnosticSnapshot {
    pub palette_12: [u16; 32],
    #[serde(with = "serde_big_array::BigArray")]
    pub palette_24: [u32; 256],
    pub raster_width: u32,
    pub raster_height: u32,
    pub framebuffer_pixels: usize,
    pub interlace_active: bool,
    pub long_frame: bool,
    pub maximum_bitplanes: u8,
    pub active_bitplanes: usize,
    pub bplcon0: u16,
    pub bplcon1: u16,
    pub bplcon2: u16,
    pub bplcon4: u16,
    pub clxcon: u16,
    pub clxdat: u16,
    pub bitplanes: DeniseBitplaneDiagnosticSnapshot,
    pub sprite_width: u8,
    pub sprites: [DeniseSpriteDiagnosticSnapshot; 8],
    pub sprite_bpl1dat_enabled: bool,
    pub sprite_runtime_line_valid: bool,
    pub sprite_runtime_beam_x: u32,
    pub sprite_runtime_beam_y: u32,
    pub ham_previous_rgb12: u16,
    pub ham_previous_rgb24: u32,
    pub last_shift_load: DeniseShiftLoadDebug,
}
