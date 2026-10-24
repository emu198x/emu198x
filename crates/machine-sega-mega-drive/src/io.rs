//! The three I/O ports and the pads on them.
//!
//! Each port has a data register and a control register, which says which
//! of the data bits the console drives. Bit 6 is TH, the line a pad uses to
//! choose what it reports. A three-button pad answers with TH high:
//! `C B Right Left Down Up`, and with TH low: `Start A 0 0 Down Up`, the
//! two low bits identifying it. A six-button pad counts TH's falls: after
//! the third it reads the four D-pad lines as all low, then on TH high it
//! reports `Mode X Y Z` in the low four bits, and after the fourth fall the
//! four lines read high. The count resets if TH stays still for about
//! 1.5 ms. Buttons are active low.

use serde::{Deserialize, Serialize};

/// Master clocks a six-button pad waits before resetting its TH count.
const SIX_BUTTON_TIMEOUT_MCLKS: u32 = 80_000;

/// A pad's buttons, true while pressed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Buttons {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub a: bool,
    pub b: bool,
    pub c: bool,
    pub start: bool,
    pub x: bool,
    pub y: bool,
    pub z: bool,
    pub mode: bool,
}

/// What is plugged into a port.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Device {
    None,
    ThreeButton,
    SixButton,
}

/// One controller port.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Port {
    pub device: Device,
    pub buttons: Buttons,
    data: u8,
    control: u8,
    th_count: u8,
    idle_mclks: u32,
}

impl Port {
    #[must_use]
    pub fn new(device: Device) -> Self {
        Self {
            device,
            buttons: Buttons::default(),
            data: 0,
            control: 0,
            th_count: 0,
            idle_mclks: 0,
        }
    }

    /// The TH line: driven by the console when control bit 6 is set, else
    /// pulled high.
    fn th(&self) -> bool {
        self.control & 0x40 == 0 || self.data & 0x40 != 0
    }

    pub fn write_data(&mut self, value: u8) {
        let before = self.th();
        self.data = value;
        self.th_changed(before);
    }

    pub fn write_control(&mut self, value: u8) {
        let before = self.th();
        self.control = value;
        self.th_changed(before);
    }

    fn th_changed(&mut self, before: bool) {
        let after = self.th();
        if before && !after && self.device == Device::SixButton {
            self.th_count = if self.th_count >= 4 {
                1
            } else {
                self.th_count + 1
            };
        }
        if before != after {
            self.idle_mclks = 0;
        }
    }

    /// Advance the six-button pad's idle timer by `mclks`.
    pub fn tick(&mut self, mclks: u32) {
        if self.th_count != 0 {
            self.idle_mclks += mclks;
            if self.idle_mclks >= SIX_BUTTON_TIMEOUT_MCLKS {
                self.th_count = 0;
                self.idle_mclks = 0;
            }
        }
    }

    #[must_use]
    pub fn control(&self) -> u8 {
        self.control
    }

    /// What the pad drives onto the lines it owns, active low.
    fn pad_lines(&self) -> u8 {
        if self.device == Device::None {
            return 0x7F;
        }
        let b = self.buttons;
        let bit = |pressed: bool, n: u8| if pressed { 0 } else { 1 << n };
        let th = self.th();
        let six = self.device == Device::SixButton;
        if th {
            if six && self.th_count == 3 {
                // After the third TH fall: Mode X Y Z.
                bit(b.c, 5)
                    | bit(b.b, 4)
                    | bit(b.mode, 3)
                    | bit(b.x, 2)
                    | bit(b.y, 1)
                    | bit(b.z, 0)
                    | 0x40
            } else {
                bit(b.c, 5)
                    | bit(b.b, 4)
                    | bit(b.right, 3)
                    | bit(b.left, 2)
                    | bit(b.down, 1)
                    | bit(b.up, 0)
                    | 0x40
            }
        } else if six && self.th_count == 3 {
            // The third TH-low: D-pad lines all low, the six-button mark.
            bit(b.start, 5) | bit(b.a, 4)
        } else if six && self.th_count == 4 {
            // The fourth: the lines read high.
            bit(b.start, 5) | bit(b.a, 4) | 0x0F
        } else {
            bit(b.start, 5) | bit(b.a, 4) | bit(b.down, 1) | bit(b.up, 0)
        }
    }

    /// The data register as the CPU reads it: output bits from the latch,
    /// input bits from the pad.
    #[must_use]
    pub fn read_data(&self) -> u8 {
        let outputs = self.control & 0x7F;
        (self.data & (outputs | 0x80)) | (self.pad_lines() & !outputs & 0x7F)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A port with the console driving TH, high. Setting the latch before
    /// the control register keeps TH from falling, which a six-button pad
    /// would count.
    fn pad(device: Device) -> Port {
        let mut port = Port::new(device);
        port.write_data(0x40);
        port.write_control(0x40);
        port
    }

    #[test]
    fn a_three_button_pad_reports_by_th() {
        let mut port = pad(Device::ThreeButton);
        port.buttons.up = true;
        port.buttons.b = true;
        port.buttons.start = true;
        port.write_data(0x40);
        assert_eq!(
            port.read_data() & 0x3F,
            0b10_1110,
            "C B R L D U, B and Up pressed"
        );
        port.write_data(0x00);
        assert_eq!(port.read_data() & 0x3F, 0b01_0010, "Start A 0 0 D U");
    }

    #[test]
    fn a_six_button_pad_answers_on_the_third_th_cycle() {
        let mut port = pad(Device::SixButton);
        port.buttons.x = true;
        port.buttons.mode = true;
        for _ in 0..2 {
            port.write_data(0x40);
            port.write_data(0x00);
        }
        port.write_data(0x40);
        port.write_data(0x00);
        assert_eq!(port.read_data() & 0x0F, 0, "the six-button mark");
        port.write_data(0x40);
        assert_eq!(port.read_data() & 0x0F, 0b0011, "Mode and X pressed");
        port.tick(SIX_BUTTON_TIMEOUT_MCLKS);
        port.write_data(0x40);
        assert_eq!(
            port.read_data() & 0x0F,
            0x0F,
            "back to the D-pad after the timeout"
        );
    }

    #[test]
    fn output_bits_read_back_from_the_latch() {
        let mut port = Port::new(Device::None);
        port.write_control(0x7F);
        port.write_data(0x55);
        assert_eq!(port.read_data(), 0x55);
    }
}
