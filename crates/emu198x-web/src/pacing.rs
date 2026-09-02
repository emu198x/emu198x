//! Frame pacing for a browser host.
//!
//! `requestAnimationFrame` fires at the display's refresh rate — typically
//! 60 Hz, and 120 Hz or more on some hardware. Emulated machines run at their
//! own rate: 50.08 Hz for a PAL Spectrum, 60.10 Hz for an NTSC NES. Running
//! one machine frame per callback ties the machine's speed to the viewer's
//! monitor, which is both wrong and silently wrong — a 20% fast Spectrum
//! still looks like a Spectrum.
//!
//! [`Pacer`] converts elapsed wall-clock time into whole machine frames.

/// Converts elapsed real time into whole machine frames to run.
#[derive(Debug, Clone)]
pub struct Pacer {
    frame_ms: f64,
    accumulated_ms: f64,
}

impl Pacer {
    /// Most frames run for a single tick.
    ///
    /// A tab that was backgrounded, or a machine that stalled behind a
    /// breakpoint, can come back owing seconds of frames. Running them all
    /// would fast-forward the emulation at whatever speed the host manages,
    /// which is worse than dropping the backlog: the viewer sees the machine
    /// sprint through what they missed instead of resuming from now.
    pub const MAX_FRAMES_PER_TICK: u32 = 4;

    /// Creates a pacer for a machine whose frame lasts `frame_ms`.
    #[must_use]
    pub const fn new(frame_ms: f64) -> Self {
        Self {
            frame_ms,
            accumulated_ms: 0.0,
        }
    }

    /// The machine's frame duration in milliseconds.
    #[must_use]
    pub const fn frame_ms(&self) -> f64 {
        self.frame_ms
    }

    /// Accumulates `elapsed_ms` and returns how many whole frames to run.
    ///
    /// The remainder carries, so a 60 Hz tick driving a 50 Hz machine yields
    /// frames in the 1,1,1,1,0 pattern that averages out correctly rather
    /// than rounding up every tick.
    pub fn frames_owed(&mut self, elapsed_ms: f64) -> u32 {
        // A negative or non-finite delta means the clock moved in a way we
        // cannot reason about; treat it as no time passing rather than
        // poisoning the accumulator with a NaN that never recovers.
        if !elapsed_ms.is_finite() || elapsed_ms < 0.0 {
            return 0;
        }
        if self.frame_ms <= 0.0 {
            return 0;
        }

        self.accumulated_ms += elapsed_ms;

        let mut owed = 0;
        while self.accumulated_ms >= self.frame_ms && owed < Self::MAX_FRAMES_PER_TICK {
            self.accumulated_ms -= self.frame_ms;
            owed += 1;
        }

        // Still owing more than a tick's worth after the cap means a real
        // stall, not jitter. Resume from now instead of replaying it.
        if self.accumulated_ms > self.frame_ms * f64::from(Self::MAX_FRAMES_PER_TICK) {
            self.accumulated_ms = 0.0;
        }

        owed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PAL Spectrum: 69888 T-states at 3.5 MHz.
    const SPECTRUM_FRAME_MS: f64 = 19.968;

    #[test]
    fn a_sixty_hertz_tick_does_not_run_sixty_frames_a_second() {
        let mut pacer = Pacer::new(SPECTRUM_FRAME_MS);
        let mut frames = 0;
        for _ in 0..60 {
            frames += pacer.frames_owed(1000.0 / 60.0);
        }
        assert!(
            (50..=51).contains(&frames),
            "a second of 60 Hz ticks ran {frames} frames; the machine runs at ~50 Hz"
        );
    }

    #[test]
    fn a_hundred_and_twenty_hertz_display_does_not_double_the_speed() {
        let mut pacer = Pacer::new(SPECTRUM_FRAME_MS);
        let mut frames = 0;
        for _ in 0..120 {
            frames += pacer.frames_owed(1000.0 / 120.0);
        }
        assert!(
            (50..=51).contains(&frames),
            "a second of 120 Hz ticks ran {frames} frames"
        );
    }

    #[test]
    fn the_fractional_remainder_carries_between_ticks() {
        let mut pacer = Pacer::new(SPECTRUM_FRAME_MS);
        assert_eq!(pacer.frames_owed(10.0), 0, "half a frame is not a frame");
        assert_eq!(pacer.frames_owed(10.0), 1, "the two halves make one frame");
    }

    #[test]
    fn a_long_stall_is_dropped_rather_than_replayed() {
        let mut pacer = Pacer::new(SPECTRUM_FRAME_MS);
        let owed = pacer.frames_owed(5_000.0);
        assert!(
            owed <= Pacer::MAX_FRAMES_PER_TICK,
            "a five-second stall owed {owed} frames"
        );
        // And the backlog is gone, rather than dribbling out over later ticks.
        assert_eq!(pacer.frames_owed(0.0), 0, "the backlog outlived the stall");
    }

    #[test]
    fn a_nonsense_delta_does_not_poison_the_accumulator() {
        let mut pacer = Pacer::new(SPECTRUM_FRAME_MS);
        assert_eq!(pacer.frames_owed(f64::NAN), 0);
        assert_eq!(pacer.frames_owed(-100.0), 0);
        // A NaN that reached the accumulator would make every later
        // comparison false, and the machine would never run again.
        assert_eq!(pacer.frames_owed(SPECTRUM_FRAME_MS), 1);
    }
}
