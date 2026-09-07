//! Program and tape loaders a machine can offer the session.
//!
//! `load_basic_program` and `autoload_tape` are shared script steps, but
//! what they do is machine-bound: the BASIC dialect to tokenise, where
//! the program lives in RAM, which system variables to patch, which keys
//! start the tape. A machine that has a loader offers it through the
//! hooks on [`MachineCore`](crate::MachineCore); the shell reads the
//! file, calls the hook, and reports the outcome the same way for every
//! machine. A machine without one gets the same "system-specific step"
//! refusal it always had.

use thiserror::Error;

/// Outcome of installing one BASIC program in RAM.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BasicProgramLoaded {
    /// Tokenised program length in bytes.
    pub program_bytes: u16,
    /// Whether the loader drove the editor to `RUN` the program.
    pub ran: bool,
}

/// Outcome of one autoload-tape sequence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TapeAutoloaded {
    /// Slot the tape was autoloaded from.
    pub slot: String,
    /// Native frames spent waiting for boot before typing.
    pub boot_frames: u32,
}

/// Why a loader hook could not do its job.
#[derive(Debug, Error)]
pub enum LoaderError {
    /// The machine offers no loader for this step.
    #[error("script step `{step}` requires a system-specific handler")]
    Unsupported {
        /// The script action that was asked for.
        step: &'static str,
    },
    /// The machine's loader ran and failed; the text is its own report.
    #[error("{0}")]
    Failed(String),
}
