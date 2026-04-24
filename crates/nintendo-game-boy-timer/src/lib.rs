//! Game Boy timer — DIV / TIMA / TMA / TAC.
//!
//! The DMG timer is built around a 16-bit free-running counter that
//! ticks every T-cycle. The high byte of that counter is exposed as
//! `DIV` ($FF04). `TIMA` ($FF05) is a separate 8-bit counter that
//! increments on the *falling edge* of `(timer_enable AND
//! selected_counter_bit)`; the bit is selected by `TAC` ($FF07) bits
//! 1-0 and the enable lives in `TAC` bit 2.
//!
//! | TAC bits 1-0 | Period (T-cycles) | Counter bit |
//! |--------------|-------------------|-------------|
//! | `00`         | 1024              | 9           |
//! | `01`         | 16                | 3           |
//! | `10`         | 64                | 5           |
//! | `11`         | 256               | 7           |
//!
//! When `TIMA` overflows it stays at `$00` for one m-cycle, then
//! reloads from `TMA` ($FF06) and latches a timer interrupt source the
//! machine OR's into `IF` bit 2. The falling-edge logic is what
//! produces the documented "DIV-write glitch" (a write to DIV with
//! the selected bit currently high triggers a TIMA increment) and the
//! "TAC-write glitch" (a TAC change that flips the selected-bit state
//! from 1 to 0 also triggers).
//!
//! Ported from `~/Projects/Emu198x-Zig/src/timer.zig`.

#![cfg_attr(not(test), no_std)]

use serde::{Deserialize, Serialize};

/// MMIO addresses for the timer registers.
pub const REG_DIV: u16 = 0xFF04;
pub const REG_TIMA: u16 = 0xFF05;
pub const REG_TMA: u16 = 0xFF06;
pub const REG_TAC: u16 = 0xFF07;

/// `IF` bit position the machine sets when a TIMA overflow latches.
pub const IF_TIMER_BIT: u8 = 2;

/// Timer block. Tick at the master clock rate (one call per T-cycle)
/// or use [`Timer::tick_m`] for one call per CPU m-cycle.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Timer {
    /// 16-bit free-running counter. The CPU sees the high byte as DIV.
    pub counter: u16,
    /// $FF05.
    pub tima: u8,
    /// $FF06.
    pub tma: u8,
    /// $FF07.
    pub tac: u8,
    /// True when TIMA has overflowed since the machine last consumed
    /// the strobe. The machine reads via [`Timer::consume_overflow`]
    /// and OR's the bit into `IF`.
    pub overflow_latched: bool,
    /// T-cycles remaining before an overflowed TIMA reloads from TMA.
    /// A value of 0 means no reload is pending.
    #[serde(default)]
    pub reload_delay: u8,
    /// True only for the T-cycle where the delayed reload happened.
    /// TIMA writes on that same cycle are ignored by hardware.
    #[serde(default)]
    pub reloaded_this_t_cycle: bool,
}

impl Timer {
    /// Create a fresh timer with all registers zero.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            counter: 0,
            tima: 0,
            tma: 0,
            tac: 0,
            overflow_latched: false,
            reload_delay: 0,
            reloaded_this_t_cycle: false,
        }
    }

    /// Advance the timer by one T-cycle.
    pub fn tick_t(&mut self) {
        self.advance_reload_delay();

        let old_bit = self.timer_bit();
        self.counter = self.counter.wrapping_add(1);
        let new_bit = self.timer_bit();
        if old_bit && !new_bit {
            self.increment_tima();
        }
    }

    /// Convenience: advance one CPU m-cycle (4 T-cycles). Use this
    /// when the machine drives the timer from the SM83's
    /// [`tick`](https://docs.rs/sharp-lr35902) cadence.
    pub fn tick_m(&mut self) {
        for _ in 0..4 {
            self.tick_t();
        }
    }

    /// Read DIV ($FF04) — the high byte of the internal counter.
    #[must_use]
    pub fn read_div(&self) -> u8 {
        (self.counter >> 8) as u8
    }

    /// Write DIV ($FF04). Resets the entire 16-bit counter to zero.
    /// If the selected timer bit was high, the reset causes a falling
    /// edge that increments TIMA (the documented "DIV-write glitch").
    pub fn write_div(&mut self) {
        let old_bit = self.timer_bit();
        self.counter = 0;
        if old_bit {
            self.increment_tima();
        }
    }

    /// Read TIMA ($FF05).
    #[must_use]
    pub const fn read_tima(&self) -> u8 {
        self.tima
    }

    /// Write TIMA ($FF05).
    pub fn write_tima(&mut self, value: u8) {
        if self.reloaded_this_t_cycle {
            return;
        }
        if self.reload_delay != 0 {
            self.reload_delay = 0;
        }
        self.tima = value;
    }

    /// Read TMA ($FF06).
    #[must_use]
    pub const fn read_tma(&self) -> u8 {
        self.tma
    }

    /// Write TMA ($FF06).
    pub fn write_tma(&mut self, value: u8) {
        self.tma = value;
    }

    /// Read TAC ($FF07).
    #[must_use]
    pub const fn read_tac(&self) -> u8 {
        self.tac
    }

    /// Write TAC ($FF07). If the (enable AND selected-bit) signal
    /// goes from high to low as a result, TIMA increments — the
    /// "TAC-write glitch".
    pub fn write_tac(&mut self, value: u8) {
        let old_bit = self.timer_bit();
        self.tac = value;
        let new_bit = self.timer_bit();
        if old_bit && !new_bit {
            self.increment_tima();
        }
    }

    /// Consume the overflow latch — returns `true` and clears the
    /// flag if a TIMA overflow has happened since the previous call.
    /// The machine OR's the result into `IF` bit 2.
    pub fn consume_overflow(&mut self) -> bool {
        let was = self.overflow_latched;
        self.overflow_latched = false;
        was
    }

    /// Returns the current state of `(timer_enabled AND
    /// selected_counter_bit)`. TIMA increments on the falling edge
    /// of this combined signal.
    fn timer_bit(&self) -> bool {
        if (self.tac & 0x04) == 0 {
            return false;
        }
        let bit_pos = match self.tac & 0b11 {
            0 => 9, // every 1024 T-cycles
            1 => 3, // every 16 T-cycles
            2 => 5, // every 64 T-cycles
            _ => 7, // every 256 T-cycles
        };
        ((self.counter >> bit_pos) & 1) != 0
    }

    fn increment_tima(&mut self) {
        self.tima = self.tima.wrapping_add(1);
        if self.tima == 0 {
            self.reload_delay = 4;
        }
    }

    fn advance_reload_delay(&mut self) {
        self.reloaded_this_t_cycle = false;
        if self.reload_delay == 0 {
            return;
        }

        self.reload_delay -= 1;
        if self.reload_delay == 0 {
            self.tima = self.tma;
            self.overflow_latched = true;
            self.reloaded_this_t_cycle = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_t(timer: &mut Timer, count: u32) {
        for _ in 0..count {
            timer.tick_t();
        }
    }

    // -- DIV / counter -------------------------------------------------

    #[test]
    fn div_increments_every_256_t_cycles() {
        let mut timer = Timer::new();
        run_t(&mut timer, 256);
        assert_eq!(timer.read_div(), 1);
    }

    #[test]
    fn div_reads_upper_byte_of_counter() {
        let mut timer = Timer::new();
        run_t(&mut timer, 1024);
        assert_eq!(timer.read_div(), 4);
    }

    #[test]
    fn write_div_resets_counter_to_zero() {
        let mut timer = Timer::new();
        run_t(&mut timer, 512);
        assert_eq!(timer.read_div(), 2);
        timer.write_div();
        assert_eq!(timer.read_div(), 0);
        assert_eq!(timer.counter, 0);
    }

    #[test]
    fn counter_wraps_at_16_bits() {
        let mut timer = Timer::new();
        run_t(&mut timer, 65_536);
        assert_eq!(timer.counter, 0);
        assert_eq!(timer.read_div(), 0);
    }

    // -- TIMA at the four selectable rates -----------------------------

    #[test]
    fn tima_increments_at_select_01_every_16_t_cycles() {
        let mut timer = Timer::new();
        timer.tac = 0x05; // enabled, clock select 01 = bit 3
        run_t(&mut timer, 16);
        assert_eq!(timer.tima, 1);
        run_t(&mut timer, 16);
        assert_eq!(timer.tima, 2);
    }

    #[test]
    fn tima_increments_at_select_10_every_64_t_cycles() {
        let mut timer = Timer::new();
        timer.tac = 0x06; // enabled, clock select 10 = bit 5
        run_t(&mut timer, 64);
        assert_eq!(timer.tima, 1);
    }

    #[test]
    fn tima_increments_at_select_11_every_256_t_cycles() {
        let mut timer = Timer::new();
        timer.tac = 0x07; // enabled, clock select 11 = bit 7
        run_t(&mut timer, 256);
        assert_eq!(timer.tima, 1);
    }

    #[test]
    fn tima_increments_at_select_00_every_1024_t_cycles() {
        let mut timer = Timer::new();
        timer.tac = 0x04; // enabled, clock select 00 = bit 9
        run_t(&mut timer, 1024);
        assert_eq!(timer.tima, 1);
    }

    #[test]
    fn tima_does_not_increment_when_disabled() {
        let mut timer = Timer::new();
        timer.tac = 0x01; // disabled, clock select 01
        run_t(&mut timer, 64);
        assert_eq!(timer.tima, 0);
    }

    // -- The DIV-write and TAC-write falling-edge glitches ------------

    #[test]
    fn write_div_falling_edge_increments_tima() {
        let mut timer = Timer::new();
        timer.tac = 0x05; // enabled, bit 3
        run_t(&mut timer, 8); // counter == 8, bit 3 = 1
        timer.write_div();
        assert_eq!(timer.tima, 1);
    }

    #[test]
    fn write_div_does_not_increment_tima_when_selected_bit_is_low() {
        let mut timer = Timer::new();
        timer.tac = 0x05; // enabled, bit 3
        run_t(&mut timer, 7); // counter == 7, bit 3 = 0
        timer.write_div();
        assert_eq!(timer.tima, 0);
    }

    #[test]
    fn write_tac_disabling_with_selected_bit_high_increments_tima() {
        let mut timer = Timer::new();
        timer.tac = 0x05; // enabled, bit 3
        run_t(&mut timer, 8); // bit 3 high
        timer.write_tac(0x01); // disable
        assert_eq!(timer.tima, 1);
    }

    #[test]
    fn write_tac_changing_select_to_a_low_bit_increments_tima() {
        let mut timer = Timer::new();
        timer.tac = 0x07; // enabled, bit 7
        // Run until bit 7 high and bit 3 low: counter == 0x80.
        run_t(&mut timer, 0x80);
        assert!((timer.counter & (1 << 7)) != 0);
        assert!((timer.counter & (1 << 3)) == 0);
        timer.write_tac(0x05); // switch to bit 3 — old bit was high, new bit low → falling edge
        assert_eq!(timer.tima, 1);
    }

    // -- Overflow latching --------------------------------------------

    #[test]
    fn tima_overflow_reloads_from_tma_after_one_m_cycle_and_latches_irq() {
        let mut timer = Timer::new();
        timer.tma = 0xAB;
        timer.tima = 0xFF;
        timer.tac = 0x05; // enabled, bit 3
        // One full 16 T-cycle period to roll TIMA over.
        run_t(&mut timer, 16);
        assert_eq!(timer.tima, 0x00);
        assert_eq!(timer.reload_delay, 4);
        assert!(!timer.consume_overflow());
        run_t(&mut timer, 4);
        assert_eq!(timer.tima, 0xAB);
        assert!(timer.consume_overflow());
        assert!(!timer.consume_overflow(), "consume clears the latch");
    }

    #[test]
    fn tima_write_during_reload_delay_cancels_reload() {
        let mut timer = Timer::new();
        timer.tma = 0xAB;
        timer.tima = 0xFF;
        timer.tac = 0x05;
        run_t(&mut timer, 16);

        timer.write_tima(0x42);
        run_t(&mut timer, 4);

        assert_eq!(timer.tima, 0x42);
        assert!(!timer.consume_overflow());
    }

    #[test]
    fn consume_overflow_starts_clear() {
        let mut timer = Timer::new();
        assert!(!timer.consume_overflow());
    }

    // -- m-cycle convenience ------------------------------------------

    #[test]
    fn tick_m_advances_counter_by_four() {
        let mut timer = Timer::new();
        timer.tick_m();
        assert_eq!(timer.counter, 4);
    }

    #[test]
    fn tick_m_detects_falling_edge_inside_the_window() {
        // Bit 3 transitions 1→0 between counter values 15 and 16.
        // Position the counter at 14 so a single tick_m (4 T-cycles)
        // covers 14→15→16→17→18 — the edge fires once.
        let mut timer = Timer::new();
        timer.tac = 0x05; // enabled, bit 3
        timer.counter = 14;
        timer.tick_m();
        assert_eq!(timer.tima, 1);
    }
}
