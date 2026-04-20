//! CIA — minimal storage layer (M3).
//!
//! At M3 the CIA is just a register bag with two wired bits: CIA-A
//! PRA bit 0 (OVL) gated by CIA-A DDRA bit 0. Everything else is
//! storage with no behaviour. Timers, TOD, serial, keyboard,
//! handshake interrupts: future milestones.
//!
//! On the Amiga both CIAs share the same chip-select address space
//! `$BFD000-$BFEFFF`. The address decoding is unusual:
//!  - CIA-A is on the LOW data bus (D0-7), at **odd** addresses.
//!  - CIA-B is on the HIGH data bus (D8-15), at **even** addresses.
//!  - Within each CIA, the register is selected by address bits 8-11
//!    (so registers are spaced 256 bytes apart).
//!
//! M3 only models CIA-A — CIA-B is added when a later milestone
//! exercises it.

pub struct Cia {
    /// Register 0 — Port A data register.
    pub pra: u8,
    /// Register 2 — Port A direction register (1 bit = output).
    pub ddra: u8,
    /// Register 1 — Port B data register.
    pub prb: u8,
    /// Register 3 — Port B direction register.
    pub ddrb: u8,
    /// External signals driving Port A inputs. Each bit holds the
    /// **effective** voltage on that line: 1 = floating high (no
    /// peripheral asserting); 0 = peripheral pulled low.
    pub pa_input_lines: u8,
    /// Same idea for Port B inputs.
    pub pb_input_lines: u8,
    /// Timer A — counter ticks down on each E clock when CRA bit 0
    /// (START) is set. Underflow latches ICR bit 0 (TA).
    pub timer_a: Timer,
    /// Timer B — separate counter; same shape as Timer A.
    pub timer_b: Timer,
    /// Interrupt-control register state. `mask` is the IMR set by
    /// CPU writes; `flags` is the IDR raised by hardware events.
    /// CPU read of ICR returns IDR | (IR-bit if any flag matches mask)
    /// AND clears IDR.
    pub icr_mask: u8,
    pub icr_flags: u8,
    /// Level-sensitive /IRQ output to Paula — `true` whenever any
    /// unmasked ICR flag is set. Paula edge-latches this signal
    /// into INTREQ.PORTS / INTREQ.EXTER, so the consumer observes
    /// rising edges rather than continuous level.
    ///
    /// Goes false when:
    ///   - A CPU ICR read clears the flags, or
    ///   - The mask is narrowed so no active flag matches.
    ///
    /// The previous emulator version set this on the rising edge
    /// and held it until ICR read; that model incorrectly re-
    /// latched INTREQ.PORTS when a handler cleared the Paula bit
    /// without reading CIA ICR. With true level semantics here and
    /// Paula-side edge detection in AmigaOcs, the double-latch
    /// doesn't happen.
    pub irq_pending: bool,
}

#[derive(Default, Clone, Copy)]
pub struct Timer {
    /// Latch — written via TxLO/TxHI; loaded into counter on START
    /// or LOAD strobe, and on continuous-mode underflow.
    pub latch: u16,
    /// Current counter value.
    pub counter: u16,
    /// CRA / CRB control register.
    pub control: u8,
}

impl Timer {
    /// Tick one E-clock period. Returns `true` if the counter just
    /// underflowed.
    pub fn tick(&mut self) -> bool {
        if self.control & 0x01 == 0 {
            return false; // not started
        }
        if self.counter == 0 {
            // Underflow.
            if self.control & 0x08 != 0 {
                // One-shot: stop after underflow.
                self.control &= !0x01;
            } else {
                // Continuous: reload from latch.
                self.counter = self.latch;
            }
            return true;
        }
        self.counter -= 1;
        false
    }

    /// Force-load the latch into the counter (LOAD strobe — bit 4
    /// of CRx; write-only, doesn't stay set).
    pub fn force_load(&mut self) {
        self.counter = self.latch;
    }
}

impl Default for Cia {
    fn default() -> Self {
        Self {
            pra: 0,
            ddra: 0,
            prb: 0,
            ddrb: 0,
            pa_input_lines: 0xFF,
            pb_input_lines: 0xFF,
            timer_a: Timer::default(),
            timer_b: Timer::default(),
            icr_mask: 0,
            icr_flags: 0,
            irq_pending: false,
        }
    }
}

impl Cia {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Write byte to a CIA-A register at the given index (0..=15).
    pub fn write_register(&mut self, reg: u8, val: u8) {
        match reg {
            0 => self.pra = val,
            1 => self.prb = val,
            2 => self.ddra = val,
            3 => self.ddrb = val,
            // $4 / $5 — Timer A latch low/high. Writing high latches
            // the new value into the counter (per CIA datasheet).
            4 => self.timer_a.latch = (self.timer_a.latch & 0xFF00) | u16::from(val),
            5 => {
                self.timer_a.latch = (self.timer_a.latch & 0x00FF) | (u16::from(val) << 8);
                if self.timer_a.control & 0x01 == 0 {
                    // When timer is stopped, writing TxHI also loads
                    // the counter (one-shot setup pattern).
                    self.timer_a.counter = self.timer_a.latch;
                }
            }
            // $6 / $7 — Timer B latch.
            6 => self.timer_b.latch = (self.timer_b.latch & 0xFF00) | u16::from(val),
            7 => {
                self.timer_b.latch = (self.timer_b.latch & 0x00FF) | (u16::from(val) << 8);
                if self.timer_b.control & 0x01 == 0 {
                    self.timer_b.counter = self.timer_b.latch;
                }
            }
            // $D — ICR write: mask programming with set/clear semantics.
            0xD => {
                if val & 0x80 != 0 {
                    self.icr_mask |= val & 0x1F;
                } else {
                    self.icr_mask &= !(val & 0x1F);
                }
                self.update_irq();
            }
            // $E — CRA: timer A control. LOAD bit (4) is a strobe.
            0xE => {
                if val & 0x10 != 0 {
                    self.timer_a.force_load();
                }
                // LOAD bit doesn't stick.
                self.timer_a.control = val & !0x10;
            }
            // $F — CRB: timer B control.
            0xF => {
                if val & 0x10 != 0 {
                    self.timer_b.force_load();
                }
                self.timer_b.control = val & !0x10;
            }
            _ => {}
        }
    }

    /// Read byte from a CIA-A register at the given index (0..=15).
    /// Some registers have read side-effects (notably ICR which clears
    /// on read), so this takes `&mut self`.
    pub fn read_register(&mut self, reg: u8) -> u8 {
        match reg {
            0 => effective_port(self.pra, self.ddra, self.pa_input_lines),
            1 => effective_port(self.prb, self.ddrb, self.pb_input_lines),
            2 => self.ddra,
            3 => self.ddrb,
            4 => (self.timer_a.counter & 0xFF) as u8,
            5 => (self.timer_a.counter >> 8) as u8,
            6 => (self.timer_b.counter & 0xFF) as u8,
            7 => (self.timer_b.counter >> 8) as u8,
            0xD => {
                // Return current flags + IR-pending bit; clear flags
                // on read. update_irq recomputes /IRQ level — with
                // flags now zero, irq_pending drops to false.
                let active = self.icr_flags & self.icr_mask;
                let ir = if active != 0 { 0x80 } else { 0 };
                let val = ir | self.icr_flags;
                self.icr_flags = 0;
                self.update_irq();
                val
            }
            0xE => self.timer_a.control,
            0xF => self.timer_b.control,
            _ => 0xFF,
        }
    }

    /// Read without side-effects — for diagnostics / tests inspecting
    /// state. Does NOT clear ICR.
    #[must_use]
    pub fn peek_register(&self, reg: u8) -> u8 {
        match reg {
            0xD => {
                let active = self.icr_flags & self.icr_mask;
                let ir = if active != 0 { 0x80 } else { 0 };
                ir | self.icr_flags
            }
            // For the other registers, side-effect-free read matches
            // the side-effecting one. We can't share code without
            // duplicating the match arms, so just inline the simple
            // ones we need.
            0 => effective_port(self.pra, self.ddra, self.pa_input_lines),
            1 => effective_port(self.prb, self.ddrb, self.pb_input_lines),
            2 => self.ddra,
            3 => self.ddrb,
            4 => (self.timer_a.counter & 0xFF) as u8,
            5 => (self.timer_a.counter >> 8) as u8,
            6 => (self.timer_b.counter & 0xFF) as u8,
            7 => (self.timer_b.counter >> 8) as u8,
            0xE => self.timer_a.control,
            0xF => self.timer_b.control,
            _ => 0xFF,
        }
    }

    /// Tick one E-clock period (= 10 CCKs). Steps timer A and B,
    /// latches underflow into ICR, and updates the IRQ output.
    pub fn tick_e_clock(&mut self) {
        if self.timer_a.tick() {
            self.icr_flags |= 0x01;
        }
        if self.timer_b.tick() {
            self.icr_flags |= 0x02;
        }
        self.update_irq();
    }

    fn update_irq(&mut self) {
        // /IRQ is level-sensitive: asserted whenever any unmasked
        // ICR flag is set. Edge detection happens in Paula (our
        // AmigaOcs wrapper) so we don't have to track edges here.
        self.irq_pending = (self.icr_flags & self.icr_mask) != 0;
    }

    /// Effective output value for Port A bit `bit`. Returns the PRA
    /// bit when DDRA marks it as output; otherwise floats high
    /// (input), per CIA pull-up behaviour.
    #[must_use]
    pub fn pra_output(&self, bit: u8) -> bool {
        let mask = 1 << bit;
        if self.ddra & mask != 0 {
            self.pra & mask != 0
        } else {
            // Input pin floats high.
            true
        }
    }

    /// True when the OVL line should be asserted (ROM mapped low).
    /// OVL = effective PRA bit 0 — high (`true`) means ROM at $0.
    #[must_use]
    pub fn ovl(&self) -> bool {
        self.pra_output(0)
    }
}

/// Decode a 24-bit Amiga address into a CIA-A register index, if the
/// address falls into the CIA-A address space (odd byte, $BFExxx).
#[must_use]
pub fn decode_cia_a(addr: u32) -> Option<u8> {
    if (0x00BF_E000..0x00BF_F000).contains(&addr) && addr & 1 == 1 {
        Some(((addr >> 8) & 0x0F) as u8)
    } else {
        None
    }
}

/// Decode a 24-bit Amiga address into a CIA-B register index, if the
/// address falls into the CIA-B address space (even byte, $BFDxxx).
#[must_use]
pub fn decode_cia_b(addr: u32) -> Option<u8> {
    if (0x00BF_D000..0x00BF_E000).contains(&addr) && addr & 1 == 0 {
        Some(((addr >> 8) & 0x0F) as u8)
    } else {
        None
    }
}

/// Compute the effective port-line state: output bits return the
/// stored data-register value; input bits return the externally
/// driven line state (floats high if no peripheral asserts).
#[must_use]
pub fn effective_port(data: u8, direction: u8, input_lines: u8) -> u8 {
    (data & direction) | (input_lines & !direction)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ovl_default_high_when_ddra_input() {
        let cia = Cia::new();
        // DDRA = 0 (input), PRA bit 0 floats high → OVL asserted
        assert!(cia.ovl());
    }

    #[test]
    fn ovl_follows_pra_when_ddra_output() {
        let mut cia = Cia::new();
        cia.write_register(2, 0x01); // DDRA bit 0 = output
        cia.write_register(0, 0x00); // PRA bit 0 = 0
        assert!(!cia.ovl());
        cia.write_register(0, 0x01); // PRA bit 0 = 1
        assert!(cia.ovl());
    }

    #[test]
    fn pra_reads_floating_high_for_inputs_at_reset() {
        let mut cia = Cia::new();
        // DDRA = $00 (all input), reads should all be high (floating).
        assert_eq!(cia.read_register(0), 0xFF);
    }

    #[test]
    fn pra_reads_mix_outputs_and_inputs() {
        let mut cia = Cia::new();
        cia.write_register(2, 0x03); // DDRA: bits 0+1 outputs, 2-7 inputs
        cia.write_register(0, 0x02); // PRA: bit 1 high, bit 0 low
        assert_eq!(cia.read_register(0), 0xFE);
    }

    #[test]
    fn pra_reads_can_be_pulled_low_by_peripheral() {
        let mut cia = Cia::new();
        cia.write_register(2, 0x03);
        cia.write_register(0, 0x02);
        cia.pa_input_lines = !0x10;
        assert_eq!(cia.read_register(0), 0xEE);
    }

    #[test]
    fn timer_a_one_shot_underflows_and_sets_icr() {
        let mut cia = Cia::new();
        // ICR mask: enable TA (bit 0).
        cia.write_register(0xD, 0x81);
        // Latch = 3 (will count 3, 2, 1, 0, then underflow).
        cia.write_register(0x4, 0x03);
        cia.write_register(0x5, 0x00);
        // CRA: START | ONE-SHOT | LOAD strobe.
        cia.write_register(0xE, 0x19);
        for _ in 0..4 {
            cia.tick_e_clock();
        }
        // Read ICR — should report TA + IR.
        assert_eq!(cia.read_register(0xD), 0x81);
        // Second read clears.
        assert_eq!(cia.read_register(0xD), 0);
        // One-shot mode: timer should be stopped.
        assert_eq!(cia.timer_a.control & 0x01, 0);
    }

    #[test]
    fn timer_a_continuous_reloads_after_underflow() {
        let mut cia = Cia::new();
        cia.write_register(0xD, 0x81); // unmask TA
        cia.write_register(0x4, 0x02); // latch = 2
        cia.write_register(0x5, 0x00);
        cia.write_register(0xE, 0x11); // START | LOAD; continuous
        // After 3 ticks the first underflow happens; counter reloads.
        for _ in 0..3 {
            cia.tick_e_clock();
        }
        assert_eq!(cia.peek_register(0xD) & 0x01, 1, "TA flag set");
        assert_eq!(cia.timer_a.control & 0x01, 1, "continuous mode keeps running");
        // Counter should hold the latch value (just reloaded).
        assert_eq!(cia.timer_a.counter, 2);
    }

    #[test]
    fn address_decoding() {
        assert_eq!(decode_cia_a(0x00BFE001), Some(0)); // PRA
        assert_eq!(decode_cia_a(0x00BFE101), Some(1)); // PRB
        assert_eq!(decode_cia_a(0x00BFE201), Some(2)); // DDRA
        assert_eq!(decode_cia_a(0x00BFE301), Some(3)); // DDRB
        assert_eq!(decode_cia_a(0x00BFEF01), Some(0xF));

        // Even bytes are CIA-B, not CIA-A
        assert_eq!(decode_cia_a(0x00BFE000), None);
        // Outside CIA address range
        assert_eq!(decode_cia_a(0x00BFD001), None);
        assert_eq!(decode_cia_a(0x00BFF001), None);
    }
}
