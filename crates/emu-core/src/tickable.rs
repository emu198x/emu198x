//! Trait for components that can be advanced by clock ticks.

use crate::Ticks;

/// A component that can be advanced by clock ticks.
///
/// This is the core abstraction for cycle-accurate emulation. Every component
/// (CPU, video chip, audio chip, etc.) implements this trait.
pub trait Tickable {
    /// Advance the component by one master clock tick.
    ///
    /// Components track their own phase relative to the master clock and
    /// perform work when appropriate (e.g., a CPU running at half the master
    /// clock rate would only do work on every other tick).
    fn tick(&mut self);

    /// Advance the component by multiple ticks.
    ///
    /// Default implementation calls `tick()` in a loop. Components may
    /// override for efficiency, but must produce identical results.
    fn tick_n(&mut self, count: Ticks) {
        for _ in 0..count.get() {
            self.tick();
        }
    }
}
