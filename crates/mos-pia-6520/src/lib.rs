//! MOS 6520/6821 Peripheral Interface Adapter (PIA).
//!
//! The 6520 provides two 8-bit I/O ports with data direction registers
//! and control lines (CA1/CA2, CB1/CB2) with edge-triggered interrupts.
//! Unlike the 6522 VIA, the PIA has no timers or shift register.
//!
//! The Atari 800XL maps the PIA at $D300-$D303 for joystick input
//! (PORTA) and memory banking control (PORTB).
//!
//! # Registers
//!
//! The PIA uses 4 addresses, but control register bit 2 selects
//! whether the data address accesses the DDR or the output register:
//!
//! | Addr | CR bit 2 | Register                          |
//! |------|----------|-----------------------------------|
//! | $00  | 0        | DDRA (Port A data direction)      |
//! | $00  | 1        | ORA (Port A output / data)        |
//! | $01  | —        | CRA (Control register A)          |
//! | $02  | 0        | DDRB (Port B data direction)      |
//! | $02  | 1        | ORB (Port B output / data)        |
//! | $03  | —        | CRB (Control register B)          |

/// MOS 6520/6821 Peripheral Interface Adapter.
pub struct Pia6520 {
    /// Port A output register.
    port_a: u8,
    /// Port A data direction register (1 = output).
    ddr_a: u8,
    /// External input lines for port A.
    pub input_a: u8,

    /// Port B output register.
    port_b: u8,
    /// Port B data direction register (1 = output).
    ddr_b: u8,
    /// External input lines for port B.
    pub input_b: u8,

    /// Control register A.
    ///
    /// - Bit 7: CA1 interrupt flag (read-only)
    /// - Bit 6: CA2 interrupt flag (read-only)
    /// - Bits 5-3: CA2 control
    /// - Bit 2: DDR access (0 = DDR, 1 = data register)
    /// - Bit 1: CA1 edge select (0 = falling, 1 = rising)
    /// - Bit 0: CA1 interrupt enable
    cra: u8,

    /// Control register B (same layout as CRA, for CB1/CB2).
    crb: u8,

    /// CA1 interrupt flag.
    irq_a1: bool,
    /// CA2 interrupt flag.
    irq_a2: bool,
    /// CB1 interrupt flag.
    irq_b1: bool,
    /// CB2 interrupt flag.
    irq_b2: bool,

    /// Previous CA1 state for edge detection.
    ca1_prev: bool,
    /// Previous CB1 state for edge detection.
    cb1_prev: bool,
}

impl Pia6520 {
    /// Create a new PIA with all registers in their reset state.
    ///
    /// All DDR bits default to 0 (input), control registers to 0,
    /// and external input lines to 0xFF (active-high pull-ups).
    #[must_use]
    pub fn new() -> Self {
        Self {
            port_a: 0,
            ddr_a: 0,
            input_a: 0xFF,

            port_b: 0,
            ddr_b: 0,
            input_b: 0xFF,

            cra: 0,
            crb: 0,

            irq_a1: false,
            irq_a2: false,
            irq_b1: false,
            irq_b2: false,

            ca1_prev: false,
            cb1_prev: false,
        }
    }

    /// Read a PIA register.
    ///
    /// `addr` should be 0-3 (only the low 2 bits are used).
    /// Reading port A clears CA1/CA2 interrupt flags.
    /// Reading port B clears CB1/CB2 interrupt flags.
    pub fn read(&mut self, addr: u8) -> u8 {
        match addr & 0x03 {
            0x00 => {
                if self.cra & 0x04 != 0 {
                    // Data register: output bits from register, input bits from external
                    self.irq_a1 = false;
                    self.irq_a2 = false;
                    (self.port_a & self.ddr_a) | (self.input_a & !self.ddr_a)
                } else {
                    // DDR
                    self.ddr_a
                }
            }
            0x01 => {
                // CRA: flags in bits 7-6, writable bits in 5-0
                let flags = if self.irq_a1 { 0x80 } else { 0 }
                    | if self.irq_a2 { 0x40 } else { 0 };
                flags | (self.cra & 0x3F)
            }
            0x02 => {
                if self.crb & 0x04 != 0 {
                    // Data register
                    self.irq_b1 = false;
                    self.irq_b2 = false;
                    (self.port_b & self.ddr_b) | (self.input_b & !self.ddr_b)
                } else {
                    // DDR
                    self.ddr_b
                }
            }
            0x03 => {
                // CRB: flags in bits 7-6, writable bits in 5-0
                let flags = if self.irq_b1 { 0x80 } else { 0 }
                    | if self.irq_b2 { 0x40 } else { 0 };
                flags | (self.crb & 0x3F)
            }
            _ => unreachable!(),
        }
    }

    /// Write a PIA register.
    ///
    /// `addr` should be 0-3 (only the low 2 bits are used).
    pub fn write(&mut self, addr: u8, value: u8) {
        match addr & 0x03 {
            0x00 => {
                if self.cra & 0x04 != 0 {
                    // Data register
                    self.port_a = value;
                } else {
                    // DDR
                    self.ddr_a = value;
                }
            }
            0x01 => {
                // CRA: only bits 5-0 are writable; bits 7-6 are read-only flags
                self.cra = value & 0x3F;
            }
            0x02 => {
                if self.crb & 0x04 != 0 {
                    // Data register
                    self.port_b = value;
                } else {
                    // DDR
                    self.ddr_b = value;
                }
            }
            0x03 => {
                // CRB: only bits 5-0 are writable
                self.crb = value & 0x3F;
            }
            _ => unreachable!(),
        }
    }

    /// Check if any enabled interrupt is pending.
    ///
    /// The PIA asserts IRQ when a flag is set and the corresponding
    /// enable bit is active:
    /// - CA1: `irq_a1 && CRA bit 0`
    /// - CA2: `irq_a2 && CRA bit 3`
    /// - CB1: `irq_b1 && CRB bit 0`
    /// - CB2: `irq_b2 && CRB bit 3`
    #[must_use]
    pub fn irq_pending(&self) -> bool {
        (self.irq_a1 && self.cra & 0x01 != 0)
            || (self.irq_a2 && self.cra & 0x08 != 0)
            || (self.irq_b1 && self.crb & 0x01 != 0)
            || (self.irq_b2 && self.crb & 0x08 != 0)
    }

    /// Set the CA1 input line. Triggers an interrupt flag on the
    /// configured edge (CRA bit 1: 0 = falling, 1 = rising).
    pub fn set_ca1(&mut self, state: bool) {
        let rising_edge = self.cra & 0x02 != 0;
        let triggered = if rising_edge {
            !self.ca1_prev && state
        } else {
            self.ca1_prev && !state
        };
        if triggered {
            self.irq_a1 = true;
        }
        self.ca1_prev = state;
    }

    /// Set the CB1 input line. Triggers an interrupt flag on the
    /// configured edge (CRB bit 1: 0 = falling, 1 = rising).
    pub fn set_cb1(&mut self, state: bool) {
        let rising_edge = self.crb & 0x02 != 0;
        let triggered = if rising_edge {
            !self.cb1_prev && state
        } else {
            self.cb1_prev && !state
        };
        if triggered {
            self.irq_b1 = true;
        }
        self.cb1_prev = state;
    }

    /// Set all port A input lines at once.
    pub fn set_port_a_input(&mut self, value: u8) {
        self.input_a = value;
    }

    /// Set all port B input lines at once.
    pub fn set_port_b_input(&mut self, value: u8) {
        self.input_b = value;
    }

    /// Read port A output register directly (for banking logic and observation).
    ///
    /// Returns the full output register value, regardless of DDR setting.
    /// Machine crates use this to read banking state without side effects.
    #[must_use]
    pub fn port_a_output(&self) -> u8 {
        self.port_a
    }

    /// Read port B output register directly (for banking logic and observation).
    ///
    /// Returns the full output register value, regardless of DDR setting.
    /// On the Atari 800XL, the machine crate reads this to determine
    /// which ROMs are banked in.
    #[must_use]
    pub fn port_b_output(&self) -> u8 {
        self.port_b
    }

    /// Read port B data direction register (1 = output, 0 = input).
    ///
    /// Used by machine crates to compute effective port output: bits
    /// configured as input float high (external pull-ups), so the
    /// effective value is `port_b_output() | !ddr_b()`.
    #[must_use]
    pub fn ddr_b(&self) -> u8 {
        self.ddr_b
    }
}

impl Default for Pia6520 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state() {
        let pia = Pia6520::new();
        assert_eq!(pia.port_a, 0);
        assert_eq!(pia.port_b, 0);
        assert_eq!(pia.ddr_a, 0);
        assert_eq!(pia.ddr_b, 0);
        assert_eq!(pia.cra, 0);
        assert_eq!(pia.crb, 0);
        assert!(!pia.irq_a1);
        assert!(!pia.irq_a2);
        assert!(!pia.irq_b1);
        assert!(!pia.irq_b2);
        assert_eq!(pia.input_a, 0xFF);
        assert_eq!(pia.input_b, 0xFF);
        assert!(!pia.irq_pending());
    }

    #[test]
    fn ddr_access_switching() {
        let mut pia = Pia6520::new();

        // CRA bit 2 = 0: address 0 accesses DDR
        pia.write(0x00, 0xF0); // Write DDR_A
        assert_eq!(pia.read(0x00), 0xF0); // Read DDR_A

        // Set CRA bit 2 = 1: address 0 now accesses data register
        pia.write(0x01, 0x04);
        pia.write(0x00, 0xAB); // Write ORA
        // Output bits (0xAB & 0xF0 = 0xA0) | input bits (0xFF & 0x0F = 0x0F)
        assert_eq!(pia.read(0x00), 0xAF);

        // Same for port B
        pia.write(0x02, 0x0F); // Write DDR_B (CRB bit 2 = 0)
        assert_eq!(pia.read(0x02), 0x0F);

        pia.write(0x03, 0x04); // Set CRB bit 2
        pia.write(0x02, 0x55); // Write ORB
        // Output bits (0x55 & 0x0F = 0x05) | input bits (0xFF & 0xF0 = 0xF0)
        assert_eq!(pia.read(0x02), 0xF5);
    }

    #[test]
    fn port_read_ddr_masking() {
        let mut pia = Pia6520::new();

        // Set up port A: low nibble output, high nibble input
        pia.write(0x00, 0x0F); // DDR_A (CRA bit 2 = 0)
        pia.write(0x01, 0x04); // CRA bit 2 = 1, select data register
        pia.write(0x00, 0xAB); // ORA = 0xAB
        pia.input_a = 0xC0; // External input

        // Read: output bits (0xAB & 0x0F = 0x0B) | input bits (0xC0 & 0xF0 = 0xC0)
        assert_eq!(pia.read(0x00), 0xCB);
    }

    #[test]
    fn port_write_stores_output_register() {
        let mut pia = Pia6520::new();

        // Enable data register access
        pia.write(0x01, 0x04); // CRA bit 2 = 1
        pia.write(0x00, 0x42); // Write ORA
        assert_eq!(pia.port_a_output(), 0x42);

        pia.write(0x03, 0x04); // CRB bit 2 = 1
        pia.write(0x02, 0x7F); // Write ORB
        assert_eq!(pia.port_b_output(), 0x7F);
    }

    #[test]
    fn reading_port_clears_interrupt_flags() {
        let mut pia = Pia6520::new();
        pia.irq_a1 = true;
        pia.irq_a2 = true;
        pia.irq_b1 = true;
        pia.irq_b2 = true;

        // Enable data register access for both ports
        pia.write(0x01, 0x04);
        pia.write(0x03, 0x04);

        // Read port A: clears CA1/CA2 flags, leaves CB flags alone
        let _ = pia.read(0x00);
        assert!(!pia.irq_a1);
        assert!(!pia.irq_a2);
        assert!(pia.irq_b1);
        assert!(pia.irq_b2);

        // Read port B: clears CB1/CB2 flags
        let _ = pia.read(0x02);
        assert!(!pia.irq_b1);
        assert!(!pia.irq_b2);
    }

    #[test]
    fn reading_ddr_does_not_clear_flags() {
        let mut pia = Pia6520::new();
        pia.irq_a1 = true;
        pia.irq_a2 = true;

        // CRA bit 2 = 0: reading address 0 accesses DDR, not data
        let _ = pia.read(0x00);
        assert!(pia.irq_a1);
        assert!(pia.irq_a2);
    }

    #[test]
    fn ca1_falling_edge_detection() {
        let mut pia = Pia6520::new();
        // CRA bit 1 = 0: falling edge
        pia.write(0x01, 0x00);
        pia.ca1_prev = true;

        pia.set_ca1(false); // Falling edge
        assert!(pia.irq_a1);

        // Rising edge should not trigger
        pia.irq_a1 = false;
        pia.set_ca1(true);
        assert!(!pia.irq_a1);
    }

    #[test]
    fn ca1_rising_edge_detection() {
        let mut pia = Pia6520::new();
        // CRA bit 1 = 1: rising edge
        pia.write(0x01, 0x02);
        pia.ca1_prev = false;

        pia.set_ca1(true); // Rising edge
        assert!(pia.irq_a1);

        // Falling edge should not trigger
        pia.irq_a1 = false;
        pia.set_ca1(false);
        assert!(!pia.irq_a1);
    }

    #[test]
    fn cb1_edge_detection() {
        let mut pia = Pia6520::new();
        // CRB bit 1 = 1: rising edge
        pia.write(0x03, 0x02);
        pia.cb1_prev = false;

        pia.set_cb1(true);
        assert!(pia.irq_b1);
    }

    #[test]
    fn irq_pending_requires_flag_and_enable() {
        let mut pia = Pia6520::new();

        // Set CA1 flag but no enable
        pia.irq_a1 = true;
        assert!(!pia.irq_pending());

        // Enable CA1 interrupt (CRA bit 0)
        pia.write(0x01, 0x01);
        assert!(pia.irq_pending());

        // Clear flag
        pia.irq_a1 = false;
        assert!(!pia.irq_pending());

        // Test CB1: set flag + enable (CRB bit 0)
        pia.irq_b1 = true;
        pia.write(0x03, 0x01);
        assert!(pia.irq_pending());
    }

    #[test]
    fn irq_pending_ca2_cb2() {
        let mut pia = Pia6520::new();

        // CA2 flag + CA2 enable (CRA bit 3)
        pia.irq_a2 = true;
        assert!(!pia.irq_pending());
        pia.write(0x01, 0x08);
        assert!(pia.irq_pending());

        pia.irq_a2 = false;

        // CB2 flag + CB2 enable (CRB bit 3)
        pia.irq_b2 = true;
        assert!(!pia.irq_pending());
        pia.write(0x03, 0x08);
        assert!(pia.irq_pending());
    }

    #[test]
    fn control_register_returns_flags() {
        let mut pia = Pia6520::new();
        pia.write(0x01, 0x3F); // Set all writable CRA bits
        pia.irq_a1 = true;
        pia.irq_a2 = true;

        let cra = pia.read(0x01);
        assert_eq!(cra & 0x80, 0x80); // CA1 flag
        assert_eq!(cra & 0x40, 0x40); // CA2 flag
        assert_eq!(cra & 0x3F, 0x3F); // Writable bits

        // CRB
        pia.write(0x03, 0x15);
        pia.irq_b1 = true;
        pia.irq_b2 = false;

        let crb = pia.read(0x03);
        assert_eq!(crb & 0x80, 0x80); // CB1 flag
        assert_eq!(crb & 0x40, 0x00); // CB2 not set
        assert_eq!(crb & 0x3F, 0x15);
    }

    #[test]
    fn portb_output_for_banking() {
        let mut pia = Pia6520::new();

        // Atari 800XL PORTB setup: all bits output
        pia.write(0x02, 0xFF); // DDR_B = all output (CRB bit 2 = 0)
        pia.write(0x03, 0x04); // CRB bit 2 = 1, select data register

        // Default state: OS ROM enabled, BASIC disabled, self-test off
        pia.write(0x02, 0xFF); // PORTB = $FF
        assert_eq!(pia.port_b_output(), 0xFF);

        // Enable BASIC (bit 1 = 0), keep OS ROM (bit 0 = 1)
        pia.write(0x02, 0xFD);
        assert_eq!(pia.port_b_output(), 0xFD);
        assert_eq!(pia.port_b_output() & 0x01, 0x01); // OS ROM enabled
        assert_eq!(pia.port_b_output() & 0x02, 0x00); // BASIC enabled (active low)
    }

    #[test]
    fn control_register_write_masks_top_bits() {
        let mut pia = Pia6520::new();

        // Writing 0xFF to CRA should only store bits 5-0
        pia.write(0x01, 0xFF);
        assert_eq!(pia.cra, 0x3F);

        pia.write(0x03, 0xFF);
        assert_eq!(pia.crb, 0x3F);
    }
}
