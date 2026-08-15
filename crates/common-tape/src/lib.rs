//! Machine-agnostic tape timing spans and playback.
//!
//! A cassette, to a machine, is one signal: a level on a pin that changes at
//! particular moments. Everything above that — pilot tones, sync pulses, bit
//! encodings, block structure — is the *format's* business, and every format
//! reduces to the same thing here: a list of "hold this level for N T-states".
//!
//! So this crate holds the model and the player, and knows nothing about any
//! system. What it deliberately does *not* hold is a clock. Span durations are
//! in the T-states of whoever loaded them, and it is the loader's job to have
//! converted. A CDT's pulse lengths are quoted in the Spectrum's 3.5 MHz
//! T-states even on an Amstrad, so `format-amstrad-cpc-cdt` scales by 40/35
//! before the spans ever reach a player.
//!
//! Promoted out of `common-sinclair-zx-spectrum` on 2026-08-14 under the
//! amendment in `knowledge/decisions/crate-naming.md`. The Spectrum's own ROM
//! block encoding — pilot counts, sync lengths, `standard_block_spans` — stayed
//! behind, because those are the Spectrum's format and not a shared model.

use serde::{Deserialize, Serialize};

/// One machine-facing tape timing span.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TapeSpan {
    /// Hold the current level for `duration` T-states, then toggle it.
    Pulse(u32),
    /// Hold `level` for `duration` T-states without an edge at the end.
    Level { duration: u32, level: bool },
    /// Stop playback and wait for explicit user resume.
    Stop,
}

/// Tape player that advances through one timing-span stream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TapePlayer {
    spans: Vec<TapeSpan>,
    span_idx: usize,
    level: bool,
    countdown: u32,
    playing: bool,
}

impl TapePlayer {
    /// Creates an empty tape player with no loaded media.
    #[must_use]
    pub fn new() -> Self {
        Self {
            spans: Vec::new(),
            span_idx: 0,
            level: false,
            countdown: 0,
            playing: false,
        }
    }

    /// Loads one timing-span stream and rewinds playback to the start.
    pub fn load_stream(&mut self, spans: Vec<TapeSpan>) {
        self.spans = spans;
        self.span_idx = 0;
        self.level = false;
        self.countdown = 0;
        self.playing = false;
    }

    /// Loads a raw pulse stream and rewinds playback to the start.
    pub fn load_pulses(&mut self, pulses: Vec<u32>) {
        self.load_stream(pulses.into_iter().map(TapeSpan::Pulse).collect());
    }

    /// Starts or resumes playback from the current tape position.
    pub fn play(&mut self) {
        if self.playing || self.span_idx >= self.spans.len() {
            return;
        }

        self.playing = true;
        if self.countdown == 0 {
            self.start_current_span();
        }
    }

    /// Stops playback without rewinding the current tape position.
    pub fn stop(&mut self) {
        self.playing = false;
    }

    /// Returns whether any tape media is currently loaded.
    #[must_use]
    pub fn has_tape(&self) -> bool {
        !self.spans.is_empty()
    }

    /// Returns whether playback is currently active.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Returns the current signal level on the machine's tape-in pin.
    #[must_use]
    pub fn ear_level(&self) -> bool {
        self.level
    }

    /// Diagnostic: returns the current playback position (span index)
    /// and total span count.
    #[must_use]
    pub fn span_position(&self) -> (usize, usize) {
        (self.span_idx, self.spans.len())
    }

    /// Diagnostic: returns the T-states remaining on the current span
    /// (the countdown).
    #[must_use]
    pub fn span_countdown(&self) -> u32 {
        self.countdown
    }

    /// Diagnostic: returns the current span variant, if any.
    #[must_use]
    pub fn current_span(&self) -> Option<&TapeSpan> {
        self.spans.get(self.span_idx)
    }

    /// Advances the tape by the supplied number of CPU T-states.
    pub fn advance_tstates(&mut self, tstates: u32) {
        if !self.playing {
            return;
        }

        let mut remaining = tstates;
        while remaining > 0 && self.playing {
            if self.countdown > remaining {
                self.countdown -= remaining;
                return;
            }

            remaining -= self.countdown;

            if matches!(self.spans.get(self.span_idx), Some(TapeSpan::Pulse(_))) {
                self.level = !self.level;
            }

            self.countdown = 0;
            self.span_idx += 1;
            self.start_current_span();
        }
    }

    fn start_current_span(&mut self) {
        while self.span_idx < self.spans.len() {
            match self.spans[self.span_idx] {
                TapeSpan::Pulse(0) => {
                    self.level = !self.level;
                    self.span_idx += 1;
                }
                TapeSpan::Pulse(duration) => {
                    self.countdown = duration;
                    return;
                }
                TapeSpan::Level { duration, level } => {
                    self.level = level;
                    if duration == 0 {
                        self.span_idx += 1;
                        continue;
                    }
                    self.countdown = duration;
                    return;
                }
                TapeSpan::Stop => {
                    self.span_idx += 1;
                    self.playing = false;
                    self.countdown = 0;
                    return;
                }
            }
        }

        self.playing = false;
        self.countdown = 0;
    }
}

impl Default for TapePlayer {
    fn default() -> Self {
        Self::new()
    }
}

/// Appends one pulse and flips the level.
///
/// A zero-length pulse flips without emitting a span. TZX produces these, and
/// treating one as a span with a zero countdown gives a player nothing to
/// count down from.
pub fn push_pulse(current_level: &mut bool, duration: u32, spans: &mut Vec<TapeSpan>) {
    if duration == 0 {
        *current_level = !*current_level;
        return;
    }

    spans.push(TapeSpan::Pulse(duration));
    *current_level = !*current_level;
}

/// Appends spans for the supplied data bytes, most-significant bit first.
///
/// Two pulses per bit, which is how every machine here encodes tape data: the
/// bit's value is in the *length* of its pulse pair, not in a level.
pub fn data_block_spans(
    data: &[u8],
    bits_in_last_byte: u8,
    zero_len: u32,
    one_len: u32,
    current_level: &mut bool,
    spans: &mut Vec<TapeSpan>,
) {
    if data.is_empty() {
        return;
    }

    let last_idx = data.len() - 1;
    for (idx, &byte) in data.iter().enumerate() {
        let bits = if idx == last_idx {
            bits_in_last_byte
        } else {
            8
        };
        for bit in (0..bits).rev() {
            let pulse = if byte & (1 << bit) != 0 {
                one_len
            } else {
                zero_len
            };
            push_pulse(current_level, pulse, spans);
            push_pulse(current_level, pulse, spans);
        }
    }
}

/// Appends pause-after-data spans.
///
/// `tstates_per_ms` is explicit because a pause is the one part of a tape
/// stream quoted in real time rather than in clock ticks, so it is the one
/// place a format has to know what clock it is speaking in. The caller decides:
/// a Spectrum ROM block uses the machine's 3,500, and a TZX file uses 3,500 as
/// the format's *reference* clock even when the machine reading it is not a
/// Spectrum.
///
/// Called from the TAP/TZX data-block path, where `pause_ms = 0` means "no
/// pause, run straight into the next block". The TZX standalone Pause block
/// (`0x20`) handles its own `pause = 0` → `Stop` semantic in the parser.
pub fn append_pause_spans(
    pause_ms: u32,
    tstates_per_ms: u32,
    current_level: &mut bool,
    spans: &mut Vec<TapeSpan>,
) {
    if pause_ms == 0 {
        return;
    }

    spans.push(TapeSpan::Level {
        duration: tstates_per_ms,
        level: *current_level,
    });

    if pause_ms > 1 {
        spans.push(TapeSpan::Level {
            duration: (pause_ms - 1) * tstates_per_ms,
            level: false,
        });
        *current_level = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pulse_toggles_the_level_when_it_expires() {
        let mut player = TapePlayer::new();
        player.load_pulses(vec![100, 100]);
        player.play();
        assert!(!player.ear_level());
        player.advance_tstates(100);
        assert!(player.ear_level(), "the first pulse ended and toggled");
        player.advance_tstates(100);
        assert!(!player.ear_level());
    }

    #[test]
    fn a_zero_length_pulse_toggles_without_consuming_time() {
        // TZX can emit these, and a player that waits for a zero countdown
        // never advances past one.
        let mut player = TapePlayer::new();
        player.load_pulses(vec![0, 50]);
        player.play();
        assert!(player.ear_level(), "the zero pulse toggled immediately");
        assert_eq!(player.span_countdown(), 50, "and moved on to the next");
    }

    #[test]
    fn a_level_span_holds_without_an_edge_at_the_end() {
        let mut player = TapePlayer::new();
        player.load_stream(vec![
            TapeSpan::Level {
                duration: 40,
                level: true,
            },
            TapeSpan::Level {
                duration: 40,
                level: true,
            },
        ]);
        player.play();
        assert!(player.ear_level());
        player.advance_tstates(40);
        assert!(
            player.ear_level(),
            "still high — a level span does not toggle"
        );
    }

    #[test]
    fn stop_halts_playback_and_leaves_the_tape_where_it_is() {
        let mut player = TapePlayer::new();
        player.load_stream(vec![
            TapeSpan::Pulse(10),
            TapeSpan::Stop,
            TapeSpan::Pulse(10),
        ]);
        player.play();
        player.advance_tstates(10);
        assert!(!player.is_playing(), "the Stop span halted playback");
        assert!(player.has_tape(), "but the tape is still loaded");
        player.play();
        assert!(player.is_playing(), "and resumes on request");
    }

    #[test]
    fn running_off_the_end_stops_rather_than_looping() {
        let mut player = TapePlayer::new();
        player.load_pulses(vec![10]);
        player.play();
        player.advance_tstates(1_000);
        assert!(!player.is_playing());
    }

    #[test]
    fn pause_zero_emits_nothing() {
        // TZX spec: pause=0 after a data block means "no pause, continue
        // immediately to the next block." Speedlock 7 tapes chain dozens of
        // pure-data blocks via pause=0; an emit-Stop behaviour here broke them
        // by flipping the player out of playing mid-load. Moved from
        // `common-sinclair-zx-spectrum` with the function.
        let mut spans = Vec::new();
        let mut current_level = true;

        append_pause_spans(0, 3_500, &mut current_level, &mut spans);

        assert!(spans.is_empty());
        assert!(current_level, "level unchanged when no pause is emitted");
    }

    #[test]
    fn a_pause_is_measured_in_the_clock_it_is_given() {
        // The same one-millisecond pause is a different number of T-states on
        // a 3.5 MHz Spectrum and a 4 MHz CPC. Passing the clock in is what
        // keeps that conversion visible at the call site.
        let mut level = false;
        let mut spectrum = Vec::new();
        append_pause_spans(2, 3_500, &mut level, &mut spectrum);
        let mut level = false;
        let mut cpc = Vec::new();
        append_pause_spans(2, 4_000, &mut level, &mut cpc);

        assert_eq!(
            spectrum[0],
            TapeSpan::Level {
                duration: 3_500,
                level: false
            }
        );
        assert_eq!(
            cpc[0],
            TapeSpan::Level {
                duration: 4_000,
                level: false
            }
        );
    }

    #[test]
    fn a_data_bit_is_two_pulses_of_the_same_length() {
        let mut level = false;
        let mut spans = Vec::new();
        // 0b1000_0000: one 1-bit then seven 0-bits.
        data_block_spans(&[0x80], 8, 10, 20, &mut level, &mut spans);
        assert_eq!(spans.len(), 16, "eight bits, two pulses each");
        assert_eq!(spans[0], TapeSpan::Pulse(20));
        assert_eq!(spans[1], TapeSpan::Pulse(20));
        assert_eq!(spans[2], TapeSpan::Pulse(10));
    }

    #[test]
    fn advancing_a_stopped_player_does_nothing() {
        let mut player = TapePlayer::new();
        player.load_pulses(vec![10]);
        player.advance_tstates(1_000);
        assert_eq!(player.span_position(), (0, 1), "never started, never moved");
    }
}
