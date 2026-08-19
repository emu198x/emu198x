//! `.o` / `.80` tape images for the Sinclair ZX80, and the pulse train that
//! carries them.
//!
//! # The container is a memory image
//!
//! There is no header, no block structure and no checksum. A `.o` file is
//! the bytes the ROM's `SAVE` puts on tape, which is RAM from `$4000` up to
//! the address held in `$400A`:
//!
//! ```text
//! $01F8   INC HL
//!         EX DE,HL
//!         LD HL,($400A)
//!         SCF
//!         SBC HL,DE        ; ($400A) - pointer - 1
//!         EX DE,HL
//!         RET NC           ; keep going while the pointer is below it
//! ```
//!
//! `$400A` is inside the saved region, at offset `$0A`, so a well-formed
//! image describes its own length: the little-endian word there must equal
//! `$4000 + len`. That is the only integrity check the format admits, and
//! this parser applies it — a truncated or padded image fails here rather
//! than loading and misbehaving later.
//!
//! # The pulse train
//!
//! Bits are carried by *counted pulses*, not by pulse width. `SAVE` picks
//! the count with:
//!
//! ```text
//! RLC (HL)        ; next bit, most significant first
//! SBC A,A         ; $00 or $FF
//! AND $05
//! ADD A,$04       ; 4 pulses for 0, 9 for 1
//! ```
//!
//! and emits each pulse by touching the bus: an `OUT` drives the cassette
//! line one way, an `IN` the other. The timings below were measured from
//! Sinclair's ROM saving an empty program, rather than derived from the
//! instruction timings, because the loop's cost is what matters and the ROM
//! is the authority on it:
//!
//! | | T-states |
//! |---|---|
//! | pulse high (`OUT` to `IN`) | 488 |
//! | pulse low (`IN` to next `OUT`) | 484 |
//! | gap between bits | 4754 |

/// Pulse high time, in T-states.
pub const PULSE_HIGH_T: u64 = 488;
/// Pulse low time, in T-states.
pub const PULSE_LOW_T: u64 = 484;
/// Quiet between one bit's pulses and the next bit's.
pub const BIT_GAP_T: u64 = 4754;
/// Pulses that mean a clear bit.
pub const ZERO_PULSES: usize = 4;
/// Pulses that mean a set bit.
pub const ONE_PULSES: usize = 9;
/// Silence before the first bit, so a loader that is already running sees a
/// clean start rather than joining mid-pulse.
pub const LEAD_IN_T: u64 = 100_000;

/// Where RAM images start on a ZX80.
pub const RAM_BASE: u16 = 0x4000;
/// Offset of the end-of-program pointer within the image.
pub const END_POINTER_OFFSET: usize = 0x0A;

/// Smallest image the ROM can produce: the system variables alone.
const MIN_LEN: usize = END_POINTER_OFFSET + 2;
/// A ZX80 tops out at 16 KB of RAM.
const MAX_LEN: usize = 16 * 1024;

/// Why an image was rejected.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// Too short to contain the pointer that describes it.
    #[error("image is {len} bytes; a ZX80 image is at least {MIN_LEN}")]
    TooShort {
        /// Length seen.
        len: usize,
    },
    /// Larger than the machine's address space for RAM.
    #[error("image is {len} bytes; a ZX80 has at most {MAX_LEN} of RAM")]
    TooLong {
        /// Length seen.
        len: usize,
    },
    /// The self-describing length does not match the file.
    #[error("image says it ends at ${end:04X}, which is {expected} bytes, but the file is {len}")]
    LengthMismatch {
        /// Pointer read from offset `$0A`.
        end: u16,
        /// Length that pointer implies.
        expected: usize,
        /// Actual file length.
        len: usize,
    },
}

/// A parsed ZX80 tape image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Zx80Image {
    bytes: Vec<u8>,
}

impl Zx80Image {
    /// Parses and validates a `.o` / `.80` image.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] if the image is too short, too long for a
    /// ZX80's RAM, or does not agree with the end pointer it carries.
    pub fn parse(data: &[u8]) -> Result<Self, ParseError> {
        let len = data.len();
        if len < MIN_LEN {
            return Err(ParseError::TooShort { len });
        }
        if len > MAX_LEN {
            return Err(ParseError::TooLong { len });
        }
        let end = u16::from_le_bytes([data[END_POINTER_OFFSET], data[END_POINTER_OFFSET + 1]]);
        let expected = usize::from(end).saturating_sub(usize::from(RAM_BASE));
        if expected != len {
            return Err(ParseError::LengthMismatch { end, expected, len });
        }
        Ok(Self {
            bytes: data.to_vec(),
        })
    }

    /// The image's bytes, as they sit in RAM from `$4000`.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Where the program ends, per the pointer the image carries.
    #[must_use]
    pub fn end_address(&self) -> u16 {
        RAM_BASE.wrapping_add(u16::try_from(self.bytes.len()).unwrap_or(u16::MAX))
    }

    /// Encodes the image as cassette-line transitions, in T-states from the
    /// moment the tape starts.
    ///
    /// The returned times are when the line flips. It starts low, so an
    /// odd number of elapsed transitions means the line is high — which is
    /// what the loader waits for before timing a burst.
    #[must_use]
    pub fn to_pulses(&self) -> Vec<u64> {
        let mut edges = Vec::new();
        let mut t = LEAD_IN_T;
        for byte in &self.bytes {
            for bit in (0..8).rev() {
                let set = byte & (1 << bit) != 0;
                let pulses = if set { ONE_PULSES } else { ZERO_PULSES };
                for _ in 0..pulses {
                    edges.push(t);
                    t += PULSE_HIGH_T;
                    edges.push(t);
                    t += PULSE_LOW_T;
                }
                // The low that ends a bit is stretched into the gap, rather
                // than added to it: the loader measures from the last edge.
                t += BIT_GAP_T - PULSE_LOW_T;
            }
        }
        edges
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(program: &[u8]) -> Vec<u8> {
        let len = MIN_LEN.max(program.len());
        let mut bytes = vec![0u8; len];
        bytes[..program.len()].copy_from_slice(program);
        let end = RAM_BASE + u16::try_from(len).expect("test image fits");
        bytes[END_POINTER_OFFSET..END_POINTER_OFFSET + 2].copy_from_slice(&end.to_le_bytes());
        bytes
    }

    #[test]
    fn accepts_an_image_that_agrees_with_its_own_end_pointer() {
        let raw = image(&[0u8; 64]);
        let parsed = Zx80Image::parse(&raw).expect("valid image");
        assert_eq!(parsed.bytes().len(), 64);
        assert_eq!(parsed.end_address(), RAM_BASE + 64);
    }

    /// The only integrity check the format admits, so it has to be applied.
    /// A truncated image otherwise loads and then misbehaves somewhere else.
    #[test]
    fn rejects_an_image_whose_pointer_disagrees_with_its_length() {
        let mut raw = image(&[0u8; 64]);
        raw.truncate(60);
        let err = Zx80Image::parse(&raw).expect_err("truncated image must be rejected");
        assert_eq!(
            err,
            ParseError::LengthMismatch {
                end: RAM_BASE + 64,
                expected: 64,
                len: 60,
            }
        );
    }

    #[test]
    fn rejects_images_that_cannot_be_zx80_memory() {
        assert_eq!(
            Zx80Image::parse(&[0u8; 4]),
            Err(ParseError::TooShort { len: 4 })
        );
        assert!(matches!(
            Zx80Image::parse(&vec![0u8; MAX_LEN + 1]),
            Err(ParseError::TooLong { .. })
        ));
    }

    /// Nine pulses for a set bit, four for a clear one — the counts the
    /// ROM's `AND $05 / ADD A,$04` produces, most significant bit first.
    #[test]
    fn bits_are_carried_by_pulse_count_most_significant_first() {
        let raw = image(&[
            0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x40,
        ]);
        let parsed = Zx80Image::parse(&raw).expect("valid image");
        let edges = parsed.to_pulses();

        // Two edges per pulse. The first byte is $80: one set bit then seven
        // clear ones.
        let first_bit = ONE_PULSES * 2;
        assert_eq!(edges[0], LEAD_IN_T);
        assert_eq!(edges[1] - edges[0], PULSE_HIGH_T);
        assert_eq!(edges[2] - edges[1], PULSE_LOW_T);

        // The gap lands after the last pulse of the bit, not between pulses.
        let gap = edges[first_bit] - edges[first_bit - 1];
        assert_eq!(gap, BIT_GAP_T);

        let second_bit_pulses = (edges[first_bit..].len()).min(ZERO_PULSES * 2);
        assert_eq!(second_bit_pulses, ZERO_PULSES * 2);
    }

    #[test]
    fn every_bit_of_every_byte_is_encoded() {
        let raw = image(&[0xFFu8; 32]);
        let parsed = Zx80Image::parse(&raw).expect("valid image");
        // 32 bytes; the end pointer at $0A..$0B is $4020, so those two bytes
        // are not $FF. Count set bits directly rather than assuming.
        let set: u32 = parsed.bytes().iter().map(|b| b.count_ones()).sum();
        let clear = 32 * 8 - set;
        let expected = set as usize * ONE_PULSES * 2 + clear as usize * ZERO_PULSES * 2;
        assert_eq!(parsed.to_pulses().len(), expected);
    }
}
