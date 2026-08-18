//! Sega Game Gear machine profiles and runtime constructors.
//!
//! The Game Gear is a Master System in a handheld shell: same Z80A, same
//! Sega VDP cropped to a 160×144 LCD, same SN76489 given stereo output. It
//! shipped from the Master System's crate until #998, which made it the one
//! machine no crate-derived view of the portfolio could see. The runtime the
//! two machines share lives in `runtime-sega-master-system-class`.

mod profiles;

pub use profiles::{Model, blank, profile_for, profiles, with_cartridge};
pub use runtime_sega_master_system_class::{Sms, SmsRuntime, SmsSessionQueryProvider, SmsVariant};
