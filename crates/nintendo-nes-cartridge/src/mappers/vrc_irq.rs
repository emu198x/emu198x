//! The IRQ counter Konami put in the VRC4, VRC6 and VRC7.
//!
//! An 8-bit counter counts up from a reload value and raises the IRQ when
//! it wraps past `$FF`. It is clocked either every CPU cycle (cycle mode)
//! or once per scanline's worth of CPU cycles (scanline mode), where a
//! prescaler takes 3 off a count of 341 each CPU cycle — the PPU's dots
//! per line over its three dots per CPU cycle — so the counter steps
//! 113⅔ cycles apart without the chip ever seeing the PPU.
//!
//! Facts from the NESdev wiki "VRC IRQ" page and Mesen2's `VrcIrq.h`
//! (GPL-3, read for behaviour only).

use serde::{Deserialize, Serialize};

/// PPU dots per scanline, which the scanline-mode prescaler counts down.
const PRESCALER_RELOAD: i16 = 341;

/// One VRC IRQ counter.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VrcIrq {
    reload: u8,
    counter: u8,
    prescaler: i16,
    enabled: bool,
    enabled_after_ack: bool,
    cycle_mode: bool,
    pending: bool,
}

impl VrcIrq {
    /// Set the whole reload value (VRC6 and VRC7).
    pub fn set_reload(&mut self, value: u8) {
        self.reload = value;
    }

    /// Set one nibble of the reload value (VRC4 writes it in two halves).
    pub fn set_reload_nibble(&mut self, value: u8, high: bool) {
        self.reload = if high {
            (self.reload & 0x0F) | ((value & 0x0F) << 4)
        } else {
            (self.reload & 0xF0) | (value & 0x0F)
        };
    }

    /// The control register: bit 0 re-enables after an acknowledge, bit 1
    /// enables, bit 2 selects cycle mode. Enabling reloads the counter and
    /// the prescaler; any write acknowledges a pending IRQ.
    pub fn set_control(&mut self, value: u8) {
        self.enabled_after_ack = value & 0x01 != 0;
        self.enabled = value & 0x02 != 0;
        self.cycle_mode = value & 0x04 != 0;
        if self.enabled {
            self.counter = self.reload;
            self.prescaler = PRESCALER_RELOAD;
        }
        self.pending = false;
    }

    /// Acknowledge: clears the IRQ and restores the enable bit saved by
    /// the last control write.
    pub fn acknowledge(&mut self) {
        self.enabled = self.enabled_after_ack;
        self.pending = false;
    }

    /// One CPU cycle.
    pub fn cpu_tick(&mut self) {
        if !self.enabled {
            return;
        }
        self.prescaler -= 3;
        if self.cycle_mode || self.prescaler <= 0 {
            if self.counter == 0xFF {
                self.counter = self.reload;
                self.pending = true;
            } else {
                self.counter += 1;
            }
            self.prescaler += PRESCALER_RELOAD;
        }
    }

    /// Whether the IRQ line is asserted.
    #[must_use]
    pub fn pending(&self) -> bool {
        self.pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_mode_counts_every_cycle_from_the_reload_value() {
        let mut irq = VrcIrq::default();
        irq.set_reload(0xFD);
        irq.set_control(0x06);
        irq.cpu_tick();
        irq.cpu_tick();
        assert!(!irq.pending(), "$FD -> $FE -> $FF");
        irq.cpu_tick();
        assert!(irq.pending(), "the wrap past $FF raises it");
        irq.acknowledge();
        assert!(!irq.pending());
        irq.cpu_tick();
        assert!(!irq.pending(), "acknowledging without A disables it");
    }

    #[test]
    fn scanline_mode_steps_every_113_or_114_cycles() {
        let mut irq = VrcIrq::default();
        irq.set_reload(0xFF);
        irq.set_control(0x03);
        let mut cycles = 0;
        while !irq.pending() {
            irq.cpu_tick();
            cycles += 1;
        }
        assert_eq!(cycles, 114, "341 / 3 rounded up for the first line");
        irq.acknowledge();
        cycles = 0;
        while !irq.pending() {
            irq.cpu_tick();
            cycles += 1;
        }
        assert!((113..=114).contains(&cycles));
    }

    #[test]
    fn the_reload_is_written_in_nibbles_on_the_vrc4() {
        let mut irq = VrcIrq::default();
        irq.set_reload_nibble(0x0E, false);
        irq.set_reload_nibble(0x0F, true);
        irq.set_control(0x06);
        irq.cpu_tick();
        irq.cpu_tick();
        assert!(irq.pending(), "$FE, $FF, then the wrap");
    }
}
