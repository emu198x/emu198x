//! Pulse-driven tape playback support for Spectrum-family machines.
//!
//! Source references:
//! - `docs/systems/spectrum.md`
//! - Adapted from `../Emu198x-Older/crates/common-sinclair-zx-spectrum/src/tape.rs`
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

// `TapeSpan` and `TapePlayer` moved to `common-tape` on 2026-08-14: they are
// the same model for every machine with a cassette port, and the Amstrad CPC
// needs them. Re-exported here so Spectrum-family code and its 12 consumers
// carry on importing them from this crate. See
// `knowledge/decisions/crate-naming.md`.
pub use common_tape::{TapePlayer, TapeSpan};

/// One standard-speed tape block ready for playback.
///
/// `data` contains the full byte stream as it appears on tape: flag byte,
/// payload bytes, and checksum.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TapeBlock {
    pub flag: u8,
    pub data: Vec<u8>,
}

/// Loading standard-speed Spectrum ROM blocks into a player.
///
/// This is an extension trait rather than a method on [`TapePlayer`] because
/// the player is machine-agnostic and this encoding is not: pilot counts, sync
/// lengths and bit pulses are the Spectrum ROM loader's format. A machine that
/// wants them imports this trait; the Amstrad CPC does not.
pub trait SpectrumTapePlayer {
    /// Converts standard-speed blocks into timing spans and loads them.
    fn load_blocks(&mut self, blocks: Vec<TapeBlock>);
}

impl SpectrumTapePlayer for TapePlayer {
    fn load_blocks(&mut self, blocks: Vec<TapeBlock>) {
        self.load_stream(standard_blocks_to_stream(&blocks));
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

    /// Direct T-state-by-T-state validation that a `Pulse(N)` span
    /// holds the EAR level for exactly N T-states before toggling.
    /// Speedlock-7's pilot-detect threshold is a 2-iteration margin
    /// on a 54-T-state-per-iter loop; if our edge timing is off by
    /// one T-state per pulse the loader's pilot count drifts enough
    /// to fail. This test pins the exact toggle moment.
    #[test]
    fn pulse_span_holds_level_for_exact_tstates_then_toggles() {
        let mut player = TapePlayer::new();
        // Two 5-T-state pulses back-to-back. Initial level is `false`,
        // so after the first pulse the level becomes `true` and after
        // the second it becomes `false` again.
        player.load_pulses(vec![5, 5]);
        player.play();

        // After each `advance_tstates(1)` we read the level. The
        // toggle for pulse 1 must land exactly at T-state 5 (i.e. on
        // the 5th advance call, after that call the level is `true`).
        let levels: Vec<bool> = (1..=10)
            .map(|_| {
                player.advance_tstates(1);
                player.ear_level()
            })
            .collect();

        // Pulse 1 (duration 5): level was `false` for T=1..4, toggles
        // to `true` on T=5.
        assert_eq!(
            levels[..5],
            [false, false, false, false, true],
            "pulse 1 must toggle on the 5th T-state, not the 4th or 6th",
        );
        // Pulse 2 (duration 5): level holds `true` for T=6..9, toggles
        // to `false` on T=10.
        assert_eq!(
            levels[5..10],
            [true, true, true, true, false],
            "pulse 2 must toggle on the 5th T-state of its own span",
        );
    }

    /// Equivalent test using a bulk `advance_tstates(N)` call instead
    /// of N×1. Both code paths run through the same inner loop but
    /// the bulk-advance shortcut at the top of `advance_tstates`
    /// (`if countdown > remaining { countdown -= remaining; return; }`)
    /// hits a different branch — needs verifying separately.
    #[test]
    fn bulk_advance_lands_toggle_at_exact_tstate() {
        let mut player = TapePlayer::new();
        player.load_pulses(vec![100]);
        player.play();

        // 99 T-states in: level still false.
        player.advance_tstates(99);
        assert!(!player.ear_level(), "level must be unchanged at T=99");

        // 1 more T-state lands the toggle: level becomes true at T=100.
        player.advance_tstates(1);
        assert!(player.ear_level(), "level must toggle exactly at T=100");
    }

    /// The pilot-detect threshold for Speedlock-7 requires a 40-
    /// iteration count over a 2 165 T-state pilot pulse. If our edge
    /// timing accumulates *any* drift over 32 consecutive pilot
    /// pulses, the loader rejects. This test verifies 32 back-to-back
    /// 2 165-T-state pulses produce exactly 32 edges at the
    /// expected positions.
    #[test]
    fn speedlock7_pilot_pulses_produce_edges_at_exact_offsets() {
        let mut player = TapePlayer::new();
        player.load_pulses(vec![2165; 32]);
        player.play();

        let mut edges = 0;
        let mut last_level = player.ear_level();
        for t in 1..=(2165 * 32) {
            player.advance_tstates(1);
            let level = player.ear_level();
            if level != last_level {
                // Edge must land exactly on a multiple of 2165.
                assert_eq!(
                    t % 2165,
                    0,
                    "edge {} landed at T={t}, expected multiple of 2165",
                    edges + 1,
                );
                edges += 1;
                last_level = level;
            }
        }
        assert_eq!(edges, 32, "exactly 32 edges expected for 32 pilot pulses");
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
