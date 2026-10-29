//! Sega Master System machine profiles and runtime constructors.
//!
//! Concrete Master System and Mark III boards retain one family catalogue.
//! The Game Gear has its own catalogue and runner; both use the common
//! runtime class, which owns dispatch among their fixed machine crates.

mod profiles;

pub use profiles::{Model, blank, profile_for, profiles, with_cartridge};
pub use runtime_sega_master_system_class::{FmUnit, Sms, SmsSessionQueryProvider, SmsVariant};

/// Shared implementation parameterised by this system's model catalogue.
pub type SmsRuntime = runtime_sega_master_system_class::SmsRuntime<Model>;
