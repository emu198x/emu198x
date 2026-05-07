//! Variant markers for the Amstrad-built Spectrum variants.
//!
//! The +2A (1987), +2B (1988 ROM revision), and +3 (1987, with built-in
//! 3" floppy drive) all share the same Amstrad 40077 gate array, the same
//! 4-ROM banked memory layout, the same `$7FFD`/`$1FFD` paging, the same
//! AY-3-8912, the same crystal, and the same timing. Their differences:
//!
//! - +2A and +2B are identical hardware; only the ROM revision differs.
//! - +3 adds a NEC µPD765A floppy disk controller and a `disk-a` media
//!   slot.
//!
//! Phantom marker types parameterise [`crate::core::SpectrumAmstradClassCore`]
//! so the three variants are *distinct types*. Snapshots can't cross
//! variants, the runtime's disk-slot dispatch differentiates at the type
//! level rather than runtime field, and any future divergence (e.g. a
//! +2B-only quirk) lands as a per-marker `impl` block.
//!
//! The `HAS_FDC` const gates the FDC chip's `enabled` flag inside the
//! core. **The FDC currently lives inside the core for back-compat with
//! the pre-extraction `-plus` crate.** It should move to a peripheral
//! later — see `wiki/decisions/spectrum-joystick-architecture.md` for
//! the equivalent peripheral-extraction reasoning that applies to the
//! FDC. For now the field stays on the core and the marker controls
//! whether it's enabled, matching the pre-extraction behaviour exactly.

/// Marker trait for the supported Amstrad-built Spectrum variants.
pub trait AmstradVariant: 'static {
    /// Stable hardware identifier used by the catalogue.
    const MODEL_ID: &'static str;

    /// Does this variant ship a built-in floppy drive?
    /// Only the +3 ships the µPD765A; +2A and +2B reuse the FDC chip
    /// instance with `enabled = false` so its `claims_port` always
    /// reports false and the bus dispatch never lands on it.
    const HAS_FDC: bool = false;

    /// Does this variant expose a `disk-a` media slot?
    /// Only the +3 has a floppy drive; the others reject disk media
    /// at the runtime layer.
    const HAS_DISK_SLOT: bool = false;
}

/// ZX Spectrum +2A (1987, 4 ROMs, no disk) variant marker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Plus2AMarker;

impl AmstradVariant for Plus2AMarker {
    const MODEL_ID: &'static str = "sinclair-zx-spectrum-plus2a";
}

/// ZX Spectrum +2B (1988 ROM revision, no disk) variant marker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Plus2BMarker;

impl AmstradVariant for Plus2BMarker {
    const MODEL_ID: &'static str = "sinclair-zx-spectrum-plus2b";
}

/// ZX Spectrum +3 (1987, 3" floppy drive) variant marker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Plus3Marker;

impl AmstradVariant for Plus3Marker {
    const MODEL_ID: &'static str = "sinclair-zx-spectrum-plus3";
    const HAS_FDC: bool = true;
    const HAS_DISK_SLOT: bool = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_markers_advertise_distinct_model_ids() {
        assert_eq!(Plus2AMarker::MODEL_ID, "sinclair-zx-spectrum-plus2a");
        assert_eq!(Plus2BMarker::MODEL_ID, "sinclair-zx-spectrum-plus2b");
        assert_eq!(Plus3Marker::MODEL_ID, "sinclair-zx-spectrum-plus3");
    }

    #[test]
    fn fdc_enabled_only_on_plus3() {
        const { assert!(!Plus2AMarker::HAS_FDC) };
        const { assert!(!Plus2BMarker::HAS_FDC) };
        const { assert!(Plus3Marker::HAS_FDC) };
    }

    #[test]
    fn disk_slot_exposed_only_on_plus3() {
        const { assert!(!Plus2AMarker::HAS_DISK_SLOT) };
        const { assert!(!Plus2BMarker::HAS_DISK_SLOT) };
        const { assert!(Plus3Marker::HAS_DISK_SLOT) };
    }
}
