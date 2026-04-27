//! Motorola MC6821 Peripheral Interface Adapter.
//!
//! The model exposes the two 8-bit ports, data-direction registers, control
//! registers, and basic control-line state needed by Dragon/CoCo bring-up.

use serde::{Deserialize, Serialize};

const CTRL_IRQ1: u8 = 0x80;
const CTRL_IRQ2: u8 = 0x40;
const CTRL_C2_DDR: u8 = 0x20;
const CTRL_C2_MODE: u8 = 0x18;
const CTRL_REG_SELECT: u8 = 0x04;

const C2_RESET: u8 = 0x10;
const C2_SET: u8 = 0x18;
const C2_STROBE_E: u8 = 0x08;

/// PIA port selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PiaPort {
    /// Port A / CA1 / CA2 side.
    A,
    /// Port B / CB1 / CB2 side.
    B,
}

/// PIA edge-sensitive control input selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PiaSignal {
    /// CA1 edge input.
    Ca1,
    /// CA2 edge input.
    Ca2,
    /// CB1 edge input.
    Cb1,
    /// CB2 edge input.
    Cb2,
}

/// Motorola MC6821 state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pia6821 {
    /// Mixed external pin level for port A after DDR/output latching.
    pub pa: u8,
    /// Mixed external pin level for port B after DDR/output latching.
    pub pb: u8,
    /// External input level sampled for port A input bits.
    pub pa_in: u8,
    /// External input level sampled for port B input bits.
    pub pb_in: u8,
    /// CA2 output/input latch level.
    pub ca2: bool,
    /// CB2 output/input latch level.
    pub cb2: bool,
    ctrl_a: u8,
    ctrl_b: u8,
    data_a: u8,
    data_b: u8,
    ddr_a: u8,
    ddr_b: u8,
    ca2_strobe_e: bool,
    cb2_strobe_e: bool,
}

impl Pia6821 {
    /// Create a reset MC6821.
    #[must_use]
    pub fn new() -> Self {
        let mut pia = Self {
            pa: 0xFF,
            pb: 0xFF,
            pa_in: 0xFF,
            pb_in: 0xFF,
            ca2: false,
            cb2: false,
            ctrl_a: 0,
            ctrl_b: 0,
            data_a: 0,
            data_b: 0,
            ddr_a: 0,
            ddr_b: 0,
            ca2_strobe_e: false,
            cb2_strobe_e: false,
        };
        pia.update_ports();
        pia
    }

    /// Reset registers and output pins.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Read one of the four register-select addresses.
    ///
    /// Address bit 1 selects port B when set; bit 0 selects the control
    /// register when set. This matches the common `RS1:RS0` MC6821 wiring.
    pub fn read(&mut self, addr: u8) -> u8 {
        match decode_register(addr) {
            Register::Data(PiaPort::A) => self.read_port_a(),
            Register::Control(PiaPort::A) => self.ctrl_a,
            Register::Data(PiaPort::B) => self.read_port_b(),
            Register::Control(PiaPort::B) => self.ctrl_b,
        }
    }

    /// Read without side effects such as clearing interrupt flags.
    #[must_use]
    pub fn peek(&self, addr: u8) -> u8 {
        match decode_register(addr) {
            Register::Data(PiaPort::A) => {
                if self.data_selected(PiaPort::A) {
                    self.mixed_port_a()
                } else {
                    self.ddr_a
                }
            }
            Register::Control(PiaPort::A) => self.ctrl_a,
            Register::Data(PiaPort::B) => {
                if self.data_selected(PiaPort::B) {
                    self.mixed_port_b()
                } else {
                    self.ddr_b
                }
            }
            Register::Control(PiaPort::B) => self.ctrl_b,
        }
    }

    /// Write one of the four register-select addresses.
    pub fn write(&mut self, addr: u8, value: u8) {
        match decode_register(addr) {
            Register::Data(PiaPort::A) => self.write_data_or_ddr(PiaPort::A, value),
            Register::Control(PiaPort::A) => self.write_control(PiaPort::A, value),
            Register::Data(PiaPort::B) => self.write_data_or_ddr(PiaPort::B, value),
            Register::Control(PiaPort::B) => self.write_control(PiaPort::B, value),
        }
    }

    /// Set external input bits for a port.
    pub fn set_input(&mut self, port: PiaPort, value: u8) {
        match port {
            PiaPort::A => self.pa_in = value,
            PiaPort::B => self.pb_in = value,
        }
        self.update_ports();
    }

    /// Return the configured output latch for a port.
    #[must_use]
    pub fn output_latch(&self, port: PiaPort) -> u8 {
        match port {
            PiaPort::A => self.data_a,
            PiaPort::B => self.data_b,
        }
    }

    /// Return the DDR for a port.
    #[must_use]
    pub fn ddr(&self, port: PiaPort) -> u8 {
        match port {
            PiaPort::A => self.ddr_a,
            PiaPort::B => self.ddr_b,
        }
    }

    /// Return the control register for a port.
    #[must_use]
    pub fn control(&self, port: PiaPort) -> u8 {
        match port {
            PiaPort::A => self.ctrl_a,
            PiaPort::B => self.ctrl_b,
        }
    }

    /// Raise one of the edge-sensitive control-line interrupt flags.
    pub fn set_signal(&mut self, signal: PiaSignal) {
        match signal {
            PiaSignal::Ca1 => self.ctrl_a |= CTRL_IRQ1,
            PiaSignal::Ca2 => self.ctrl_a |= CTRL_IRQ2,
            PiaSignal::Cb1 => self.ctrl_b |= CTRL_IRQ1,
            PiaSignal::Cb2 => self.ctrl_b |= CTRL_IRQ2,
        }
    }

    /// Whether either enabled interrupt source is active on port A.
    #[must_use]
    pub fn irq_a(&self) -> bool {
        irq_active(self.ctrl_a)
    }

    /// Whether either enabled interrupt source is active on port B.
    #[must_use]
    pub fn irq_b(&self) -> bool {
        irq_active(self.ctrl_b)
    }

    fn read_port_a(&mut self) -> u8 {
        if self.data_selected(PiaPort::A) {
            if self.ca2_strobe_e {
                self.ca2 = false;
            }
            let value = self.mixed_port_a();
            if self.ca2_strobe_e {
                self.ca2 = true;
            }
            self.ctrl_a &= !(CTRL_IRQ1 | CTRL_IRQ2);
            value
        } else {
            self.ddr_a
        }
    }

    fn read_port_b(&mut self) -> u8 {
        if self.data_selected(PiaPort::B) {
            let value = self.mixed_port_b();
            self.ctrl_b &= !(CTRL_IRQ1 | CTRL_IRQ2);
            value
        } else {
            self.ddr_b
        }
    }

    fn write_data_or_ddr(&mut self, port: PiaPort, value: u8) {
        if self.data_selected(port) {
            match port {
                PiaPort::A => self.data_a = value,
                PiaPort::B => {
                    self.data_b = value;
                    if self.cb2_strobe_e {
                        self.cb2 = false;
                    }
                }
            }
            self.update_ports();
            if port == PiaPort::B && self.cb2_strobe_e {
                self.cb2 = true;
                self.cb2_strobe_e = false;
            }
        } else {
            match port {
                PiaPort::A => self.ddr_a = value,
                PiaPort::B => self.ddr_b = value,
            }
            self.update_ports();
        }
    }

    fn write_control(&mut self, port: PiaPort, value: u8) {
        match port {
            PiaPort::A => {
                self.ctrl_a = value;
                self.update_c2(PiaPort::A, value);
            }
            PiaPort::B => {
                self.ctrl_b = value;
                self.update_c2(PiaPort::B, value);
            }
        }
    }

    fn update_c2(&mut self, port: PiaPort, control: u8) {
        let c2_is_output = control & CTRL_C2_DDR != 0;
        if !c2_is_output {
            return;
        }

        match control & CTRL_C2_MODE {
            C2_RESET => self.set_c2(port, false),
            C2_SET => self.set_c2(port, true),
            C2_STROBE_E => match port {
                PiaPort::A => self.ca2_strobe_e = true,
                PiaPort::B => self.cb2_strobe_e = true,
            },
            _ => {
                if port == PiaPort::A {
                    self.ca2_strobe_e = false;
                } else {
                    self.cb2_strobe_e = false;
                }
            }
        }
    }

    fn set_c2(&mut self, port: PiaPort, value: bool) {
        match port {
            PiaPort::A => {
                self.ca2 = value;
                self.ca2_strobe_e = false;
            }
            PiaPort::B => {
                self.cb2 = value;
                self.cb2_strobe_e = false;
            }
        }
    }

    fn data_selected(&self, port: PiaPort) -> bool {
        self.control(port) & CTRL_REG_SELECT != 0
    }

    fn mixed_port_a(&self) -> u8 {
        (self.data_a & self.ddr_a) | (self.pa_in & !self.ddr_a)
    }

    fn mixed_port_b(&self) -> u8 {
        (self.data_b & self.ddr_b) | (self.pb_in & !self.ddr_b)
    }

    fn update_ports(&mut self) {
        self.pa = self.mixed_port_a();
        self.pb = self.mixed_port_b();
    }
}

impl Default for Pia6821 {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Register {
    Data(PiaPort),
    Control(PiaPort),
}

fn decode_register(addr: u8) -> Register {
    match addr & 0x03 {
        0x00 => Register::Data(PiaPort::A),
        0x01 => Register::Control(PiaPort::A),
        0x02 => Register::Data(PiaPort::B),
        _ => Register::Control(PiaPort::B),
    }
}

fn irq_active(control: u8) -> bool {
    let ca1_enabled = control & 0x01 != 0;
    let ca2_enabled = control & 0x08 != 0;
    let ca1_active = control & CTRL_IRQ1 != 0;
    let ca2_active = control & CTRL_IRQ2 != 0;
    (ca1_enabled && ca1_active) || (ca2_enabled && ca2_active)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_defaults_to_input_ports_pulled_high() {
        let pia = Pia6821::new();

        assert_eq!(pia.peek(0), 0);
        assert_eq!(pia.pa, 0xFF);
        assert_eq!(pia.pb, 0xFF);
        assert_eq!(pia.ddr(PiaPort::A), 0);
        assert_eq!(pia.ddr(PiaPort::B), 0);
    }

    #[test]
    fn control_bit_selects_ddr_or_data_register() {
        let mut pia = Pia6821::new();

        pia.write(0, 0xF0);
        assert_eq!(pia.ddr(PiaPort::A), 0xF0);
        assert_eq!(pia.read(0), 0xF0);

        pia.write(1, 0x04);
        pia.write(0, 0xA5);
        assert_eq!(pia.output_latch(PiaPort::A), 0xA5);
        assert_eq!(pia.read(0), 0xAF);
    }

    #[test]
    fn input_bits_are_mixed_with_output_bits() {
        let mut pia = Pia6821::new();

        pia.write(0, 0x0F);
        pia.write(1, 0x04);
        pia.write(0, 0x05);
        pia.set_input(PiaPort::A, 0xA0);

        assert_eq!(pia.read(0), 0xA5);
        assert_eq!(pia.pa, 0xA5);
    }

    #[test]
    fn reading_data_port_clears_irq_flags() {
        let mut pia = Pia6821::new();
        pia.write(1, 0x05);
        pia.set_signal(PiaSignal::Ca1);
        pia.set_signal(PiaSignal::Ca2);

        assert_eq!(pia.control(PiaPort::A) & 0xC0, 0xC0);
        assert_eq!(pia.read(0), 0xFF);
        assert_eq!(pia.control(PiaPort::A) & 0xC0, 0);
    }

    #[test]
    fn c2_output_control_can_set_and_reset_lines() {
        let mut pia = Pia6821::new();

        pia.write(1, 0x38);
        assert!(pia.ca2);

        pia.write(1, 0x30);
        assert!(!pia.ca2);

        pia.write(3, 0x38);
        assert!(pia.cb2);

        pia.write(3, 0x30);
        assert!(!pia.cb2);
    }
}
