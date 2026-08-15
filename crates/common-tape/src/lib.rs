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
    fn advancing_a_stopped_player_does_nothing() {
        let mut player = TapePlayer::new();
        player.load_pulses(vec![10]);
        player.advance_tstates(1_000);
        assert_eq!(player.span_position(), (0, 1), "never started, never moved");
    }
}
