//! Variant markers for the 128K-class machines.
//!
//! The Sinclair 128K ("toastrack", 1985) and the Sinclair-branded
//! Amstrad-built grey +2 (1986) share the same chip set, ULA, memory map,
//! AY, and timing. Their differences live entirely above the chip layer:
//! a different ROM bundle (`128-{0,1}.rom` vs `plus2-{0,1}.rom`) and a
//! different copyright banner ("(C) 1986 Sinclair Research Ltd" vs
//! "©1986, ©1982 Amstrad Consumer Electronics plc").
//!
//! Rather than two duplicated machine structs, the layer crate parameterises
//! [`crate::core::Spectrum128kClassCore`] over a phantom marker so the two
//! variants are *distinct types* — snapshots can't cross variants, and any
//! future divergence (e.g. a +2-only quirk) lands as a per-marker `impl`
//! block rather than enum branches at every call site.

/// Marker trait for the supported 128K-class variants.
///
/// Implemented as zero-sized phantom types — the marker contributes no
/// state to the machine, only type-level identity.
pub trait Class128kVariant: 'static {
    /// Stable hardware identifier used by the catalogue.
    const MODEL_ID: &'static str;
}

/// Sinclair 128K ("toastrack") variant marker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Sinclair128KMarker;

impl Class128kVariant for Sinclair128KMarker {
    const MODEL_ID: &'static str = "sinclair-zx-spectrum-128k";
}

/// Amstrad-built grey +2 variant marker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct AmstradPlus2Marker;

impl Class128kVariant for AmstradPlus2Marker {
    const MODEL_ID: &'static str = "sinclair-zx-spectrum-plus2";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_markers_advertise_distinct_model_ids() {
        assert_eq!(Sinclair128KMarker::MODEL_ID, "sinclair-zx-spectrum-128k");
        assert_eq!(AmstradPlus2Marker::MODEL_ID, "sinclair-zx-spectrum-plus2");
    }
}
