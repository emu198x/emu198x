//! Sinclair ZX Spectrum +2 (grey, Amstrad-built) machine wrapper.
//!
//! The grey +2 (1986) is electrically identical to the 128K — same
//! Sinclair 7K010E ULA, same Z80, same Memory128K, same AY-3-8912, same
//! crystal, same timing. Differences are above the chip layer:
//! `plus2-{0,1}.rom` instead of `128-{0,1}.rom`, a different copyright
//! banner ("©1986, ©1982 Amstrad Consumer Electronics plc"), and a
//! different case with built-in cassette and Sinclair Interface 2-style
//! joystick ports (joystick handling lives at the runtime/peripheral
//! layer, not in the machine).
//!
//! The hardware composition lives in
//! [`common_sinclair_zx_spectrum_128k_class::Spectrum128kClassCore`] and
//! is shared with the 128K via a phantom variant marker.

pub use common_sinclair_zx_spectrum_128k_class::{
    AmstradPlus2Marker, Memory128K, Spectrum128kClassCore,
};

/// Machine-local state for a Sinclair-branded Amstrad-built grey +2.
pub type SpectrumPlus2 = Spectrum128kClassCore<AmstradPlus2Marker>;

#[cfg(test)]
mod tests {
    use super::*;
    use common_sinclair_zx_spectrum::memory::MemoryBus;
    use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH, TIMING_128K};

    #[test]
    fn machine_reports_plus2_model_id() {
        let m = SpectrumPlus2::new();
        assert_eq!(m.model_id(), "sinclair-zx-spectrum-plus2");
    }

    #[test]
    fn machine_defaults_have_expected_shape() {
        let m = SpectrumPlus2::new();
        assert_eq!(m.framebuffer.len(), SCREEN_WIDTH * SCREEN_HEIGHT);
        assert_eq!(m.keyboard, [0xFF; 8]);
        assert_eq!(m.kempston, 0);
    }

    #[test]
    fn machine_runs_one_frame_at_128k_cadence() {
        let mut m = SpectrumPlus2::new();
        m.run_frame();
        assert_eq!(m.hc_value(), 0);
    }

    #[test]
    fn rom_loader_accepts_plus2_rom_pair() {
        let mut m = SpectrumPlus2::new();
        let mut rom0 = vec![0u8; 16384];
        rom0[0x0000] = 0xa1;
        rom0[0x3fff] = 0xa2;
        let mut rom1 = vec![0u8; 16384];
        rom1[0x0000] = 0xb1;

        m.memory.load_roms(&rom0, &rom1);
        assert_eq!(m.memory.read(0x0000), 0xa1);
        assert_eq!(m.memory.read(0x3fff), 0xa2);
        // Switch to ROM 1 (the 48 BASIC half of the +2 firmware bundle).
        m.memory.write_7ffd(0x10);
        assert_eq!(m.memory.read(0x0000), 0xb1);
    }

    #[test]
    fn paging_writes_change_bank_at_c000() {
        let mut m = SpectrumPlus2::new();
        m.memory.ram_bank_mut(0)[0] = 0x20;
        m.memory.ram_bank_mut(3)[0] = 0x23;

        assert_eq!(m.memory.read(0xC000), 0x20);
        m.memory.write_7ffd(0x03);
        assert_eq!(m.memory.read(0xC000), 0x23);
    }

    #[test]
    fn contention_map_matches_128k() {
        let mut m = SpectrumPlus2::new();
        // Bank 5 at $4000 always contended.
        assert!(m.memory.is_contended(0x4000));
        // $C000: bank 0 (even) = uncontended.
        assert!(!m.memory.is_contended(0xC000));
        // Switch to bank 1 (odd) — $C000 becomes contended.
        m.memory.write_7ffd(0x01);
        assert!(m.memory.is_contended(0xC000));
    }

    #[test]
    fn frame_halfcycles_match_128k_family() {
        let m = SpectrumPlus2::new();
        // Sanity: the +2 plugs into the same TIMING_128K cadence —
        // the frame length comes from the layer crate's SpectrumDriver
        // impl, which hardcodes TIMING_128K.
        assert_eq!(TIMING_128K.halfcycles_per_frame, 354_540);
        assert_eq!(m.framebuffer.len(), SCREEN_WIDTH * SCREEN_HEIGHT);
    }
}
