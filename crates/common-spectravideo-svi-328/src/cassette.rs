//! SVI cassette waveform and transport.
//!
//! Sample counts and framing follow MAME's `svi_cas.cpp`: a 44.1 kHz square
//! wave, 1,600 leader periods, `$7f` sync, and MSB-first payload bytes.

use serde::{Deserialize, Serialize};

const CPU_TSTATE_HZ: u32 = 3_579_545;
const WAVE_SAMPLE_HZ: u32 = 44_100;
const INITIAL_SILENCE: u32 = 200;
const BLOCK_GAP: u32 = 24_220;
const LEADER_PERIODS: usize = 1_600;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct WaveSpan {
    high: bool,
    samples: u32,
}

/// Read-only cassette deck driven by PPI port C motor control.
#[derive(Default, Serialize, Deserialize)]
pub(crate) struct Cassette {
    spans: Vec<WaveSpan>,
    span_index: usize,
    samples_remaining: u32,
    sample_phase: u32,
    motor_running: bool,
}

impl Cassette {
    pub(crate) fn insert(&mut self, blocks: &[Vec<u8>]) {
        self.spans = encode_waveform(blocks);
        self.rewind();
    }

    pub(crate) fn rewind(&mut self) {
        self.span_index = 0;
        self.samples_remaining = self.spans.first().map_or(0, |span| span.samples);
        self.sample_phase = 0;
        self.motor_running = false;
    }

    pub(crate) fn set_motor(&mut self, running: bool) {
        self.motor_running = running;
    }

    pub(crate) fn is_present(&self) -> bool {
        !self.spans.is_empty()
    }

    pub(crate) fn input_high(&self) -> bool {
        self.is_present() && self.span_index < self.spans.len() && self.spans[self.span_index].high
    }

    pub(crate) fn tick_tstate(&mut self) {
        if !self.motor_running || self.span_index >= self.spans.len() {
            return;
        }
        self.sample_phase += WAVE_SAMPLE_HZ;
        while self.sample_phase >= CPU_TSTATE_HZ {
            self.sample_phase -= CPU_TSTATE_HZ;
            self.advance_sample();
        }
    }

    fn advance_sample(&mut self) {
        if self.samples_remaining > 1 {
            self.samples_remaining -= 1;
            return;
        }
        self.span_index += 1;
        self.samples_remaining = self
            .spans
            .get(self.span_index)
            .map_or(0, |span| span.samples);
    }
}

fn encode_waveform(blocks: &[Vec<u8>]) -> Vec<WaveSpan> {
    let mut spans = Vec::new();
    push_span(&mut spans, false, INITIAL_SILENCE);
    for block in blocks {
        for period in 0..LEADER_PERIODS {
            push_zero(&mut spans, period.is_multiple_of(4));
            push_one(&mut spans);
        }
        push_zero(&mut spans, true);
        for _ in 0..7 {
            push_one(&mut spans);
        }
        for byte in block {
            push_zero(&mut spans, true);
            for bit in (0..8).rev() {
                if byte & (1 << bit) == 0 {
                    push_zero(&mut spans, false);
                } else {
                    push_one(&mut spans);
                }
            }
        }
        push_span(&mut spans, true, BLOCK_GAP);
    }
    spans
}

fn push_zero(spans: &mut Vec<WaveSpan>, long_high: bool) {
    push_span(spans, true, if long_high { 21 } else { 18 });
    push_span(spans, false, 19);
}

fn push_one(spans: &mut Vec<WaveSpan>) {
    push_span(spans, true, 9);
    push_span(spans, false, 9);
}

fn push_span(spans: &mut Vec<WaveSpan>, high: bool, samples: u32) {
    if let Some(last) = spans.last_mut()
        && last.high == high
    {
        last.samples += samples;
        return;
    }
    spans.push(WaveSpan { high, samples });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserted_tape_is_stopped_and_low_after_initial_silence() {
        let mut tape = Cassette::default();
        tape.insert(&[vec![0x80]]);
        assert!(tape.is_present());
        assert!(!tape.motor_running);
        assert!(!tape.input_high());
    }

    #[test]
    fn motor_advances_from_silence_into_leader() {
        let mut tape = Cassette::default();
        tape.insert(&[vec![0x80]]);
        tape.set_motor(true);
        for _ in 0..((CPU_TSTATE_HZ / WAVE_SAMPLE_HZ + 1) * INITIAL_SILENCE) {
            tape.tick_tstate();
        }
        assert!(tape.input_high());
    }

    #[test]
    fn stopping_motor_freezes_the_waveform() {
        let mut tape = Cassette::default();
        tape.insert(&[vec![0x80]]);
        let before = (tape.span_index, tape.samples_remaining);
        for _ in 0..10_000 {
            tape.tick_tstate();
        }
        assert_eq!((tape.span_index, tape.samples_remaining), before);
    }
}
