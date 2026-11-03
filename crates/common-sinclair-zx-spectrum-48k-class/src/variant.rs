//! Variant markers for the 48K-class machines.
//!
//! The 16K (1982), 48K (1982), and Spectrum+ (1984) all share the same
//! Ferranti 6C001E ULA, the same 48 BASIC ROM, the same Z80, and the
//! same keyboard matrix. Their differences live in: RAM size (16K is
//! half-equipped) and identity (Spectrum+ has a different keyboard
//! housing and a reset button — same chips, different catalogue entry).
//!
//! Rather than three duplicated machine structs, the layer crate
//! parameterises [`crate::core::SpectrumMachineCore`] over a phantom
//! marker so each variant is a *distinct type*. Snapshots can't cross
//! variants, and per-machine metadata (release year, discontinued year,
//! marketing copy) attaches to the marker rather than to the runtime.

/// Marker trait for the supported 48K-class variants.
///
/// Implemented as zero-sized phantom types — the marker contributes no
/// state to the machine, only type-level identity.
pub trait Variant48kClass: 'static {
    /// Stable hardware identifier used by the catalogue.
    const MODEL_ID: &'static str;

    /// Whether the board has the "spider": a transistor across the Z80 that
    /// ORs A0 with /IORQ before the ULA's /IOREQ pin. Every Ferranti ULA's
    /// contention handler ignores A0, so without it the ULA holds up every
    /// I/O cycle, "interrupting requests for I/O ports such as the ZX
    /// Printer" (Smith, *The ZX Spectrum ULA*, p. 250). It was added from the
    /// Issue 2 board on; the Issue 1 lacks it. The ULA still answers only even
    /// ports either way: this changes contention, not decoding.
    const SPIDER: bool = true;
}

/// ZX Spectrum 16K (1982) variant marker.
///
/// Electrically identical to the 48K but with the upper 32 KiB of RAM
/// physically absent. Reads from `$8000-$FFFF` return floating-bus
/// values; writes are dropped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Spectrum16kMarker;

impl Variant48kClass for Spectrum16kMarker {
    const MODEL_ID: &'static str = "sinclair-zx-spectrum-16k";
}

/// ZX Spectrum 48K (1982) variant marker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Spectrum48kMarker;

impl Variant48kClass for Spectrum48kMarker {
    const MODEL_ID: &'static str = "sinclair-zx-spectrum-48k";
}

/// ZX Spectrum 48K on the Issue 1 board (1982): the 5C102E ULA, with the
/// "dead cockroach" fix for its I/O wait, and no spider, so every I/O cycle
/// contends (Smith, *The ZX Spectrum ULA*, p. 250).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Spectrum48kIssue1Marker;

impl Variant48kClass for Spectrum48kIssue1Marker {
    const MODEL_ID: &'static str = "sinclair-zx-spectrum-48k-issue1";
    const SPIDER: bool = false;
}

/// ZX Spectrum 48K on the Issue 2 board (1982-83): the 5C112E ULA, with the
/// spider. Its port-254 bit 6 follows MIC as well as EAR, unlike the Issue 3's
/// 6C001E (Smith, Table 20-2, p. 220).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Spectrum48kIssue2Marker;

impl Variant48kClass for Spectrum48kIssue2Marker {
    const MODEL_ID: &'static str = "sinclair-zx-spectrum-48k-issue2";
}

/// ZX Spectrum+ (1984) variant marker.
///
/// Electrically identical to the 48K — same Ferranti ULA, same ROM,
/// same RAM, same keyboard matrix. Distinguished here for catalogue
/// identity and per-variant metadata; snapshots cannot cross between
/// the 48K and Spectrum+ at the type level.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SpectrumPlusMarker;

impl Variant48kClass for SpectrumPlusMarker {
    const MODEL_ID: &'static str = "sinclair-zx-spectrum-plus";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_markers_advertise_distinct_model_ids() {
        assert_eq!(Spectrum16kMarker::MODEL_ID, "sinclair-zx-spectrum-16k");
        assert_eq!(Spectrum48kMarker::MODEL_ID, "sinclair-zx-spectrum-48k");
        assert_eq!(SpectrumPlusMarker::MODEL_ID, "sinclair-zx-spectrum-plus");
    }
}
