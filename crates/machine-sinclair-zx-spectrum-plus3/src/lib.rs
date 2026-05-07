//! ZX Spectrum +3 machine wrapper.
//!
//! 1987 Amstrad-built black case with the 40077 gate array, 4 ROMs,
//! `$7FFD`/`$1FFD` paging, AY-3-8912, **and a built-in 3" floppy drive**
//! driven by a NEC µPD765A FDC reading DSK / EDSK images. The hardware
//! composition lives in
//! [`common_sinclair_zx_spectrum_amstrad_class::SpectrumAmstradClassCore`]
//! and is shared with the +2A and +2B (no drive); the phantom variant
//! marker (`Plus3Marker`) gates the FDC's `enabled` flag and exposes
//! the disk-insertion / -ejection methods on this variant only.

pub use common_sinclair_zx_spectrum_amstrad_class::{
    MemoryPlus, Plus3Marker, SpectrumAmstradClassCore,
};
pub use nec_upd765a::DiskImage;

/// Machine-local state for a stock ZX Spectrum +3.
pub type SpectrumPlus3 = SpectrumAmstradClassCore<Plus3Marker>;

#[cfg(test)]
mod tests {
    use super::*;
    use common_sinclair_zx_spectrum::memory::MemoryBus;
    use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH};

    #[test]
    fn machine_reports_plus3_model_id() {
        let m = SpectrumPlus3::new();
        assert_eq!(m.model_id(), "sinclair-zx-spectrum-plus3");
    }

    #[test]
    fn machine_defaults_have_expected_shape() {
        let m = SpectrumPlus3::new();
        assert_eq!(m.framebuffer.len(), SCREEN_WIDTH * SCREEN_HEIGHT);
        assert_eq!(m.keyboard, [0xFF; 8]);
    }

    #[test]
    fn fdc_is_enabled_on_plus3() {
        let m = SpectrumPlus3::new();
        assert!(m.fdc.enabled, "+3 ships with FDC enabled");
    }

    #[test]
    fn machine_runs_one_frame_at_plus2a_cadence() {
        let mut m = SpectrumPlus3::new();
        m.run_frame();
        assert_eq!(m.hc_value(), 0);
    }

    #[test]
    fn paging_writes_change_bank_at_c000() {
        let mut m = SpectrumPlus3::new();
        m.memory.ram_bank_mut(0)[0] = 0x30;
        m.memory.ram_bank_mut(7)[0] = 0x37;

        assert_eq!(m.memory.read(0xC000), 0x30);
        m.memory.write_7ffd(0x07);
        assert_eq!(m.memory.read(0xC000), 0x37);
    }

    #[test]
    fn insert_and_eject_disk_round_trip() {
        let mut m = SpectrumPlus3::new();
        // Smoke test: methods exist and don't panic on a default
        // (empty) DiskImage. Real DSK images come through
        // format-amstrad-dsk in the runtime layer.
        m.insert_disk(DiskImage::default());
        m.eject_disk();
    }
}
