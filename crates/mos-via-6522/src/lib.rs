//! MOS 6522 Versatile Interface Adapter (VIA).
//!
//! The 6522 provides two 8-bit I/O ports, two 16-bit timers, a serial
//! shift register, and an interrupt controller. The 1541 floppy drive
//! uses two VIAs: VIA1 for the IEC serial bus interface and VIA2 for
//! the disk controller.
//!
//! # Registers ($0-$F)
//!
//! | Reg | Name | Description                         |
//! |-----|------|-------------------------------------|
//! | $0  | ORB  | Port B data (handshake on read)     |
//! | $1  | ORA  | Port A data (handshake on read)     |
//! | $2  | DDRB | Port B data direction (1 = output)  |
//! | $3  | DDRA | Port A data direction (1 = output)  |
//! | $4  | T1CL | Timer 1 counter low (read clears T1 IRQ) |
//! | $5  | T1CH | Timer 1 counter high (write starts T1) |
//! | $6  | T1LL | Timer 1 latch low                   |
//! | $7  | T1LH | Timer 1 latch high                  |
//! | $8  | T2CL | Timer 2 counter low (read clears T2 IRQ) |
//! | $9  | T2CH | Timer 2 counter high (write starts T2) |
//! | $A  | SR   | Shift register                      |
//! | $B  | ACR  | Auxiliary control register           |
//! | $C  | PCR  | Peripheral control register          |
//! | $D  | IFR  | Interrupt flag register              |
//! | $E  | IER  | Interrupt enable register            |
//! | $F  | ORA  | Port A data (no handshake)           |

#![allow(clippy::cast_possible_truncation)]

/// MOS 6522 Versatile Interface Adapter.
pub struct Via6522 {
    /// Port A output register.
    port_a: u8,
    /// Port B output register.
    port_b: u8,
    /// Port A data direction register (1 = output).
    ddr_a: u8,
    /// Port B data direction register (1 = output).
    ddr_b: u8,
    /// External input lines for port A (active-high, directly readable).
    pub external_a: u8,
    /// External input lines for port B (active-high, directly readable).
    pub external_b: u8,

    /// Timer 1 counter (16-bit, counts down).
    timer1_counter: u16,
    /// Timer 1 latch (16-bit, reloaded into counter on underflow).
    timer1_latch: u16,
    /// Timer 1 has underflowed (one-shot: stops after first; free-run: repeats).
    timer1_fired: bool,
    /// Timer 1 is active (counting). In one-shot mode, clears after first underflow.
    timer1_running: bool,

    /// Timer 2 counter (16-bit, counts down).
    timer2_counter: u16,
    /// Timer 2 latch low byte (only low byte is latched).
    timer2_latch_lo: u8,
    /// Timer 2 has underflowed.
    timer2_fired: bool,
    /// Timer 2 is active.
    timer2_running: bool,

    /// Shift register.
    shift_register: u8,
    /// Shift count (number of bits shifted).
    shift_count: u8,

    /// Auxiliary control register (ACR).
    /// Bits 7-6: T1 control (00/01 = one-shot, 10/11 = free-run)
    /// Bit 5: T2 control (0 = timed, 1 = count PB6 pulses)
    /// Bits 4-2: Shift register control
    /// Bit 1: PB latching enable
    /// Bit 0: PA latching enable
    acr: u8,

    /// Peripheral control register (PCR).
    /// Bits 7-5: CB2 control
    /// Bit 4: CB1 edge (0 = negative, 1 = positive)
    /// Bits 3-1: CA2 control
    /// Bit 0: CA1 edge (0 = negative, 1 = positive)
    pcr: u8,

    /// Interrupt flag register (IFR).
    /// Bit 7: any enabled interrupt active (read-only)
    /// Bit 6: Timer 1
    /// Bit 5: Timer 2
    /// Bit 4: CB1
    /// Bit 3: CB2
    /// Bit 2: Shift register
    /// Bit 1: CA1
    /// Bit 0: CA2
    ifr: u8,

    /// Interrupt enable register (IER).
    /// Same bit layout as IFR (bit 7 = set/clear control on write).
    ier: u8,

    /// Previous CA1 input state (for edge detection).
    ca1_prev: bool,
    /// Previous CB1 input state (for edge detection).
    cb1_prev: bool,

    /// PB7 output toggle (toggled by T1 in free-run + PB7 mode).
    pb7_output: bool,
}

impl Via6522 {
    /// Create a new VIA with all registers in their reset state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            port_a: 0,
            port_b: 0,
            ddr_a: 0,
            ddr_b: 0,
            external_a: 0xFF,
            external_b: 0xFF,
            timer1_counter: 0xFFFF,
            timer1_latch: 0xFFFF,
            timer1_fired: false,
            timer1_running: false,
            timer2_counter: 0xFFFF,
            timer2_latch_lo: 0xFF,
            timer2_fired: false,
            timer2_running: false,
            shift_register: 0,
            shift_count: 0,
            acr: 0,
            pcr: 0,
            ifr: 0,
            ier: 0,
            ca1_prev: false,
            cb1_prev: false,
            pb7_output: false,
        }
    }

    /// Tick the VIA for one clock cycle.
    ///
    /// Counts down timers and sets interrupt flags on underflow.
    pub fn tick(&mut self) {
        self.tick_timer1();
        self.tick_timer2();
    }

    /// Check if the VIA has an active (and enabled) interrupt.
    #[must_use]
    pub fn irq_active(&self) -> bool {
        (self.ifr & self.ier & 0x7F) != 0
    }

    /// Read a VIA register.
    pub fn read(&mut self, reg: u8) -> u8 {
        match reg & 0x0F {
            0x00 => {
                // ORB: Port B data (with handshake — clears CB1/CB2 flags)
                self.ifr &= !(IFR_CB1 | IFR_CB2);
                self.read_port_b()
            }
            0x01 => {
                // ORA: Port A data (with handshake — clears CA1/CA2 flags)
                self.ifr &= !(IFR_CA1 | IFR_CA2);
                self.read_port_a()
            }
            0x02 => self.ddr_b,
            0x03 => self.ddr_a,
            0x04 => {
                // T1C-L: read low byte AND clear T1 interrupt flag
                self.ifr &= !IFR_T1;
                self.timer1_counter as u8
            }
            0x05 => {
                // T1C-H: read high byte
                (self.timer1_counter >> 8) as u8
            }
            0x06 => {
                // T1L-L: read latch low byte
                self.timer1_latch as u8
            }
            0x07 => {
                // T1L-H: read latch high byte
                (self.timer1_latch >> 8) as u8
            }
            0x08 => {
                // T2C-L: read low byte AND clear T2 interrupt flag
                self.ifr &= !IFR_T2;
                self.timer2_counter as u8
            }
            0x09 => {
                // T2C-H: read high byte
                (self.timer2_counter >> 8) as u8
            }
            0x0A => self.shift_register,
            0x0B => self.acr,
            0x0C => self.pcr,
            0x0D => {
                // IFR: bit 7 reflects whether any enabled interrupt is active
                let irq_any = if (self.ifr & self.ier & 0x7F) != 0 {
                    0x80
                } else {
                    0
                };
                (self.ifr & 0x7F) | irq_any
            }
            0x0E => {
                // IER: bit 7 always reads as 1
                self.ier | 0x80
            }
            0x0F => {
                // ORA no-handshake: read port A without clearing CA1/CA2 flags
                self.read_port_a()
            }
            _ => 0xFF,
        }
    }

    /// Write a VIA register.
    pub fn write(&mut self, reg: u8, value: u8) {
        match reg & 0x0F {
            0x00 => {
                // ORB: Port B data (with handshake — clears CB1/CB2 flags)
                self.ifr &= !(IFR_CB1 | IFR_CB2);
                self.port_b = value;
            }
            0x01 => {
                // ORA: Port A data (with handshake — clears CA1/CA2 flags)
                self.ifr &= !(IFR_CA1 | IFR_CA2);
                self.port_a = value;
            }
            0x02 => self.ddr_b = value,
            0x03 => self.ddr_a = value,
            0x04 => {
                // T1L-L: write latch low byte (also stored in latch)
                self.timer1_latch = (self.timer1_latch & 0xFF00) | u16::from(value);
            }
            0x05 => {
                // T1C-H: write latch high byte, load counter from latch,
                // start timer, clear T1 interrupt flag.
                self.timer1_latch = (self.timer1_latch & 0x00FF) | (u16::from(value) << 8);
                self.timer1_counter = self.timer1_latch;
                self.timer1_running = true;
                self.timer1_fired = false;
                self.ifr &= !IFR_T1;
                // Reset PB7 toggle output
                self.pb7_output = false;
            }
            0x06 => {
                // T1L-L: write latch low byte only
                self.timer1_latch = (self.timer1_latch & 0xFF00) | u16::from(value);
            }
            0x07 => {
                // T1L-H: write latch high byte only, clear T1 interrupt flag
                self.timer1_latch = (self.timer1_latch & 0x00FF) | (u16::from(value) << 8);
                self.ifr &= !IFR_T1;
            }
            0x08 => {
                // T2L-L: write latch low byte
                self.timer2_latch_lo = value;
            }
            0x09 => {
                // T2C-H: load counter (high from value, low from latch),
                // start timer, clear T2 interrupt flag.
                self.timer2_counter =
                    u16::from(self.timer2_latch_lo) | (u16::from(value) << 8);
                self.timer2_running = true;
                self.timer2_fired = false;
                self.ifr &= !IFR_T2;
            }
            0x0A => {
                self.shift_register = value;
                self.shift_count = 0;
                self.ifr &= !IFR_SR;
            }
            0x0B => self.acr = value,
            0x0C => self.pcr = value,
            0x0D => {
                // IFR: writing 1s clears the corresponding flags
                self.ifr &= !value;
            }
            0x0E => {
                // IER: bit 7 selects set (1) or clear (0) mode
                if value & 0x80 != 0 {
                    self.ier |= value & 0x7F;
                } else {
                    self.ier &= !(value & 0x7F);
                }
            }
            0x0F => {
                // ORA no-handshake: write port A without clearing CA1/CA2
                self.port_a = value;
            }
            _ => {}
        }
    }

    /// Set the CA1 input line. Call this when the external signal changes.
    ///
    /// Edge detection: triggers on the configured edge (PCR bit 0).
    /// Sets IFR bit 1 (CA1) on the active edge.
    pub fn set_ca1(&mut self, state: bool) {
        let active_edge = self.pcr & 0x01 != 0; // 1 = positive, 0 = negative
        let triggered = if active_edge {
            !self.ca1_prev && state // Rising edge
        } else {
            self.ca1_prev && !state // Falling edge
        };
        if triggered {
            self.ifr |= IFR_CA1;
        }
        self.ca1_prev = state;
    }

    /// Set the CB1 input line. Call this when the external signal changes.
    ///
    /// Edge detection: triggers on the configured edge (PCR bit 4).
    /// Sets IFR bit 4 (CB1) on the active edge.
    pub fn set_cb1(&mut self, state: bool) {
        let active_edge = self.pcr & 0x10 != 0; // 1 = positive, 0 = negative
        let triggered = if active_edge {
            !self.cb1_prev && state // Rising edge
        } else {
            self.cb1_prev && !state // Falling edge
        };
        if triggered {
            self.ifr |= IFR_CB1;
        }
        self.cb1_prev = state;
    }

    /// Set the CA2 flag directly. Used when external logic detects the
    /// condition that should set the CA2 interrupt flag.
    pub fn set_ca2_flag(&mut self) {
        self.ifr |= IFR_CA2;
    }

    /// Set the CB2 flag directly.
    pub fn set_cb2_flag(&mut self) {
        self.ifr |= IFR_CB2;
    }

    /// Read port A output value (combines port register and DDR).
    #[must_use]
    pub fn port_a_output(&self) -> u8 {
        self.port_a & self.ddr_a
    }

    /// Read port B output value (combines port register and DDR).
    ///
    /// If ACR bits 7-6 indicate PB7 output mode (free-run + PB7 toggle),
    /// bit 7 reflects the PB7 toggle output instead of port_b bit 7.
    #[must_use]
    pub fn port_b_output(&self) -> u8 {
        let mut out = self.port_b & self.ddr_b;
        if self.acr & 0x80 != 0 {
            // PB7 is driven by Timer 1
            out = (out & 0x7F) | if self.pb7_output { 0x80 } else { 0 };
        }
        out
    }

    /// Get the current IFR value (for diagnostic/debug use).
    #[must_use]
    pub fn ifr(&self) -> u8 {
        self.ifr
    }

    /// Get the current IER value (for diagnostic/debug use).
    #[must_use]
    pub fn ier(&self) -> u8 {
        self.ier
    }

    /// Get the ACR value.
    #[must_use]
    pub fn acr(&self) -> u8 {
        self.acr
    }

    /// Get Timer 1 counter value.
    #[must_use]
    pub fn timer1_counter(&self) -> u16 {
        self.timer1_counter
    }

    /// Get Timer 2 counter value.
    #[must_use]
    pub fn timer2_counter(&self) -> u16 {
        self.timer2_counter
    }

    // --- Internal helpers ---

    fn read_port_a(&self) -> u8 {
        (self.port_a & self.ddr_a) | (self.external_a & !self.ddr_a)
    }

    fn read_port_b(&self) -> u8 {
        let mut val = (self.port_b & self.ddr_b) | (self.external_b & !self.ddr_b);
        if self.acr & 0x80 != 0 {
            // PB7 is driven by Timer 1 toggle
            val = (val & 0x7F) | if self.pb7_output { 0x80 } else { 0 };
        }
        val
    }

    fn tick_timer1(&mut self) {
        if !self.timer1_running {
            // In free-run mode, the timer always counts even after first underflow
            if self.acr & 0x40 != 0 {
                // Free-run: keep counting
            } else {
                return;
            }
        }

        let (new_val, underflow) = self.timer1_counter.overflowing_sub(1);
        self.timer1_counter = new_val;

        if underflow {
            // Timer 1 underflowed
            self.ifr |= IFR_T1;

            if self.acr & 0x40 != 0 {
                // Free-run mode: reload from latch and keep going
                self.timer1_counter = self.timer1_latch;
                // Toggle PB7 if ACR bit 7 set
                if self.acr & 0x80 != 0 {
                    self.pb7_output = !self.pb7_output;
                }
            } else {
                // One-shot mode: stop
                self.timer1_running = false;
                self.timer1_fired = true;
            }
        }
    }

    fn tick_timer2(&mut self) {
        if !self.timer2_running {
            return;
        }

        // ACR bit 5: 0 = timed (counts every tick), 1 = count PB6 pulses
        if self.acr & 0x20 != 0 {
            return; // Pulse counting mode — not driven by tick()
        }

        let (new_val, underflow) = self.timer2_counter.overflowing_sub(1);
        self.timer2_counter = new_val;

        if underflow {
            self.ifr |= IFR_T2;
            self.timer2_running = false;
            self.timer2_fired = true;
            // Timer 2 is always one-shot
        }
    }
}

impl Default for Via6522 {
    fn default() -> Self {
        Self::new()
    }
}

// IFR/IER bit masks
const IFR_CA2: u8 = 0x01;
const IFR_CA1: u8 = 0x02;
const IFR_SR: u8 = 0x04;
const IFR_CB2: u8 = 0x08;
const IFR_CB1: u8 = 0x10;
const IFR_T2: u8 = 0x20;
const IFR_T1: u8 = 0x40;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer1_countdown_and_underflow() {
        let mut via = Via6522::new();
        // Set latch to 3, start timer
        via.write(0x04, 3); // T1L-L
        via.write(0x05, 0); // T1C-H = start (loads counter from latch)

        assert!(via.timer1_running);
        assert_eq!(via.timer1_counter, 3);
        assert_eq!(via.ifr & IFR_T1, 0); // No IRQ yet

        via.tick(); // 3 -> 2
        assert_eq!(via.timer1_counter, 2);
        via.tick(); // 2 -> 1
        via.tick(); // 1 -> 0
        via.tick(); // 0 -> 0xFFFF (underflow)
        assert_ne!(via.ifr & IFR_T1, 0); // T1 IRQ set
    }

    #[test]
    fn timer1_one_shot_stops() {
        let mut via = Via6522::new();
        via.acr = 0x00; // One-shot mode (ACR bits 7-6 = 00)
        via.write(0x04, 2);
        via.write(0x05, 0); // Start

        via.tick(); // 2 -> 1
        via.tick(); // 1 -> 0
        via.tick(); // underflow
        assert!(!via.timer1_running);
        assert_ne!(via.ifr & IFR_T1, 0);
    }

    #[test]
    fn timer1_free_run_reloads() {
        let mut via = Via6522::new();
        via.acr = 0x40; // Free-run mode
        via.write(0x04, 2);
        via.write(0x05, 0); // Start (counter = 2)

        via.tick(); // 2 -> 1
        via.tick(); // 1 -> 0
        via.tick(); // underflow -> reload to 2

        assert_ne!(via.ifr & IFR_T1, 0);
        assert_eq!(via.timer1_counter, 2); // Reloaded from latch
    }

    #[test]
    fn timer1_write_high_starts_and_clears_irq() {
        let mut via = Via6522::new();
        via.ifr = IFR_T1; // Pre-set T1 flag
        via.write(0x04, 10); // Latch low
        via.write(0x05, 0); // Write high → starts timer, clears T1 flag
        assert!(via.timer1_running);
        assert_eq!(via.ifr & IFR_T1, 0);
        assert_eq!(via.timer1_counter, 10);
    }

    #[test]
    fn timer1_read_low_clears_irq() {
        let mut via = Via6522::new();
        via.ifr = IFR_T1;
        let _ = via.read(0x04); // Read T1C-L
        assert_eq!(via.ifr & IFR_T1, 0);
    }

    #[test]
    fn timer2_one_shot() {
        let mut via = Via6522::new();
        via.write(0x08, 3); // T2L-L
        via.write(0x09, 0); // T2C-H = start

        assert!(via.timer2_running);
        via.tick(); // 3 -> 2
        via.tick(); // 2 -> 1
        via.tick(); // 1 -> 0
        via.tick(); // underflow
        assert!(!via.timer2_running);
        assert_ne!(via.ifr & IFR_T2, 0);
    }

    #[test]
    fn timer2_read_low_clears_irq() {
        let mut via = Via6522::new();
        via.ifr = IFR_T2;
        let _ = via.read(0x08);
        assert_eq!(via.ifr & IFR_T2, 0);
    }

    #[test]
    fn ifr_write_clears_flags() {
        let mut via = Via6522::new();
        via.ifr = IFR_T1 | IFR_T2 | IFR_CA1;
        via.write(0x0D, IFR_T1 | IFR_CA1); // Clear T1 and CA1
        assert_eq!(via.ifr, IFR_T2); // Only T2 remains
    }

    #[test]
    fn ier_set_clear_mode() {
        let mut via = Via6522::new();
        // Set bits: bit 7 = 1 means "set these bits"
        via.write(0x0E, 0x80 | IFR_T1 | IFR_CB1);
        assert_eq!(via.ier & IFR_T1, IFR_T1);
        assert_eq!(via.ier & IFR_CB1, IFR_CB1);

        // Clear bits: bit 7 = 0 means "clear these bits"
        via.write(0x0E, IFR_T1); // Clear T1 enable
        assert_eq!(via.ier & IFR_T1, 0);
        assert_eq!(via.ier & IFR_CB1, IFR_CB1); // CB1 still set
    }

    #[test]
    fn ier_reads_with_bit7_set() {
        let mut via = Via6522::new();
        via.ier = 0x42;
        assert_eq!(via.read(0x0E), 0xC2); // Bit 7 always 1 on read
    }

    #[test]
    fn cb1_edge_sets_flag() {
        let mut via = Via6522::new();
        via.pcr = 0x10; // CB1 positive edge
        via.cb1_prev = false;

        via.set_cb1(true); // Rising edge
        assert_ne!(via.ifr & IFR_CB1, 0);
    }

    #[test]
    fn cb1_negative_edge() {
        let mut via = Via6522::new();
        via.pcr = 0x00; // CB1 negative edge
        via.cb1_prev = true;

        via.set_cb1(false); // Falling edge
        assert_ne!(via.ifr & IFR_CB1, 0);
    }

    #[test]
    fn ca1_edge_sets_flag() {
        let mut via = Via6522::new();
        via.pcr = 0x01; // CA1 positive edge
        via.ca1_prev = false;

        via.set_ca1(true);
        assert_ne!(via.ifr & IFR_CA1, 0);
    }

    #[test]
    fn external_port_reads() {
        let mut via = Via6522::new();
        via.ddr_a = 0x0F; // Low nibble = output, high nibble = input
        via.port_a = 0xAB;
        via.external_a = 0xC0;

        let val = via.read(0x0F); // ORA no-handshake
        // Output bits: 0xAB & 0x0F = 0x0B
        // Input bits: 0xC0 & 0xF0 = 0xC0
        assert_eq!(val, 0xCB);
    }

    #[test]
    fn port_b_external() {
        let mut via = Via6522::new();
        via.ddr_b = 0x00; // All input
        via.external_b = 0x42;

        let val = via.read(0x00);
        assert_eq!(val, 0x42);
    }

    #[test]
    fn pb7_toggle_on_free_run() {
        let mut via = Via6522::new();
        via.acr = 0xC0; // Free-run + PB7 output
        via.ddr_b = 0x80; // PB7 = output
        via.write(0x04, 1);
        via.write(0x05, 0); // Start, counter = 1

        assert!(!via.pb7_output);
        via.tick(); // 1 -> 0
        via.tick(); // underflow -> toggle PB7
        assert!(via.pb7_output);
        // Reload latch = 1
        via.ifr = 0; // Clear for next check
        via.tick(); // 1 -> 0
        via.tick(); // underflow -> toggle again
        assert!(!via.pb7_output);
    }

    #[test]
    fn irq_active_requires_both_flag_and_enable() {
        let mut via = Via6522::new();
        via.ifr = IFR_T1;
        assert!(!via.irq_active()); // IER not set

        via.ier = IFR_T1;
        assert!(via.irq_active()); // Both set

        via.ifr = 0;
        assert!(!via.irq_active()); // Flag cleared
    }

    #[test]
    fn read_orb_clears_cb_flags() {
        let mut via = Via6522::new();
        via.ifr = IFR_CB1 | IFR_CB2 | IFR_T1;
        let _ = via.read(0x00); // Read ORB
        assert_eq!(via.ifr & IFR_CB1, 0);
        assert_eq!(via.ifr & IFR_CB2, 0);
        assert_ne!(via.ifr & IFR_T1, 0); // T1 untouched
    }

    #[test]
    fn read_ora_clears_ca_flags() {
        let mut via = Via6522::new();
        via.ifr = IFR_CA1 | IFR_CA2 | IFR_T2;
        let _ = via.read(0x01); // Read ORA
        assert_eq!(via.ifr & IFR_CA1, 0);
        assert_eq!(via.ifr & IFR_CA2, 0);
        assert_ne!(via.ifr & IFR_T2, 0); // T2 untouched
    }

    #[test]
    fn ora_no_handshake_preserves_ca_flags() {
        let mut via = Via6522::new();
        via.ifr = IFR_CA1 | IFR_CA2;
        let _ = via.read(0x0F); // ORA no-handshake
        // CA flags should NOT be cleared
        assert_ne!(via.ifr & IFR_CA1, 0);
        assert_ne!(via.ifr & IFR_CA2, 0);
    }

    #[test]
    fn timer1_latch_write_does_not_start() {
        let mut via = Via6522::new();
        via.write(0x06, 0x10); // T1L-L
        via.write(0x07, 0x00); // T1L-H — should NOT start timer
        assert!(!via.timer1_running);
        // But should clear T1 IRQ flag
        via.ifr = IFR_T1;
        via.write(0x07, 0x00);
        assert_eq!(via.ifr & IFR_T1, 0);
    }
}
