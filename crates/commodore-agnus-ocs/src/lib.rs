//! Commodore Agnus OCS — beam counter, DMA controller, copper, and blitter.
//!
//! Agnus is the master DMA controller in the Original Chip Set (OCS). It owns
//! the system bus during DMA slots, generates the beam position counters, and
//! contains the copper coprocessor and blitter sub-units.

mod agnus;
mod copper;

pub use agnus::{Agnus, SlotOwner, PAL_CCKS_PER_LINE, PAL_LINES_PER_FRAME, LOWRES_DDF_TO_PLANE};
pub use copper::{Copper, State as CopperState};
