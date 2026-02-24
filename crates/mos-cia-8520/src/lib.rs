//! MOS 8520 Complex Interface Adapter (CIA).
//!
//! The 8520 is a general-purpose I/O and timer chip used in the Amiga (two
//! instances: CIA-A and CIA-B). It provides two 8-bit I/O ports, two 16-bit
//! countdown timers, a 24-bit time-of-day counter, a serial shift register,
//! and an interrupt controller.

/// MOS 8520 Complex Interface Adapter.
pub struct Cia8520 {
    label: &'static str,
    port_a: u8,
    port_b: u8,
    ddr_a: u8,
    ddr_b: u8,
    pub external_a: u8,
    pub external_b: u8,

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

    icr_status: u8,
    icr_mask: u8,

    cra: u8,
    crb: u8,

    sdr: u8,
    tod_counter: u32,
    tod_alarm: u32,

    // TOD read latch: reading the MSB (reg A) freezes a snapshot.
    // Subsequent reads of regs 9/8 return latched values.
    // Reading reg 8 releases the latch.
    tod_latch: u32,
    tod_latched: bool,

    // Timer read latch: reading low byte latches the corresponding high byte
    // until the high byte register is read.
    timer_a_read_hi_latch: u8,
    timer_a_read_hi_latched: bool,
    timer_b_read_hi_latch: u8,
    timer_b_read_hi_latched: bool,

    // TOD write halt: writing the MSB (reg A) stops the counter.
    // Writing the LSB (reg 8) restarts it. This prevents the counter
    // from advancing during a multi-byte write.
    tod_halted: bool,
}

impl Cia8520 {
    pub fn new(label: &'static str) -> Self {
        Self {
            label,
            port_a: 0xFF,
            port_b: 0xFF,
            ddr_a: 0,
            ddr_b: 0,
            external_a: 0xFF,
            external_b: 0xFF,
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
            sdr: 0,
            tod_counter: 0,
            tod_alarm: 0,
            tod_latch: 0,
            tod_latched: false,
            timer_a_read_hi_latch: 0xFF,
            timer_a_read_hi_latched: false,
            timer_b_read_hi_latch: 0xFF,
            timer_b_read_hi_latched: false,
            tod_halted: false,
        }
    }

    pub fn tick(&mut self) {
        let mut timer_a_underflow = false;

        if self.timer_a_force_load {
            self.timer_a = self.timer_a_latch;
            self.timer_a_force_load = false;
        }

        if self.timer_a_running && (self.cra & 0x20 == 0) {
            if self.timer_a == 0 {
                self.icr_status |= 0x01;
                timer_a_underflow = true;
                self.timer_a = self.timer_a_latch;
                if self.timer_a_oneshot {
                    self.timer_a_running = false;
                    self.cra &= !0x01;
                }
            } else {
                self.timer_a -= 1;
            }
        }

        if self.timer_b_force_load {
            self.timer_b = self.timer_b_latch;
            self.timer_b_force_load = false;
        }

        if self.timer_b_running {
            let timer_b_source = (self.crb >> 5) & 0x03;
            let timer_b_should_count = match timer_b_source {
                0x00 => true,
                0x02 | 0x03 => timer_a_underflow,
                _ => false,
            };

            if timer_b_should_count {
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
        }
    }

    pub fn irq_active(&self) -> bool {
        (self.icr_status & self.icr_mask & 0x1F) != 0
    }

    pub fn read(&mut self, reg: u8) -> u8 {
        match reg & 0x0F {
            0x00 => (self.port_a & self.ddr_a) | (self.external_a & !self.ddr_a),
            0x01 => (self.port_b & self.ddr_b) | (self.external_b & !self.ddr_b),
            0x02 => self.ddr_a,
            0x03 => self.ddr_b,
            0x04 => {
                self.timer_a_read_hi_latch = (self.timer_a >> 8) as u8;
                self.timer_a_read_hi_latched = true;
                self.timer_a as u8
            }
            0x05 => {
                let hi = if self.timer_a_read_hi_latched {
                    self.timer_a_read_hi_latch
                } else {
                    (self.timer_a >> 8) as u8
                };
                self.timer_a_read_hi_latched = false;
                hi
            }
            0x06 => {
                self.timer_b_read_hi_latch = (self.timer_b >> 8) as u8;
                self.timer_b_read_hi_latched = true;
                self.timer_b as u8
            }
            0x07 => {
                let hi = if self.timer_b_read_hi_latched {
                    self.timer_b_read_hi_latch
                } else {
                    (self.timer_b >> 8) as u8
                };
                self.timer_b_read_hi_latched = false;
                hi
            }
            // TOD read with latch: reading MSB freezes snapshot,
            // reading LSB releases latch.
            0x08 => {
                let val = if self.tod_latched {
                    self.tod_latch
                } else {
                    self.tod_counter
                };
                self.tod_latched = false;
                val as u8
            }
            0x09 => {
                let val = if self.tod_latched {
                    self.tod_latch
                } else {
                    self.tod_counter
                };
                (val >> 8) as u8
            }
            0x0A => {
                // Reading MSB latches the full 24-bit value
                if !self.tod_latched {
                    self.tod_latch = self.tod_counter;
                    self.tod_latched = true;
                }
                (self.tod_latch >> 16) as u8
            }
            0x0C => self.sdr,
            0x0D => self.read_icr_and_clear(),
            0x0E => self.cra,
            0x0F => self.crb,
            _ => 0xFF,
        }
    }

    pub fn read_icr_and_clear(&mut self) -> u8 {
        let any = if self.irq_active() { 0x80 } else { 0x00 };
        let result = self.icr_status | any;
        self.icr_status = 0;
        result
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
                    // 8520: In one-shot mode, writing the timer high byte
                    // initiates counting regardless of the start bit.
                    if self.timer_a_oneshot {
                        self.timer_a_running = true;
                        self.cra |= 0x01;
                    }
                }
            }
            0x06 => self.timer_b_latch = (self.timer_b_latch & 0xFF00) | u16::from(value),
            0x07 => {
                self.timer_b_latch = (self.timer_b_latch & 0x00FF) | (u16::from(value) << 8);
                if !self.timer_b_running {
                    self.timer_b = self.timer_b_latch;
                    // 8520: In one-shot mode, writing the timer high byte
                    // initiates counting regardless of the start bit.
                    if self.timer_b_oneshot {
                        self.timer_b_running = true;
                        self.crb |= 0x01;
                    }
                }
            }
            // TOD write with halt: writing MSB stops counter,
            // writing LSB restarts it.
            0x08 => {
                self.write_tod_register(0, value);
                self.tod_halted = false; // writing LSB restarts counter
            }
            0x09 => {
                self.write_tod_register(1, value);
            }
            0x0A => {
                self.write_tod_register(2, value);
                self.tod_halted = true; // writing MSB halts counter
            }
            0x0C => self.sdr = value,
            0x0D => {
                if value & 0x80 != 0 {
                    self.icr_mask |= value & 0x1F;
                } else {
                    self.icr_mask &= !(value & 0x1F);
                }
            }
            0x0E => {
                // LOAD (bit 4) is a strobe and does not read back as a latched bit.
                self.cra = value & !0x10;
                self.timer_a_running = value & 0x01 != 0;
                self.timer_a_oneshot = value & 0x08 != 0;
                if value & 0x10 != 0 {
                    self.timer_a_force_load = true;
                }
            }
            0x0F => {
                // LOAD (bit 4) is a strobe and does not read back as a latched bit.
                self.crb = value & !0x10;
                self.timer_b_running = value & 0x01 != 0;
                self.timer_b_oneshot = value & 0x08 != 0;
                if value & 0x10 != 0 {
                    self.timer_b_force_load = true;
                }
            }
            _ => {}
        }
    }

    /// Pulse the TOD counter. Call this from the system when the
    /// appropriate external signal arrives:
    /// - CIA-A: VSYNC (once per frame, ~50 Hz PAL)
    /// - CIA-B: HSYNC (once per scanline, ~15,625 Hz PAL)
    pub fn tod_pulse(&mut self) {
        if self.tod_halted {
            return;
        }
        self.tod_counter = (self.tod_counter.wrapping_add(1)) & 0xFFFFFF;
        if self.tod_counter == self.tod_alarm {
            self.icr_status |= 0x04;
        }
    }

    fn write_tod_register(&mut self, byte_index: u8, value: u8) {
        let shift = u32::from(byte_index) * 8;
        let mask = !(0xFFu32 << shift);
        if self.crb & 0x80 != 0 {
            self.tod_alarm = (self.tod_alarm & mask) | (u32::from(value) << shift);
            self.tod_alarm &= 0xFFFFFF;
        } else {
            self.tod_counter = (self.tod_counter & mask) | (u32::from(value) << shift);
            self.tod_counter &= 0xFFFFFF;
        }
    }

    pub fn tod_counter(&self) -> u32 {
        self.tod_counter
    }
    pub fn tod_alarm(&self) -> u32 {
        self.tod_alarm
    }
    pub fn tod_halted(&self) -> bool {
        self.tod_halted
    }

    /// Directly set the TOD counter. Used to simulate battclock.resource
    /// writing the RTC time after timer.device init clears the counter.
    pub fn set_tod_counter(&mut self, value: u32) {
        self.tod_counter = value & 0xFFFFFF;
    }

    // Diagnostic accessors for test instrumentation
    pub fn timer_a(&self) -> u16 {
        self.timer_a
    }
    pub fn timer_b(&self) -> u16 {
        self.timer_b
    }
    pub fn timer_a_running(&self) -> bool {
        self.timer_a_running
    }
    pub fn timer_b_running(&self) -> bool {
        self.timer_b_running
    }
    pub fn icr_status(&self) -> u8 {
        self.icr_status
    }
    pub fn icr_mask(&self) -> u8 {
        self.icr_mask
    }

    pub fn port_a_output(&self) -> u8 {
        (self.port_a & self.ddr_a) | (self.external_a & !self.ddr_a)
    }

    pub fn port_b_output(&self) -> u8 {
        (self.port_b & self.ddr_b) | (self.external_b & !self.ddr_b)
    }

    /// Inject a complete serial byte (keyboard clocked 8 bits via CNT).
    /// Sets ICR bit 3 (SP) and stores byte in SDR.
    pub fn receive_serial_byte(&mut self, byte: u8) {
        self.sdr = byte;
        self.icr_status |= 0x08;
    }

    /// Hardware reset: clears all registers to power-on state.
    /// Called when the 68000 RESET instruction asserts the reset line.
    pub fn reset(&mut self) {
        self.port_a = 0xFF;
        self.port_b = 0xFF;
        self.ddr_a = 0;
        self.ddr_b = 0;
        self.timer_a = 0xFFFF;
        self.timer_a_latch = 0xFFFF;
        self.timer_a_running = false;
        self.timer_a_oneshot = false;
        self.timer_a_force_load = false;
        self.timer_b = 0xFFFF;
        self.timer_b_latch = 0xFFFF;
        self.timer_b_running = false;
        self.timer_b_oneshot = false;
        self.timer_b_force_load = false;
        self.icr_status = 0;
        self.icr_mask = 0;
        self.cra = 0;
        self.crb = 0;
        self.sdr = 0;
        self.tod_latched = false;
        self.timer_a_read_hi_latched = false;
        self.timer_b_read_hi_latched = false;
        self.tod_halted = false;
        // TOD counter/alarm are not reset by hardware reset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_low_read_latches_high_until_high_read() {
        let mut cia = Cia8520::new("T");
        cia.timer_a = 0x1234;
        cia.timer_a_running = true;
        cia.cra = 0x01;

        assert_eq!(cia.read(0x04), 0x34);
        cia.tick();
        assert_eq!(cia.timer_a, 0x1233);

        // High byte returns the value latched by the earlier low-byte read.
        assert_eq!(cia.read(0x05), 0x12);

        // After the latch is consumed, reads return the live high byte.
        cia.timer_a = 0xABCD;
        assert_eq!(cia.read(0x05), 0xAB);
    }

    #[test]
    fn timer_b_low_read_latches_high_until_high_read() {
        let mut cia = Cia8520::new("T");
        cia.timer_b = 0x5678;
        cia.timer_b_running = true;
        cia.crb = 0x01;

        assert_eq!(cia.read(0x06), 0x78);
        cia.tick();
        assert_eq!(cia.timer_b, 0x5677);
        assert_eq!(cia.read(0x07), 0x56);
    }

    #[test]
    fn cra_crb_load_bit_is_strobe_and_reads_back_clear() {
        let mut cia = Cia8520::new("T");

        cia.write(0x04, 0x34);
        cia.write(0x05, 0x12);
        cia.write(0x0E, 0x10); // LOAD strobe only
        assert_eq!(cia.read(0x0E) & 0x10, 0);
        assert!(cia.timer_a_force_load);
        cia.tick();
        assert_eq!(cia.timer_a, 0x1234);
        assert!(!cia.timer_a_force_load);

        cia.write(0x06, 0x78);
        cia.write(0x07, 0x56);
        cia.write(0x0F, 0x10); // LOAD strobe only
        assert_eq!(cia.read(0x0F) & 0x10, 0);
        assert!(cia.timer_b_force_load);
        cia.tick();
        assert_eq!(cia.timer_b, 0x5678);
        assert!(!cia.timer_b_force_load);
    }

    #[test]
    fn timer_a_oneshot_high_byte_write_autostarts_and_stops_on_underflow() {
        let mut cia = Cia8520::new("T");

        // One-shot selected, start bit clear.
        cia.write(0x0E, 0x08);
        assert!(!cia.timer_a_running());

        cia.write(0x04, 0x02);
        cia.write(0x05, 0x00);

        // 8520 one-shot auto-starts on timer high-byte write.
        assert!(cia.timer_a_running());
        assert_ne!(cia.read(0x0E) & 0x01, 0);
        assert_eq!(cia.timer_a(), 0x0002);

        cia.tick(); // 2 -> 1
        cia.tick(); // 1 -> 0
        cia.tick(); // underflow, reload, stop (one-shot)

        assert_eq!(cia.timer_a(), 0x0002);
        assert!(!cia.timer_a_running());
        assert_eq!(cia.read(0x0E) & 0x01, 0);
        assert_ne!(cia.icr_status() & 0x01, 0);
    }

    #[test]
    fn timer_b_chained_mode_counts_only_timer_a_underflows() {
        let mut cia = Cia8520::new("T");

        // Timer A: free-run, underflow every 2 ticks from initial value 1.
        cia.timer_a = 0x0001;
        cia.timer_a_latch = 0x0001;
        cia.timer_a_running = true;
        cia.cra = 0x01;

        // Timer B: count Timer A underflows (CRB bits 6:5 = 10b), start.
        cia.timer_b = 0x0002;
        cia.timer_b_latch = 0x0002;
        cia.timer_b_running = true;
        cia.crb = 0x41;

        cia.tick(); // TA: 1 -> 0, no underflow yet
        assert_eq!(cia.timer_b(), 0x0002);

        cia.tick(); // TA underflow -> TB: 2 -> 1
        assert_eq!(cia.timer_b(), 0x0001);

        cia.tick(); // TA: 1 -> 0, no TB count
        assert_eq!(cia.timer_b(), 0x0001);

        cia.tick(); // TA underflow -> TB: 1 -> 0
        assert_eq!(cia.timer_b(), 0x0000);

        cia.tick(); // TA: 1 -> 0, still no TB count
        assert_eq!(cia.timer_b(), 0x0000);

        cia.tick(); // TA underflow -> TB underflow/reload
        assert_eq!(cia.timer_b(), 0x0002);
        assert_ne!(cia.icr_status() & 0x03, 0);
    }

    #[test]
    fn icr_read_sets_master_bit_only_when_masked_and_clears_status() {
        let mut cia = Cia8520::new("T");

        cia.receive_serial_byte(0xA5); // ICR bit 3 (SP)
        assert_eq!(cia.icr_status() & 0x08, 0x08);
        assert!(!cia.irq_active());

        // Status bit visible without master bit when masked off.
        let masked_off = cia.read_icr_and_clear();
        assert_eq!(masked_off & 0x08, 0x08);
        assert_eq!(masked_off & 0x80, 0x00);
        assert_eq!(cia.icr_status(), 0);

        // Enable SP mask, trigger again, then read with master bit set.
        cia.write(0x0D, 0x88); // set mask bit 3
        cia.receive_serial_byte(0x5A);
        assert!(cia.irq_active());

        let masked_on = cia.read_icr_and_clear();
        assert_eq!(masked_on & 0x08, 0x08);
        assert_eq!(masked_on & 0x80, 0x80);
        assert_eq!(cia.icr_status(), 0);
        assert!(!cia.irq_active());
    }
}
