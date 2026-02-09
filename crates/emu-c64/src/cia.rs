//! CIA 6526 Complex Interface Adapter.
//!
//! Two identical CIAs in the C64:
//! - CIA1 ($DC00-$DC0F): keyboard scanning, joystick, Timer A/B → IRQ
//! - CIA2 ($DD00-$DD0F): VIC-II bank, serial bus, Timer A/B → NMI
//!
//! # Registers (per CIA)
//!
//! | Reg | Read               | Write              |
//! |-----|--------------------|--------------------|
//! | $x0 | Port A data        | Port A data        |
//! | $x1 | Port B data        | Port B data        |
//! | $x2 | Port A DDR         | Port A DDR         |
//! | $x3 | Port B DDR         | Port B DDR         |
//! | $x4 | Timer A low (cnt)  | Timer A low (latch)|
//! | $x5 | Timer A high (cnt) | Timer A high (latch)|
//! | $x6 | Timer B low (cnt)  | Timer B low (latch)|
//! | $x7 | Timer B high (cnt) | Timer B high (latch)|
//! | $x8 | TOD 10ths          | TOD 10ths          |
//! | $x9 | TOD seconds        | TOD seconds        |
//! | $xA | TOD minutes        | TOD minutes        |
//! | $xB | TOD hours          | TOD hours          |
//! | $xC | Serial shift reg   | Serial shift reg   |
//! | $xD | ICR (read/clear)   | ICR (set/clear mask)|
//! | $xE | Control reg A      | Control reg A      |
//! | $xF | Control reg B      | Control reg B      |

#![allow(clippy::cast_possible_truncation)]

use crate::keyboard::KeyboardMatrix;

/// CIA 6526 instance.
pub struct Cia {
    /// Port A output register.
    port_a: u8,
    /// Port B output register.
    port_b: u8,
    /// Port A data direction register (1 = output).
    ddr_a: u8,
    /// Port B data direction register (1 = output).
    ddr_b: u8,

    /// Timer A counter.
    timer_a: u16,
    /// Timer A latch.
    timer_a_latch: u16,
    /// Timer A running.
    timer_a_running: bool,
    /// Timer A one-shot mode (true) or continuous (false).
    timer_a_oneshot: bool,
    /// Timer A force-load strobe pending.
    timer_a_force_load: bool,

    /// Timer B counter.
    timer_b: u16,
    /// Timer B latch.
    timer_b_latch: u16,
    /// Timer B running.
    timer_b_running: bool,
    /// Timer B one-shot mode.
    timer_b_oneshot: bool,
    /// Timer B force-load strobe pending.
    timer_b_force_load: bool,

    /// Interrupt control: status flags (bits 0-4).
    icr_status: u8,
    /// Interrupt control: enable mask (bits 0-4).
    icr_mask: u8,

    /// Control register A.
    cra: u8,
    /// Control register B.
    crb: u8,
}

impl Cia {
    #[must_use]
    pub fn new() -> Self {
        Self {
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
            icr_status: 0,
            icr_mask: 0,
            cra: 0,
            crb: 0,
        }
    }

    /// Tick the CIA for one CPU cycle.
    ///
    /// Counts down Timer A and Timer B if running.
    pub fn tick(&mut self) {
        // Timer A force load
        if self.timer_a_force_load {
            self.timer_a = self.timer_a_latch;
            self.timer_a_force_load = false;
        }

        // Timer A countdown
        if self.timer_a_running {
            if self.timer_a == 0 {
                // Underflow
                self.icr_status |= 0x01; // Timer A underflow flag
                self.timer_a = self.timer_a_latch; // Reload
                if self.timer_a_oneshot {
                    self.timer_a_running = false;
                    self.cra &= !0x01; // Clear start bit
                }
            } else {
                self.timer_a -= 1;
            }
        }

        // Timer B force load
        if self.timer_b_force_load {
            self.timer_b = self.timer_b_latch;
            self.timer_b_force_load = false;
        }

        // Timer B countdown (simplified: always counts CPU cycles, ignoring
        // Timer A underflow mode for v1)
        if self.timer_b_running {
            if self.timer_b == 0 {
                self.icr_status |= 0x02; // Timer B underflow flag
                self.timer_b = self.timer_b_latch;
                if self.timer_b_oneshot {
                    self.timer_b_running = false;
                    self.crb &= !0x01;
                }
            } else {
                self.timer_b -= 1;
            }
        }
    }

    /// Check if the CIA has an active IRQ/NMI.
    #[must_use]
    pub fn irq_active(&self) -> bool {
        (self.icr_status & self.icr_mask & 0x1F) != 0
    }

    /// Read a CIA register.
    ///
    /// For CIA1, pass the keyboard matrix to read port B (keyboard rows).
    /// For CIA2, pass `None` for keyboard.
    #[must_use]
    pub fn read(&self, reg: u8) -> u8 {
        self.read_internal(reg, None)
    }

    /// Read a CIA register with keyboard matrix for CIA1 port B.
    #[must_use]
    pub fn read_with_keyboard(&self, reg: u8, keyboard: &KeyboardMatrix) -> u8 {
        self.read_internal(reg, Some(keyboard))
    }

    fn read_internal(&self, reg: u8, keyboard: Option<&KeyboardMatrix>) -> u8 {
        match reg & 0x0F {
            0x00 => {
                // Port A data: output bits from port_a, input bits float high
                (self.port_a & self.ddr_a) | (!self.ddr_a)
            }
            0x01 => {
                // Port B data: for CIA1, this reads the keyboard matrix
                let port_output = (self.port_b & self.ddr_b) | (!self.ddr_b);
                if let Some(kbd) = keyboard {
                    // CIA1: scan keyboard using port A as column select
                    let col_mask = (self.port_a & self.ddr_a) | (!self.ddr_a);
                    let kbd_data = kbd.scan(col_mask);
                    // Merge: output bits from port_b, input bits from keyboard
                    (self.port_b & self.ddr_b) | (kbd_data & !self.ddr_b)
                } else {
                    port_output
                }
            }
            0x02 => self.ddr_a,
            0x03 => self.ddr_b,
            0x04 => self.timer_a as u8,
            0x05 => (self.timer_a >> 8) as u8,
            0x06 => self.timer_b as u8,
            0x07 => (self.timer_b >> 8) as u8,
            // TOD registers: return 0 (stubbed)
            0x08..=0x0B => 0,
            // Serial shift register: return 0 (stubbed)
            0x0C => 0,
            0x0D => {
                // ICR read: returns status with bit 7 = any active, then clears status.
                // Note: we return a snapshot; the actual clear happens in read_icr_and_clear().
                let any = if (self.icr_status & self.icr_mask & 0x1F) != 0 {
                    0x80
                } else {
                    0x00
                };
                self.icr_status | any
            }
            0x0E => self.cra,
            0x0F => self.crb,
            _ => 0xFF,
        }
    }

    /// Read ICR and clear status (side-effectful read).
    /// This should be called by the bus layer for reads of register $xD.
    pub fn read_icr_and_clear(&mut self) -> u8 {
        let any = if (self.icr_status & self.icr_mask & 0x1F) != 0 {
            0x80
        } else {
            0x00
        };
        let result = self.icr_status | any;
        self.icr_status = 0;
        result
    }

    /// Write a CIA register.
    pub fn write(&mut self, reg: u8, value: u8) {
        match reg & 0x0F {
            0x00 => self.port_a = value,
            0x01 => self.port_b = value,
            0x02 => self.ddr_a = value,
            0x03 => self.ddr_b = value,
            0x04 => {
                // Timer A latch low byte
                self.timer_a_latch = (self.timer_a_latch & 0xFF00) | u16::from(value);
            }
            0x05 => {
                // Timer A latch high byte
                self.timer_a_latch = (self.timer_a_latch & 0x00FF) | (u16::from(value) << 8);
                // If timer is stopped, writing high byte loads the counter
                if !self.timer_a_running {
                    self.timer_a = self.timer_a_latch;
                }
            }
            0x06 => {
                self.timer_b_latch = (self.timer_b_latch & 0xFF00) | u16::from(value);
            }
            0x07 => {
                self.timer_b_latch = (self.timer_b_latch & 0x00FF) | (u16::from(value) << 8);
                if !self.timer_b_running {
                    self.timer_b = self.timer_b_latch;
                }
            }
            // TOD: ignored for v1
            0x08..=0x0B => {}
            // Serial shift register: ignored
            0x0C => {}
            0x0D => {
                // ICR write: bit 7 = set(1) or clear(0) the mask bits
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
                if value & 0x10 != 0 {
                    // Force load: copy latch → counter
                    self.timer_a_force_load = true;
                }
            }
            0x0F => {
                self.crb = value;
                self.timer_b_running = value & 0x01 != 0;
                self.timer_b_oneshot = value & 0x08 != 0;
                if value & 0x10 != 0 {
                    self.timer_b_force_load = true;
                }
            }
            _ => {}
        }
    }

    /// Get port A output value (for reading VIC bank from CIA2).
    #[must_use]
    pub fn port_a_output(&self) -> u8 {
        (self.port_a & self.ddr_a) | (!self.ddr_a)
    }

    /// Debug: Timer A counter value.
    #[must_use]
    pub fn timer_a(&self) -> u16 {
        self.timer_a
    }

    /// Debug: Timer B counter value.
    #[must_use]
    pub fn timer_b(&self) -> u16 {
        self.timer_b
    }

    /// Debug: ICR status flags.
    #[must_use]
    pub fn icr_status(&self) -> u8 {
        self.icr_status
    }

    /// Debug: ICR mask bits.
    #[must_use]
    pub fn icr_mask(&self) -> u8 {
        self.icr_mask
    }

    /// Debug: Control register A.
    #[must_use]
    pub fn cra(&self) -> u8 {
        self.cra
    }

    /// Debug: Control register B.
    #[must_use]
    pub fn crb(&self) -> u8 {
        self.crb
    }
}

impl Default for Cia {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_a_countdown() {
        let mut cia = Cia::new();
        // Set latch to 10, start timer
        cia.write(0x04, 10); // Low byte
        cia.write(0x05, 0); // High byte (also loads counter when stopped)
        cia.write(0x0E, 0x01); // Start, continuous mode

        // Timer counts: 10→9→...→1→0 (10 ticks), then on tick 11 it detects 0
        // and signals underflow
        for _ in 0..11 {
            cia.tick();
        }
        assert!(cia.icr_status & 0x01 != 0);
    }

    #[test]
    fn timer_a_oneshot() {
        let mut cia = Cia::new();
        cia.write(0x04, 5);
        cia.write(0x05, 0);
        cia.write(0x0E, 0x09); // Start + one-shot

        for _ in 0..6 {
            cia.tick();
        }
        assert!(cia.icr_status & 0x01 != 0);
        // Timer should have stopped
        assert!(!cia.timer_a_running);
    }

    #[test]
    fn icr_read_clears_status() {
        let mut cia = Cia::new();
        cia.icr_status = 0x01;
        cia.icr_mask = 0x01;

        let val = cia.read_icr_and_clear();
        assert_eq!(val, 0x81); // Status + bit 7
        assert_eq!(cia.icr_status, 0); // Cleared
    }

    #[test]
    fn icr_mask_set_clear() {
        let mut cia = Cia::new();
        // Set bits 0 and 1
        cia.write(0x0D, 0x83); // Set mode (bit 7) + bits 0,1
        assert_eq!(cia.icr_mask, 0x03);

        // Clear bit 0
        cia.write(0x0D, 0x01); // Clear mode (bit 7 = 0) + bit 0
        assert_eq!(cia.icr_mask, 0x02);
    }

    #[test]
    fn irq_active_when_status_and_mask() {
        let mut cia = Cia::new();
        cia.icr_status = 0x01;
        cia.icr_mask = 0x00;
        assert!(!cia.irq_active()); // Status set but mask clear

        cia.icr_mask = 0x01;
        assert!(cia.irq_active()); // Both set
    }

    #[test]
    fn port_a_output() {
        let mut cia = Cia::new();
        cia.write(0x02, 0xFF); // DDR: all output
        cia.write(0x00, 0x42); // Port A data
        assert_eq!(cia.port_a_output(), 0x42);
    }

    #[test]
    fn keyboard_scan_via_cia1() {
        let mut cia = Cia::new();
        let mut kbd = KeyboardMatrix::new();

        // CIA1: Port A is output (columns), Port B is input (rows)
        cia.write(0x02, 0xFF); // DDR A: all output
        cia.write(0x03, 0x00); // DDR B: all input
        cia.write(0x00, 0xFD); // Select column 1 (bit 1 = 0)

        // Press key at col=1, row=1
        kbd.set_key(1, 1, true);

        let result = cia.read_with_keyboard(0x01, &kbd);
        assert_eq!(result & 0x02, 0x00); // Row 1 should be low (pressed)
    }
}
