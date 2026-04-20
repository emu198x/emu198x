//! CIA wiring for the OCS machine.
//!
//! As of task #96 (port session 2026-04-20), the CIA implementation
//! lives in the standalone `mos-cia-8520` crate. This module:
//!
//!   - Re-exports `Cia8520` under the short name `Cia` used
//!     throughout the machine.
//!   - Re-exports `Timer` (a struct kept for test-accessor parity
//!     with the previous in-tree impl).
//!   - Provides `decode_cia_a` / `decode_cia_b` address decoders —
//!     Amiga-specific and therefore not in the chip crate.
//!   - Provides `effective_port` as a pure function (previously
//!     private to the in-tree struct); tests use it to reason
//!     about port read semantics.
//!   - Provides a small `CiaExt` trait that adds helpers the
//!     machine uses: `ovl()` (CIA-A PRA bit 0 effective output —
//!     drives the memory-overlay line) and `peek_register()` (a
//!     side-effect-free read for debugging/tests — the archive's
//!     `read()` takes `&mut self` because it clears ICR on read).

pub use mos_cia_8520::Cia8520 as Cia;

/// Compact struct mirroring the machine's historical `Timer`
/// accessor surface — used by boot tests that want a live snapshot
/// of one of the timer slots. Populated on demand via
/// [`CiaExt::timer_a_snapshot`] / [`timer_b_snapshot`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Timer {
    pub latch: u16,
    pub counter: u16,
    pub control: u8,
}

/// Machine-side extension methods on top of `Cia8520`.
pub trait CiaExt {
    /// OVL output — CIA-A PRA bit 0 effective value. When `true`,
    /// Gary maps ROM at `$000000`; when `false`, chip RAM owns that
    /// window. Computed from the latched PRA bit only when DDRA bit
    /// 0 is an output; input state floats high (`true`) — matching
    /// the pull-up behaviour the 8520 chip presents to the pin.
    fn ovl(&self) -> bool;
    /// Side-effect-free read of the 16 register space. Mirrors the
    /// side-effecting `read()` EXCEPT it does not clear the ICR
    /// data-register on reads of `$D`. Used by tests that want to
    /// inspect ICR state without disturbing it.
    fn peek_register(&self, reg: u8) -> u8;
    /// Snapshot view of Timer A state.
    fn timer_a_snapshot(&self) -> Timer;
    /// Snapshot view of Timer B state.
    fn timer_b_snapshot(&self) -> Timer;
    /// Convenience: are any unmasked ICR flags active?
    /// (Same semantics as `irq_active` — named for parity with the
    /// historical field.)
    fn irq_pending(&self) -> bool;
}

impl CiaExt for Cia {
    fn ovl(&self) -> bool {
        // OVL pin reflects the effective PRA bit 0. If DDRA bit 0 is
        // set (output), the PRA latch drives the pin; otherwise the
        // input floats high (pull-up).
        let ddra = self.ddr_a();
        if ddra & 0x01 != 0 {
            self.port_a_latch() & 0x01 != 0
        } else {
            true
        }
    }

    fn peek_register(&self, reg: u8) -> u8 {
        match reg & 0x0F {
            0x00 => effective_port(self.port_a_latch(), self.ddr_a(), self.external_a),
            0x01 => effective_port(self.port_b_latch(), self.ddr_b(), self.external_b),
            0x02 => self.ddr_a(),
            0x03 => self.ddr_b(),
            0x04 => (self.timer_a() & 0xFF) as u8,
            0x05 => (self.timer_a() >> 8) as u8,
            0x06 => (self.timer_b() & 0xFF) as u8,
            0x07 => (self.timer_b() >> 8) as u8,
            0x08 => (self.tod_counter() & 0xFF) as u8,
            0x09 => ((self.tod_counter() >> 8) & 0xFF) as u8,
            0x0A => ((self.tod_counter() >> 16) & 0xFF) as u8,
            0x0C => self.sdr(),
            0x0D => {
                let flags = self.icr_status();
                let ir = if self.irq_active() { 0x80 } else { 0 };
                ir | flags
            }
            0x0E => self.cra(),
            0x0F => self.crb(),
            _ => 0xFF,
        }
    }

    fn timer_a_snapshot(&self) -> Timer {
        Timer {
            latch: 0,
            counter: self.timer_a(),
            control: self.cra(),
        }
    }

    fn timer_b_snapshot(&self) -> Timer {
        Timer {
            latch: 0,
            counter: self.timer_b(),
            control: self.crb(),
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_active()
    }
}

/// Decode a 24-bit Amiga address into a CIA-A register index, if
/// the address falls into the CIA-A address space (odd byte,
/// `$BFExxx`). CIA-A decodes on odd bytes (D0-D7 bus); register
/// select uses address bits 8-11, giving 256-byte stride.
#[must_use]
pub fn decode_cia_a(addr: u32) -> Option<u8> {
    if (0x00BF_E000..0x00BF_F000).contains(&addr) && addr & 1 == 1 {
        Some(((addr >> 8) & 0x0F) as u8)
    } else {
        None
    }
}

/// Decode into CIA-B register index. CIA-B lives on D8-D15 so it
/// uses even bytes in `$BFDxxx`.
#[must_use]
pub fn decode_cia_b(addr: u32) -> Option<u8> {
    if (0x00BF_D000..0x00BF_E000).contains(&addr) && addr & 1 == 0 {
        Some(((addr >> 8) & 0x0F) as u8)
    } else {
        None
    }
}

/// Compute a port read: output bits come from the data latch; input
/// bits come from whatever's driving the line externally (or float
/// high through pull-ups).
#[must_use]
pub fn effective_port(data: u8, direction: u8, input_lines: u8) -> u8 {
    (data & direction) | (input_lines & !direction)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_decoding() {
        assert_eq!(decode_cia_a(0x00BFE001), Some(0));
        assert_eq!(decode_cia_a(0x00BFE101), Some(1));
        assert_eq!(decode_cia_a(0x00BFE201), Some(2));
        assert_eq!(decode_cia_a(0x00BFEF01), Some(0xF));
        assert_eq!(decode_cia_a(0x00BFE000), None); // even byte = CIA-B
        assert_eq!(decode_cia_a(0x00BFD001), None); // CIA-B range
        assert_eq!(decode_cia_a(0x00BFF001), None); // outside
        assert_eq!(decode_cia_b(0x00BFD000), Some(0));
        assert_eq!(decode_cia_b(0x00BFDF00), Some(0xF));
        assert_eq!(decode_cia_b(0x00BFD001), None); // odd byte = CIA-A
    }

    #[test]
    fn effective_port_outputs_driven_inputs_float() {
        // bits 0-1 output (data driven), bits 2-7 input (lines).
        let out = effective_port(0x03, 0x03, 0xFC);
        assert_eq!(out, 0xFF);
        let out = effective_port(0x02, 0x03, 0xFC);
        assert_eq!(out, 0xFE);
    }

    #[test]
    fn ovl_default_high_when_ddra_input() {
        let cia = Cia::new("A");
        // DDRA = 0 (input), PRA bit 0 floats high → OVL asserted
        assert!(cia.ovl());
    }

    #[test]
    fn ovl_follows_pra_when_ddra_output() {
        let mut cia = Cia::new("A");
        cia.write(0x02, 0x01); // DDRA bit 0 = output
        cia.write(0x00, 0x00); // PRA bit 0 = 0
        assert!(!cia.ovl());
        cia.write(0x00, 0x01); // PRA bit 0 = 1
        assert!(cia.ovl());
    }

    #[test]
    fn peek_register_icr_does_not_clear_flags() {
        let mut cia = Cia::new("T");
        cia.receive_serial_byte(0);
        assert_eq!(cia.peek_register(0x0D) & 0x08, 0x08);
        // Peek must not have cleared the flag.
        assert_eq!(cia.peek_register(0x0D) & 0x08, 0x08);
        // A real read DOES clear.
        let _ = cia.read(0x0D);
        assert_eq!(cia.peek_register(0x0D) & 0x08, 0);
    }
}
