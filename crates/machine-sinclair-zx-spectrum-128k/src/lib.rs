//! ZX Spectrum 128K machine wrapper.
//!
//! The hardware composition (Z80 + Sinclair 7K010E ULA + Memory128K + AY
//! + beeper + tape) lives in
//! [`common_sinclair_zx_spectrum_128k_class::Spectrum128kClassCore`] and is
//! shared with the Sinclair-branded Amstrad-built grey +2. This crate
//! exposes only the 128K-flavoured type alias plus re-exports of the
//! memory map and snapshot helpers that downstream crates (runtime,
//! catalogue, snapshot import) reach through `m.memory`.
//!
//! The +2 lives in the sibling `machine-sinclair-zx-spectrum-plus2`
//! crate, which aliases the same core with a different variant marker.

pub use common_sinclair_zx_spectrum_128k_class::{
    Memory128K, Sinclair128KMarker, Spectrum128kClassCore,
};

/// Machine-local state for a Sinclair ZX Spectrum 128K.
pub type Spectrum128K = Spectrum128kClassCore<Sinclair128KMarker>;

#[cfg(test)]
mod tests {
    use super::*;
    use common_sinclair_zx_spectrum::memory::MemoryBus;
    use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH};

    #[test]
    fn machine_reports_128k_model_id() {
        let m = Spectrum128K::new();
        assert_eq!(m.model_id(), "sinclair-zx-spectrum-128k");
    }

    #[test]
    fn machine_defaults_have_expected_shape() {
        let m = Spectrum128K::new();
        assert_eq!(m.framebuffer.len(), SCREEN_WIDTH * SCREEN_HEIGHT);
        assert_eq!(m.keyboard, [0xFF; 8]);
        assert!(!m.kempston.attached, "Kempston defaults to unattached");
        assert_eq!(m.kempston.state, 0);
    }

    #[test]
    fn rom_loader_round_trip_via_memory_bus() {
        let mut m = Spectrum128K::new();
        let mut rom0 = vec![0u8; 16384];
        rom0[0x0000] = 0x11;
        rom0[0x3fff] = 0x22;
        let mut rom1 = vec![0u8; 16384];
        rom1[0x0000] = 0x33;

        m.memory.load_roms(&rom0, &rom1);
        assert_eq!(m.memory.read(0x0000), 0x11);
        assert_eq!(m.memory.read(0x3fff), 0x22);
        // Switch to ROM 1 via $7FFD bit 4.
        m.memory.write_7ffd(0x10);
        assert_eq!(m.memory.read(0x0000), 0x33);
    }
}
