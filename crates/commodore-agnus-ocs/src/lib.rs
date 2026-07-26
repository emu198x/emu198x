//! Commodore Agnus OCS — beam counter, DMA controller, copper, and blitter.
//!
//! Agnus is the master DMA controller in the Original Chip Set (OCS). It owns
//! the system bus during DMA slots, generates the beam position counters, and
//! contains the copper coprocessor and blitter sub-units.

mod agnus;
mod copper;

pub use agnus::bits;
pub use agnus::{
    Agnus, AgnusRegion, BlitterBus, BlitterDmaOp, BlitterProgress, CckBusPlan, HIRES_DDF_TO_PLANE,
    LOWRES_DDF_TO_PLANE, LOWRES_DDF_TO_PLANE_AGA, NTSC_CCKS_PER_FRAME, NTSC_CCKS_PER_LINE_LONG,
    NTSC_CCKS_PER_LINE_SHORT, NTSC_LINES_PER_FRAME, NTSC_VBL_END_LINE, OriginalAgnusRevision,
    PAL_CCKS_PER_FRAME, PAL_CCKS_PER_LINE, PAL_LINES_PER_FRAME, PAL_VBL_END_LINE,
    PaulaReturnProgressPolicy, SlotOwner, SpriteDmaVerticalTiming, VBL_END_LINE,
};
pub use copper::{Copper, State as CopperState};
