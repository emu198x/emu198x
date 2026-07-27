//! Shared board-level substrate for Commodore Amiga machine crates.
//!
//! Holds the modules that are identical (or near-identical) across
//! the OCS / ECS / AGA chipset family: the CIA-A / CIA-B pair, the
//! copper, the RTC, the memory map, and the chipset-agnostic parts
//! of the Denise-side rendering.
//!
//! Per-chipset machine crates (`machine-commodore-amiga-ocs`,
//! `machine-commodore-amiga-ecs`, future `-aga`) plug their
//! specific Agnus + Denise chips on top of this substrate and add
//! chipset-specific wiring in their own `lib.rs`.
//!
//! Mirrors the Spectrum's `common-sinclair-zx-spectrum` substrate
//! pattern. See Seam 1 of
//! `knowledge/decisions/amiga-full-family-architecture-review.md`.

pub mod board;
pub mod cia;
pub mod copper;
pub mod denise;
pub mod denise_chip;
pub mod driver;
pub mod memory;
pub mod rtc;

pub use board::{
    BusResponse, BusTransaction, CIA_E_CLOCK_DIVISOR, ChipRamBus, SizedBusResponse,
    SizedBusTransaction, TICKS_PER_CCK,
};
pub use denise_chip::DeniseChip;
pub use driver::AmigaDriver;
