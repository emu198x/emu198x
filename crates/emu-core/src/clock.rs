//! Master clock configuration.

use crate::Ticks;

/// Master clock configuration for a system.
///
/// Each system has a master crystal that drives all timing. Components may
/// run at divided rates, but everything derives from this frequency.
#[derive(Debug, Clone, Copy)]
pub struct MasterClock {
    /// Crystal frequency in Hz (e.g., `3_546_895` for PAL Spectrum).
    pub frequency_hz: u64,
}

impl MasterClock {
    #[must_use]
    pub const fn new(frequency_hz: u64) -> Self {
        Self { frequency_hz }
    }

    /// Ticks per frame at the given frame rate (integer division).
    #[must_use]
    pub const fn ticks_per_frame(&self, frames_per_second: u64) -> Ticks {
        Ticks::new(self.frequency_hz / frames_per_second)
    }
}

#[cfg(test)]
mod tests {
    use super::MasterClock;
    use crate::Ticks;

    #[test]
    fn ticks_per_frame_uses_integer_division_of_frequency() {
        let clock = MasterClock::new(3_546_895);

        assert_eq!(clock.frequency_hz, 3_546_895);
        assert_eq!(clock.ticks_per_frame(50), Ticks::new(70_937));
    }
}
