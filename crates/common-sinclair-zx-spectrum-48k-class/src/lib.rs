//! Shared 48K-class Spectrum wiring, below the concrete physical board owners.
//!
//! Each machine crate fixes its PCB and fitted ULA. RAM population is the memory
//! type; case and ROM revisions do not create a physical owner. The core saves
//! its board independently so a relabelled envelope cannot choose new wiring.
//! The current ULA backend qualifies functional 5C/6C behaviour; analog video,
//! power circuitry and sub-cycle DRAM propagation margins remain unqualified.

pub mod board;
pub use board::PhysicalBoard;
pub use emu198x_ferranti_ula_6c001e::{FerrantiUla, UlaRevision, UlaTick, hdl_model};
pub use emu198x_peripheral_kempston_sinclair_zx_spectrum_joystick::{
    KempstonButton, KempstonJoystick,
};
pub mod core;
pub mod tape_input;
pub mod variant;

pub use core::SpectrumMachineCore;
pub use tape_input::TapeInput;
pub use variant::Variant48kClass;
