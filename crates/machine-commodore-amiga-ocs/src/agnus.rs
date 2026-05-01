//! Agnus wiring for the OCS machine.
//!
//! As of task #139 (port session 2026-04-20), the Agnus implementation
//! lives in the standalone `commodore-agnus-ocs` crate. This module
//! re-exports the chip type and provides a few derived constants the
//! machine uses to convert between CCK time (Agnus's native unit) and
//! master/4 ticks (the machine's primary clock).

pub use commodore_agnus_ocs::{
    Agnus, AgnusRegion, CckBusPlan, NTSC_CCKS_PER_FRAME, NTSC_LINES_PER_FRAME, PAL_CCKS_PER_FRAME,
    PAL_CCKS_PER_LINE, PAL_LINES_PER_FRAME, SlotOwner, VBL_END_LINE, bits,
};

/// PAL line length in CCKs — alias for the chip-crate constant under
/// the name the machine has historically used.
pub const PAL_LINE_CCKS: u16 = PAL_CCKS_PER_LINE;

/// PAL frame line count.
pub const PAL_FRAME_LINES: u16 = PAL_LINES_PER_FRAME;

/// PAL line length in master/4 ticks (= lores pixels = 68000 CPU
/// clocks). One CCK = 2 ticks.
pub const PAL_LINE_TICKS: u16 = PAL_CCKS_PER_LINE * 2;

/// PAL frame length in master/4 ticks: 227 × 312 × 2 = 141,648.
pub const PAL_FRAME_TICKS: u64 = (PAL_LINE_TICKS as u64) * (PAL_FRAME_LINES as u64);

/// NTSC frame length in master/4 ticks. NTSC alternates short (227
/// CCK) and long (228 CCK) lines per HRM p. 785, so the frame total
/// is 131 × 227 + 131 × 228 = 59,605 CCK = 119,210 ticks.
pub const NTSC_FRAME_TICKS: u64 = (NTSC_CCKS_PER_FRAME as u64) * 2;
