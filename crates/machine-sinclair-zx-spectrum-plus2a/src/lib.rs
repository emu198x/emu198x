//! ZX Spectrum +2A machine wrapper.
//!
//! 1987 Amstrad-built grey case with the 40077 gate array, 4 ROMs,
//! `$7FFD`/`$1FFD` paging, AY-3-8912, and AY-route audio. **No floppy
//! drive.** The hardware composition lives in
//! [`common_sinclair_zx_spectrum_amstrad_class::SpectrumAmstradClassCore`]
//! and is shared with the +2B (ROM revision) and +3 (built-in 3" floppy
//! drive); the phantom variant marker keeps the three as distinct types.

pub use common_sinclair_zx_spectrum_amstrad_class::{
    MemoryPlus, Plus2AMarker, SpectrumAmstradClassCore,
};

/// Machine-local state for a stock ZX Spectrum +2A.
pub type SpectrumPlus2A = SpectrumAmstradClassCore<Plus2AMarker>;

#[cfg(test)]
mod tests {
    use super::*;
    use common_sinclair_zx_spectrum::memory::MemoryBus;
    use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH};

    #[test]
    fn machine_reports_plus2a_model_id() {
        let m = SpectrumPlus2A::new();
        assert_eq!(m.model_id(), "sinclair-zx-spectrum-plus2a");
    }

    #[test]
    fn machine_defaults_have_expected_shape() {
        let m = SpectrumPlus2A::new();
        assert_eq!(m.framebuffer.len(), SCREEN_WIDTH * SCREEN_HEIGHT);
        assert_eq!(m.keyboard, [0xFF; 8]);
    }

    #[test]
    fn fdc_is_dormant_on_plus2a() {
        let m = SpectrumPlus2A::new();
        assert!(!m.fdc.enabled, "+2A has no floppy drive — FDC must stay dormant");
    }

    #[test]
    fn machine_runs_one_frame_at_plus2a_cadence() {
        let mut m = SpectrumPlus2A::new();
        m.run_frame();
        assert_eq!(m.hc_value(), 0);
    }

    #[test]
    fn rom_loader_accepts_4_rom_bundle() {
        let mut m = SpectrumPlus2A::new();
        let rom0 = vec![0xa0; 16384];
        let rom1 = vec![0xa1; 16384];
        let rom2 = vec![0xa2; 16384];
        let rom3 = vec![0xa3; 16384];

        m.memory.load_roms(&rom0, &rom1, &rom2, &rom3);
        assert_eq!(m.memory.read(0x0000), 0xa0);
        // Switch to ROM 3: $7FFD bit 4 + $1FFD bit 2.
        m.memory.write_7ffd(0x10);
        m.memory.write_1ffd(0x04);
        assert_eq!(m.memory.read(0x0000), 0xa3);
    }
}
