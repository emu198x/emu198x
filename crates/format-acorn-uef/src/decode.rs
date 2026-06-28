//! UEF container decoding: gzip layer, chunk walking, and waveform synthesis.
//!
//! Ported from MAME's `src/lib/formats/uef_cas.cpp` (Wilbert Pol, BSD-3-Clause),
//! re-expressed as a clock-neutral [`TapePulse`] stream rather than fixed-rate
//! PCM, and using `flate2` for the optional gzip layer.

use std::io::Read;

use flate2::read::GzDecoder;

use common_acorn_cassette::TapePulse;

use crate::error::UefError;
use crate::pulse::UefTape;

/// 10-byte signature every (decompressed) UEF image begins with: `UEF File!\0`.
const MAGIC: [u8; 10] = [0x55, 0x45, 0x46, 0x20, 0x46, 0x69, 0x6c, 0x65, 0x21, 0x00];

/// Two-byte gzip signature.
const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

/// Offset of the first chunk: the magic plus the two version bytes.
const FIRST_CHUNK: usize = MAGIC.len() + 2;

/// Default Kansas-City base frequency in hertz. A `0` bit is one cycle here; a
/// `1` bit is two cycles at twice this frequency.
const DEFAULT_BASE_HZ: f64 = 1200.0;

/// Parse a UEF image into its cassette waveform.
///
/// Accepts both raw and gzip-compressed images. Returns [`UefError`] when the
/// gzip layer is corrupt, the magic is absent, or a chunk runs past the end of
/// the image.
pub fn parse(data: &[u8]) -> Result<UefTape, UefError> {
    let inflated = inflate_if_gzip(data)?;
    let bytes: &[u8] = &inflated;

    if bytes.len() < FIRST_CHUNK {
        return Err(UefError::TooSmall(bytes.len()));
    }
    if bytes[..MAGIC.len()] != MAGIC {
        return Err(UefError::BadMagic);
    }

    let mut decoder = Decoder::new();
    let mut pos = FIRST_CHUNK;
    while pos + 6 <= bytes.len() {
        let id = u16le(&bytes[pos..]).ok_or(UefError::BadMagic)?;
        let length = u32le(&bytes[pos + 2..]).ok_or(UefError::BadMagic)? as usize;
        let payload_start = pos + 6;
        let available = bytes.len() - payload_start;
        if length > available {
            return Err(UefError::TruncatedChunk {
                id,
                offset: pos,
                length,
                available,
            });
        }
        let payload = &bytes[payload_start..payload_start + length];
        decoder.chunk(id, payload, pos)?;
        pos = payload_start + length;
    }

    Ok(decoder.finish())
}

/// Inflate `data` when it carries the gzip signature, otherwise borrow it.
fn inflate_if_gzip(data: &[u8]) -> Result<std::borrow::Cow<'_, [u8]>, UefError> {
    if data.len() >= 2 && data[..2] == GZIP_MAGIC {
        let mut out = Vec::new();
        GzDecoder::new(data)
            .read_to_end(&mut out)
            .map_err(|e| UefError::Gzip(e.to_string()))?;
        Ok(std::borrow::Cow::Owned(out))
    } else {
        Ok(std::borrow::Cow::Borrowed(data))
    }
}

/// Byte framing parity for `&0104` defined-format data blocks.
#[derive(Clone, Copy)]
enum Parity {
    None,
    Even,
    Odd,
}

/// Carries the running tape state across chunks and accumulates the waveform.
struct Decoder {
    base_hz: f64,
    /// Half-period of a `0`-bit cycle (base frequency), in nanoseconds.
    zero_half_ns: u32,
    /// Half-period of a `1`-bit / carrier cycle (twice the base), in nanoseconds.
    one_half_ns: u32,
    /// Nanoseconds per integer-gap unit.
    gap_unit_ns: f64,
    /// Cycles emitted per bit: 1 at 1200 baud, 4 at 300 baud.
    loops: u32,
    pulses: Vec<TapePulse>,
    skipped: Vec<u16>,
}

impl Decoder {
    fn new() -> Self {
        let mut decoder = Self {
            base_hz: DEFAULT_BASE_HZ,
            zero_half_ns: 0,
            one_half_ns: 0,
            gap_unit_ns: 0.0,
            loops: 1,
            pulses: Vec::new(),
            skipped: Vec::new(),
        };
        decoder.set_base(DEFAULT_BASE_HZ);
        decoder
    }

    /// Recompute the cached half-periods after a base-frequency change.
    fn set_base(&mut self, base_hz: f64) {
        let base = if base_hz > 0.0 {
            base_hz
        } else {
            DEFAULT_BASE_HZ
        };
        self.base_hz = base;
        self.zero_half_ns = (1.0e9 / (2.0 * base)).round() as u32;
        self.one_half_ns = (1.0e9 / (4.0 * base)).round() as u32;
        self.gap_unit_ns = 1.0e9 / (2.0 * base);
    }

    fn finish(self) -> UefTape {
        UefTape {
            pulses: self.pulses,
            skipped_chunks: self.skipped,
        }
    }

    /// Push `count` cycles of the given half-period, skipping empty runs.
    fn cycles(&mut self, half_period_ns: u32, count: u32) {
        if count == 0 {
            return;
        }
        self.pulses.push(TapePulse::Cycles {
            half_period_ns,
            count,
        });
    }

    /// Push a flat gap, splitting durations that overflow a `u32`.
    fn gap(&mut self, mut duration_ns: u64) {
        while duration_ns > u64::from(u32::MAX) {
            self.pulses.push(TapePulse::Gap {
                duration_ns: u32::MAX,
            });
            duration_ns -= u64::from(u32::MAX);
        }
        if duration_ns > 0 {
            self.pulses.push(TapePulse::Gap {
                duration_ns: duration_ns as u32,
            });
        }
    }

    /// Emit one Kansas-City data bit: `0` is one base-frequency cycle, `1` is two
    /// double-frequency cycles, each repeated `loops` times for slower bauds.
    fn bit(&mut self, set: bool) {
        if set {
            self.cycles(self.one_half_ns, 2 * self.loops);
        } else {
            self.cycles(self.zero_half_ns, self.loops);
        }
    }

    /// Emit a framed byte: a start bit, `data_bits` data bits (LSB first), an
    /// optional parity bit, and `stop_bits` stop bits.
    fn framed_byte(&mut self, byte: u8, data_bits: u8, parity: Parity, stop_bits: u8) {
        self.bit(false);
        let data_bits = data_bits.min(8);
        let mut ones = 0u32;
        for i in 0..data_bits {
            let set = (byte >> i) & 1 == 1;
            ones += u32::from(set);
            self.bit(set);
        }
        match parity {
            Parity::Even => self.bit(!ones.is_multiple_of(2)),
            Parity::Odd => self.bit(ones.is_multiple_of(2)),
            Parity::None => {}
        }
        for _ in 0..stop_bits {
            self.bit(true);
        }
    }

    /// Decode one chunk into the waveform.
    fn chunk(&mut self, id: u16, payload: &[u8], offset: usize) -> Result<(), UefError> {
        match id {
            // Implicit-framing (8N1) data block.
            0x0100 => {
                for &byte in payload {
                    self.framed_byte(byte, 8, Parity::None, 1);
                }
            }
            // Defined-format data block: a 3-byte framing header then data.
            0x0104 => {
                if payload.len() < 3 {
                    return Err(UefError::MalformedChunk {
                        id,
                        offset,
                        field: 3,
                    });
                }
                let data_bits = payload[0];
                let parity = match payload[1] {
                    b'E' | b'e' => Parity::Even,
                    b'O' | b'o' => Parity::Odd,
                    _ => Parity::None,
                };
                // Stop-bit count is signed; a negative value adds an extra short
                // high-tone cycle, which we approximate by its magnitude here.
                let stop_bits = (payload[2] as i8).unsigned_abs();
                for &byte in &payload[3..] {
                    self.framed_byte(byte, data_bits, parity, stop_bits);
                }
            }
            // Carrier tone: a run of high-frequency cycles.
            0x0110 => {
                let cycles = u16le(payload).ok_or(UefError::MalformedChunk {
                    id,
                    offset,
                    field: 2,
                })?;
                self.cycles(self.one_half_ns, u32::from(cycles));
            }
            // Carrier tone with a dummy `&AA` byte between two runs.
            0x0111 => {
                if payload.len() < 4 {
                    return Err(UefError::MalformedChunk {
                        id,
                        offset,
                        field: 4,
                    });
                }
                let before = u16le(&payload[0..]).unwrap_or(0);
                let after = u16le(&payload[2..]).unwrap_or(0);
                self.cycles(self.one_half_ns, u32::from(before));
                self.framed_byte(0xAA, 8, Parity::None, 1);
                self.cycles(self.one_half_ns, u32::from(after));
            }
            // Integer gap: silence measured in half-base-period units.
            0x0112 => {
                let units = u16le(payload).ok_or(UefError::MalformedChunk {
                    id,
                    offset,
                    field: 2,
                })?;
                let ns = (f64::from(units) * self.gap_unit_ns).round() as u64;
                self.gap(ns);
            }
            // Base-frequency change.
            0x0113 => {
                let hz = uef_float(payload).ok_or(UefError::MalformedChunk {
                    id,
                    offset,
                    field: 4,
                })?;
                self.set_base(hz);
            }
            // Floating-point gap in seconds.
            0x0116 => {
                let seconds = uef_float(payload).ok_or(UefError::MalformedChunk {
                    id,
                    offset,
                    field: 4,
                })?;
                if seconds > 0.0 {
                    self.gap((seconds * 1.0e9).round() as u64);
                }
            }
            // Baud-rate change: only the standard 1200 and 300 are modelled.
            0x0117 => {
                let baud = u16le(payload).ok_or(UefError::MalformedChunk {
                    id,
                    offset,
                    field: 2,
                })?;
                match baud {
                    1200 => self.loops = 1,
                    300 => self.loops = 4,
                    _ => self.skipped.push(id),
                }
            }
            // Recognised but not synthesised (metadata, or not yet modelled).
            _ => self.skipped.push(id),
        }
        Ok(())
    }
}

/// Read a little-endian `u16` from the front of `bytes`.
fn u16le(bytes: &[u8]) -> Option<u16> {
    match bytes {
        [a, b, ..] => Some(u16::from_le_bytes([*a, *b])),
        _ => None,
    }
}

/// Read a little-endian `u32` from the front of `bytes`.
fn u32le(bytes: &[u8]) -> Option<u32> {
    match bytes {
        [a, b, c, d, ..] => Some(u32::from_le_bytes([*a, *b, *c, *d])),
        _ => None,
    }
}

/// Decode a UEF 4-byte float (IEEE-754 single, little-endian) into seconds/hertz.
fn uef_float(bytes: &[u8]) -> Option<f64> {
    let [b0, b1, b2, b3, ..] = bytes else {
        return None;
    };
    let mantissa = (u32::from(*b0) | (u32::from(*b1) << 8) | (u32::from(*b2) << 16)) | 0x0080_0000;
    let mut result = f64::from(mantissa) * 2f64.powi(-23);
    let exponent = i32::from((u16::from_le_bytes([*b2, *b3]) & 0x7f80) >> 7) - 127;
    result *= 2f64.powi(exponent);
    if b3 & 0x80 != 0 {
        result = -result;
    }
    Some(result)
}
