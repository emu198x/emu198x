//! AGA Denise facade — type alias over the generic board-level
//! wrapper in `common_commodore_amiga::denise`. Concrete chip is
//! [`commodore_denise_aga::DeniseAga`] (Lisa).
//!
//! All shared helpers (`DmaClaim`, `dma_claim`, etc.) re-exported
//! from the substrate for callers that import via `crate::denise::*`.

use commodore_denise_aga::DeniseAga;
pub use common_commodore_amiga::denise::{FB_HEIGHT, FB_WIDTH, dma_claim};

pub type Denise = common_commodore_amiga::denise::Denise<DeniseAga>;
