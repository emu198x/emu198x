//! CIA 8520 Complex Interface Adapter.

pub struct Cia {
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
    tod_divider: u16,
}

impl Cia {
    pub const TOD_DIVISOR: u16 = 14_188;

    pub fn new() -> Self {
        Self {
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
            tod_divider: 0,
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

        self.tick_tod();
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
            0x04 => self.timer_a as u8,
            0x05 => (self.timer_a >> 8) as u8,
            0x06 => self.timer_b as u8,
            0x07 => (self.timer_b >> 8) as u8,
            0x08 => self.tod_counter as u8,
            0x09 => (self.tod_counter >> 8) as u8,
            0x0A => (self.tod_counter >> 16) as u8,
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
                if !self.timer_a_running { self.timer_a = self.timer_a_latch; }
            }
            0x06 => self.timer_b_latch = (self.timer_b_latch & 0xFF00) | u16::from(value),
            0x07 => {
                self.timer_b_latch = (self.timer_b_latch & 0x00FF) | (u16::from(value) << 8);
                if !self.timer_b_running { self.timer_b = self.timer_b_latch; }
            }
            0x08 => self.write_tod_register(0, value),
            0x09 => self.write_tod_register(1, value),
            0x0A => self.write_tod_register(2, value),
            0x0C => self.sdr = value,
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
                if value & 0x10 != 0 { self.timer_a_force_load = true; }
            }
            0x0F => {
                self.crb = value;
                self.timer_b_running = value & 0x01 != 0;
                self.timer_b_oneshot = value & 0x08 != 0;
                if value & 0x10 != 0 { self.timer_b_force_load = true; }
            }
            _ => {}
        }
    }

    fn tick_tod(&mut self) {
        self.tod_divider = self.tod_divider.wrapping_add(1);
        if self.tod_divider < Self::TOD_DIVISOR { return; }
        self.tod_divider = 0;
        self.tod_counter = (self.tod_counter.wrapping_add(1)) & 0xFFFFFF;
        if self.tod_counter == self.tod_alarm { self.icr_status |= 0x04; }
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

    pub fn port_a_output(&self) -> u8 {
        (self.port_a & self.ddr_a) | (self.external_a & !self.ddr_a)
    }
}
