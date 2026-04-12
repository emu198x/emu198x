//! Shared machine-facing error types.

use thiserror::Error;

use crate::media::MediaKind;

/// Error surfaced through the shared machine control boundary.
#[derive(Debug, Error)]
pub enum MachineError {
    /// The requested media kind is not supported by the target machine.
    #[error("media kind {kind:?} is not supported")]
    UnsupportedMediaKind {
        /// The unsupported media kind.
        kind: MediaKind,
    },

    /// The requested media slot does not exist on the target machine.
    #[error("media slot {slot} is not known")]
    UnknownMediaSlot {
        /// The slot identifier supplied by the caller.
        slot: String,
    },

    /// The supplied media bytes failed validation.
    #[error("media for slot {slot} is invalid: {reason}")]
    InvalidMedia {
        /// The slot identifier being loaded.
        slot: String,
        /// Human-readable validation detail.
        reason: String,
    },

    /// The snapshot envelope or payload was rejected.
    #[error("snapshot is invalid: {reason}")]
    InvalidSnapshot {
        /// Human-readable validation detail.
        reason: String,
    },

    /// The requested operation does not exist for the current machine.
    #[error("operation {operation} is not supported")]
    UnsupportedOperation {
        /// Stable operation name.
        operation: &'static str,
    },

    /// Host-facing sinks rejected data emitted by the machine.
    #[error("host interaction failed: {reason}")]
    Host {
        /// Human-readable failure detail.
        reason: String,
    },
}
