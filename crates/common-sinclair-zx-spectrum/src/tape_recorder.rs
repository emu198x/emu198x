//! Tape SAVE capture for Spectrum-family machines.
//!
//! The mirror image of [`crate::tape::TapePlayer`]. During a `SAVE` the ROM
//! toggles the MIC line (port `$FE` bit 3) to lay a standard-speed signal:
//! a pilot tone, two sync pulses, then two pulses per data bit. This recorder
//! timestamps each MIC edge against a monotonic T-state clock, and on flush
//! decodes the captured pulse train back into [`TapeBlock`]s — pilot → sync →
//! bit pairs → bytes. The decode is reliable because the ROM emits exact
//! standard timings, not noisy real-world tape.
//!
//! Source references:
//! - `docs/systems/spectrum.md`
//! - Pulse constants shared with `crate::tape` (the playback encoder this
//!   inverts).

use crate::tape::TapeBlock;

/// Lowest pulse length (T-states) accepted as a pilot pulse (`PILOT_PULSE`
/// 2168 less a generous margin; a one-bit pulse, 1710, sits below this).
const PILOT_MIN: u32 = 1_900;
/// Highest pulse length accepted as a pilot pulse.
const PILOT_MAX: u32 = 2_400;
/// Consecutive pilot-range pulses needed to confirm a pilot tone. The ROM
/// emits thousands; data never holds this many identical-length pulses.
const MIN_PILOT_PULSES: usize = 256;
/// Shortest pulse treated as a data bit (excludes the sub-edge noise floor).
const BIT_MIN: u32 = 400;
/// First pulse length too long to be a data bit — ends the data block (a
/// pause gap or the next block's pilot). One-bit is 1710; pilot is 1900+.
const BIT_MAX: u32 = 1_900;
/// Split between a zero-bit pulse (855) and a one-bit pulse (1710).
const BIT_THRESHOLD: u32 = 1_283;

/// Captures MIC-line edges during a `SAVE` and decodes them to tape blocks.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TapeRecorder {
    /// Monotonic T-state clock, advanced in lockstep with the machine.
    clock: u64,
    /// Clock value at the previous MIC edge.
    last_edge: u64,
    /// Current MIC level (port `$FE` bit 3).
    level: bool,
    /// Whether the first edge has been seen (the lead-in idle is not a pulse).
    started: bool,
    /// Pulse lengths between successive MIC edges, in T-states.
    pulses: Vec<u32>,
}

impl TapeRecorder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Advances the monotonic clock by `tstates`, called each machine tick.
    pub fn advance(&mut self, tstates: u32) {
        self.clock += u64::from(tstates);
    }

    /// Records the MIC line level (port `$FE` bit 3). A change closes the
    /// current pulse and opens the next; an unchanged level is ignored.
    pub fn set_mic_level(&mut self, level: bool) {
        if level == self.level && self.started {
            return;
        }
        if self.started {
            let duration = u32::try_from(self.clock - self.last_edge).unwrap_or(u32::MAX);
            self.pulses.push(duration);
        }
        self.last_edge = self.clock;
        self.level = level;
        self.started = true;
    }

    /// Whether any MIC activity has been captured (a `SAVE` toggles the line;
    /// nothing else does).
    #[must_use]
    pub fn has_signal(&self) -> bool {
        !self.pulses.is_empty()
    }

    /// Clears all captured signal (e.g. after a flush, or on a fresh mount).
    pub fn clear(&mut self) {
        self.pulses.clear();
        self.started = false;
        self.last_edge = self.clock;
    }

    /// Decodes the captured pulse train into standard-speed tape blocks.
    ///
    /// Each block's `data` is the full on-tape byte stream — flag, payload, and
    /// trailing checksum — matching the [`TapeBlock`] playback convention.
    #[must_use]
    pub fn decode(&self) -> Vec<TapeBlock> {
        let mut blocks = Vec::new();
        let mut i = 0;

        while i < self.pulses.len() {
            // Seek a pilot tone: a long run of pilot-range pulses.
            let mut pilot = 0;
            while i < self.pulses.len() && is_pilot(self.pulses[i]) {
                pilot += 1;
                i += 1;
            }
            if pilot < MIN_PILOT_PULSES {
                // Not a pilot; step past one pulse and keep looking.
                if i < self.pulses.len() && !is_pilot(self.pulses[i]) {
                    i += 1;
                }
                continue;
            }

            // Two sync pulses bridge the pilot tone and the data.
            if i + 1 >= self.pulses.len() {
                break;
            }
            i += 2;

            // Data: two pulses per bit, MSB first, until a non-bit pulse.
            let mut bits = Vec::new();
            while i + 1 < self.pulses.len() {
                let pulse = self.pulses[i];
                if !(BIT_MIN..BIT_MAX).contains(&pulse) {
                    break;
                }
                bits.push(u8::from(pulse >= BIT_THRESHOLD));
                i += 2;
            }

            if let Some(block) = bits_to_block(&bits) {
                blocks.push(block);
            }
        }

        blocks
    }
}

fn is_pilot(pulse: u32) -> bool {
    (PILOT_MIN..=PILOT_MAX).contains(&pulse)
}

/// Packs MSB-first bits into bytes and wraps them as a tape block. The first
/// byte is the flag; the last is the checksum (kept, matching the on-tape
/// stream). Returns `None` for a runt block (no flag + checksum).
fn bits_to_block(bits: &[u8]) -> Option<TapeBlock> {
    let mut bytes = Vec::with_capacity(bits.len() / 8);
    for chunk in bits.chunks_exact(8) {
        let byte = chunk.iter().fold(0u8, |acc, &bit| (acc << 1) | bit);
        bytes.push(byte);
    }
    if bytes.len() < 2 {
        return None;
    }
    Some(TapeBlock {
        flag: bytes[0],
        data: bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tape::{PAUSE_MS, TapeSpan, standard_block_spans};

    /// Drives the recorder from a playback span stream exactly as the machine
    /// would drive it from MIC writes: each pulse holds the level then toggles.
    fn feed(recorder: &mut TapeRecorder, spans: &[TapeSpan]) {
        let mut level = false;
        recorder.set_mic_level(level);
        for span in spans {
            match span {
                TapeSpan::Pulse(duration) => {
                    recorder.advance(*duration);
                    level = !level;
                    recorder.set_mic_level(level);
                }
                TapeSpan::Level { duration, .. } => recorder.advance(*duration),
                TapeSpan::Stop => {}
            }
        }
    }

    /// Builds a tape block with the on-tape byte stream the ROM lays down:
    /// flag, payload, and the trailing XOR checksum.
    fn block(flag: u8, payload: &[u8]) -> TapeBlock {
        let mut data = vec![flag];
        data.extend_from_slice(payload);
        let checksum = data.iter().fold(0u8, |acc, &byte| acc ^ byte);
        data.push(checksum);
        TapeBlock { flag, data }
    }

    fn encode(blocks: &[TapeBlock]) -> Vec<TapeSpan> {
        let mut level = false;
        let mut spans = Vec::new();
        for block in blocks {
            standard_block_spans(block, PAUSE_MS, &mut level, &mut spans);
        }
        spans
    }

    #[test]
    fn decodes_a_played_back_header_and_data_block() {
        // A real SAVE: a 17-byte header (program name + params) then the data.
        let blocks = vec![
            block(
                0x00,
                &[
                    0x00, b'M', b'Y', b'P', b'R', b'O', b'G', b'R', b'A', b'M', 6, 0, 8, 0, 6, 0,
                ],
            ),
            block(0xFF, &[0x01, 0x08, 0x99, 0x22, 0x48, 0x49]),
        ];

        let mut recorder = TapeRecorder::new();
        feed(&mut recorder, &encode(&blocks));

        assert!(recorder.has_signal());
        assert_eq!(recorder.decode(), blocks);
    }

    #[test]
    fn no_signal_decodes_to_nothing() {
        let recorder = TapeRecorder::new();
        assert!(!recorder.has_signal());
        assert!(recorder.decode().is_empty());
    }

    #[test]
    fn idle_mic_without_a_pilot_yields_no_blocks() {
        // A few stray toggles with no pilot tone must not fabricate a block.
        let mut recorder = TapeRecorder::new();
        recorder.set_mic_level(false);
        for _ in 0..8 {
            recorder.advance(1_000);
            recorder.set_mic_level(true);
            recorder.advance(1_000);
            recorder.set_mic_level(false);
        }
        assert!(recorder.decode().is_empty());
    }
}
