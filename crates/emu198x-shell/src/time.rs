//! Time and clock descriptors shared by machine runtimes.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// Monotonic machine timestamp.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct MachineTime(pub u64);

impl MachineTime {
    /// Creates a new timestamp.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the raw machine-time value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Returns a new timestamp advanced by `delta`.
    #[must_use]
    pub const fn saturating_add(self, delta: u64) -> Self {
        Self(self.0.saturating_add(delta))
    }
}

/// Clock frequency expressed as a rational number in Hz.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClockRate {
    /// Frequency numerator in Hz.
    pub numerator_hz: u64,
    /// Frequency denominator in Hz.
    pub denominator_hz: u64,
}

impl ClockRate {
    /// Creates an integer clock frequency in Hz.
    #[must_use]
    pub const fn from_hz(hz: u64) -> Self {
        Self {
            numerator_hz: hz,
            denominator_hz: 1,
        }
    }

    /// Creates a rational clock frequency in Hz.
    #[must_use]
    pub const fn from_ratio(numerator_hz: u64, denominator_hz: u64) -> Self {
        Self {
            numerator_hz,
            denominator_hz,
        }
    }
}

/// Human-readable description of a machine's authoritative clock unit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockDesc {
    /// Stable name of the authoritative time unit.
    pub unit: Cow<'static, str>,
    /// Frequency of the authoritative unit.
    pub rate: ClockRate,
}

impl ClockDesc {
    /// Creates a clock descriptor.
    #[must_use]
    pub fn new(unit: impl Into<Cow<'static, str>>, rate: ClockRate) -> Self {
        Self {
            unit: unit.into(),
            rate,
        }
    }
}
