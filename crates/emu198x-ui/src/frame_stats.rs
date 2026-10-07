//! Opt-in measurements of unique native presentation submissions.
//! These are host timings, not GPU completion or physical monitor timings.
use emu198x_native_video::VideoFilter;
use emu198x_shell::MachineTime;
use std::time::{Duration, Instant};

#[derive(Default)]
pub(crate) struct FrameStats {
    previous: Option<(Instant, MachineTime, VideoFilter, u64)>,
    intervals: Vec<f64>,
    submissions: Vec<f64>,
    emulated_seconds: f64,
    skipped_frames: usize,
}
impl FrameStats {
    pub(crate) fn suspend(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn observe(
        &mut self,
        time: MachineTime,
        clock_hz: f64,
        filter: VideoFilter,
        submit: Duration,
        expected: Duration,
    ) {
        let now = Instant::now();
        let current = (now, time, filter, clock_hz.to_bits());
        let Some((previous, old_time, old_filter, old_clock)) = self.previous else {
            self.previous = Some(current);
            return;
        };
        if time == old_time && filter == old_filter && clock_hz.to_bits() == old_clock {
            return;
        }
        self.previous = Some(current);
        if time <= old_time || filter != old_filter || clock_hz.to_bits() != old_clock {
            self.intervals.clear();
            self.submissions.clear();
            self.emulated_seconds = 0.0;
            self.skipped_frames = 0;
            return;
        }
        let delta = (time.get() - old_time.get()) as f64 / clock_hz;
        self.emulated_seconds += delta;
        self.skipped_frames += usize::from(delta > expected.as_secs_f64() * 1.5);
        self.intervals
            .push(now.duration_since(previous).as_secs_f64() * 1000.0);
        self.submissions.push(submit.as_secs_f64() * 1000.0);
        if self.intervals.len() == 300 {
            let wall_seconds = self.intervals.iter().sum::<f64>() / 1000.0;
            self.intervals.sort_by(f64::total_cmp);
            self.submissions.sort_by(f64::total_cmp);
            eprintln!(
                "native-frame-stats filter={filter} samples=300 wall_seconds={wall_seconds:.4} emulated_seconds={:.4} submissions_hz={:.3} interval_p50_ms={:.3} interval_p95_ms={:.3} submit_p95_ms={:.3} skipped_frame_intervals={}",
                self.emulated_seconds,
                300.0 / wall_seconds,
                self.intervals[149],
                self.intervals[284],
                self.submissions[284],
                self.skipped_frames
            );
            self.intervals.clear();
            self.submissions.clear();
            self.emulated_seconds = 0.0;
            self.skipped_frames = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redraws_do_not_add_samples_and_occlusion_discards_partial_windows() {
        let mut stats = FrameStats::default();
        let expected = Duration::from_millis(20);
        stats.observe(
            MachineTime::new(20),
            1000.0,
            VideoFilter::Monitor,
            Duration::ZERO,
            expected,
        );
        stats.observe(
            MachineTime::new(20),
            1000.0,
            VideoFilter::Monitor,
            Duration::ZERO,
            expected,
        );
        assert!(stats.intervals.is_empty());
        stats.observe(
            MachineTime::new(40),
            1000.0,
            VideoFilter::Monitor,
            Duration::ZERO,
            expected,
        );
        assert_eq!(stats.intervals.len(), 1);
        stats.suspend();
        assert!(stats.previous.is_none());
        assert!(stats.intervals.is_empty());
        assert_eq!(stats.emulated_seconds, 0.0);
        stats.observe(
            MachineTime::new(1000),
            1000.0,
            VideoFilter::Monitor,
            Duration::ZERO,
            expected,
        );
        assert!(stats.intervals.is_empty());
    }
}
