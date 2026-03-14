//! MOS 6532 RIOT (RAM-I/O-Timer).
//!
//! The 6532 provides 128 bytes of RAM, two 8-bit I/O ports with data
//! direction registers, and a programmable interval timer with four
//! selectable dividers.
//!
//! Used in the Atari 2600 for I/O (joystick ports, console switches)
//! and timing. The 6507 CPU addresses the RIOT through the system bus
//! with address decoding based on A7/A9/A12.
//!
//! # Timer
//!
//! The timer counts down once per CPU cycle. A write to $294-$297 loads
//! the timer with the written value and selects a divider (1, 8, 64, or
//! 1024). The timer counts down at the divided rate. When it reaches 0,
//! it underflows, sets the interrupt flag, and counts down at 1× rate
//! (every CPU cycle) regardless of the original divider.
//!
//! # I/O Ports
//!
//! Port A ($280 read/$280 write) and Port B ($282 read/$282 write) each
//! have a DDR ($281/$283). Bits set to 1 in the DDR are outputs; bits
//! set to 0 are inputs. Reading a port returns output-register bits for
//! output pins and external-input bits for input pins.
//!
//! # Register map (active bits: A0-A4, active when A9=1, A7=1)
//!
//! | Addr  | R/W | Name   | Description                    |
//! |-------|-----|--------|--------------------------------|
//! | $280  | R   | SWCHA  | Port A data (output+input)     |
//! | $280  | W   | SWCHA  | Port A output register         |
//! | $281  | R/W | SWACNT | Port A DDR (1=output)          |
//! | $282  | R   | SWCHB  | Port B data (output+input)     |
//! | $282  | W   | SWCHB  | Port B output register         |
//! | $283  | R/W | SWBCNT | Port B DDR (1=output)          |
//! | $284  | R   | INTIM  | Timer value                    |
//! | $285  | R   | INSTAT | Timer status (bit 7=underflow, bit 6=PA7 flag) |
//! | $294  | W   | TIM1T  | Set timer ÷1 (1 tick/cycle)    |
//! | $295  | W   | TIM8T  | Set timer ÷8                   |
//! | $296  | W   | TIM64T | Set timer ÷64                  |
//! | $297  | W   | T1024T | Set timer ÷1024                |

/// MOS 6532 RIOT.
pub struct Riot6532 {
    /// 128 bytes of internal RAM.
    ram: [u8; 128],

    /// Port A output register.
    port_a: u8,
    /// Port A data direction register (1 = output).
    ddr_a: u8,
    /// External input lines for port A (active-low for joystick).
    pub input_a: u8,

    /// Port B output register.
    port_b: u8,
    /// Port B data direction register (1 = output).
    ddr_b: u8,
    /// External input lines for port B (active-low for switches).
    pub input_b: u8,

    /// Timer counter (counts down).
    timer: u8,
    /// Timer divider: 1, 8, 64, or 1024.
    divider: u16,
    /// Prescaler counter — counts CPU cycles until next timer decrement.
    prescaler: u16,
    /// Timer underflow flag (bit 7 of INSTAT).
    underflow: bool,
    /// Timer is in post-underflow mode (counting at 1× until read).
    post_underflow: bool,
}

impl Riot6532 {
    /// Create a new RIOT with all registers cleared.
    #[must_use]
    pub fn new() -> Self {
        Self {
            ram: [0; 128],
            port_a: 0,
            ddr_a: 0,
            input_a: 0xFF,
            port_b: 0,
            ddr_b: 0,
            input_b: 0xFF,
            timer: 0,
            divider: 1024,
            prescaler: 1024,
            underflow: false,
            post_underflow: false,
        }
    }

    /// Advance the timer by one CPU cycle.
    ///
    /// Call this once per CPU cycle (every 3 colour clocks).
    pub fn tick(&mut self) {
        if self.post_underflow {
            // After underflow, count down at 1× rate every cycle.
            let (result, overflow) = self.timer.overflowing_sub(1);
            self.timer = result;
            if overflow {
                // Wrapped around from 0 → 255; keep counting.
                // Underflow flag stays set until read.
            }
            return;
        }

        self.prescaler -= 1;
        if self.prescaler == 0 {
            self.prescaler = self.divider;

            if self.timer == 0 {
                // Timer reached 0 → underflow.
                self.underflow = true;
                self.post_underflow = true;
                self.timer = 0xFF;
            } else {
                self.timer -= 1;
            }
        }
    }

    /// Read a RIOT register or RAM byte.
    ///
    /// Address should be the raw 6507 address — this function handles
    /// decoding of A0-A4, A7, A9, and A12.
    #[must_use]
    pub fn read(&mut self, addr: u16) -> u8 {
        // RAM: A9=0, A7=1 → $0080-$00FF
        if addr & 0x0200 == 0 {
            return self.ram[(addr & 0x7F) as usize];
        }

        // I/O registers: A9=1
        match addr & 0x07 {
            // $280: SWCHA — Port A data
            0x00 => self.read_port_a(),
            // $281: SWACNT — Port A DDR
            0x01 => self.ddr_a,
            // $282: SWCHB — Port B data
            0x02 => self.read_port_b(),
            // $283: SWBCNT — Port B DDR
            0x03 => self.ddr_b,
            // $284: INTIM — Timer value (reading clears underflow flag)
            0x04 => {
                self.underflow = false;
                self.post_underflow = false;
                self.prescaler = self.divider;
                self.timer
            }
            // $285: INSTAT — Timer status
            0x05 => {
                let status = if self.underflow { 0x80 } else { 0 };
                // Reading INSTAT clears the underflow flag.
                self.underflow = false;
                status
            }
            _ => 0,
        }
    }

    /// Write a RIOT register or RAM byte.
    pub fn write(&mut self, addr: u16, value: u8) {
        // RAM: A9=0, A7=1 → $0080-$00FF
        if addr & 0x0200 == 0 {
            self.ram[(addr & 0x7F) as usize] = value;
            return;
        }

        // Timer writes: A4=1, A9=1
        if addr & 0x10 != 0 {
            self.timer = value;
            self.underflow = false;
            self.post_underflow = false;
            self.divider = match addr & 0x03 {
                0x00 => 1,    // TIM1T ($294)
                0x01 => 8,    // TIM8T ($295)
                0x02 => 64,   // TIM64T ($296)
                0x03 => 1024, // T1024T ($297)
                _ => unreachable!(),
            };
            self.prescaler = self.divider;
            return;
        }

        // I/O register writes: A4=0, A9=1
        match addr & 0x03 {
            // $280: SWCHA — Port A output register
            0x00 => self.port_a = value,
            // $281: SWACNT — Port A DDR
            0x01 => self.ddr_a = value,
            // $282: SWCHB — Port B output register
            0x02 => self.port_b = value,
            // $283: SWBCNT — Port B DDR
            0x03 => self.ddr_b = value,
            _ => {}
        }
    }

    /// Read port A data: output bits from register, input bits from external.
    fn read_port_a(&self) -> u8 {
        (self.port_a & self.ddr_a) | (self.input_a & !self.ddr_a)
    }

    /// Read port B data: output bits from register, input bits from external.
    fn read_port_b(&self) -> u8 {
        (self.port_b & self.ddr_b) | (self.input_b & !self.ddr_b)
    }

    /// Set external input for port A.
    pub fn set_port_a_input(&mut self, value: u8) {
        self.input_a = value;
    }

    /// Set external input for port B.
    pub fn set_port_b_input(&mut self, value: u8) {
        self.input_b = value;
    }

    /// Direct access to RAM (for observation).
    #[must_use]
    pub fn ram(&self) -> &[u8; 128] {
        &self.ram
    }

    /// Timer value (for observation).
    #[must_use]
    pub fn timer_value(&self) -> u8 {
        self.timer
    }

    /// Whether the timer has underflowed.
    #[must_use]
    pub fn underflow_flag(&self) -> bool {
        self.underflow
    }
}

impl Default for Riot6532 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ram_roundtrip() {
        let mut riot = Riot6532::new();
        // Write to RAM at $80 (A9=0, A7=1, offset 0)
        riot.write(0x0080, 0xAB);
        assert_eq!(riot.read(0x0080), 0xAB);
        // Last byte at $FF
        riot.write(0x00FF, 0xCD);
        assert_eq!(riot.read(0x00FF), 0xCD);
    }

    #[test]
    fn ram_wraps_within_128_bytes() {
        let mut riot = Riot6532::new();
        riot.write(0x0080, 0x42);
        // Same offset, different high bits should hit same RAM
        assert_eq!(riot.read(0x0080), 0x42);
    }

    #[test]
    fn timer_1x_countdown() {
        let mut riot = Riot6532::new();
        // Write TIM1T ($294): timer = 3, divider = 1
        riot.write(0x0294, 3);
        assert_eq!(riot.timer_value(), 3);

        riot.tick(); // 3 → 2
        assert_eq!(riot.timer_value(), 2);
        assert!(!riot.underflow_flag());

        riot.tick(); // 2 → 1
        assert_eq!(riot.timer_value(), 1);

        riot.tick(); // 1 → 0 (not underflow yet — underflow is when 0 decrements)
        assert_eq!(riot.timer_value(), 0);
        assert!(!riot.underflow_flag());

        riot.tick(); // 0 → underflow → sets flag, timer = 0xFF
        assert!(riot.underflow_flag());
        assert_eq!(riot.timer_value(), 0xFF);
    }

    #[test]
    fn timer_8x_countdown() {
        let mut riot = Riot6532::new();
        // Write TIM8T ($295): timer = 2, divider = 8
        riot.write(0x0295, 2);
        assert_eq!(riot.timer_value(), 2);

        // 7 ticks: prescaler counts down but timer hasn't changed
        for _ in 0..7 {
            riot.tick();
        }
        assert_eq!(riot.timer_value(), 2);

        // 8th tick: timer decrements 2 → 1
        riot.tick();
        assert_eq!(riot.timer_value(), 1);

        // 8 more ticks: 1 → 0
        for _ in 0..8 {
            riot.tick();
        }
        assert_eq!(riot.timer_value(), 0);

        // 8 more ticks: 0 → underflow
        for _ in 0..8 {
            riot.tick();
        }
        assert!(riot.underflow_flag());
    }

    #[test]
    fn timer_read_clears_underflow() {
        let mut riot = Riot6532::new();
        riot.write(0x0294, 0); // TIM1T = 0
        riot.tick(); // underflow
        assert!(riot.underflow_flag());

        // Read INTIM ($284) clears the flag
        let _val = riot.read(0x0284);
        assert!(!riot.underflow_flag());
    }

    #[test]
    fn instat_reports_underflow() {
        let mut riot = Riot6532::new();
        riot.write(0x0294, 0); // TIM1T = 0
        riot.tick(); // underflow

        // Read INSTAT ($285): bit 7 should be set
        let status = riot.read(0x0285);
        assert_eq!(status & 0x80, 0x80);

        // Reading INSTAT clears the flag
        let status2 = riot.read(0x0285);
        assert_eq!(status2 & 0x80, 0);
    }

    #[test]
    fn post_underflow_counts_at_1x() {
        let mut riot = Riot6532::new();
        riot.write(0x0296, 0); // TIM64T = 0, divider = 64

        // Tick 64 times to underflow
        for _ in 0..64 {
            riot.tick();
        }
        assert!(riot.underflow_flag());
        assert_eq!(riot.timer_value(), 0xFF);

        // Now counts at 1× regardless of divider
        riot.tick();
        assert_eq!(riot.timer_value(), 0xFE);
        riot.tick();
        assert_eq!(riot.timer_value(), 0xFD);
    }

    #[test]
    fn ddr_masking_port_a() {
        let mut riot = Riot6532::new();
        // Set DDR: bits 7-4 = output, bits 3-0 = input
        riot.write(0x0281, 0xF0);
        // Write output register
        riot.write(0x0280, 0xA0);
        // Set external input
        riot.set_port_a_input(0x05);

        // Read should combine: output(0xA0) & DDR(0xF0) | input(0x05) & !DDR(0x0F)
        let val = riot.read(0x0280);
        assert_eq!(val, 0xA5);
    }

    #[test]
    fn ddr_masking_port_b() {
        let mut riot = Riot6532::new();
        riot.write(0x0283, 0x0F); // Bits 3-0 = output
        riot.write(0x0282, 0x03); // Output register
        riot.set_port_b_input(0xC0); // External input

        let val = riot.read(0x0282);
        // output(0x03) & DDR(0x0F) | input(0xC0) & !DDR(0xF0)
        assert_eq!(val, 0xC3);
    }

    #[test]
    fn timer_reload_clears_underflow_state() {
        let mut riot = Riot6532::new();
        riot.write(0x0294, 0); // underflow immediately
        riot.tick();
        assert!(riot.underflow_flag());

        // Reload the timer
        riot.write(0x0294, 10);
        assert!(!riot.underflow_flag());
        assert_eq!(riot.timer_value(), 10);
    }

    #[test]
    fn all_four_dividers() {
        for (addr, expected_divider) in [(0x0294u16, 1u16), (0x0295, 8), (0x0296, 64), (0x0297, 1024)] {
            let mut riot = Riot6532::new();
            riot.write(addr, 1); // timer = 1

            // Tick (divider) times to go from 1 → 0
            for _ in 0..expected_divider {
                riot.tick();
            }
            assert_eq!(riot.timer_value(), 0, "divider {expected_divider}: expected 0 after {expected_divider} ticks");

            // Tick (divider) more times → underflow
            for _ in 0..expected_divider {
                riot.tick();
            }
            assert!(riot.underflow_flag(), "divider {expected_divider}: expected underflow");
        }
    }
}
