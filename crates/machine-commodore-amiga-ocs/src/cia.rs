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

pub struct CiaA {
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
    ///
    /// Defaults to all-high (no peripherals attached). Peripheral
    /// modules (mouse, joystick, floppy) override these bits as
    /// they're added in later milestones.
    pub pa_input_lines: u8,
    /// Same idea for Port B inputs.
    pub pb_input_lines: u8,
}

impl Default for CiaA {
    fn default() -> Self {
        Self {
            pra: 0,
            ddra: 0,
            prb: 0,
            ddrb: 0,
            pa_input_lines: 0xFF,
            pb_input_lines: 0xFF,
        }
    }
}

impl CiaA {
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
            _ => {} // future milestones
        }
    }

    /// Read byte from a CIA-A register at the given index (0..=15).
    #[must_use]
    pub fn read_register(&self, reg: u8) -> u8 {
        match reg {
            0 => effective_port(self.pra, self.ddra, self.pa_input_lines),
            1 => effective_port(self.prb, self.ddrb, self.pb_input_lines),
            2 => self.ddra,
            3 => self.ddrb,
            _ => 0xFF,
        }
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
/// Returns `Some(reg)` on hit, `None` otherwise.
#[must_use]
pub fn decode_cia_a(addr: u32) -> Option<u8> {
    if (0x00BF_E000..0x00BF_F000).contains(&addr) && addr & 1 == 1 {
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
        let cia = CiaA::new();
        // DDRA = 0 (input), PRA bit 0 floats high → OVL asserted
        assert!(cia.ovl());
    }

    #[test]
    fn ovl_follows_pra_when_ddra_output() {
        let mut cia = CiaA::new();
        cia.write_register(2, 0x01); // DDRA bit 0 = output
        cia.write_register(0, 0x00); // PRA bit 0 = 0
        assert!(!cia.ovl());
        cia.write_register(0, 0x01); // PRA bit 0 = 1
        assert!(cia.ovl());
    }

    #[test]
    fn pra_reads_floating_high_for_inputs_at_reset() {
        let cia = CiaA::new();
        // DDRA = $00 (all input), reads should all be high (floating).
        assert_eq!(cia.read_register(0), 0xFF);
    }

    #[test]
    fn pra_reads_mix_outputs_and_inputs() {
        let mut cia = CiaA::new();
        cia.write_register(2, 0x03); // DDRA: bits 0+1 outputs, 2-7 inputs
        cia.write_register(0, 0x02); // PRA: bit 1 high, bit 0 low
        // Expected: bit 0 = 0 (PRA), bit 1 = 1 (PRA), bits 2-7 = 1 (input)
        // = 0b1111_1110 = $FE
        assert_eq!(cia.read_register(0), 0xFE);
    }

    #[test]
    fn pra_reads_can_be_pulled_low_by_peripheral() {
        let mut cia = CiaA::new();
        cia.write_register(2, 0x03); // DDRA bits 0+1 output
        cia.write_register(0, 0x02); // PRA bit 1 high
        // Peripheral pulls bit 4 (/TRK0) low.
        cia.pa_input_lines = !0x10;
        // Expected: bits 0+1 = PRA (10), bit 4 = 0 (peripheral),
        // other input bits = 1 → 0b1110_1110 = $EE
        assert_eq!(cia.read_register(0), 0xEE);
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
