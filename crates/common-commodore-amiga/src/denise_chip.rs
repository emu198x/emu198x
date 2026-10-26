//! `DeniseChip` trait — the surface the board-level `Denise` wrapper
//! requires from any concrete Denise variant.
//!
//! The trait and its comparator-phase type are the OCS Denise crate's
//! own, so every variant (OCS Denise, ECS Super Denise, AGA Lisa)
//! implements it in its own crate without depending on this family glue.
//! They are re-exported here so board code keeps its existing paths.
//!
//! See Seam 1 of
//! `knowledge/decisions/amiga-full-family-architecture-review.md`.

pub use emu198x_commodore_denise_ocs::{DeniseChip, HorizontalDiwComparatorPhase};
