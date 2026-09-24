//! Variant markers for the 128K-class machines.
//!
//! The Toastrack and grey +2 share the memory map, AY and ULA-family
//! composition. Their ROMs and interrupt phases differ. The marker selects
//! the ULA profile both at construction and after saved-state restoration.

use sinclair_ula_7k010e::SinclairUla;

/// Marker trait for the supported 128K-class variants.
///
/// Implemented as zero-sized phantom types — the marker contributes no
/// serialized state to the machine, only identity and timing selection.
pub trait Class128kVariant: 'static {
    /// Stable hardware identifier used by the catalogue.
    const MODEL_ID: &'static str;

    /// Reattach variant-specific timing without changing serialized state.
    fn configure_ula(ula: &mut SinclairUla) {
        ula.reattach_config();
    }
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

    fn configure_ula(ula: &mut SinclairUla) {
        ula.reattach_plus2_config();
    }
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
