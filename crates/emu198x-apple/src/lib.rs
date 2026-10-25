//! C interface over the Emu198x runtime catalogue, for native Apple hosts.
//!
//! Swift links the static library and calls the functions declared in
//! `include/emu198x.h`. Each one forwards to the matching [`Fleet`] method,
//! so a native host runs exactly the emulation the browser player runs.
//!
//! The workspace forbids unsafe code. This crate is one of two exceptions, and
//! only its `ffi` module may use it. See
//! `knowledge/decisions/apple-bridge-unsafe-exception.md`.

use std::collections::VecDeque;

use emu198x_fleet_web::Fleet;

#[allow(unsafe_code)]
pub mod ffi;

pub use ffi::Emu198xBuffer;

/// Status codes returned by every C function. `include/emu198x.h` defines
/// the same values, and `tests/header.rs` checks that they agree.
pub mod status {
    /// The call succeeded.
    pub const OK: i32 = 0;
    /// The emulator refused the call; `emu198x_last_error` says why.
    pub const ERROR: i32 = 1;
    /// A pointer, string or number the caller passed was unusable.
    pub const INVALID_ARGUMENT: i32 = 2;
    /// The emulator panicked during this call. The machine is now poisoned.
    pub const PANICKED: i32 = 3;
    /// The machine panicked earlier and refuses further work.
    pub const POISONED: i32 = 4;
}

/// One machine, plus audio it has produced that the host has not yet read.
pub struct Emu198xMachine {
    fleet: Fleet,
    audio: VecDeque<f32>,
    /// One second of interleaved stereo at the configured rate.
    audio_limit: usize,
    poisoned: bool,
}

impl Emu198xMachine {
    fn new(family: &str, variant: &str) -> Result<Self, String> {
        Ok(Self {
            fleet: Fleet::new(family, variant)?,
            audio: VecDeque::new(),
            audio_limit: 0,
            poisoned: false,
        })
    }

    fn configure_audio(&mut self, rate: u32) -> Result<(), String> {
        self.fleet.configure_audio(rate)?;
        self.audio_limit = usize::try_from(rate).map_or(usize::MAX, |rate| rate * 2);
        Ok(())
    }

    /// Copies whole stereo frames into `out` and returns how many samples
    /// it wrote. Samples that do not fit stay queued for the next call; past
    /// one second the oldest are dropped, so a host that stops reading
    /// cannot grow the queue without bound.
    fn read_audio(&mut self, out: &mut [f32]) -> Result<usize, String> {
        self.audio.extend(self.fleet.audio()?);
        let excess = self.audio.len().saturating_sub(self.audio_limit);
        let dropped = (excess + excess % 2).min(self.audio.len());
        self.audio.drain(..dropped);
        let count = out.len().min(self.audio.len()) & !1;
        for (slot, sample) in out.iter_mut().zip(self.audio.drain(..count)) {
            *slot = sample;
        }
        Ok(count)
    }
}
