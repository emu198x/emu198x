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

    /// Power-LED output — CIA-A PRA bit 1, **active low**. Returns
    /// `true` when the LED is bright (bit clear), which is also when
    /// the switchable audio LED filter is engaged. Computed from the
    /// latched PRA bit only when DDRA bit 1 is an output; an input pin
    /// floats high (LED dim, filter off), matching the 8520 pull-up —
    /// the same convention as [`CiaExt::ovl`].
    fn power_led(&self) -> bool;
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

    fn power_led(&self) -> bool {
        let pin_high = if self.ddr_a() & 0x02 != 0 {
            self.port_a_latch() & 0x02 != 0
        } else {
            true
        };
        !pin_high
    }
}

/// The glue chip that decodes the CIA region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CiaDecoder {
    /// Gary (A500, A2000, A500+) and the A1000's logic: the whole
    /// `$A00000-$BFFFFF` region is CIA space.
    Gary,
    /// Gayle (A600, A1200): only the `$xxDxxx` and `$xxExxx` pages of that
    /// region reach the CIAs, as WinUAE's `isgaylenocia` models.
    Gayle,
}

/// The CIAs one access selects, and the register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CiaSelect {
    /// CIA-A, on data bits 7-0, is selected by A12 low.
    pub a: bool,
    /// CIA-B, on data bits 15-8, is selected by A13 low.
    pub b: bool,
    /// The register, from A11-A8.
    pub reg: u8,
}

/// Decode an access in CIA space (Amiga Hardware Reference Manual, 3rd
/// edition, appendix F): the CIAs are selected "when the upper three
/// address bits are 101", CIA-A when A12 is low and CIA-B when A13 is low,
/// A11-A8 pick the register, and all other bits are don't-cares. With A12
/// and A13 both low both are selected; with both high neither is, but the
/// access is still a VPA cycle. Returns `None` outside CIA space.
#[must_use]
pub fn decode_cia_select(addr: u32, decoder: CiaDecoder) -> Option<CiaSelect> {
    let addr = addr & 0x00FF_FFFF;
    if addr >> 21 != 0b101 {
        return None;
    }
    if decoder == CiaDecoder::Gayle && !matches!(addr & 0xF000, 0xD000 | 0xE000) {
        return None;
    }
    Some(CiaSelect {
        a: addr & 0x1000 == 0,
        b: addr & 0x2000 == 0,
        reg: ((addr >> 8) & 0x0F) as u8,
    })
}

/// Decode a 24-bit Amiga address into a CIA-A register index, if the
/// address falls in the documented CIA-A window (`$BFExxx`). For test and
/// debugger backdoors that address a register directly; the CPU's bus
/// path uses [`decode_cia_select`].
#[must_use]
pub fn decode_cia_a(addr: u32) -> Option<u8> {
    if (0x00BF_E000..0x00BF_F000).contains(&addr) {
        Some(((addr >> 8) & 0x0F) as u8)
    } else {
        None
    }
}

/// Decode into CIA-B register index, for the documented CIA-B window
/// (even bytes in `$BFDxxx`). Backdoor use only, as for
/// [`decode_cia_a`].
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
        // Odd-address byte access is the canonical CIA-A path.
        assert_eq!(decode_cia_a(0x00BFE001), Some(0));
        assert_eq!(decode_cia_a(0x00BFE101), Some(1));
        assert_eq!(decode_cia_a(0x00BFE201), Some(2));
        assert_eq!(decode_cia_a(0x00BFEF01), Some(0xF));
        assert_eq!(decode_cia_a(0x00BFE000), Some(0));
        assert_eq!(decode_cia_a(0x00BFED00), Some(0xD));
        assert_eq!(decode_cia_a(0x00BFD001), None); // CIA-B range
        assert_eq!(decode_cia_a(0x00BFF001), None); // outside
        assert_eq!(decode_cia_b(0x00BFD000), Some(0));
        assert_eq!(decode_cia_b(0x00BFDF00), Some(0xF));
        assert_eq!(decode_cia_b(0x00BFD001), None); // odd byte = CIA-A
    }

    #[test]
    fn cia_select_follows_the_hardware_reference_manual() {
        let gary = |addr| decode_cia_select(addr, CiaDecoder::Gary);
        let select = |a, b, reg| Some(CiaSelect { a, b, reg });
        // The documented addresses.
        assert_eq!(gary(0x00BF_E001), select(true, false, 0));
        assert_eq!(gary(0x00BF_D100), select(false, true, 1));
        // A12 and A13 both low select both; both high select neither.
        assert_eq!(gary(0x00BF_CF00), select(true, true, 0xF));
        assert_eq!(gary(0x00BF_F201), select(false, false, 2));
        // Every address with the top three bits 101 is CIA space.
        assert_eq!(gary(0x00A0_E401), select(true, false, 4));
        assert_eq!(gary(0x00B5_D300), select(false, true, 3));
        assert_eq!(gary(0x009F_FFFF), None);
        assert_eq!(gary(0x00C0_0000), None);
        // Gayle passes only the $xxDxxx and $xxExxx pages.
        let gayle = |addr| decode_cia_select(addr, CiaDecoder::Gayle);
        assert_eq!(gayle(0x00BF_E001), select(true, false, 0));
        assert_eq!(gayle(0x00A0_D000), select(false, true, 0));
        assert_eq!(gayle(0x00BF_C001), None);
        assert_eq!(gayle(0x00BF_F001), None);
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

    #[test]
    fn power_led_off_when_ddra_input() {
        let cia = Cia::new();
        // DDRA = 0 (input), PRA bit 1 floats high → LED dim, filter off.
        assert!(!cia.power_led());
    }

    #[test]
    fn power_led_is_active_low_when_ddra_output() {
        let mut cia = Cia::new();
        cia.write(0x02, 0x02); // DDRA bit 1 = output
        cia.write(0x00, 0x00); // PRA bit 1 = 0 → LED bright, filter on
        assert!(cia.power_led());
        cia.write(0x00, 0x02); // PRA bit 1 = 1 → LED dim, filter off
        assert!(!cia.power_led());
    }
}
