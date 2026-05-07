//! ZX Spectrum +2B machine wrapper.
//!
//! 1988 Amstrad-built black case with a ROM revision over the +2A. Same
//! 40077 gate array, same 4-ROM banked memory, same `$7FFD`/`$1FFD`
//! paging, same AY-3-8912, same crystal, same timing. **No floppy
//! drive.** The hardware composition lives in
//! [`common_sinclair_zx_spectrum_amstrad_class::SpectrumAmstradClassCore`]
//! and is shared with the +2A and +3; the phantom variant marker keeps
//! the three as distinct types.

pub use common_sinclair_zx_spectrum_amstrad_class::{
    MemoryPlus, Plus2BMarker, SpectrumAmstradClassCore,
};

/// Machine-local state for a stock ZX Spectrum +2B.
pub type SpectrumPlus2B = SpectrumAmstradClassCore<Plus2BMarker>;

#[cfg(test)]
mod tests {
    use super::*;
    use common_sinclair_zx_spectrum::memory::MemoryBus;
    use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH};

    #[test]
    fn machine_reports_plus2b_model_id() {
        let m = SpectrumPlus2B::new();
        assert_eq!(m.model_id(), "sinclair-zx-spectrum-plus2b");
    }

    #[test]
    fn machine_defaults_have_expected_shape() {
        let m = SpectrumPlus2B::new();
        assert_eq!(m.framebuffer.len(), SCREEN_WIDTH * SCREEN_HEIGHT);
        assert_eq!(m.keyboard, [0xFF; 8]);
    }

    #[test]
    fn fdc_is_dormant_on_plus2b() {
        let m = SpectrumPlus2B::new();
        assert!(!m.fdc.enabled, "+2B has no floppy drive — FDC must stay dormant");
    }

    #[test]
    fn machine_runs_one_frame_at_plus2b_cadence() {
        let mut m = SpectrumPlus2B::new();
        m.run_frame();
        assert_eq!(m.hc_value(), 0);
    }

    #[test]
    fn paging_writes_change_bank_at_c000() {
        let mut m = SpectrumPlus2B::new();
        m.memory.ram_bank_mut(0)[0] = 0x20;
        m.memory.ram_bank_mut(3)[0] = 0x23;

        assert_eq!(m.memory.read(0xC000), 0x20);
        m.memory.write_7ffd(0x03);
        assert_eq!(m.memory.read(0xC000), 0x23);
    }
}
