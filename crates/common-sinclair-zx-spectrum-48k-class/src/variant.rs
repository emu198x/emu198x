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
