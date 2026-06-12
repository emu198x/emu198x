//! AGA Denise facade — type alias over the generic board-level
//! wrapper in `common_commodore_amiga::denise`. Concrete chip is
//! [`commodore_denise_aga::DeniseAga`] (Lisa).
//!
//! Shared helpers (`ddf_window`, etc.) re-exported from the substrate
//! for callers that import via `crate::denise::*`. The per-CCK DMA slot
//! arbitration lives in Agnus (`current_slot` / `cck_bus_plan`, #30)
//! and is consumed from the shared `AmigaDriver` body, not here.

use commodore_denise_aga::DeniseAga;
pub use common_commodore_amiga::denise::{FB_HEIGHT, FB_WIDTH};

pub type Denise = common_commodore_amiga::denise::Denise<DeniseAga>;
