//! CIA wiring for the OCS machine.
//!
//! Owns only what's Amiga-specific about the two 8520 instances:
//!
//!   - Re-exports `Cia8520` under the short name `Cia` used
//!     throughout the machine.
//!   - Provides `decode_cia_a` / `decode_cia_b` address decoders —
//!     CIA-A is on the odd-byte half of `$BFExxx`, CIA-B on the
//!     even-byte half of `$BFDxxx`.
//!   - Provides a small `CiaExt` trait that adds the machine-level
//!     `ovl()` helper — CIA-A PRA bit 0 drives Gary's overlay line,
//!     which is not a chip concern.
//!
//! Everything else (register peek/read/write, timer/TOD snapshots,
//! port pin drive) lives on `Cia8520` directly.

pub use mos_cia_8520::Cia8520 as Cia;

/// Machine-side extension methods on top of `Cia8520`.
pub trait CiaExt {
    /// OVL output — CIA-A PRA bit 0 effective value. When `true`,
    /// Gary maps ROM at `$000000`; when `false`, chip RAM owns that
    /// window. Computed from the latched PRA bit only when DDRA bit
    /// 0 is an output; input state floats high (`true`) — matching
    /// the pull-up behaviour the 8520 chip presents to the pin.
    fn ovl(&self) -> bool;
}

impl CiaExt for Cia {
    fn ovl(&self) -> bool {
        let ddra = self.ddr_a();
        if ddra & 0x01 != 0 {
            self.port_a_latch() & 0x01 != 0
        } else {
            true
        }
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
    fn ovl_default_high_when_ddra_input() {
        let cia = Cia::new();
        // DDRA = 0 (input), PRA bit 0 floats high → OVL asserted.
        assert!(cia.ovl());
    }

    #[test]
    fn ovl_follows_pra_when_ddra_output() {
        let mut cia = Cia::new();
        cia.write(0x02, 0x01); // DDRA bit 0 = output
        cia.write(0x00, 0x00); // PRA bit 0 = 0
        assert!(!cia.ovl());
        cia.write(0x00, 0x01); // PRA bit 0 = 1
        assert!(cia.ovl());
    }
}
