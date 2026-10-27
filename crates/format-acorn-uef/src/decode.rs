//! Cassette waveform synthesis from a UEF image's tape chunks.
//!
//! The container (gzip layer, magic, chunk walking) is
//! `format198x-acorn-uef`'s. What a tape chunk sounds like is this crate's:
//! ported from MAME's `src/lib/formats/uef_cas.cpp` (Wilbert Pol,
//! BSD-3-Clause) and re-expressed as a clock-neutral [`TapePulse`] stream
//! rather than fixed-rate PCM.

use common_acorn_cassette::TapePulse;
use format198x_acorn_uef::{TapeChunk, Uef, UefError};

use crate::pulse::UefTape;

/// Default Kansas-City base frequency in hertz. A `0` bit is one cycle here; a
/// `1` bit is two cycles at twice this frequency.
const DEFAULT_BASE_HZ: f64 = 1200.0;

/// Parse a UEF image into its cassette waveform.
///
/// Accepts both raw and gzip-compressed images. Returns [`UefError`] when the
/// gzip layer is corrupt, the magic is absent, a chunk runs past the end of
/// the image, or a tape chunk is shorter than its fields.
pub fn parse(data: &[u8]) -> Result<UefTape, UefError> {
    let uef = Uef::parse(data)?;
    let mut decoder = Decoder::new();
    for chunk in &uef.chunks {
        match chunk.tape()? {
            Some(tape) => decoder.chunk(chunk.id, tape),
            // Recognised but not synthesised (metadata, or not yet modelled).
            None => decoder.skipped.push(chunk.id),
        }
    }
    Ok(decoder.finish())
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

    /// Decode one tape chunk into the waveform.
    fn chunk(&mut self, id: u16, tape: TapeChunk<'_>) {
        match tape {
            // Implicit-framing (8N1) data block.
            TapeChunk::ImplicitData(data) => {
                for &byte in data {
                    self.framed_byte(byte, 8, Parity::None, 1);
                }
            }
            // Defined-format data block: its own framing, then data.
            TapeChunk::DefinedFormat {
                data_bits,
                parity,
                stop_bits,
                data,
            } => {
                let parity = match parity {
                    b'E' | b'e' => Parity::Even,
                    b'O' | b'o' => Parity::Odd,
                    _ => Parity::None,
                };
                // Stop-bit count is signed; a negative value adds an extra short
                // high-tone cycle, which we approximate by its magnitude here.
                let stop_bits = stop_bits.unsigned_abs();
                for &byte in data {
                    self.framed_byte(byte, data_bits, parity, stop_bits);
                }
            }
            // Carrier tone: a run of high-frequency cycles.
            TapeChunk::Carrier { cycles } => self.cycles(self.one_half_ns, u32::from(cycles)),
            // Carrier tone with a dummy `&AA` byte between two runs.
            TapeChunk::CarrierWithDummy { before, after } => {
                self.cycles(self.one_half_ns, u32::from(before));
                self.framed_byte(0xAA, 8, Parity::None, 1);
                self.cycles(self.one_half_ns, u32::from(after));
            }
            // Integer gap: silence measured in half-base-period units.
            TapeChunk::IntegerGap { units } => {
                let ns = (f64::from(units) * self.gap_unit_ns).round() as u64;
                self.gap(ns);
            }
            // Base-frequency change.
            TapeChunk::BaseFrequency(hz) => self.set_base(f64::from(hz)),
            // Floating-point gap in seconds.
            TapeChunk::FloatGap(seconds) => {
                if seconds > 0.0 {
                    self.gap((f64::from(seconds) * 1.0e9).round() as u64);
                }
            }
            // Baud-rate change: only the standard 1200 and 300 are modelled.
            TapeChunk::BaudRate(baud) => match baud {
                1200 => self.loops = 1,
                300 => self.loops = 4,
                _ => self.skipped.push(id),
            },
        }
    }
}
