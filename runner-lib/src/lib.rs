//! Shared runner infrastructure for emulated machines.
//!
//! This crate provides window management, audio output, CRT shader effects,
//! and input handling for any system implementing the `Machine` trait.
//!
//! # Example
//!
//! ```ignore
//! use runner_lib::{run, RunnerConfig};
//! use machine_spectrum::Spectrum48K;
//!
//! fn main() {
//!     let mut machine = Spectrum48K::new();
//!     machine.load_file("48.rom", &rom_data).unwrap();
//!
//!     run(machine, RunnerConfig {
//!         title: "ZX Spectrum 48K".into(),
//!         scale: 3,
//!         crt_enabled: false,
//!     });
//! }
//! ```

mod audio;
mod crt;
mod runner;

pub use runner::{Runner, RunnerConfig, run};
