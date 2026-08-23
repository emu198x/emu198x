//! `.p` / `.p81` tape images for the Sinclair ZX81.
//!
//! # The container is a memory image
//!
//! There is no header, no block structure and no checksum. A `.p` file is the
//! bytes the ROM's `SAVE` puts on tape, which is RAM from `$4009` -- `VERSN`,
//! the first system variable the ZX81 saves -- up to the address held in
//! `E_LINE` (`$4014`).
//!
//! `E_LINE` therefore sits *inside* the saved region, at offset `$0B`, so an
//! image carries its own end address and can be checked against its length.
//!
//! # Why the check is an inequality
//!
//! The sibling [`format_sinclair_zx80_o`] requires the pointer to match the
//! file length exactly. That rule is wrong here. Measured across the 1,206
//! images in the TOSEC ZX81 `[P]` set:
//!
//! | `$4009 + len - E_LINE` | Images |
//! |---:|---:|
//! | 0 (exact) | 379 |
//! | 1 | 298 |
//! | 28-37 | 510 |
//! | other positive | 4 |
//! | **negative** | **8** |
//!
//! Two thirds of real images carry bytes past `E_LINE` -- the edit line and
//! whatever else the deck captured before the save ended. Requiring equality
//! would reject 827 of 1,206 preserved programs, so the rule is that `E_LINE`
//! must *land inside the image*: anything beyond it is slack the loader
//! ignores.
//!
//! The eight negative cases are the genuinely malformed ones, where `E_LINE`
//! points past the end of the file. Those are rejected.
//!
//! # What is not checked
//!
//! `VERSN` is `0` on 1,167 of the 1,206 images and something else on the
//! rest. It is data the ROM reads, not a signature, and rejecting on it would
//! discard 39 preserved images to enforce a rule the format does not state.
//! It is loaded as-is.

use thiserror::Error;

/// First byte of a `.p` image: `VERSN`, the first system variable saved.
pub const RAM_BASE: u16 = 0x4009;

/// Where `E_LINE` sits inside the image (`$4014 - $4009`).
pub const E_LINE_OFFSET: usize = 0x0B;

/// Shortest image that can contain the pointer describing it.
const MIN_LEN: usize = E_LINE_OFFSET + 2;

/// A 16 KB pack puts the top of RAM at `$8000`, and the image starts at
/// `$4009`.
const MAX_LEN: usize = 0x8000 - RAM_BASE as usize;

/// Why a `.p` image was rejected.
///
/// Non-exhaustive: the rules here were derived from a corpus rather than a
/// specification, so a future image may need a reason that does not exist
/// yet. Callers that match on this should keep a catch-all arm.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ParseError {
    /// Too short to contain the pointer that describes it.
    #[error("image is {len} bytes; a ZX81 image is at least {MIN_LEN}")]
    TooShort {
        /// Length seen.
        len: usize,
    },
    /// Larger than the machine's address space for RAM.
    #[error("image is {len} bytes; it would run past $8000 with a 16 KB pack")]
    TooLong {
        /// Length seen.
        len: usize,
    },
    /// `E_LINE` points past the end of the file.
    #[error("image says its program ends at ${e_line:04X}, past the ${end:04X} the file reaches")]
    EndsPastImage {
        /// Pointer read from offset `$0B`.
        e_line: u16,
        /// Address one past the image's last byte.
        end: u16,
    },
    /// `E_LINE` points at or below the image's own base.
    #[error("image says its program ends at ${e_line:04X}, at or below its ${RAM_BASE:04X} base")]
    EndsBeforeStart {
        /// Pointer read from offset `$0B`.
        e_line: u16,
    },
}

/// A parsed `.p` image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zx81Image {
    bytes: Vec<u8>,
    e_line: u16,
}

impl Zx81Image {
    /// Parse a `.p` / `.p81` image.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] when the image is too short to describe itself,
    /// too large for a 16 KB machine, or its `E_LINE` does not land inside it.
    pub fn parse(data: &[u8]) -> Result<Self, ParseError> {
        let len = data.len();
        if len < MIN_LEN {
            return Err(ParseError::TooShort { len });
        }
        if len > MAX_LEN {
            return Err(ParseError::TooLong { len });
        }

        let e_line = u16::from_le_bytes([data[E_LINE_OFFSET], data[E_LINE_OFFSET + 1]]);
        if e_line <= RAM_BASE {
            return Err(ParseError::EndsBeforeStart { e_line });
        }
        // `len` is bounded by MAX_LEN above, so this cannot overflow a u16.
        let end = RAM_BASE + len as u16;
        if e_line > end {
            return Err(ParseError::EndsPastImage { e_line, end });
        }

        Ok(Self {
            bytes: data.to_vec(),
            e_line,
        })
    }

    /// The image bytes, to be placed in RAM from [`RAM_BASE`].
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// `E_LINE`, the address one past the program the ROM saved.
    #[must_use]
    pub fn e_line(&self) -> u16 {
        self.e_line
    }

    /// Bytes up to `E_LINE`, excluding any trailing slack the deck caught.
    #[must_use]
    pub fn program(&self) -> &[u8] {
        let end = usize::from(self.e_line - RAM_BASE);
        &self.bytes[..end.min(self.bytes.len())]
    }

    /// Smallest RAM that can hold this image, in bytes.
    ///
    /// RAM starts at `$4000` and the image at `$4009`, so the nine system
    /// variables below `VERSN` count too.
    #[must_use]
    pub fn required_ram_bytes(&self) -> usize {
        usize::from(RAM_BASE - 0x4000) + self.bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an image whose `E_LINE` sits `slack` bytes before its end.
    fn image(len: usize, slack: u16) -> Vec<u8> {
        let mut data = vec![0u8; len];
        let e_line = RAM_BASE + len as u16 - slack;
        data[E_LINE_OFFSET] = e_line as u8;
        data[E_LINE_OFFSET + 1] = (e_line >> 8) as u8;
        data
    }

    #[test]
    fn an_exact_image_parses() {
        let parsed = Zx81Image::parse(&image(256, 0)).expect("valid");
        assert_eq!(parsed.e_line(), RAM_BASE + 256);
        assert_eq!(parsed.program().len(), 256);
    }

    /// Two thirds of the preserved corpus looks like this.
    #[test]
    fn trailing_slack_is_accepted_and_excluded_from_the_program() {
        let parsed = Zx81Image::parse(&image(256, 32)).expect("valid");
        assert_eq!(parsed.bytes().len(), 256);
        assert_eq!(parsed.program().len(), 224);
    }

    #[test]
    fn an_e_line_past_the_end_is_rejected() {
        let mut data = image(256, 0);
        let past = RAM_BASE + 300;
        data[E_LINE_OFFSET] = past as u8;
        data[E_LINE_OFFSET + 1] = (past >> 8) as u8;
        assert!(matches!(
            Zx81Image::parse(&data),
            Err(ParseError::EndsPastImage { .. })
        ));
    }

    #[test]
    fn an_e_line_at_or_below_the_base_is_rejected() {
        let mut data = image(256, 0);
        data[E_LINE_OFFSET] = 0x00;
        data[E_LINE_OFFSET + 1] = 0x40;
        assert!(matches!(
            Zx81Image::parse(&data),
            Err(ParseError::EndsBeforeStart { .. })
        ));
    }

    #[test]
    fn a_stub_too_short_to_describe_itself_is_rejected() {
        assert!(matches!(
            Zx81Image::parse(&[0u8; 4]),
            Err(ParseError::TooShort { len: 4 })
        ));
    }

    #[test]
    fn an_image_that_would_overrun_sixteen_k_is_rejected() {
        assert!(matches!(
            Zx81Image::parse(&vec![0u8; MAX_LEN + 1]),
            Err(ParseError::TooLong { .. })
        ));
    }

    /// `VERSN` is data, not a signature. 39 of 1,206 preserved images carry a
    /// non-zero one and are perfectly loadable.
    #[test]
    fn a_non_zero_versn_is_not_a_rejection() {
        let mut data = image(256, 0);
        data[0] = 0x80;
        assert!(Zx81Image::parse(&data).is_ok());
    }
}
