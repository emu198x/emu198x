//! Commodore TAP parsing.
//!
//! This crate parses the pulse stream stored in `.tap` images for Commodore
//! cassette-based machines. For C64 playback, the important output is the
//! sequence of pulse durations in native machine cycles.

use serde::{Deserialize, Serialize};
use thiserror::Error;

const TAP_HEADER_SIZE: usize = 20;
const TAP_MAGIC_C64: &[u8; 12] = b"C64-TAPE-RAW";
const TAP_MAGIC_C16: &[u8; 12] = b"C16-TAPE-RAW";

/// Parsed Commodore TAP image.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TapImage {
    /// TAP format revision byte.
    pub version: u8,
    /// Target machine family declared by the image.
    pub system: TapSystem,
    /// Video timing family declared by the image.
    pub video: TapVideo,
    /// Raw pulse lengths in machine cycles.
    pub pulses: Vec<u32>,
}

/// TAP target machine family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TapSystem {
    C64,
    Vic20,
    C16,
    Pet,
    Cbm500,
    Cbm600,
    Unknown(u8),
}

/// TAP video timing family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TapVideo {
    Pal,
    Ntsc,
    NtscOld,
    PalN,
    Unknown(u8),
}

/// TAP parse failure.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum TapParseError {
    /// The file was too short to contain the fixed header.
    #[error("TAP image is too short: expected at least {expected} bytes, got {actual}")]
    TooShort { expected: usize, actual: usize },

    /// The magic header was not recognised.
    #[error("TAP image has an unrecognised magic header")]
    InvalidMagic,

    /// The image declares a TAP revision this parser does not support.
    #[error("unsupported TAP version {version}")]
    UnsupportedVersion { version: u8 },

    /// The declared payload length does not fit within the provided bytes.
    #[error(
        "TAP image declares {declared} payload bytes, but only {available} bytes are available"
    )]
    TruncatedPayload { declared: usize, available: usize },

    /// An extended pulse was truncated.
    #[error("TAP image ended in the middle of one extended pulse")]
    TruncatedExtendedPulse,

    /// One extended pulse encoded a zero cycle count, which is invalid.
    #[error("TAP image encoded an extended pulse with zero cycles")]
    ZeroExtendedPulse,
}

/// Parses a Commodore TAP image into native pulse lengths.
///
/// # Errors
///
/// Returns an error if the image header or pulse stream is malformed.
pub fn parse_tap(bytes: &[u8]) -> Result<TapImage, TapParseError> {
    if bytes.len() < TAP_HEADER_SIZE {
        return Err(TapParseError::TooShort {
            expected: TAP_HEADER_SIZE,
            actual: bytes.len(),
        });
    }

    let magic = &bytes[..12];
    if magic != TAP_MAGIC_C64 && magic != TAP_MAGIC_C16 {
        return Err(TapParseError::InvalidMagic);
    }

    let version = bytes[12];
    if !matches!(version, 0 | 1) {
        return Err(TapParseError::UnsupportedVersion { version });
    }

    let declared_payload_len = u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let payload = &bytes[TAP_HEADER_SIZE..];
    let declared_payload_len = declared_payload_len as usize;
    if declared_payload_len > payload.len() {
        return Err(TapParseError::TruncatedPayload {
            declared: declared_payload_len,
            available: payload.len(),
        });
    }

    let payload = &payload[..declared_payload_len];
    let mut pulses = Vec::new();
    let mut index = 0;
    while index < payload.len() {
        let value = payload[index];
        index += 1;

        let pulse_cycles = if value == 0 {
            if version == 0 {
                // TAP v0 has no long-pulse encoding: a 0 byte just means the
                // pulse ran past the 255*8-cycle maximum, exact length lost.
                // Approximate with 256*8, matching VICE (`tap.c`: v0 zero →
                // `pulse_length = 256`, i.e. 256 byte-units = 2048 cycles).
                256 * 8
            } else {
                if index + 3 > payload.len() {
                    return Err(TapParseError::TruncatedExtendedPulse);
                }
                let cycles = read_le_u24(&payload[index..index + 3]);
                index += 3;
                if cycles == 0 {
                    return Err(TapParseError::ZeroExtendedPulse);
                }
                cycles
            }
        } else {
            u32::from(value) * 8
        };

        pulses.push(pulse_cycles);
    }

    Ok(TapImage {
        version,
        system: TapSystem::from_header(bytes[13]),
        video: TapVideo::from_header(bytes[14]),
        pulses,
    })
}

impl TapSystem {
    const fn from_header(value: u8) -> Self {
        match value {
            0 => Self::C64,
            1 => Self::Vic20,
            2 => Self::C16,
            3 => Self::Pet,
            4 => Self::Cbm500,
            5 => Self::Cbm600,
            other => Self::Unknown(other),
        }
    }

    const fn to_header(self) -> u8 {
        match self {
            Self::C64 => 0,
            Self::Vic20 => 1,
            Self::C16 => 2,
            Self::Pet => 3,
            Self::Cbm500 => 4,
            Self::Cbm600 => 5,
            Self::Unknown(value) => value,
        }
    }
}

impl TapVideo {
    const fn from_header(value: u8) -> Self {
        match value {
            0 => Self::Pal,
            1 => Self::Ntsc,
            2 => Self::NtscOld,
            3 => Self::PalN,
            other => Self::Unknown(other),
        }
    }

    const fn to_header(self) -> u8 {
        match self {
            Self::Pal => 0,
            Self::Ntsc => 1,
            Self::NtscOld => 2,
            Self::PalN => 3,
            Self::Unknown(value) => value,
        }
    }
}

/// Encodes a [`TapImage`] to `.tap` bytes (version 1, so long pulses use the
/// three-byte extended form). The inverse of [`parse_tap`] for the pulse stream.
#[must_use]
pub fn encode_tap(image: &TapImage) -> Vec<u8> {
    let mut payload = Vec::with_capacity(image.pulses.len());
    for &cycles in &image.pulses {
        let quantised = cycles / 8;
        if (1..=255).contains(&quantised) {
            payload.push(quantised as u8);
        } else {
            // Extended pulse: 0x00 followed by the 24-bit cycle count.
            payload.push(0x00);
            let bytes = cycles.to_le_bytes();
            payload.extend_from_slice(&bytes[..3]);
        }
    }

    let mut out = Vec::with_capacity(TAP_HEADER_SIZE + payload.len());
    out.extend_from_slice(TAP_MAGIC_C64);
    out.push(1); // version 1
    out.push(image.system.to_header());
    out.push(image.video.to_header());
    out.push(0); // reserved
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&payload);
    out
}

const fn read_le_u24(bytes: &[u8]) -> u32 {
    (bytes[0] as u32) | ((bytes[1] as u32) << 8) | ((bytes[2] as u32) << 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tap(version: u8, payload: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0; TAP_HEADER_SIZE];
        bytes[..12].copy_from_slice(TAP_MAGIC_C64);
        bytes[12] = version;
        bytes[13] = 0;
        bytes[14] = 0;
        bytes[16..20].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn parses_version_zero_standard_pulses() {
        let image = parse_tap(&make_tap(0, &[0x24, 0x30])).expect("TAP should parse");

        assert_eq!(image.system, TapSystem::C64);
        assert_eq!(image.video, TapVideo::Pal);
        assert_eq!(image.pulses, vec![0x24 * 8, 0x30 * 8]);
    }

    #[test]
    fn parses_version_zero_zero_byte_as_2048_cycles() {
        let image = parse_tap(&make_tap(0, &[0x00])).expect("TAP should parse");
        assert_eq!(image.pulses, vec![2048]);
    }

    #[test]
    fn parses_version_one_extended_pulse() {
        let image = parse_tap(&make_tap(1, &[0x00, 0x20, 0x03, 0x00])).expect("TAP should parse");
        assert_eq!(image.pulses, vec![0x0320]);
    }

    #[test]
    fn rejects_unknown_magic() {
        let mut bytes = make_tap(1, &[0x20]);
        bytes[0] = b'X';
        assert_eq!(parse_tap(&bytes), Err(TapParseError::InvalidMagic));
    }

    #[test]
    fn rejects_truncated_extended_pulse() {
        assert_eq!(
            parse_tap(&make_tap(1, &[0x00, 0x01, 0x02])),
            Err(TapParseError::TruncatedExtendedPulse)
        );
    }

    #[test]
    fn encode_round_trips_short_and_extended_pulses() {
        // A short pulse (fits in one byte) and a long one (needs the extended
        // form) exercise both encoder branches.
        let image = TapImage {
            version: 1,
            system: TapSystem::C64,
            video: TapVideo::Pal,
            pulses: vec![0x24 * 8, 0x0320, 0x30 * 8],
        };
        let bytes = encode_tap(&image);
        let reparsed = parse_tap(&bytes).expect("encoded TAP should parse");
        assert_eq!(reparsed.system, TapSystem::C64);
        assert_eq!(reparsed.video, TapVideo::Pal);
        assert_eq!(reparsed.pulses, image.pulses);
    }
}
