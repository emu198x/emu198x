//! Agnus wiring for the ECS machine.
//!
//! Mirror of `machine-commodore-amiga-ocs/src/agnus.rs` but pinned to
//! the ECS Agnus wrapper. `AgnusEcs` from `commodore-agnus-ecs` Derefs
//! to the OCS Agnus inside, so all OCS slot allocation / DMA / copper
//! / blitter logic passes through unchanged. The wrapper carries the
//! BEAMCON0 register, programmable sync generator, and ECS-only
//! extension state.

pub use commodore_agnus_ecs::AgnusEcs;
pub use commodore_agnus_ocs::{
    Agnus, AgnusRegion, BlitterCckOutcome, CckBusPlan, NTSC_CCKS_PER_FRAME, NTSC_LINES_PER_FRAME,
    PAL_CCKS_PER_FRAME, PAL_CCKS_PER_LINE, PAL_LINES_PER_FRAME, SlotOwner, VBL_END_LINE, bits,
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
