//! The fundamental unit of time in the emulator.

/// A count of master clock ticks.
///
/// This is the fundamental unit of time in the emulator. All timing is
/// expressed in ticks of the master crystal oscillator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Ticks(pub u64);

impl Ticks {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn new(count: u64) -> Self {
        Self(count)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl core::ops::Add for Ticks {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl core::ops::AddAssign for Ticks {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl core::ops::Sub for Ticks {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }
}
