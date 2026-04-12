//! Error types for common Spectrum-family components.

use thiserror::Error;

/// Error returned when a ROM image does not match the expected shape.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RomImageError {
    /// The supplied byte slice was not exactly 16 KiB.
    #[error("expected 16384-byte ROM image, got {actual} bytes")]
    WrongSize {
        /// Actual byte length of the supplied ROM image.
        actual: usize,
    },
}
