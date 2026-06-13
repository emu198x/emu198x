//! Paula analog output filter chain.
//!
//! Between the DAC and the RCA jacks the Amiga runs the mixed stereo
//! signal through up to three analog filter stages. We model the vAmiga
//! topology (`AudioFilter.cpp`), distilled in
//! `syntheses/commodore-amiga/amiga-paula-audio-model.md`:
//!
//! 1. **Static low-pass** — 1-pole RC, R=360Ω / C=100nF → ~4421 Hz.
//!    Fitted on the A500/A1000 only; the A1200 omits it.
//! 2. **LED filter** — 2-pole Sallen-Key biquad, R1=R2=10kΩ,
//!    C1=6.8nF / C2=3.9nF → ~9777 Hz with a high Q (~20.9, a resonant
//!    peak). Switchable via CIA-A PRA bit 1 (the power-LED line):
//!    bright LED = filter on. The A1000 wires it always-on.
//! 3. **Static high-pass** — 1-pole RC DC blocker at ~5 Hz, always on.
//!
//! Coefficients are computed once for the host sample rate. The IIR
//! state is transient (a few samples) and intentionally not part of the
//! snapshot — a restore drops it and re-settles within microseconds.

use crate::profiles::Model;
use crate::runtime::AUDIO_SAMPLE_RATE_HZ;

use std::f64::consts::PI;

// ── Component values (vAmiga AudioFilter.cpp:245-266) ────────────────
const STATIC_LP_R: f64 = 360.0;
const STATIC_LP_C: f64 = 1e-7;
const LED_R1: f64 = 10_000.0;
const LED_R2: f64 = 10_000.0;
const LED_C1: f64 = 6.8e-9;
const LED_C2: f64 = 3.9e-9;
const HP_R_OCS: f64 = 1390.0;
const HP_C_OCS: f64 = 2.233e-5;
const HP_R_AGA: f64 = 1360.0;
const HP_C_AGA: f64 = 2.2e-5;

/// One-pole IIR, used both as a low-pass (`apply_lp`) and — by
/// subtracting the low-passed signal from the input — a high-pass
/// (`apply_hp`). Holds independent left/right history.
#[derive(Clone, Copy)]
struct OnePole {
    a1: f64,
    a2: f64,
    sl: f64,
    sr: f64,
}

impl OnePole {
    /// Matched Z-transform one-pole, from a cutoff in Hz
    /// (vAmiga `OnePoleFilter::setup`).
    fn new(sample_rate: f64, cutoff: f64) -> Self {
        let cutoff = cutoff.min(sample_rate / 2.0 - 1e-4);
        let a = 2.0 - ((2.0 * PI * cutoff) / sample_rate).cos();
        let b = a - (a * a - 1.0).sqrt();
        Self {
            a1: 1.0 - b,
            a2: b,
            sl: 0.0,
            sr: 0.0,
        }
    }

    /// Build from physical R/C values: `f_c = 1 / (2π·R·C)`.
    fn from_rc(sample_rate: f64, r: f64, c: f64) -> Self {
        Self::new(sample_rate, 1.0 / (2.0 * PI * r * c))
    }

    fn apply_lp(&mut self, l: f64, r: f64) -> (f64, f64) {
        self.sl = self.a1 * l + self.a2 * self.sl;
        self.sr = self.a1 * r + self.a2 * self.sr;
        (self.sl, self.sr)
    }

    /// `HP(x) = x - LP(x)` (vAmiga `OnePoleFilter::applyHP`).
    fn apply_hp(&mut self, l: f64, r: f64) -> (f64, f64) {
        self.sl = self.a1 * l + self.a2 * self.sl;
        self.sr = self.a1 * r + self.a2 * self.sr;
        (l - self.sl, r - self.sr)
    }
}

/// Two-pole (biquad) low-pass in Direct Form I, with independent
/// left/right history (vAmiga `TwoPoleFilter`).
#[derive(Clone, Copy)]
struct TwoPole {
    a1: f64,
    a2: f64,
    b1: f64,
    b2: f64,
    // [x[n-1], x[n-2], y[n-1], y[n-2]] per channel.
    l: [f64; 4],
    r: [f64; 4],
}

impl TwoPole {
    /// Sallen-Key biquad from the four R/C values
    /// (vAmiga `AudioFilter::setupLedFilter`).
    fn sallen_key(sample_rate: f64, r1: f64, r2: f64, c1: f64, c2: f64) -> Self {
        let rc = (r1 * r2 * c1 * c2).sqrt();
        let cutoff = (1.0 / (2.0 * PI * rc)).min(sample_rate / 2.0 - 1e-4);
        let q = rc / (c2 * (r1 + r2));

        let a = 1.0 / ((2.0 * PI * cutoff) / sample_rate).tan();
        let b = 1.0 / q;
        let a1 = 1.0 / (1.0 + b * a + a * a);
        Self {
            a1,
            a2: 2.0 * a1,
            b1: 2.0 * (1.0 - a * a) * a1,
            b2: (1.0 - b * a + a * a) * a1,
            l: [0.0; 4],
            r: [0.0; 4],
        }
    }

    fn apply(&mut self, l: f64, r: f64) -> (f64, f64) {
        let yl = self.a1 * l + self.a2 * self.l[0] + self.a1 * self.l[1]
            - self.b1 * self.l[2]
            - self.b2 * self.l[3];
        let yr = self.a1 * r + self.a2 * self.r[0] + self.a1 * self.r[1]
            - self.b1 * self.r[2]
            - self.b2 * self.r[3];
        self.l = [l, self.l[0], yl, self.l[2]];
        self.r = [r, self.r[0], yr, self.r[2]];
        (yl, yr)
    }
}

/// The Amiga's analog output filter chain for one machine model.
#[derive(Clone, Copy)]
pub struct AmigaAudioFilter {
    /// Static low-pass — present on A500/A1000, absent on A1200.
    lo: Option<OnePole>,
    /// LED filter (switchable, or always-on on the A1000).
    led: TwoPole,
    /// DC-blocking high-pass — always on.
    hi: OnePole,
    /// A1000 wires the LED filter permanently on (no CIA bypass).
    led_always_on: bool,
}

impl AmigaAudioFilter {
    /// Build the chain for a model. The static low-pass is fitted on
    /// the A500/A1000 and dropped on the A1200; the high-pass cutoff
    /// differs slightly on the A1200; and the A1000 forces the LED
    /// stage on.
    #[must_use]
    pub fn for_model(model: Model) -> Self {
        let fs = f64::from(AUDIO_SAMPLE_RATE_HZ);
        let led = TwoPole::sallen_key(fs, LED_R1, LED_R2, LED_C1, LED_C2);
        match model {
            Model::A1200AgaPal | Model::A1200AgaNtsc => Self {
                lo: None,
                led,
                hi: OnePole::from_rc(fs, HP_R_AGA, HP_C_AGA),
                led_always_on: false,
            },
            Model::A1000OcsPal | Model::A1000OcsNtsc => Self {
                lo: Some(OnePole::from_rc(fs, STATIC_LP_R, STATIC_LP_C)),
                led,
                hi: OnePole::from_rc(fs, HP_R_OCS, HP_C_OCS),
                led_always_on: true,
            },
            // A500, A500+A501, A500+, A600, maxed configs, …
            _ => Self {
                lo: Some(OnePole::from_rc(fs, STATIC_LP_R, STATIC_LP_C)),
                led,
                hi: OnePole::from_rc(fs, HP_R_OCS, HP_C_OCS),
                led_always_on: false,
            },
        }
    }

    /// Filter one stereo host sample in place. `led_bright` is CIA-A's
    /// power-LED state (PRA bit 1 clear) — when the LED is bright the
    /// switchable filter is engaged.
    pub fn apply(&mut self, left: &mut f32, right: &mut f32, led_bright: bool) {
        let (mut l, mut r) = (f64::from(*left), f64::from(*right));
        if let Some(lo) = &mut self.lo {
            (l, r) = lo.apply_lp(l, r);
        }
        if self.led_always_on || led_bright {
            (l, r) = self.led.apply(l, r);
        }
        (l, r) = self.hi.apply_hp(l, r);
        *left = l as f32;
        *right = r as f32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Peak |output| over the final cycle of a steady sine of frequency
    /// `freq`, after letting the filter settle. Mono (left channel).
    fn steady_state_gain(filter: &mut AmigaAudioFilter, freq: f64, led: bool) -> f32 {
        let fs = f64::from(AUDIO_SAMPLE_RATE_HZ);
        let samples_per_cycle = (fs / freq) as usize;
        let settle = samples_per_cycle * 50;
        let mut peak = 0.0f32;
        for n in 0..(settle + samples_per_cycle) {
            let x = (2.0 * PI * freq * n as f64 / fs).sin() as f32;
            let (mut l, mut r) = (x, x);
            filter.apply(&mut l, &mut r, led);
            if n >= settle {
                peak = peak.max(l.abs());
            }
        }
        peak
    }

    #[test]
    fn dc_offset_is_removed_by_the_high_pass() {
        let mut f = AmigaAudioFilter::for_model(Model::A500OcsPal);
        let mut last = (0.0f32, 0.0f32);
        for _ in 0..20_000 {
            let (mut l, mut r) = (0.5f32, 0.5f32);
            f.apply(&mut l, &mut r, false);
            last = (l, r);
        }
        assert!(
            last.0.abs() < 0.01 && last.1.abs() < 0.01,
            "DC should settle toward zero; got {last:?}"
        );
    }

    #[test]
    fn low_frequencies_pass_roughly_unattenuated() {
        let mut f = AmigaAudioFilter::for_model(Model::A500OcsPal);
        let gain = steady_state_gain(&mut f, 500.0, true);
        assert!(gain > 0.9, "500 Hz should pass nearly unity; got {gain}");
    }

    #[test]
    fn high_frequencies_are_attenuated() {
        let mut f = AmigaAudioFilter::for_model(Model::A500OcsPal);
        let gain = steady_state_gain(&mut f, 15_000.0, true);
        assert!(gain < 0.3, "15 kHz should be well attenuated; got {gain}");
    }

    #[test]
    fn led_filter_attenuates_more_when_engaged() {
        let mut on = AmigaAudioFilter::for_model(Model::A500OcsPal);
        let mut off = AmigaAudioFilter::for_model(Model::A500OcsPal);
        let with_led = steady_state_gain(&mut on, 14_000.0, true);
        let without_led = steady_state_gain(&mut off, 14_000.0, false);
        assert!(
            with_led < without_led,
            "engaging the LED filter must cut more high-frequency energy; \
             on={with_led} off={without_led}"
        );
    }

    #[test]
    fn a1200_lacks_the_static_low_pass() {
        // With the LED filter off, the only difference between the A500
        // and A1200 chains is the A500's static low-pass. At a mid
        // frequency below the LED cutoff, the A1200 (no static LP) must
        // pass more energy than the A500.
        let mut a500 = AmigaAudioFilter::for_model(Model::A500OcsPal);
        let mut a1200 = AmigaAudioFilter::for_model(Model::A1200AgaPal);
        let a500_gain = steady_state_gain(&mut a500, 6_000.0, false);
        let a1200_gain = steady_state_gain(&mut a1200, 6_000.0, false);
        assert!(
            a1200_gain > a500_gain,
            "A1200 omits the static low-pass so it should pass more at 6 kHz; \
             a1200={a1200_gain} a500={a500_gain}"
        );
    }

    #[test]
    fn output_stays_finite() {
        let mut f = AmigaAudioFilter::for_model(Model::A1000OcsPal);
        for n in 0..10_000 {
            let x = (2.0 * PI * 9_777.0 * n as f64 / f64::from(AUDIO_SAMPLE_RATE_HZ)).sin() as f32;
            let (mut l, mut r) = (x, x);
            f.apply(&mut l, &mut r, true);
            assert!(
                l.is_finite() && r.is_finite(),
                "filter output must stay finite"
            );
        }
    }
}
