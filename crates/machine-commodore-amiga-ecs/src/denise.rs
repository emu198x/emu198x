//! ECS Denise facade — type alias over the generic board-level
//! wrapper in `common_commodore_amiga::denise`. Concrete chip is
//! [`commodore_denise_ecs::DeniseEcs`].
//!
//! All shared helpers (`DmaClaim`, `dma_claim`, etc.) re-exported
//! from the substrate for callers that import via `crate::denise::*`.

pub use common_commodore_amiga::denise::{FB_HEIGHT, FB_WIDTH, dma_claim};
use commodore_denise_ecs::DeniseEcs;

pub type Denise = common_commodore_amiga::denise::Denise<DeniseEcs>;
