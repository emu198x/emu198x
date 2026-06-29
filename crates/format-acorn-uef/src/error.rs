//! Error type for UEF parsing.

use thiserror::Error;

/// Failure modes when parsing a UEF cassette image.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum UefError {
    /// The image is shorter than the mandatory 12-byte preamble (10-byte magic
    /// plus the two version bytes).
    #[error("file is too small to be a UEF image ({0} bytes)")]
    TooSmall(usize),

    /// The 10-byte `UEF File!\0` magic was absent after any gzip layer was
    /// removed.
    #[error("missing UEF magic header")]
    BadMagic,

    /// The gzip wrapper could not be inflated.
    #[error("gzip decompression failed: {0}")]
    Gzip(String),

    /// A chunk header declared a payload that runs past the end of the image.
    #[error(
        "chunk {id:#06x} at offset {offset} claims {length} payload bytes but only {available} remain"
    )]
    TruncatedChunk {
        /// Chunk identifier.
        id: u16,
        /// Offset of the chunk header within the (decompressed) image.
        offset: usize,
        /// Declared payload length.
        length: usize,
        /// Bytes actually remaining after the header.
        available: usize,
    },

    /// A chunk's payload was too short for the fields its identifier requires.
    #[error("chunk {id:#06x} at offset {offset} has a malformed {field}-byte payload")]
    MalformedChunk {
        /// Chunk identifier.
        id: u16,
        /// Offset of the chunk header within the (decompressed) image.
        offset: usize,
        /// Minimum payload size the chunk required.
        field: usize,
    },
}
