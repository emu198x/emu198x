//! Shared runtime surface for the Sega Master System class of machines.
//!
//! The Master System and the Game Gear are distinct machines — separate
//! `machine_id`s, labels, milestones and release artifacts — built from the
//! same silicon and driven by the same runtime wrapper. This crate holds
//! the half they share so that each machine can own a runtime crate
//! declaring exactly one `machine_id` (#998).
//!
//! What belongs here: anything identical across the class — the
//! `MachineCore` implementation, the query surface, the snapshot envelope,
//! controller input. What does not: the profile catalogue, which is where
//! the machines differ and where their identity is stated.
//!
//! This is the sixth piece of the within-family layering, and it exists
//! only for families shipping more than one machine. See
//! `knowledge/decisions/within-family-layering.md`.

mod input;
mod queries;
mod runtime;
mod snapshot;

pub use machine_sega_master_system::{Sms, SmsVariant};
pub use queries::SmsSessionQueryProvider;
pub use runtime::SmsRuntime;
