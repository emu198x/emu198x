//! The surface a machine-driving helper needs, independent of who is driving.
//!
//! Helpers like the Spectrum's tape autoload are written against a session
//! because that is what existed when they were written, not because they need
//! one. What they actually use is small: run frames, queue keys, send a
//! control command, ask the machine a question, read the clock.
//!
//! Naming that surface lets the browser run the same helper. The alternative
//! considered was giving the browser host a [`HeadlessSession`]; it owns its
//! frame and audio sinks and its audio buffer retains the whole session, which
//! is a leak in a tab that stays open. A trait costs nothing and keeps the two
//! hosts' capture policies their own.
//!
//! [`HeadlessSession`]: crate::HeadlessSession

use crate::control::ControlCommand;
use crate::host::InputEvent;
use crate::machine::RunResult;
use crate::query::{QueryError, QueryResult};
use crate::session::{BootWaitResult, SessionError};
use crate::time::MachineTime;

/// A live machine something can drive frame by frame.
///
/// Five methods, deliberately. Everything else a helper needs — whether the
/// machine has booted, whether a tape is loaded — is a query path, so it
/// stays the machine's own vocabulary rather than becoming trait surface that
/// every host must grow as helpers ask new questions.
pub trait SessionDriver {
    /// The machine's current time.
    fn time(&self) -> MachineTime;

    /// Resolves one query path against the driver's current state.
    ///
    /// # Errors
    ///
    /// Returns [`QueryError`] if the path is unknown or unavailable.
    fn query(&self, path: &str) -> Result<QueryResult, QueryError>;

    /// Queues an input event for the next frame.
    fn queue_input(&mut self, event: InputEvent);

    /// Applies one control command to the machine.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the machine rejects the command.
    fn command(&mut self, command: &ControlCommand) -> Result<(), SessionError>;

    /// Runs `count` native frames.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the machine rejects a run.
    fn run_frames(&mut self, count: u32) -> Result<RunResult, SessionError>;

    /// Reads one query path that answers yes or no.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the path is unknown or is not a boolean.
    fn query_bool(&self, path: &str) -> Result<bool, SessionError> {
        let result = self.query(path)?;
        result
            .value
            .as_bool()
            .ok_or_else(|| SessionError::UnexpectedQueryValue {
                path: path.to_owned(),
                expected: "a boolean",
            })
    }

    /// Runs native frames until the machine reports `boot.detected = true`.
    ///
    /// Provided, so both hosts wait for boot the same way. A machine already
    /// booted returns immediately having run nothing.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if a run fails, if the runtime does not expose
    /// the generic `boot.*` paths, if those resolve to unexpected shapes, or
    /// if `max_frames` expires first.
    fn wait_for_boot(&mut self, max_frames: u32) -> Result<BootWaitResult, SessionError> {
        let mut state = boot_query_state(self)?;
        if state.detected {
            return Ok(BootWaitResult {
                frames: 0,
                reached: self.time(),
                reason: state.reason,
                row: state.row,
            });
        }

        for frames in 1..=max_frames {
            let result = self.run_frames(1)?;
            state = boot_query_state(self)?;
            if state.detected {
                return Ok(BootWaitResult {
                    frames,
                    reached: result.reached,
                    reason: state.reason,
                    row: state.row,
                });
            }
        }

        Err(SessionError::BootTimeout {
            max_frames,
            reason: state.reason,
        })
    }
}

/// The `boot.*` paths read together, so a caller sees one consistent answer.
pub(crate) struct BootQueryState {
    pub(crate) detected: bool,
    pub(crate) reason: String,
    pub(crate) row: Option<u64>,
}

pub(crate) fn boot_query_state<D>(driver: &D) -> Result<BootQueryState, SessionError>
where
    D: SessionDriver + ?Sized,
{
    let detected = driver.query_bool("boot.detected")?;
    let reason = optional_query_string(driver, "boot.reason")
        .unwrap_or_else(|| "boot.detected remained false".to_owned());
    let row = optional_query_u64(driver, "boot.row");

    Ok(BootQueryState {
        detected,
        reason,
        row,
    })
}

/// A path the runtime does not publish is absent, not an error: `boot.reason`
/// and `boot.row` are commentary on `boot.detected`, and a runtime that
/// answers the question without narrating it is not broken.
fn optional_query_string<D>(driver: &D, path: &str) -> Option<String>
where
    D: SessionDriver + ?Sized,
{
    match driver.query(path) {
        Ok(result) => result.value.as_str().map(str::to_owned),
        Err(QueryError::UnknownPath { .. } | QueryError::UnavailablePath { .. }) => None,
    }
}

fn optional_query_u64<D>(driver: &D, path: &str) -> Option<u64>
where
    D: SessionDriver + ?Sized,
{
    match driver.query(path) {
        Ok(result) => result.value.as_u64(),
        Err(QueryError::UnknownPath { .. } | QueryError::UnavailablePath { .. }) => None,
    }
}
