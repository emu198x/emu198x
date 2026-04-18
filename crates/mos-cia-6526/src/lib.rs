#![allow(clippy::cast_possible_truncation)]

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cia6526 {
    pub irq: bool,
    pub pa: u8,
    pub pb: u8,
    pub pa_in: u8,
    pub pb_in: u8,
    pub flag: bool,
    port_a: u8,
    port_b: u8,
    ddr_a: u8,
    ddr_b: u8,
    timer_a: u16,
    timer_a_latch: u16,
    timer_a_running: bool,
    timer_a_oneshot: bool,
    timer_a_force_load: bool,
    timer_b: u16,
    timer_b_latch: u16,
    timer_b_running: bool,
    timer_b_oneshot: bool,
    timer_b_force_load: bool,
    timer_b_count_ta_underflow: bool,
    icr_status: u8,
    icr_mask: u8,
    cra: u8,
    crb: u8,
    tod: [u8; 4],
    tod_alarm: [u8; 4],
    tod_latch: [u8; 4],
    tod_latched: bool,
    tod_halted: bool,
    tod_divider: u32,
    tod_counter: u32,
    prev_flag: bool,
    shift_register: u8,
    shift_count: u8,
    sp_output: bool,
}

impl Cia6526 {
    #[must_use]
    pub fn new() -> Self {
        Self::new_with_tod(19_705)
    }

    #[must_use]
    pub fn new_with_tod(tod_divider: u32) -> Self {
        let mut cia = Self {
            irq: false,
            pa: 0xFF,
            pb: 0xFF,
            pa_in: 0xFF,
            pb_in: 0xFF,
            flag: true,
            port_a: 0xFF,
            port_b: 0xFF,
            ddr_a: 0,
            ddr_b: 0,
            timer_a: 0xFFFF,
            timer_a_latch: 0xFFFF,
            timer_a_running: false,
            timer_a_oneshot: false,
            timer_a_force_load: false,
            timer_b: 0xFFFF,
            timer_b_latch: 0xFFFF,
            timer_b_running: false,
            timer_b_oneshot: false,
            timer_b_force_load: false,
            timer_b_count_ta_underflow: false,
            icr_status: 0,
            icr_mask: 0,
            cra: 0,
            crb: 0,
            tod: [0; 4],
            tod_alarm: [0; 4],
            tod_latch: [0; 4],
            tod_latched: false,
            tod_halted: true,
            tod_divider,
            tod_counter: 0,
            prev_flag: true,
            shift_register: 0,
            shift_count: 0,
            sp_output: false,
        };
        cia.update_pins();
        cia
    }

    pub fn tick(&mut self) {
        self.poll_flag();
        self.tick_tod();

        if self.timer_a_force_load {
            self.timer_a = self.timer_a_latch;
            self.timer_a_force_load = false;
        }

        let mut ta_underflowed = false;
        if self.timer_a_running {
            if self.timer_a == 0 {
                ta_underflowed = true;
                self.icr_status |= 0x01;
                self.timer_a = self.timer_a_latch;
                if self.timer_a_oneshot {
                    self.timer_a_running = false;
                    self.cra &= !0x01;
                }
            } else {
                self.timer_a -= 1;
            }
        }

        if ta_underflowed && self.sp_output && self.shift_count < 8 {
            self.shift_register = self.shift_register.wrapping_shl(1);
            self.shift_count += 1;
            if self.shift_count == 8 {
                self.icr_status |= 0x08;
            }
        }

        if self.timer_b_force_load {
            self.timer_b = self.timer_b_latch;
            self.timer_b_force_load = false;
        }

        let timer_b_tick = if self.timer_b_count_ta_underflow {
            ta_underflowed
        } else {
            true
        };
        if self.timer_b_running && timer_b_tick {
            if self.timer_b == 0 {
                self.icr_status |= 0x02;
                self.timer_b = self.timer_b_latch;
                if self.timer_b_oneshot {
                    self.timer_b_running = false;
                    self.crb &= !0x01;
                }
            } else {
                self.timer_b -= 1;
            }
        }

        self.update_pins();
    }

    pub fn read(&mut self, reg: u8) -> u8 {
        match reg & 0x0F {
            0x00 => (self.port_a & self.ddr_a) | (self.pa_in & !self.ddr_a),
            0x01 => (self.port_b & self.ddr_b) | (self.pb_in & !self.ddr_b),
            0x02 => self.ddr_a,
            0x03 => self.ddr_b,
            0x04 => self.timer_a as u8,
            0x05 => (self.timer_a >> 8) as u8,
            0x06 => self.timer_b as u8,
            0x07 => (self.timer_b >> 8) as u8,
            0x08 => {
                let value = if self.tod_latched {
                    self.tod_latch[0]
                } else {
                    self.tod[0]
                };
                self.tod_latched = false;
                value
            }
            0x09 => {
                if self.tod_latched {
                    self.tod_latch[1]
                } else {
                    self.tod[1]
                }
            }
            0x0A => {
                if self.tod_latched {
                    self.tod_latch[2]
                } else {
                    self.tod[2]
                }
            }
            0x0B => {
                if !self.tod_latched {
                    self.tod_latch = self.tod;
                    self.tod_latched = true;
                }
                self.tod_latch[3]
            }
            0x0C => self.shift_register,
            0x0D => {
                let any = if (self.icr_status & self.icr_mask & 0x1F) != 0 {
                    0x80
                } else {
                    0x00
                };
                let result = self.icr_status | any;
                self.icr_status = 0;
                self.update_pins();
                result
            }
            0x0E => self.cra,
            0x0F => self.crb,
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, reg: u8, value: u8) {
        match reg & 0x0F {
            0x00 => self.port_a = value,
            0x01 => self.port_b = value,
            0x02 => self.ddr_a = value,
            0x03 => self.ddr_b = value,
            0x04 => self.timer_a_latch = (self.timer_a_latch & 0xFF00) | u16::from(value),
            0x05 => {
                self.timer_a_latch = (self.timer_a_latch & 0x00FF) | (u16::from(value) << 8);
                if !self.timer_a_running {
                    self.timer_a = self.timer_a_latch;
                }
            }
            0x06 => self.timer_b_latch = (self.timer_b_latch & 0xFF00) | u16::from(value),
            0x07 => {
                self.timer_b_latch = (self.timer_b_latch & 0x00FF) | (u16::from(value) << 8);
                if !self.timer_b_running {
                    self.timer_b = self.timer_b_latch;
                }
            }
            // TOD / alarm writes — CRB bit 7 selects alarm vs clock.
            // Clock writes: $B halts, $8 restarts. Alarm writes never
            // affect the TOD halt state (datasheet confirmed).
            0x08 => {
                if self.crb & 0x80 != 0 {
                    self.tod_alarm[0] = value & 0x0F;
                } else {
                    self.tod[0] = value & 0x0F;
                    self.tod_halted = false;
                    self.tod_counter = 0;
                }
            }
            0x09 => {
                if self.crb & 0x80 != 0 {
                    self.tod_alarm[1] = value & 0x7F;
                } else {
                    self.tod[1] = value & 0x7F;
                }
            }
            0x0A => {
                if self.crb & 0x80 != 0 {
                    self.tod_alarm[2] = value & 0x7F;
                } else {
                    self.tod[2] = value & 0x7F;
                }
            }
            0x0B => {
                if self.crb & 0x80 != 0 {
                    self.tod_alarm[3] = value & 0x9F;
                } else {
                    self.tod[3] = value & 0x9F;
                    self.tod_halted = true;
                }
            }
            0x0C => {
                self.shift_register = value;
                self.shift_count = 0;
            }
            0x0D => {
                if value & 0x80 != 0 {
                    self.icr_mask |= value & 0x1F;
                } else {
                    self.icr_mask &= !(value & 0x1F);
                }
            }
            0x0E => {
                self.cra = value;
                self.timer_a_running = value & 0x01 != 0;
                self.timer_a_oneshot = value & 0x08 != 0;
                self.sp_output = value & 0x40 != 0;
                if value & 0x10 != 0 {
                    self.timer_a_force_load = true;
                }
            }
            0x0F => {
                self.crb = value;
                self.timer_b_running = value & 0x01 != 0;
                self.timer_b_oneshot = value & 0x08 != 0;
                self.timer_b_count_ta_underflow = value & 0x40 != 0;
                if value & 0x10 != 0 {
                    self.timer_b_force_load = true;
                }
            }
            _ => {}
        }
        self.update_pins();
    }

    #[must_use]
    pub fn irq_active(&self) -> bool {
        (self.icr_status & self.icr_mask & 0x1F) != 0
    }

    #[must_use]
    pub fn timer_a(&self) -> u16 {
        self.timer_a
    }

    #[must_use]
    pub fn timer_a_latch(&self) -> u16 {
        self.timer_a_latch
    }

    #[must_use]
    pub fn timer_b(&self) -> u16 {
        self.timer_b
    }

    #[must_use]
    pub fn timer_b_latch(&self) -> u16 {
        self.timer_b_latch
    }

    #[must_use]
    pub fn icr_status(&self) -> u8 {
        self.icr_status
    }

    #[must_use]
    pub fn icr_mask(&self) -> u8 {
        self.icr_mask
    }

    #[must_use]
    pub fn cra(&self) -> u8 {
        self.cra
    }

    #[must_use]
    pub fn crb(&self) -> u8 {
        self.crb
    }

    #[must_use]
    pub fn port_a_latch(&self) -> u8 {
        self.port_a
    }

    #[must_use]
    pub fn port_b_latch(&self) -> u8 {
        self.port_b
    }

    #[must_use]
    pub fn ddr_a(&self) -> u8 {
        self.ddr_a
    }

    #[must_use]
    pub fn ddr_b(&self) -> u8 {
        self.ddr_b
    }

    #[must_use]
    pub fn port_a_drive_state(&self) -> u8 {
        (self.port_a & self.ddr_a) | !self.ddr_a
    }

    #[must_use]
    pub fn port_b_drive_state(&self) -> u8 {
        (self.port_b & self.ddr_b) | !self.ddr_b
    }

    fn update_pins(&mut self) {
        self.pa = (self.port_a & self.ddr_a) | (self.pa_in & !self.ddr_a);
        self.pb = (self.port_b & self.ddr_b) | (self.pb_in & !self.ddr_b);
        self.irq = (self.icr_status & self.icr_mask & 0x1F) != 0;
    }

    fn poll_flag(&mut self) {
        if self.prev_flag && !self.flag {
            self.icr_status |= 0x10;
        }
        self.prev_flag = self.flag;
    }

    fn tick_tod(&mut self) {
        if self.tod_halted {
            return;
        }
        self.tod_counter += 1;
        if self.tod_counter < self.tod_divider {
            return;
        }
        self.tod_counter = 0;

        self.tod[0] = (self.tod[0] + 1) & 0x0F;
        if self.tod[0] < 10 {
            return;
        }
        self.tod[0] = 0;

        self.tod[1] = bcd_increment(self.tod[1]);
        if self.tod[1] < 0x60 {
            return;
        }
        self.tod[1] = 0;

        self.tod[2] = bcd_increment(self.tod[2]);
        if self.tod[2] < 0x60 {
            return;
        }
        self.tod[2] = 0;

        let pm = self.tod[3] & 0x80;
        let hours = self.tod[3] & 0x1F;
        let next = bcd_increment(hours);
        if next == 0x12 {
            self.tod[3] = 0x12 | (pm ^ 0x80);
        } else if next == 0x13 {
            self.tod[3] = 0x01 | pm;
        } else {
            self.tod[3] = next | pm;
        }

        // Alarm match: when all four TOD bytes equal the alarm, raise
        // ICR bit 2 (ALARM). Checked after any TOD increment.
        if self.tod == self.tod_alarm {
            self.icr_status |= 0x04;
        }
    }
}

impl Default for Cia6526 {
    fn default() -> Self {
        Self::new()
    }
}

fn bcd_increment(value: u8) -> u8 {
    let lo = value & 0x0F;
    let hi = value & 0xF0;
    if lo < 9 {
        hi | (lo + 1)
    } else if hi < 0x90 {
        (hi + 0x10) & 0xF0
    } else {
        0x00
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_a_countdown_fires_icr_on_underflow() {
        let mut cia = Cia6526::new();
        cia.write(0x04, 10);
        cia.write(0x05, 0);
        cia.write(0x0E, 0x01);
        for _ in 0..11 {
            cia.tick();
        }
        assert!(cia.icr_status & 0x01 != 0);
    }

    #[test]
    fn timer_a_oneshot_stops_after_underflow() {
        let mut cia = Cia6526::new();
        cia.write(0x04, 5);
        cia.write(0x05, 0);
        cia.write(0x0E, 0x09);
        for _ in 0..6 {
            cia.tick();
        }
        assert!(cia.icr_status & 0x01 != 0);
        assert!(!cia.timer_a_running);
    }

    #[test]
    fn icr_read_clears_status_and_deasserts_irq() {
        let mut cia = Cia6526::new();
        cia.icr_status = 0x01;
        cia.icr_mask = 0x01;
        cia.update_pins();
        assert!(cia.irq);

        let value = cia.read(0x0D);
        assert_eq!(value, 0x81);
        assert_eq!(cia.icr_status, 0);
        assert!(!cia.irq);
    }

    #[test]
    fn icr_mask_set_and_clear_bits() {
        let mut cia = Cia6526::new();
        cia.write(0x0D, 0x83);
        assert_eq!(cia.icr_mask, 0x03);
        cia.write(0x0D, 0x01);
        assert_eq!(cia.icr_mask, 0x02);
    }

    #[test]
    fn irq_active_requires_status_and_mask() {
        let mut cia = Cia6526::new();
        cia.icr_status = 0x01;
        cia.icr_mask = 0x00;
        assert!(!cia.irq_active());
        cia.icr_mask = 0x01;
        assert!(cia.irq_active());
    }

    #[test]
    fn irq_pin_updates_on_tick() {
        let mut cia = Cia6526::new();
        cia.write(0x04, 0);
        cia.write(0x05, 0);
        cia.write(0x0D, 0x81);
        cia.write(0x0E, 0x01);
        assert!(!cia.irq);
        cia.tick();
        assert!(cia.irq);
    }

    #[test]
    fn port_a_read_combines_output_and_external() {
        let mut cia = Cia6526::new();
        cia.write(0x02, 0xF0);
        cia.write(0x00, 0xAB);
        cia.pa_in = 0x55;
        assert_eq!(cia.read(0x00), 0xA5);
    }

    #[test]
    fn port_a_pin_reflects_ddr_masking() {
        let mut cia = Cia6526::new();
        cia.write(0x02, 0xFF);
        cia.write(0x00, 0x42);
        assert_eq!(cia.pa, 0x42);
    }

    #[test]
    fn port_b_reads_external_through_input_bits() {
        let mut cia = Cia6526::new();
        cia.write(0x02, 0xFF);
        cia.write(0x03, 0x00);
        cia.write(0x00, 0xFD);
        cia.pb_in = !0x02;
        assert_eq!(cia.read(0x01) & 0x02, 0x00);
    }

    #[test]
    fn timer_b_cascade_counts_timer_a_underflows() {
        let mut cia = Cia6526::new();
        cia.write(0x04, 3);
        cia.write(0x05, 0);
        cia.write(0x06, 2);
        cia.write(0x07, 0);
        cia.write(0x0F, 0x41);
        cia.write(0x0E, 0x01);
        for _ in 0..4 {
            cia.tick();
        }
        assert_eq!(cia.timer_b(), 1);
        for _ in 0..4 {
            cia.tick();
        }
        assert_eq!(cia.timer_b(), 0);
        for _ in 0..4 {
            cia.tick();
        }
        assert!(cia.icr_status() & 0x02 != 0);
    }

    #[test]
    fn timer_b_phi2_mode_ignores_timer_a() {
        let mut cia = Cia6526::new();
        cia.write(0x06, 5);
        cia.write(0x07, 0);
        cia.write(0x0F, 0x01);
        for _ in 0..3 {
            cia.tick();
        }
        assert_eq!(cia.timer_b(), 2);
    }

    #[test]
    fn tod_counts_at_pal_50hz() {
        let mut cia = Cia6526::new();
        cia.write(0x0B, 0x00);
        cia.write(0x0A, 0x00);
        cia.write(0x09, 0x00);
        cia.write(0x08, 0x00);
        for _ in 0..19_705 {
            cia.tick();
        }
        assert_eq!(cia.tod[0], 1);
    }

    #[test]
    fn flag_falling_edge_sets_icr_bit_four() {
        let mut cia = Cia6526::new();
        cia.flag = false;
        cia.tick();
        assert_eq!(cia.icr_status() & 0x10, 0x10);
    }
}
