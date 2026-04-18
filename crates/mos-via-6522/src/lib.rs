#![allow(clippy::cast_possible_truncation)]

use serde::{Deserialize, Serialize};

const IRQ_CA2: u8 = 0x01;
const IRQ_CA1: u8 = 0x02;
const IRQ_SR: u8 = 0x04;
const IRQ_CB2: u8 = 0x08;
const IRQ_CB1: u8 = 0x10;
const IRQ_T2: u8 = 0x20;
const IRQ_T1: u8 = 0x40;

/// Shift Register operating mode (ACR bits 4:2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SrMode {
    /// 000: SR disabled, PCR controls CB1/CB2.
    Disabled,
    /// 001: Shift in at T2 rate (CB1 = output clock, CB2 = input data).
    ShiftInT2,
    /// 010: Shift in at Φ2 rate.
    ShiftInPhi2,
    /// 011: Shift in under external clock on CB1 (CB1 = input).
    ShiftInExt,
    /// 100: Free-running shift out at T2 rate — the 8-bit pattern
    /// repeats indefinitely on CB2, no SR-done interrupt.
    ShiftOutFree,
    /// 101: Shift out at T2 rate.
    ShiftOutT2,
    /// 110: Shift out at Φ2 rate.
    ShiftOutPhi2,
    /// 111: Shift out under external clock on CB1 (CB1 = input).
    ShiftOutExt,
}

impl SrMode {
    fn from_acr(acr: u8) -> Self {
        match (acr >> 2) & 0x07 {
            0b000 => Self::Disabled,
            0b001 => Self::ShiftInT2,
            0b010 => Self::ShiftInPhi2,
            0b011 => Self::ShiftInExt,
            0b100 => Self::ShiftOutFree,
            0b101 => Self::ShiftOutT2,
            0b110 => Self::ShiftOutPhi2,
            0b111 => Self::ShiftOutExt,
            _ => unreachable!(),
        }
    }

    fn is_output(self) -> bool {
        matches!(
            self,
            Self::ShiftOutFree
                | Self::ShiftOutT2
                | Self::ShiftOutPhi2
                | Self::ShiftOutExt
        )
    }

    fn is_external_clock(self) -> bool {
        matches!(self, Self::ShiftInExt | Self::ShiftOutExt)
    }

    fn is_phi2_clock(self) -> bool {
        matches!(self, Self::ShiftInPhi2 | Self::ShiftOutPhi2)
    }

    fn is_t2_clock(self) -> bool {
        matches!(
            self,
            Self::ShiftInT2 | Self::ShiftOutFree | Self::ShiftOutT2
        )
    }
}

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
    t1_irq_armed: bool,
    t1_reload_pending: bool,
    t1_pb7_output: bool,
    t2_counter: u16,
    t2_latch_low: u8,
    t2_running: bool,
    shift_register: u8,
    /// Count of bits shifted in the current 8-pulse burst. Reaches 8,
    /// then IFR_SR fires and shifting stops (except in free-run mode).
    sr_shift_count: u8,
    /// Whether a shift burst is currently active. Triggered by reading
    /// or writing the shift register. Stays true indefinitely in
    /// free-run mode.
    sr_active: bool,
    /// Down-counter for T2-rate shift pulses. Loaded with T2L-L + 2
    /// each pulse; when it hits 0, one bit is shifted.
    sr_t2_timer: u8,
    /// Down-counter for Φ2-rate shift pulses. Every 2 Φ2 ticks produces
    /// one shift (CB1 output clock flips at Φ2 rate).
    sr_phi2_timer: u8,
    /// Current state of the CB1 shift-clock line when driven as output
    /// (true = high). Used for toggling during shift operations.
    sr_cb1_clock: bool,
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
            t1_irq_armed: false,
            t1_reload_pending: false,
            t1_pb7_output: false,
            t2_counter: 0,
            t2_latch_low: 0,
            t2_running: false,
            shift_register: 0,
            sr_shift_count: 0,
            sr_active: false,
            sr_t2_timer: 0,
            sr_phi2_timer: 0,
            sr_cb1_clock: true,
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
            if self.t1_reload_pending {
                self.t1_counter = self.t1_latch;
                self.t1_reload_pending = false;
            } else if self.t1_counter == 0 {
                if self.t1_irq_armed {
                    self.raise_interrupt(IRQ_T1);
                    if self.t1_pb7_enabled() {
                        self.t1_pb7_output = !self.t1_pb7_output;
                    }
                    if !self.t1_free_run() {
                        self.t1_irq_armed = false;
                    }
                }
                self.t1_counter = u16::MAX;
                self.t1_reload_pending = true;
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

        self.tick_shift_register();

        self.update_pins();
    }

    /// Drive one Φ2 tick of the shift register. T2-rate modes count
    /// down through sr_t2_timer; Φ2-rate modes shift on alternate Φ2
    /// ticks (CB1 clock toggles at full Φ2 speed). External-clock
    /// modes are driven by edges on CB1 via `set_cb1_level`.
    fn tick_shift_register(&mut self) {
        let mode = SrMode::from_acr(self.acr);
        if mode == SrMode::Disabled {
            return;
        }
        if mode.is_external_clock() {
            // External clocking is edge-driven via CB1 input; nothing
            // to do on a Φ2 tick.
            return;
        }
        if !self.sr_active && mode != SrMode::ShiftOutFree {
            return;
        }

        if mode.is_phi2_clock() {
            // Toggle CB1 clock every Φ2 tick; shift on the trailing edge
            // (clock going from high to low).
            let prev = self.sr_cb1_clock;
            self.sr_cb1_clock = !prev;
            if prev && !self.sr_cb1_clock {
                self.advance_shift(mode);
            }
        } else if mode.is_t2_clock() {
            // T2-rate clock: use T2L-L + 2 as the period, toggle CB1
            // clock on each timer expiry, shift on the trailing edge.
            if self.sr_t2_timer == 0 {
                self.sr_t2_timer = self.t2_latch_low.saturating_add(2);
                let prev = self.sr_cb1_clock;
                self.sr_cb1_clock = !prev;
                if prev && !self.sr_cb1_clock {
                    self.advance_shift(mode);
                }
            } else {
                self.sr_t2_timer -= 1;
            }
        }
    }

    /// Handle one shift pulse (trailing edge of CB1 shift clock).
    fn advance_shift(&mut self, mode: SrMode) {
        if mode.is_output() {
            // Rotate left; MSB recirculates into bit 0 and also appears
            // on CB2 output.
            let msb = (self.shift_register >> 7) & 1;
            self.shift_register = (self.shift_register << 1) | msb;
            self.cb2_out = msb != 0;
            self.cb2_drive = true;
        } else {
            // Shift in: new bit arrives on CB2 input, shifted into bit 0.
            let bit = u8::from(self.cb2);
            self.shift_register = (self.shift_register << 1) | bit;
        }

        if mode != SrMode::ShiftOutFree {
            self.sr_shift_count = self.sr_shift_count.wrapping_add(1);
            if self.sr_shift_count >= 8 {
                self.raise_interrupt(IRQ_SR);
                self.sr_active = false;
                self.sr_shift_count = 0;
            }
        }
    }

    /// Called when software reads or writes the SR register. Resets
    /// the 8-pulse counter, clears IFR_SR, and starts (or restarts)
    /// a shift burst — except in disabled or free-run modes.
    fn trigger_sr_access(&mut self) {
        let mode = SrMode::from_acr(self.acr);
        if mode == SrMode::Disabled {
            return;
        }
        self.sr_shift_count = 0;
        self.clear_interrupts(IRQ_SR);
        if mode != SrMode::ShiftOutFree {
            self.sr_active = true;
        }
    }

    /// Applies one external CB1 level change immediately. Mirrors
    /// `set_ca1_level`: fires IRQ_CB1 on the configured active edge,
    /// latches IRB if PB latching is enabled, and also drives the SR
    /// shift in ShiftInExt / ShiftOutExt modes (shift on the trailing
    /// CB1→low edge).
    pub fn set_cb1_level(&mut self, level_high: bool) {
        let prev = self.cb1;
        self.cb1 = level_high;

        if self.edge_matches(prev, self.cb1, self.cb1_active_high()) {
            self.raise_interrupt(IRQ_CB1);
            if self.pb_latch_enabled() {
                self.irb = self.pb_in;
            }
            if self.cb2_is_output() && self.cb2_output_mode() == 0x00 {
                self.cb2_handshake_high = false;
            }
        }

        let mode = SrMode::from_acr(self.acr);
        if mode.is_external_clock() && self.sr_active && prev && !level_high {
            self.advance_shift(mode);
        }

        self.prev_cb1 = self.cb1;
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
            0x0A => self.trigger_sr_access(),
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

    pub fn read_port_a_with_value(&mut self, value: u8) -> u8 {
        self.clear_port_a_interrupts();
        self.update_pins();
        value
    }

    /// Applies one external CA1 level change immediately.
    ///
    /// This models board wiring that can present a CA1 edge between full VIA
    /// `phi2` ticks, such as the IEC ATN transition on the 1541 serial VIA.
    pub fn set_ca1_level(&mut self, level_high: bool) {
        self.ca1 = level_high;
        if self.edge_matches(self.prev_ca1, self.ca1, self.ca1_active_high()) {
            self.raise_interrupt(IRQ_CA1);
            if self.pa_latch_enabled() {
                self.ira = self.pa_in;
            }
            if self.ca2_is_output() && self.ca2_output_mode() == 0x00 {
                self.ca2_handshake_high = false;
            }
        }
        self.prev_ca1 = self.ca1;
        self.update_pins();
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
            // Per MOS 6522 preliminary datasheet: "Bit 7 will read as a
            // logic 0" — IER read returns only the enable bits.
            0x0E => self.ier & 0x7F,
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
            0x01 => {
                // ORA with handshake: clears CA1/CA2 interrupts and
                // pulses CA2 if configured.
                self.ora = value;
                self.clear_port_a_interrupts();
                if self.ca2_is_output() && self.ca2_output_mode() == 0x01 {
                    self.ca2_pulse_low = true;
                }
            }
            0x0F => {
                // ORA-alt (no-handshake): writes the register but does
                // NOT clear CA1/CA2 IFR or trigger the CA2 pulse.
                self.ora = value;
            }
            0x02 => self.ddrb = value,
            0x03 => self.ddra = value,
            0x04 => self.t1_latch = (self.t1_latch & 0xFF00) | u16::from(value),
            0x05 => {
                self.t1_latch = (self.t1_latch & 0x00FF) | (u16::from(value) << 8);
                self.t1_counter = self.t1_latch;
                self.t1_running = true;
                self.t1_irq_armed = true;
                self.t1_reload_pending = false;
                self.t1_pb7_output = false;
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
            0x0A => {
                self.shift_register = value;
                self.trigger_sr_access();
            }
            0x0B => {
                let prev_acr = self.acr;
                self.acr = value;
                // Per reference note 6: mode 000 forces SR IFR clear.
                if SrMode::from_acr(self.acr) == SrMode::Disabled
                    && SrMode::from_acr(prev_acr) != SrMode::Disabled
                {
                    self.clear_interrupts(IRQ_SR);
                    self.sr_active = false;
                }
            }
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

    #[must_use]
    pub fn compose_port_a_read(&self, input: u8) -> u8 {
        (self.ora & self.ddra) | (input & !self.ddra)
    }

    #[must_use]
    pub fn compose_port_b_read(&self, input: u8) -> u8 {
        (self.port_b_output() & self.ddrb) | (input & !self.ddrb)
    }

    #[must_use]
    pub fn port_a_drive_state(&self) -> u8 {
        (self.ora & self.ddra) | !self.ddra
    }

    #[must_use]
    pub fn port_b_drive_state(&self) -> u8 {
        (self.port_b_output() & self.ddrb) | !self.ddrb
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

        // When SR is enabled (ACR4:2 ≠ 000), CB1 acts as shift clock
        // and CB2 as shift data; PCR CB1/CB2 settings are overridden.
        let sr_mode = SrMode::from_acr(self.acr);
        if sr_mode != SrMode::Disabled {
            if sr_mode.is_external_clock() {
                // CB1 is input in external-clock modes.
                self.cb1_drive = false;
                self.cb1_out = true;
            } else {
                // CB1 is driven by the internal shift clock.
                self.cb1_drive = true;
                self.cb1_out = self.sr_cb1_clock;
            }
            if sr_mode.is_output() {
                // CB2 reflects the latest shifted MSB (cb2_out set in
                // advance_shift). Keep cb2_drive=true.
                self.cb2_drive = true;
            } else {
                // Shift-in modes: CB2 is input.
                self.cb2_drive = false;
                self.cb2_out = true;
            }
        } else {
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
        }

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
    use super::{IRQ_CA1, IRQ_CB2, IRQ_SR, IRQ_T1, IRQ_T2, Via6522};

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
        assert_eq!(via.peek(0x04), 0xFF);

        via.tick();

        assert!(via.t1_running);
        assert_eq!(via.peek(0x04), 0x02);
    }

    #[test]
    fn timer1_one_shot_keeps_visible_counter_advancing_after_irq() {
        let mut via = Via6522::new();
        via.write(0x0E, 0x80 | IRQ_T1);
        via.write(0x0B, 0x00);
        via.write(0x04, 0x01);
        via.write(0x05, 0x00);

        via.tick();
        via.tick();

        assert!(via.irq);
        assert_eq!(via.peek(0x0D) & IRQ_T1, IRQ_T1);
        assert!(via.t1_running);
        assert_eq!(via.peek(0x04), 0xFF);
        assert!(!via.t1_irq_armed);

        via.tick();
        via.tick();

        assert_eq!(via.peek(0x0D) & IRQ_T1, IRQ_T1);
        assert!(via.t1_running);
        assert_eq!(via.peek(0x04), 0x00);
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
    fn set_ca1_level_raises_interrupt_without_tick() {
        let mut via = Via6522::new();
        via.write(0x0E, 0x80 | IRQ_CA1);
        via.write(0x0C, 0x01);

        via.set_ca1_level(false);
        assert_eq!(via.peek(0x0D) & IRQ_CA1, 0);

        via.set_ca1_level(true);
        assert_eq!(via.peek(0x0D) & IRQ_CA1, IRQ_CA1);
        assert!(via.irq);
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
    fn reading_ifr_does_not_clear_pending_flags() {
        let mut via = Via6522::new();
        via.write(0x0E, 0x80 | IRQ_CA1);
        via.ca1 = false;
        via.tick();

        assert_eq!(via.peek(0x0D) & IRQ_CA1, IRQ_CA1);
        assert_eq!(via.read(0x0D) & IRQ_CA1, IRQ_CA1);
        assert_eq!(via.peek(0x0D) & IRQ_CA1, IRQ_CA1);
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

    // ─── Shift register mode tests ────────────────────────────────────

    #[test]
    fn shift_out_phi2_rate_emits_msb_first_over_16_ticks() {
        // Mode 110 (shift out at Φ2 rate). CB1 toggles every Φ2, shift
        // fires on trailing edge, so one bit per 2 Φ2 ticks → 16 ticks
        // for a full byte.
        let mut via = Via6522::new();
        via.write(0x0B, 0b000_110_00); // ACR SR = 110, rest zero
        via.write(0x0A, 0b1010_0101); // $A5 — pattern to verify MSB-first
        // Collect CB2 on each trailing-edge shift.
        let mut observed: Vec<bool> = Vec::new();
        let mut prev_clock = via.sr_cb1_clock;
        for _ in 0..20 {
            via.tick();
            if prev_clock && !via.sr_cb1_clock {
                observed.push(via.cb2_out);
            }
            prev_clock = via.sr_cb1_clock;
        }
        // Expect 8 shifts, MSB first: 1 0 1 0 0 1 0 1
        assert_eq!(observed.len(), 8);
        assert_eq!(observed, vec![true, false, true, false, false, true, false, true]);
        // IFR_SR should have fired after the 8th shift.
        assert_ne!(via.ifr & IRQ_SR, 0, "SR IFR should fire after 8 shifts");
    }

    #[test]
    fn sr_disabled_mode_clears_ifr_and_does_not_shift() {
        let mut via = Via6522::new();
        via.write(0x0B, 0b000_110_00); // start in shift-out Φ2 mode
        via.write(0x0A, 0xFF);
        // Advance a few ticks, forcing the first shift to raise IFR.
        for _ in 0..20 {
            via.tick();
        }
        assert_ne!(via.ifr & IRQ_SR, 0);

        // Disable: ACR SR = 000. Note 6 says IFR_SR must clear.
        via.write(0x0B, 0b000_000_00);
        assert_eq!(via.ifr & IRQ_SR, 0);

        // Writing/reading SR in disabled mode must not start shifting.
        via.write(0x0A, 0xAA);
        for _ in 0..20 {
            via.tick();
        }
        assert_eq!(via.ifr & IRQ_SR, 0);
        assert!(!via.sr_active);
    }

    #[test]
    fn sr_external_clock_shift_in_on_cb1_falling_edge() {
        // Mode 011 — shift in on external CB1 falling edges, data on CB2.
        // Drive 8 bits of 1,0,1,0,1,0,1,0 in order; after 8 shifts the
        // shift register contains those bits with the first-arrived bit
        // in the MSB.
        let mut via = Via6522::new();
        via.write(0x0B, 0b000_011_00);
        via.write(0x0A, 0x00); // start fresh; triggers sr_active

        let data_bits = [true, false, true, false, true, false, true, false];
        for bit in data_bits {
            via.cb2 = bit;
            via.set_cb1_level(true); // rising
            via.set_cb1_level(false); // falling — shifts one bit
        }
        assert_eq!(via.shift_register, 0b1010_1010);
        assert_ne!(via.ifr & IRQ_SR, 0);
    }
}
