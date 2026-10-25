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

/// Query paths for tape position, named once so every machine that owns a
/// [`TapePlayer`] answers them identically.
///
/// `tape.loaded` and `tape.playing` alone cannot distinguish a loader that
/// finished from one that gave up: both end with `playing: false`. These say
/// how far the tape actually got.
pub const POSITION_QUERY_PATHS: [&str; 4] = [
    "tape.span_index",
    "tape.span_count",
    "tape.span_countdown",
    "tape.progress",
];

/// Which way [`TapePlayer::seek_block`] moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockSeek {
    /// To the start of the block playing, or of the one before it if the
    /// tape is already at a block's start.
    Previous,
    /// To the start of the next block.
    Next,
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

    /// Where each block starts, found by its gaps.
    ///
    /// The player keeps no block list, so the stream is searched the way a
    /// deck's blank search hears it: a block starts at the tape's first span
    /// and at the first span after each run of gaps. A gap is a low
    /// [`TapeSpan::Level`] lasting at least `min_gap` T-states, or a
    /// [`TapeSpan::Stop`]. Pauses between TAP blocks are gaps; two TZX blocks
    /// with no pause between them read as one block, as on a real deck.
    #[must_use]
    pub fn block_starts(&self, min_gap: u32) -> Vec<usize> {
        let mut starts = Vec::new();
        let mut after_gap = true;
        for (index, span) in self.spans.iter().enumerate() {
            let gap = match *span {
                TapeSpan::Level { duration, level } => !level && duration >= min_gap,
                TapeSpan::Stop => true,
                TapeSpan::Pulse(_) => false,
            };
            if gap {
                after_gap = true;
            } else if after_gap {
                starts.push(index);
                after_gap = false;
            }
        }
        starts
    }

    /// The block the tape is in, counted from 0, and how many blocks it has.
    /// See [`Self::block_starts`] for what counts as a block.
    #[must_use]
    pub fn block_position(&self, min_gap: u32) -> (usize, usize) {
        let starts = self.block_starts(min_gap);
        let current = starts
            .partition_point(|&start| start <= self.span_idx)
            .saturating_sub(1);
        (current, starts.len())
    }

    /// Moves to the start of the previous or next block, and returns whether
    /// it moved.
    ///
    /// Previous goes to the start of the block playing once the tape has
    /// moved into it, and to the block before when it is already at a start,
    /// as a CD player's back button does. Next on the last block, and either
    /// way on a tape with no blocks, does nothing. A playing tape plays on
    /// from the new place; a stopped one stays stopped.
    ///
    /// The level is set to what the stream holds at that point, so the
    /// pulses that follow keep their phase (#1825).
    pub fn seek_block(&mut self, direction: BlockSeek, min_gap: u32) -> bool {
        let starts = self.block_starts(min_gap);
        let current = starts
            .partition_point(|&start| start <= self.span_idx)
            .checked_sub(1);
        let target = match (direction, current) {
            (BlockSeek::Next, None) => starts.first(),
            (BlockSeek::Next, Some(block)) => starts.get(block + 1),
            (BlockSeek::Previous, None) => None,
            (BlockSeek::Previous, Some(block)) if self.span_idx > starts[block] => {
                starts.get(block)
            }
            (BlockSeek::Previous, Some(block)) => block
                .checked_sub(1)
                .and_then(|previous| starts.get(previous)),
        };
        let Some(&target) = target else {
            return false;
        };
        self.span_idx = target;
        self.level = self.level_at(target);
        self.countdown = 0;
        if self.playing {
            self.start_current_span();
        }
        true
    }

    /// The level the stream holds as span `index` begins, replaying the
    /// spans before it as playback would.
    fn level_at(&self, index: usize) -> bool {
        self.spans[..index]
            .iter()
            .fold(false, |level, span| match *span {
                TapeSpan::Pulse(_) => !level,
                TapeSpan::Level { level, .. } => level,
                TapeSpan::Stop => level,
            })
    }

    /// Rewinds to the start of the tape and stops, as a deck's REWIND does.
    ///
    /// The tape is left as [`Self::load_stream`] leaves a new one: first
    /// span, level low, nothing counted down. A machine's stop release, if
    /// it has one, sees an ordinary stop.
    pub fn rewind(&mut self) {
        self.playing = false;
        self.span_idx = 0;
        self.level = false;
        self.countdown = 0;
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

    /// Which span is playing.
    ///
    /// Reaches [`Self::span_count`] once the tape has drained, so
    /// `span_index == span_count` is "played to the end" and anything less is
    /// "stopped part-way".
    #[must_use]
    pub fn span_index(&self) -> usize {
        self.span_idx
    }

    /// How many spans the loaded tape holds.
    #[must_use]
    pub fn span_count(&self) -> usize {
        self.spans.len()
    }

    /// How far through the tape playback has got, from `0.0` to `1.0`.
    ///
    /// An empty deck reads `1.0` rather than `0.0`: there is nothing left to
    /// play, which is the same answer a fully drained tape gives. Callers that
    /// need to tell "no tape" from "finished tape" want [`Self::span_count`].
    #[must_use]
    pub fn progress(&self) -> f64 {
        let total = self.spans.len();
        if total == 0 {
            return 1.0;
        }
        self.span_idx as f64 / total as f64
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

/// Opt-in end-of-tape policy: release the tape-in level a fixed time after
/// playback stops.
///
/// A bare [`TapePlayer`] holds the level its last span left on the pin for as
/// long as the machine runs. A real deck's output is AC-coupled, so once the
/// signal stops the input settles back to its idle level. The Spectrum family
/// opts in with FUSE's behaviour: `tape_stop()` schedules `tape_stop_mic_off`,
/// which clears the input one frame after the tape stops (`fuse-1.7.0/tape.c`).
///
/// It is off unless a machine holds one and drives its player through
/// [`StopRelease::advance`]. Machines whose hardware nobody has checked keep
/// calling [`TapePlayer::advance_tstates`] directly, so their behaviour and
/// their snapshot layout are untouched.
///
/// A stop is noticed on the first `advance` that finds the player stopped, so
/// the release can be late by up to the `tstates` of that call. The machines
/// here advance one T-state at a time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopRelease {
    /// T-states left before the release, while one is pending.
    remaining: Option<u32>,
    /// Whether the player was playing at the end of the previous advance.
    was_playing: bool,
}

impl StopRelease {
    /// Creates the policy with no release pending.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            remaining: None,
            was_playing: false,
        }
    }

    /// Advances `player` by `tstates`, then releases the input to low once
    /// `release_after` T-states have passed since playback stopped. Read the
    /// input through [`Self::input_level`]; the player keeps its own level,
    /// so a tape played again carries on in phase.
    ///
    /// Any stop counts: the stream running out, a `Stop` span, or an explicit
    /// [`TapePlayer::stop`] between calls. Playing again before the release
    /// is due cancels it. The delay is in the player's T-states; a machine
    /// following FUSE passes its own frame length.
    pub fn advance(&mut self, player: &mut TapePlayer, tstates: u32, release_after: u32) {
        player.advance_tstates(tstates);

        if player.playing {
            self.was_playing = true;
            self.remaining = None;
            return;
        }

        if self.was_playing {
            self.was_playing = false;
            self.remaining = Some(release_after);
            return;
        }

        if let Some(left) = self.remaining {
            if left <= tstates {
                self.remaining = None;
            } else {
                self.remaining = Some(left - tstates);
            }
        }
    }

    /// Whether playback has stopped and the release is still to come. Until
    /// then the input still carries the level the tape left on it.
    #[must_use]
    pub const fn pending(&self) -> bool {
        self.remaining.is_some()
    }

    /// The level on the machine's tape-in pin: the tape's own while it
    /// plays and until the release, low after it.
    ///
    /// The release is the pin's, not the tape's. It used to write low into
    /// the player, which moved the tape's phase: a tape stopped on a high
    /// pulse came back inverted, and the first pause span, which holds the
    /// level its block ends on, then swallowed the block's last edge. A
    /// Spectrum stopped mid-pilot to add a POKE reported "R Tape loading
    /// error" on the next block (#1794).
    #[must_use]
    pub fn input_level(&self, player: &TapePlayer) -> bool {
        (player.is_playing() || self.pending()) && player.ear_level()
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

    /// A tape whose last pulse leaves the level high, played to its end.
    fn drained_high(release: &mut StopRelease, after: u32) -> TapePlayer {
        let mut player = TapePlayer::new();
        player.load_pulses(vec![10]);
        player.play();
        for _ in 0..10 {
            release.advance(&mut player, 1, after);
        }
        assert!(!player.is_playing(), "the single pulse drained the tape");
        assert!(player.ear_level(), "and its edge left the level high");
        player
    }

    /// The levels a tape puts on the pin while it plays, one per T-state.
    /// With `stop`, the tape is stopped at that T-state, left long enough
    /// for the release, then played again; the stopped stretch is not
    /// recorded, so an unbroken tape and a resumed one should match.
    fn played_levels(spans: Vec<TapeSpan>, stop: Option<u32>) -> Vec<bool> {
        let mut release = StopRelease::new();
        let mut player = TapePlayer::new();
        player.load_stream(spans);
        player.play();
        let mut levels = Vec::new();
        let mut t = 0;
        while player.is_playing() {
            if Some(t) == stop {
                player.stop();
                for _ in 0..1_000 {
                    release.advance(&mut player, 1, 100);
                }
                assert!(!release.pending(), "the release happened while stopped");
                player.play();
            }
            release.advance(&mut player, 1, 100);
            levels.push(release.input_level(&player));
            t += 1;
        }
        levels
    }

    #[test]
    fn stopping_and_playing_again_keeps_the_tape_in_phase() {
        // Pulses and then a pause, whose first span holds the level the
        // block ended on. Stopped while the level is high, a tape that came
        // back inverted lost the block's last edge to that span (#1794): the
        // Spectrum ROM never saw the end of the last bit.
        let mut level = false;
        let mut spans = Vec::new();
        for _ in 0..5 {
            push_pulse(&mut level, 100, &mut spans);
        }
        append_pause_spans(2, 1_000, &mut level, &mut spans);
        let unbroken = played_levels(spans.clone(), None);
        for stop in [50, 150, 250] {
            assert_eq!(
                played_levels(spans.clone(), Some(stop)),
                unbroken,
                "stopped at T-state {stop}"
            );
        }
    }

    #[test]
    fn rewinding_plays_the_tape_again_exactly_as_from_new() {
        let spans: Vec<TapeSpan> = [70, 30, 50, 90, 10].map(TapeSpan::Pulse).to_vec();
        let levels = |player: &mut TapePlayer| {
            player.play();
            let mut out = Vec::new();
            while player.is_playing() {
                player.advance_tstates(1);
                out.push(player.ear_level());
            }
            out
        };
        let mut fresh = TapePlayer::new();
        fresh.load_stream(spans.clone());
        let expected = levels(&mut fresh);

        let mut player = TapePlayer::new();
        player.load_stream(spans);
        player.play();
        player.advance_tstates(85); // part-way through the second pulse, level high
        assert!(player.ear_level());
        player.rewind();
        assert!(!player.is_playing(), "a deck stops when it has rewound");
        assert_eq!(player.span_index(), 0);
        assert!(!player.ear_level());
        assert_eq!(levels(&mut player), expected);
    }

    /// Three blocks as TAP builds them: pulses, then a pause. The middle
    /// block has an odd number of pulses, so it ends with the level high.
    fn three_blocks() -> Vec<TapeSpan> {
        let mut level = false;
        let mut spans = Vec::new();
        for pulses in [4, 5, 6] {
            for _ in 0..pulses {
                push_pulse(&mut level, 100, &mut spans);
            }
            append_pause_spans(3, 1_000, &mut level, &mut spans);
        }
        spans
    }

    const GAP: u32 = 1_000;

    #[test]
    fn blocks_start_after_each_gap() {
        let mut player = TapePlayer::new();
        player.load_stream(three_blocks());
        // Each block: its pulses, a 1 ms level and a 2 ms low gap.
        assert_eq!(player.block_starts(GAP), vec![0, 6, 13]);
        assert_eq!(player.block_position(GAP), (0, 3));
        let mut empty = TapePlayer::new();
        empty.load_stream(Vec::new());
        assert_eq!(empty.block_position(GAP), (0, 0));
    }

    #[test]
    fn a_stop_span_and_a_run_of_gaps_each_make_one_boundary() {
        let mut player = TapePlayer::new();
        player.load_stream(vec![
            TapeSpan::Pulse(10),
            TapeSpan::Stop,
            TapeSpan::Level {
                duration: 5_000,
                level: false,
            },
            TapeSpan::Pulse(10),
            TapeSpan::Level {
                duration: 10,
                level: false,
            },
            TapeSpan::Pulse(10),
        ]);
        assert_eq!(
            player.block_starts(GAP),
            vec![0, 3],
            "a short low level is not a gap"
        );
    }

    #[test]
    fn next_and_previous_move_like_a_cd_player() {
        let mut player = TapePlayer::new();
        player.load_stream(three_blocks());
        assert!(player.seek_block(BlockSeek::Next, GAP));
        assert_eq!(player.span_index(), 6);
        assert!(player.seek_block(BlockSeek::Next, GAP));
        assert_eq!(player.span_index(), 13);
        assert!(
            !player.seek_block(BlockSeek::Next, GAP),
            "no block after the last"
        );
        assert_eq!(player.span_index(), 13);

        // Part-way into the last block, Previous goes to its start first.
        player.play();
        player.advance_tstates(250);
        assert!(player.span_index() > 13);
        assert!(player.seek_block(BlockSeek::Previous, GAP));
        assert_eq!(player.span_index(), 13);
        assert!(player.is_playing(), "a playing tape plays on");
        player.stop();
        assert!(player.seek_block(BlockSeek::Previous, GAP));
        assert_eq!(player.span_index(), 6);
        assert!(!player.is_playing(), "a stopped tape stays stopped");
        assert!(player.seek_block(BlockSeek::Previous, GAP));
        assert!(
            !player.seek_block(BlockSeek::Previous, GAP),
            "nothing before the first"
        );
        assert_eq!(player.span_index(), 0);
    }

    #[test]
    fn a_block_reached_by_seeking_plays_as_it_does_in_an_unbroken_play() {
        // The phase rule from #1825: after a seek the block must produce
        // exactly the levels it produces when the tape reaches it by playing.
        let levels_from = |player: &mut TapePlayer, from: usize| {
            let mut out = Vec::new();
            while player.is_playing() {
                if player.span_index() >= from {
                    out.push(player.ear_level());
                }
                player.advance_tstates(1);
            }
            out
        };
        for block in [1, 2] {
            let mut unbroken = TapePlayer::new();
            unbroken.load_stream(three_blocks());
            let start = unbroken.block_starts(GAP)[block];
            unbroken.play();
            let expected = levels_from(&mut unbroken, start);

            let mut seeking = TapePlayer::new();
            seeking.load_stream(three_blocks());
            seeking.play();
            seeking.advance_tstates(150); // part-way into the first block
            for _ in 0..block {
                seeking.seek_block(BlockSeek::Next, GAP);
            }
            assert_eq!(levels_from(&mut seeking, start), expected, "block {block}");
        }
    }

    #[test]
    fn a_block_after_a_stop_keeps_the_level_the_stop_left() {
        // No pause resets the level here: three pulses leave it high, and the
        // block after the stop starts high, as playing through would leave it.
        let spans = vec![
            TapeSpan::Pulse(10),
            TapeSpan::Pulse(10),
            TapeSpan::Pulse(10),
            TapeSpan::Stop,
            TapeSpan::Pulse(10),
        ];
        let mut unbroken = TapePlayer::new();
        unbroken.load_stream(spans.clone());
        unbroken.play();
        unbroken.advance_tstates(100);
        assert!(!unbroken.is_playing(), "the stop span stopped it");
        assert!(unbroken.ear_level());

        let mut seeking = TapePlayer::new();
        seeking.load_stream(spans);
        assert!(seeking.seek_block(BlockSeek::Next, GAP));
        assert_eq!(seeking.span_index(), unbroken.span_index());
        assert_eq!(seeking.ear_level(), unbroken.ear_level());
    }

    #[test]
    fn a_bare_player_holds_the_level_after_the_tape_ends() {
        // The default for every machine that does not opt in (the CPC).
        let mut player = TapePlayer::new();
        player.load_pulses(vec![10]);
        player.play();
        player.advance_tstates(10);
        assert!(!player.is_playing());
        for _ in 0..1_000_000 {
            player.advance_tstates(1);
        }
        assert!(player.ear_level(), "nothing releases a bare player's level");
    }

    #[test]
    fn stop_release_drops_the_level_exactly_its_delay_after_the_stop() {
        let mut release = StopRelease::new();
        let mut player = drained_high(&mut release, 69_888);
        assert!(release.pending());
        for t in 1..69_888 {
            release.advance(&mut player, 1, 69_888);
            assert!(
                release.input_level(&player),
                "still held {t} T-states after the stop"
            );
        }
        release.advance(&mut player, 1, 69_888);
        assert!(
            !release.input_level(&player),
            "released one frame after the stop"
        );
        assert!(!release.pending());
    }

    #[test]
    fn stop_release_counts_an_explicit_stop() {
        let mut release = StopRelease::new();
        let mut player = TapePlayer::new();
        player.load_pulses(vec![5, 1_000]);
        player.play();
        for _ in 0..5 {
            release.advance(&mut player, 1, 100);
        }
        assert!(player.ear_level(), "the first pulse ended high");
        player.stop();
        for _ in 0..100 {
            release.advance(&mut player, 1, 100);
        }
        assert!(release.input_level(&player), "held through the delay");
        release.advance(&mut player, 1, 100);
        assert!(!release.input_level(&player), "released after it");
    }

    #[test]
    fn playing_again_before_the_release_cancels_it() {
        let mut release = StopRelease::new();
        let mut player = TapePlayer::new();
        player.load_pulses(vec![5, 1_000]);
        player.play();
        for _ in 0..5 {
            release.advance(&mut player, 1, 100);
        }
        player.stop();
        for _ in 0..50 {
            release.advance(&mut player, 1, 100);
        }
        player.play();
        for _ in 0..200 {
            release.advance(&mut player, 1, 100);
        }
        assert!(player.is_playing());
        assert!(player.ear_level(), "a playing tape keeps its own level");
        assert!(!release.pending());
    }

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

    /// A drained tape and an abandoned one are distinguishable.
    ///
    /// This is the whole point of the position paths. `has_tape` and
    /// `is_playing` cannot tell the two apart — a loader that consumed
    /// everything and one that stopped half way both end up not playing with
    /// a tape still loaded. The span index says which happened.
    #[test]
    fn position_separates_a_drained_tape_from_an_abandoned_one() {
        let pulses = vec![100u32; 8];

        let mut drained = TapePlayer::new();
        drained.load_pulses(pulses.clone());
        drained.play();
        drained.advance_tstates(100 * 8 + 50);

        let mut abandoned = TapePlayer::new();
        abandoned.load_pulses(pulses);
        abandoned.play();
        abandoned.advance_tstates(100 * 3);
        abandoned.stop();

        // Indistinguishable on the old surface.
        assert_eq!(drained.has_tape(), abandoned.has_tape());
        assert!(!drained.is_playing());
        assert!(!abandoned.is_playing());

        // Distinguishable on the new one.
        assert_eq!(drained.span_index(), drained.span_count());
        assert!(abandoned.span_index() < abandoned.span_count());
        assert_eq!(drained.progress(), 1.0);
        assert!(abandoned.progress() < 1.0);
    }

    /// An empty deck reads as finished, not as stalled at the start.
    ///
    /// `0/0` has no meaningful ratio, and reporting `0.0` would make "no tape"
    /// look like "a tape that has not started" — the confusion these paths
    /// exist to remove. `span_count` is how a caller tells them apart.
    #[test]
    fn an_empty_deck_reads_as_finished() {
        let player = TapePlayer::new();
        assert_eq!(player.span_count(), 0);
        assert_eq!(player.span_index(), 0);
        assert_eq!(player.progress(), 1.0);
    }

    /// The count is the tape's, not the playback's, so it does not move.
    #[test]
    fn the_span_count_is_stable_while_the_index_advances() {
        let mut player = TapePlayer::new();
        player.load_pulses(vec![100u32; 5]);
        player.play();

        let count = player.span_count();
        assert_eq!(count, 5);

        let mut seen = vec![player.span_index()];
        for _ in 0..4 {
            player.advance_tstates(100);
            seen.push(player.span_index());
            assert_eq!(player.span_count(), count, "the tape did not get longer");
        }
        assert!(
            seen.windows(2).all(|w| w[1] >= w[0]),
            "the index must not go backwards: {seen:?}",
        );
        assert!(
            seen.last() > seen.first(),
            "the index must advance: {seen:?}"
        );
    }
}
