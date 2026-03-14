//! Intel 8255 Programmable Peripheral Interface (PPI).
//!
//! Three 8-bit I/O ports (A, B, C) with configurable direction, plus a
//! control register for mode selection and bit set/reset on port C.
//!
//! The MSX uses Mode 0 exclusively: Port A = output (slot select),
//! Port B = input (keyboard data), Port C = output (keyboard row +
//! cassette + LED).

/// Intel 8255 PPI.
pub struct Ppi8255 {
    /// Port A output latch.
    pub port_a: u8,
    /// Port B input value (set externally).
    pub port_b: u8,
    /// Port C output latch.
    pub port_c: u8,
    /// Control/mode register.
    control: u8,
}

impl Ppi8255 {
    /// Create a new PPI. Default mode: $82 (Mode 0, Port B input).
    #[must_use]
    pub fn new() -> Self {
        Self {
            port_a: 0,
            port_b: 0xFF, // Input defaults high (active-low)
            port_c: 0,
            control: 0x82, // MSX default: Mode 0, Port B input
        }
    }

    /// Read from a PPI register (address bits 1-0 select A/B/C/control).
    #[must_use]
    pub fn read(&self, port: u8) -> u8 {
        match port & 0x03 {
            0 => self.port_a,
            1 => self.port_b,
            2 => self.port_c,
            3 => self.control,
            _ => unreachable!(),
        }
    }

    /// Write to a PPI register.
    pub fn write(&mut self, port: u8, value: u8) {
        match port & 0x03 {
            0 => self.port_a = value,
            1 => {} // Port B is input-only in MSX config
            2 => self.port_c = value,
            3 => {
                if value & 0x80 != 0 {
                    // Mode set
                    self.control = value;
                    // Reset all ports on mode set
                    self.port_a = 0;
                    self.port_c = 0;
                } else {
                    // Bit set/reset on Port C
                    let bit = (value >> 1) & 0x07;
                    if value & 0x01 != 0 {
                        self.port_c |= 1 << bit;
                    } else {
                        self.port_c &= !(1 << bit);
                    }
                }
            }
            _ => unreachable!(),
        }
    }

    /// The keyboard row selected by Port C bits 3-0.
    #[must_use]
    pub fn keyboard_row(&self) -> u8 {
        self.port_c & 0x0F
    }
}

impl Default for Ppi8255 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state() {
        let ppi = Ppi8255::new();
        assert_eq!(ppi.port_a, 0);
        assert_eq!(ppi.port_b, 0xFF);
        assert_eq!(ppi.port_c, 0);
    }

    #[test]
    fn port_a_write_read() {
        let mut ppi = Ppi8255::new();
        ppi.write(0, 0xAB);
        assert_eq!(ppi.read(0), 0xAB);
    }

    #[test]
    fn port_c_bit_set_reset() {
        let mut ppi = Ppi8255::new();
        // Set bit 3 of port C
        ppi.write(3, 0x07); // bit 7=0, bit 3-1=011(bit3), bit 0=1(set)
        assert_eq!(ppi.port_c & 0x08, 0x08);
        // Reset bit 3
        ppi.write(3, 0x06); // bit 3-1=011(bit3), bit 0=0(reset)
        assert_eq!(ppi.port_c & 0x08, 0x00);
    }

    #[test]
    fn mode_set_resets_ports() {
        let mut ppi = Ppi8255::new();
        ppi.write(0, 0xFF); // Port A = $FF
        ppi.write(3, 0x82); // Mode set
        assert_eq!(ppi.port_a, 0); // Reset to 0
    }

    #[test]
    fn keyboard_row_from_port_c() {
        let mut ppi = Ppi8255::new();
        ppi.write(2, 0x05); // Row 5
        assert_eq!(ppi.keyboard_row(), 5);
    }
}
