#![allow(clippy::cast_possible_truncation)]

use serde::{Deserialize, Serialize};

const IRQ_CA2: u8 = 0x01;
const IRQ_CA1: u8 = 0x02;
const IRQ_CB2: u8 = 0x08;
const IRQ_CB1: u8 = 0x10;
const IRQ_T2: u8 = 0x20;
const IRQ_T1: u8 = 0x40;

/// Fresh-workspace MOS 6522 with the board-facing behavior needed for 1541 bring-up.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Via6522 {
    pub irq: bool,
    pub pa: u8,
    pub pb: u8,
    pub pa_in: u8,
    pub pb_in: u8,
    pub ca1: bool,
    pub ca2: bool,
    pub cb1: bool,
    pub cb2: bool,
    pub ca2_out: bool,
    pub ca2_drive: bool,
    pub cb1_out: bool,
    pub cb1_drive: bool,
    pub cb2_out: bool,
    pub cb2_drive: bool,
    ora: u8,
    orb: u8,
    ddra: u8,
    ddrb: u8,
    ira: u8,
    irb: u8,
    t1_counter: u16,
    t1_latch: u16,
    t1_running: bool,
    t1_pb7_output: bool,
    t2_counter: u16,
    t2_latch_low: u8,
    t2_running: bool,
    shift_register: u8,
    acr: u8,
    pcr: u8,
    ifr: u8,
    ier: u8,
    prev_ca1: bool,
    prev_ca2: bool,
    prev_cb1: bool,
    prev_cb2: bool,
    prev_pb6: bool,
    ca2_handshake_high: bool,
    ca2_pulse_low: bool,
    cb2_handshake_high: bool,
    cb2_pulse_low: bool,
}

impl Via6522 {
    #[must_use]
    pub fn new() -> Self {
        let mut via = Self {
            irq: false,
            pa: 0xFF,
            pb: 0xFF,
            pa_in: 0xFF,
            pb_in: 0xFF,
            ca1: true,
            ca2: true,
            cb1: true,
            cb2: true,
            ca2_out: true,
            ca2_drive: false,
            cb1_out: true,
            cb1_drive: false,
            cb2_out: true,
            cb2_drive: false,
            ora: 0,
            orb: 0,
            ddra: 0,
            ddrb: 0,
            ira: 0xFF,
            irb: 0xFF,
            t1_counter: 0,
            t1_latch: 0,
            t1_running: false,
            t1_pb7_output: false,
            t2_counter: 0,
            t2_latch_low: 0,
            t2_running: false,
            shift_register: 0,
            acr: 0,
            pcr: 0,
            ifr: 0,
            ier: 0,
            prev_ca1: true,
            prev_ca2: true,
            prev_cb1: true,
            prev_cb2: true,
            prev_pb6: true,
            ca2_handshake_high: true,
            ca2_pulse_low: false,
            cb2_handshake_high: true,
            cb2_pulse_low: false,
        };
        via.update_pins();
        via
    }

    pub fn tick(&mut self) {
        let pb6_falling = self.prev_pb6 && !self.pb6_input_high();
        self.poll_lines();

        if self.t1_running {
            if self.t1_counter == 0 {
                self.raise_interrupt(IRQ_T1);
                if self.t1_pb7_enabled() {
                    self.t1_pb7_output = !self.t1_pb7_output;
                }
                if self.t1_free_run() {
                    self.t1_counter = self.t1_latch;
                } else {
                    self.t1_running = false;
                }
            } else {
                self.t1_counter -= 1;
            }
        }

        if self.t2_running && self.t2_should_tick(pb6_falling) {
            if self.t2_counter == 0 {
                self.raise_interrupt(IRQ_T2);
                self.t2_running = false;
            } else {
                self.t2_counter -= 1;
            }
        }

        if self.ca2_pulse_low {
            self.ca2_pulse_low = false;
        }
        if self.cb2_pulse_low {
            self.cb2_pulse_low = false;
        }

        self.update_pins();
    }

    pub fn read(&mut self, reg: u8) -> u8 {
        let reg = reg & 0x0F;
        let result = self.peek(reg);

        match reg {
            0x00 => self.clear_port_b_interrupts(),
            0x01 => self.clear_port_a_interrupts(),
            0x04 => self.clear_interrupts(IRQ_T1),
            0x08 => self.clear_interrupts(IRQ_T2),
            0x0D => self.ifr = 0,
            _ => {}
        }

        self.update_pins();
        result
    }

    pub fn read_port_b_with_value(&mut self, value: u8) -> u8 {
        self.clear_port_b_interrupts();
        self.update_pins();
        value
    }

    #[must_use]
    pub fn peek(&self, reg: u8) -> u8 {
        match reg & 0x0F {
            0x00 => self.read_port_b_data(),
            0x01 | 0x0F => self.read_port_a_data(),
            0x02 => self.ddrb,
            0x03 => self.ddra,
            0x04 => self.t1_counter as u8,
            0x05 => (self.t1_counter >> 8) as u8,
            0x06 => self.t1_latch as u8,
            0x07 => (self.t1_latch >> 8) as u8,
            0x08 => self.t2_counter as u8,
            0x09 => (self.t2_counter >> 8) as u8,
            0x0A => self.shift_register,
            0x0B => self.acr,
            0x0C => self.pcr,
            0x0D => self.ifr | self.irq_bit(),
            0x0E => self.ier | 0x80,
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, reg: u8, value: u8) {
        match reg & 0x0F {
            0x00 => {
                self.orb = value;
                self.clear_port_b_interrupts();
                if self.cb2_is_output() && self.cb2_output_mode() == 0x01 {
                    self.cb2_pulse_low = true;
                }
            }
            0x01 | 0x0F => {
                self.ora = value;
                self.clear_port_a_interrupts();
                if self.ca2_is_output() && self.ca2_output_mode() == 0x01 {
                    self.ca2_pulse_low = true;
                }
            }
            0x02 => self.ddrb = value,
            0x03 => self.ddra = value,
            0x04 => self.t1_latch = (self.t1_latch & 0xFF00) | u16::from(value),
            0x05 => {
                self.t1_latch = (self.t1_latch & 0x00FF) | (u16::from(value) << 8);
                self.t1_counter = self.t1_latch;
                self.t1_running = true;
                self.clear_interrupts(IRQ_T1);
            }
            0x06 => self.t1_latch = (self.t1_latch & 0xFF00) | u16::from(value),
            0x07 => self.t1_latch = (self.t1_latch & 0x00FF) | (u16::from(value) << 8),
            0x08 => self.t2_latch_low = value,
            0x09 => {
                self.t2_counter = (u16::from(value) << 8) | u16::from(self.t2_latch_low);
                self.t2_running = true;
                self.clear_interrupts(IRQ_T2);
            }
            0x0A => self.shift_register = value,
            0x0B => self.acr = value,
            0x0C => {
                self.pcr = value;
                if self.ca2_is_output() {
                    self.ca2_handshake_high = true;
                }
                if self.cb2_is_output() {
                    self.cb2_handshake_high = true;
                }
            }
            0x0D => self.clear_interrupts(value & 0x7F),
            0x0E => {
                if value & 0x80 != 0 {
                    self.ier |= value & 0x7F;
                } else {
                    self.ier &= !(value & 0x7F);
                }
            }
            _ => {}
        }

        self.update_pins();
    }

    #[must_use]
    pub const fn ora(&self) -> u8 {
        self.ora
    }

    #[must_use]
    pub const fn orb(&self) -> u8 {
        self.orb
    }

    #[must_use]
    pub const fn ddra(&self) -> u8 {
        self.ddra
    }

    #[must_use]
    pub const fn ddrb(&self) -> u8 {
        self.ddrb
    }

    fn poll_lines(&mut self) {
        if self.edge_matches(self.prev_ca1, self.ca1, self.ca1_active_high()) {
            self.raise_interrupt(IRQ_CA1);
            if self.pa_latch_enabled() {
                self.ira = self.pa_in;
            }
            if self.ca2_is_output() && self.ca2_output_mode() == 0x00 {
                self.ca2_handshake_high = false;
            }
        }

        if !self.ca2_is_output()
            && self.edge_matches(self.prev_ca2, self.ca2, self.ca2_active_high())
        {
            self.raise_interrupt(IRQ_CA2);
        }

        if self.edge_matches(self.prev_cb1, self.cb1, self.cb1_active_high()) {
            self.raise_interrupt(IRQ_CB1);
            if self.pb_latch_enabled() {
                self.irb = self.pb_in;
            }
            if self.cb2_is_output() && self.cb2_output_mode() == 0x00 {
                self.cb2_handshake_high = false;
            }
        }

        if !self.cb2_is_output()
            && self.edge_matches(self.prev_cb2, self.cb2, self.cb2_active_high())
        {
            self.raise_interrupt(IRQ_CB2);
        }

        self.prev_ca1 = self.ca1;
        self.prev_ca2 = self.ca2;
        self.prev_cb1 = self.cb1;
        self.prev_cb2 = self.cb2;
        self.prev_pb6 = self.pb6_input_high();
    }

    fn update_pins(&mut self) {
        let port_a = self.read_port_a_data();
        let port_b = self.read_port_b_data();
        self.pa = port_a;
        self.pb = port_b;

        self.ca2_drive = self.ca2_is_output();
        self.ca2_out = if !self.ca2_drive {
            true
        } else {
            match self.ca2_output_mode() {
                0x00 => self.ca2_handshake_high,
                0x01 => !self.ca2_pulse_low,
                0x02 => false,
                _ => true,
            }
        };

        self.cb1_drive = false;
        self.cb1_out = true;

        self.cb2_drive = self.cb2_is_output();
        self.cb2_out = if !self.cb2_drive {
            true
        } else {
            match self.cb2_output_mode() {
                0x00 => self.cb2_handshake_high,
                0x01 => !self.cb2_pulse_low,
                0x02 => false,
                _ => true,
            }
        };

        self.irq = (self.ifr & self.ier) != 0;
    }

    fn read_port_a_data(&self) -> u8 {
        let input = if self.pa_latch_enabled() {
            self.ira
        } else {
            self.pa_in
        };
        (self.ora & self.ddra) | (input & !self.ddra)
    }

    fn read_port_b_data(&self) -> u8 {
        let input = if self.pb_latch_enabled() {
            self.irb
        } else {
            self.pb_in
        };
        let output = self.port_b_output();
        (output & self.ddrb) | (input & !self.ddrb)
    }

    fn port_b_output(&self) -> u8 {
        let mut output = self.orb;
        if self.t1_pb7_enabled() {
            output = (output & 0x7F) | if self.t1_pb7_output { 0x80 } else { 0 };
        }
        output
    }

    fn clear_port_a_interrupts(&mut self) {
        self.clear_interrupts(IRQ_CA1);
        if !self.ca2_input_no_irq_clear() || self.ca2_is_output() {
            self.clear_interrupts(IRQ_CA2);
        }
        if self.ca2_is_output() && self.ca2_output_mode() == 0x00 {
            self.ca2_handshake_high = true;
        }
    }

    fn clear_port_b_interrupts(&mut self) {
        self.clear_interrupts(IRQ_CB1);
        if !self.cb2_input_no_irq_clear() || self.cb2_is_output() {
            self.clear_interrupts(IRQ_CB2);
        }
        if self.cb2_is_output() && self.cb2_output_mode() == 0x00 {
            self.cb2_handshake_high = true;
        }
    }

    fn clear_interrupts(&mut self, mask: u8) {
        self.ifr &= !mask;
        self.update_pins();
    }

    fn raise_interrupt(&mut self, mask: u8) {
        self.ifr |= mask;
        self.update_pins();
    }

    fn irq_bit(&self) -> u8 {
        if (self.ifr & self.ier) != 0 {
            0x80
        } else {
            0x00
        }
    }

    fn t1_free_run(&self) -> bool {
        self.acr & 0x40 != 0
    }

    fn t1_pb7_enabled(&self) -> bool {
        self.acr & 0x80 != 0
    }

    fn t2_should_tick(&self, pb6_falling: bool) -> bool {
        if self.acr & 0x20 == 0 {
            true
        } else {
            pb6_falling
        }
    }

    fn pa_latch_enabled(&self) -> bool {
        self.acr & 0x01 != 0
    }

    fn pb_latch_enabled(&self) -> bool {
        self.acr & 0x02 != 0
    }

    fn pb6_input_high(&self) -> bool {
        self.pb_in & 0x40 != 0
    }

    fn ca1_active_high(&self) -> bool {
        self.pcr & 0x01 != 0
    }

    fn ca2_is_output(&self) -> bool {
        self.pcr & 0x08 != 0
    }

    fn ca2_active_high(&self) -> bool {
        self.pcr & 0x04 != 0
    }

    fn ca2_input_no_irq_clear(&self) -> bool {
        self.pcr & 0x02 != 0
    }

    fn ca2_output_mode(&self) -> u8 {
        (self.pcr >> 1) & 0x03
    }

    fn cb1_active_high(&self) -> bool {
        self.pcr & 0x10 != 0
    }

    fn cb2_is_output(&self) -> bool {
        self.pcr & 0x80 != 0
    }

    fn cb2_active_high(&self) -> bool {
        self.pcr & 0x40 != 0
    }

    fn cb2_input_no_irq_clear(&self) -> bool {
        self.pcr & 0x20 != 0
    }

    fn cb2_output_mode(&self) -> u8 {
        (self.pcr >> 5) & 0x03
    }

    fn edge_matches(&self, previous: bool, current: bool, active_high: bool) -> bool {
        previous != current && current == active_high
    }
}

impl Default for Via6522 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{IRQ_CA1, IRQ_CB2, IRQ_T1, IRQ_T2, Via6522};

    #[test]
    fn port_reads_mix_outputs_and_inputs() {
        let mut via = Via6522::new();
        via.pa_in = 0xA5;
        via.pb_in = 0x5A;
        via.write(0x03, 0xF0);
        via.write(0x02, 0x0F);
        via.write(0x01, 0x3C);
        via.write(0x00, 0xC3);

        assert_eq!(via.peek(0x01), 0x35);
        assert_eq!(via.peek(0x00), 0x53);
    }

    #[test]
    fn timer1_raises_irq_and_reloads_in_free_run() {
        let mut via = Via6522::new();
        via.write(0x0E, 0x80 | IRQ_T1);
        via.write(0x0B, 0x40);
        via.write(0x04, 0x02);
        via.write(0x05, 0x00);

        via.tick();
        via.tick();
        via.tick();

        assert!(via.irq);
        assert_eq!(via.peek(0x0D) & IRQ_T1, IRQ_T1);
        assert!(via.t1_running);
        assert_eq!(via.peek(0x04), 0x02);
    }

    #[test]
    fn timer1_stops_in_one_shot_mode() {
        let mut via = Via6522::new();
        via.write(0x0B, 0x00);
        via.write(0x04, 0x01);
        via.write(0x05, 0x00);

        via.tick();
        via.tick();

        assert!(!via.t1_running);
        assert_eq!(via.peek(0x0D) & IRQ_T1, IRQ_T1);
    }

    #[test]
    fn timer2_counts_phi2_cycles() {
        let mut via = Via6522::new();
        via.write(0x0E, 0x80 | IRQ_T2);
        via.write(0x08, 0x01);
        via.write(0x09, 0x00);

        via.tick();
        via.tick();

        assert!(via.irq);
        assert_eq!(via.peek(0x0D) & IRQ_T2, IRQ_T2);
        assert!(!via.t2_running);
    }

    #[test]
    fn timer2_can_count_pb6_falling_edges() {
        let mut via = Via6522::new();
        via.write(0x0B, 0x20);
        via.write(0x08, 0x01);
        via.write(0x09, 0x00);

        via.pb_in = 0xFF;
        via.tick();
        via.pb_in = 0xBF;
        via.tick();
        assert_eq!(via.peek(0x08), 0x00);
        via.pb_in = 0xFF;
        via.tick();
        via.pb_in = 0xBF;
        via.tick();

        assert_eq!(via.peek(0x0D) & IRQ_T2, IRQ_T2);
    }

    #[test]
    fn ca1_edge_sets_interrupt_and_port_access_clears_it() {
        let mut via = Via6522::new();
        via.write(0x0E, 0x80 | IRQ_CA1);

        via.ca1 = false;
        via.tick();

        assert!(via.irq);
        assert_eq!(via.peek(0x0D) & IRQ_CA1, IRQ_CA1);
        assert_eq!(via.read(0x01), 0xFF);
        assert_eq!(via.peek(0x0D) & IRQ_CA1, 0);
        assert!(!via.irq);
    }

    #[test]
    fn port_a_input_latch_holds_value_until_next_edge() {
        let mut via = Via6522::new();
        via.write(0x0B, 0x01);
        via.pa_in = 0x12;
        via.ca1 = false;
        via.tick();
        via.pa_in = 0x34;

        assert_eq!(via.peek(0x01), 0x12);
    }

    #[test]
    fn cb2_input_no_irq_clear_mode_persists_until_ifr_write() {
        let mut via = Via6522::new();
        via.write(0x0C, 0x20);
        via.cb2 = false;
        via.tick();
        assert_eq!(via.peek(0x0D) & IRQ_CB2, IRQ_CB2);

        let _ = via.read(0x00);
        assert_eq!(via.peek(0x0D) & IRQ_CB2, IRQ_CB2);

        via.write(0x0D, IRQ_CB2);
        assert_eq!(via.peek(0x0D) & IRQ_CB2, 0);
    }

    #[test]
    fn ier_set_and_clear_drive_irq_bit() {
        let mut via = Via6522::new();
        via.ca1 = false;
        via.tick();
        assert!(!via.irq);
        assert_eq!(via.peek(0x0D) & 0x80, 0);

        via.write(0x0E, 0x80 | IRQ_CA1);
        assert!(via.irq);
        assert_eq!(via.peek(0x0D) & 0x80, 0x80);

        via.write(0x0E, IRQ_CA1);
        assert!(!via.irq);
    }
}
