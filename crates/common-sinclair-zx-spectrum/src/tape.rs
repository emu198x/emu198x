//! Pulse-driven tape playback support for Spectrum-family machines.
//!
//! Source references:
//! - `docs/systems/spectrum.md`
//! - Adapted from `/Users/stevehill/Projects/Emu198x-Older/crates/common-sinclair-zx-spectrum/src/tape.rs`
//!
//! Standard ROM-speed TAP blocks and TZX timing blocks both reduce to a
//! machine-facing stream of timing spans. Most spans are pulse lengths that end
//! with one EAR edge, but TZX pauses and signal-level directives also require
//! explicit level holds and stop markers.

/// Standard ROM tape pilot pulse length in T-states.
pub const PILOT_PULSE: u32 = 2_168;

/// Standard ROM tape first sync pulse length in T-states.
pub const SYNC1_PULSE: u32 = 667;

/// Standard ROM tape second sync pulse length in T-states.
pub const SYNC2_PULSE: u32 = 735;

/// Standard ROM tape zero-bit pulse length in T-states.
pub const ZERO_PULSE: u32 = 855;

/// Standard ROM tape one-bit pulse length in T-states.
pub const ONE_PULSE: u32 = 1_710;

/// Standard ROM pilot pulse count for header blocks.
pub const PILOT_COUNT_HEADER: u32 = 8_063;

/// Standard ROM pilot pulse count for data blocks.
pub const PILOT_COUNT_DATA: u32 = 3_223;

/// Standard ROM inter-block pause length in milliseconds.
pub const PAUSE_MS: u32 = 1_000;

/// CPU T-states per millisecond at 3.5 MHz.
pub const TSTATES_PER_MS: u32 = 3_500;

/// One machine-facing tape timing span.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TapeSpan {
    /// Hold the current level for `duration` T-states, then toggle it.
    Pulse(u32),
    /// Hold `level` for `duration` T-states without an edge at the end.
    Level { duration: u32, level: bool },
    /// Stop playback and wait for explicit user resume.
    Stop,
}

/// One standard-speed tape block ready for playback.
///
/// `data` contains the full byte stream as it appears on tape: flag byte,
/// payload bytes, and checksum.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TapeBlock {
    pub flag: u8,
    pub data: Vec<u8>,
}

/// Tape player that advances through one timing-span stream.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

    /// Converts standard-speed blocks into timing spans and loads them.
    pub fn load_blocks(&mut self, blocks: Vec<TapeBlock>) {
        self.load_stream(standard_blocks_to_stream(&blocks));
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

    /// Returns the current EAR level.
    #[must_use]
    pub fn ear_level(&self) -> bool {
        self.level
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

fn standard_blocks_to_stream(blocks: &[TapeBlock]) -> Vec<TapeSpan> {
    let mut current_level = false;
    let mut spans = Vec::new();
    for block in blocks {
        standard_block_spans(block, PAUSE_MS, &mut current_level, &mut spans);
    }
    spans
}

/// Appends the standard ROM timing stream for one tape block.
pub fn standard_block_spans(
    block: &TapeBlock,
    pause_ms: u32,
    current_level: &mut bool,
    spans: &mut Vec<TapeSpan>,
) {
    let pilot_count = if block.flag < 0x80 {
        PILOT_COUNT_HEADER
    } else {
        PILOT_COUNT_DATA
    };

    for _ in 0..pilot_count {
        push_pulse(current_level, PILOT_PULSE, spans);
    }

    push_pulse(current_level, SYNC1_PULSE, spans);
    push_pulse(current_level, SYNC2_PULSE, spans);
    data_block_spans(&block.data, 8, ZERO_PULSE, ONE_PULSE, current_level, spans);
    append_pause_spans(pause_ms, current_level, spans);
}

/// Appends spans for the supplied data bytes, most-significant bit first.
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

/// Appends pause-after-data spans. Called from the TAP/TZX data-
/// block path (where pause_ms=0 means "no pause, run straight into
/// the next block"). The TZX standalone Pause block (0x20) handles
/// its own pause=0 → Stop semantic separately in its parser.
pub fn append_pause_spans(pause_ms: u32, current_level: &mut bool, spans: &mut Vec<TapeSpan>) {
    if pause_ms == 0 {
        return;
    }

    spans.push(TapeSpan::Level {
        duration: TSTATES_PER_MS,
        level: *current_level,
    });

    if pause_ms > 1 {
        spans.push(TapeSpan::Level {
            duration: (pause_ms - 1) * TSTATES_PER_MS,
            level: false,
        });
        *current_level = false;
    }
}

fn push_pulse(current_level: &mut bool, duration: u32, spans: &mut Vec<TapeSpan>) {
    if duration == 0 {
        *current_level = !*current_level;
        return;
    }

    spans.push(TapeSpan::Pulse(duration));
    *current_level = !*current_level;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tape_player_toggles_ear_across_pulses() {
        let mut player = TapePlayer::new();

        assert!(!player.is_playing());
        assert!(!player.ear_level());

        player.load_pulses(vec![100, 200, 300]);
        player.play();
        assert!(player.has_tape());
        assert!(player.is_playing());
        assert!(!player.ear_level());

        player.advance_tstates(100);
        assert!(player.ear_level());

        player.advance_tstates(200);
        assert!(!player.ear_level());

        player.advance_tstates(300);
        assert!(player.ear_level());
        assert!(!player.is_playing());
    }

    #[test]
    fn tape_player_resume_preserves_partial_pulse_progress() {
        let mut player = TapePlayer::new();
        player.load_pulses(vec![10, 10]);
        player.play();

        player.advance_tstates(4);
        player.stop();
        assert!(!player.is_playing());
        assert!(!player.ear_level());

        player.play();
        player.advance_tstates(6);
        assert!(player.ear_level());
    }

    #[test]
    fn tape_player_level_spans_hold_exact_level() {
        let mut player = TapePlayer::new();
        player.load_stream(vec![
            TapeSpan::Level {
                duration: 5,
                level: true,
            },
            TapeSpan::Level {
                duration: 7,
                level: false,
            },
        ]);

        player.play();
        assert!(player.ear_level());

        player.advance_tstates(5);
        assert!(!player.ear_level());

        player.advance_tstates(7);
        assert!(!player.is_playing());
        assert!(!player.ear_level());
    }

    #[test]
    fn tape_player_stop_span_requires_explicit_resume() {
        let mut player = TapePlayer::new();
        player.load_stream(vec![TapeSpan::Pulse(1), TapeSpan::Stop, TapeSpan::Pulse(1)]);

        player.play();
        player.advance_tstates(1);
        assert!(!player.is_playing());
        assert!(player.ear_level());

        player.play();
        assert!(player.is_playing());
        assert!(player.ear_level());

        player.advance_tstates(1);
        assert!(!player.ear_level());
        assert!(!player.is_playing());
    }

    #[test]
    fn standard_block_generates_header_pilot_sync_and_pause_spans() {
        let block = TapeBlock {
            flag: 0x00,
            data: vec![0x00],
        };
        let mut spans = Vec::new();
        let mut current_level = false;

        standard_block_spans(&block, 2, &mut current_level, &mut spans);

        let pilot_count = spans
            .iter()
            .take_while(|span| matches!(span, TapeSpan::Pulse(PILOT_PULSE)))
            .count();
        assert_eq!(pilot_count, PILOT_COUNT_HEADER as usize);
        assert_eq!(spans[pilot_count], TapeSpan::Pulse(SYNC1_PULSE));
        assert_eq!(spans[pilot_count + 1], TapeSpan::Pulse(SYNC2_PULSE));
        assert_eq!(
            spans.last(),
            Some(&TapeSpan::Level {
                duration: TSTATES_PER_MS,
                level: false,
            })
        );
        assert!(!current_level);
    }

    #[test]
    fn data_block_spans_follow_bit_values() {
        let mut spans = Vec::new();
        let mut current_level = false;
        data_block_spans(
            &[0xA5],
            8,
            ZERO_PULSE,
            ONE_PULSE,
            &mut current_level,
            &mut spans,
        );

        assert_eq!(spans.len(), 16);
        assert_eq!(spans[0], TapeSpan::Pulse(ONE_PULSE));
        assert_eq!(spans[1], TapeSpan::Pulse(ONE_PULSE));
        assert_eq!(spans[2], TapeSpan::Pulse(ZERO_PULSE));
        assert_eq!(spans[3], TapeSpan::Pulse(ZERO_PULSE));
        assert_eq!(spans[14], TapeSpan::Pulse(ONE_PULSE));
        assert_eq!(spans[15], TapeSpan::Pulse(ONE_PULSE));
        assert!(!current_level);
    }

    #[test]
    fn data_block_spans_honor_partial_last_byte() {
        let mut spans = Vec::new();
        let mut current_level = false;
        data_block_spans(
            &[0xFF],
            3,
            ZERO_PULSE,
            ONE_PULSE,
            &mut current_level,
            &mut spans,
        );

        assert_eq!(
            spans,
            vec![
                TapeSpan::Pulse(ONE_PULSE),
                TapeSpan::Pulse(ONE_PULSE),
                TapeSpan::Pulse(ONE_PULSE),
                TapeSpan::Pulse(ONE_PULSE),
                TapeSpan::Pulse(ONE_PULSE),
                TapeSpan::Pulse(ONE_PULSE),
            ]
        );
        assert!(!current_level);
    }

    #[test]
    fn pause_zero_emits_nothing() {
        // TZX spec: pause=0 after a data block means "no pause,
        // continue immediately to the next block." Speedlock 7 tapes
        // chain dozens of pure-data blocks via pause=0; the old
        // emit-Stop behaviour broke them by flipping tape.is_playing
        // false mid-load.
        let mut spans = Vec::new();
        let mut current_level = true;

        append_pause_spans(0, &mut current_level, &mut spans);

        assert!(spans.is_empty());
        assert!(current_level, "level unchanged when no pause is emitted");
    }
}
