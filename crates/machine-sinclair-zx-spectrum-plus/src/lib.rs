//! ZX Spectrum+ machine wrapper.
//!
//! The Sinclair Spectrum+ (1984) is **electrically identical** to the
//! 48K — same Ferranti 6C001E ULA, same Z80, same 16 KiB ROM, same
//! 48 KiB RAM, same Issue 2/3 keyboard matrix. The differences are
//! cosmetic: a full-stroke keyboard with extra keys, a reset button,
//! and a slightly different case. Software-wise it boots the 48K ROM
//! and shows the same banner.
//!
//! This crate exists for catalogue identity rather than emulation
//! difference: the Spectrum+ gets its own variant entry in the SOLID
//! catalogue so any future drift between the two — accidentally or
//! deliberately introduced — surfaces as a per-variant test failure
//! rather than going unnoticed.
//!
//! The hardware composition lives in
//! [`common_sinclair_zx_spectrum_48k_class::SpectrumMachineCore`] and
//! is shared with the 48K and 16K. Because the Spectrum+ shares the
//! 48K's exact memory layout, the type alias is identical to the 48K's
//! — there is no phantom variant marker. Catalogue distinctness comes
//! from the runtime's `Model::SpectrumPlus` profile entry.

pub use common_sinclair_zx_spectrum::memory::Spectrum48kMemory;
pub use common_sinclair_zx_spectrum_48k_class::{SpectrumMachineCore, TapeInput};
pub use ferranti_ula_6c001e::BoardIssue;

/// Machine-local state for a Sinclair ZX Spectrum+.
///
/// Identical to `Spectrum48k` at the type level — both are
/// `SpectrumMachineCore<Spectrum48kMemory>`. The runtime layer keeps the
/// two distinct via separate `Model` enum entries and separate runtime
/// aliases.
pub type SpectrumPlus = SpectrumMachineCore<Spectrum48kMemory>;

#[cfg(test)]
mod tests {
    use super::*;
    use common_sinclair_zx_spectrum::memory::MemoryBus;
    use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH};

    #[test]
    fn machine_defaults_to_issue3() {
        let m = SpectrumPlus::new();
        assert_eq!(m.issue(), BoardIssue::Issue3);
        assert_eq!(m.framebuffer().len(), SCREEN_WIDTH * SCREEN_HEIGHT);
    }

    #[test]
    fn machine_runs_one_frame() {
        let mut m = SpectrumPlus::new();
        m.run_frame();
        assert_eq!(m.hc(), 0);
    }

    #[test]
    fn rom_loader_round_trip() {
        let mut m = SpectrumPlus::new();
        let mut rom = vec![0u8; 16 * 1024];
        rom[0x0000] = 0xc0;
        rom[0x3fff] = 0xc1;
        m.load_rom_bytes(&rom).expect("16 KiB ROM image should load");
        assert_eq!(m.read(0x0000), 0xc0);
        assert_eq!(m.read(0x3fff), 0xc1);
    }

    #[test]
    fn ram_writes_cover_full_upper_48k() {
        // Spectrum+ has the full 48K RAM map (unlike the 16K).
        let mut m = SpectrumPlus::new();
        m.write(0x4000, 0x10);
        m.write(0x8000, 0x80);
        m.write(0xffff, 0xff);
        assert_eq!(m.read(0x4000), 0x10);
        assert_eq!(m.read(0x8000), 0x80);
        assert_eq!(m.read(0xffff), 0xff);
    }
}
