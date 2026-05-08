//! ZX Spectrum 16K machine, expressed as an alias of the shared 48K-class
//! composition over a 16 KiB-RAM memory map.
//!
//! The 16K differs from the 48K solely in its memory map: 16 KiB ROM at
//! `$0000-$3FFF`, 16 KiB RAM at `$4000-$7FFF`, and an electrically
//! disconnected upper 32 KiB at `$8000-$FFFF` (reads return `$FF`, writes
//! are silently dropped). Everything else — Ferranti ULA, Z80, beeper,
//! keyboard, tape, framebuffer, timing — is the 48K-class core.

use common_sinclair_zx_spectrum::memory::Spectrum16kMemory;
use common_sinclair_zx_spectrum_48k_class::{Spectrum16kMarker, SpectrumMachineCore};

/// Machine-local state for a stock ZX Spectrum 16K.
pub type Spectrum16K = SpectrumMachineCore<Spectrum16kMemory, Spectrum16kMarker>;

#[cfg(test)]
mod tests {
    use super::*;
    use common_sinclair_zx_spectrum::memory::MemoryBus;
    use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH};
    use ferranti_ula_6c001e::BoardIssue;

    #[test]
    fn machine_defaults_to_issue3() {
        let machine = Spectrum16K::new();

        assert_eq!(machine.issue(), BoardIssue::Issue3);
        assert_eq!(machine.border_color(), 7);
        assert_eq!(machine.framebuffer().len(), SCREEN_WIDTH * SCREEN_HEIGHT);
        assert_eq!(machine.read_fe(0xfffe), 0xbf);
    }

    #[test]
    fn rom_writes_are_ignored_in_lower_16k() {
        let mut machine = Spectrum16K::new();

        machine.write(0x0001, 0xaa);
        assert_eq!(machine.read(0x0001), 0x00);
    }

    #[test]
    fn ram_reads_and_writes_only_lower_16k() {
        let mut machine = Spectrum16K::new();

        machine.write(0x4000, 0x11);
        machine.write(0x5fff, 0x22);
        machine.write(0x7fff, 0x33);

        assert_eq!(machine.read(0x4000), 0x11);
        assert_eq!(machine.read(0x5fff), 0x22);
        assert_eq!(machine.read(0x7fff), 0x33);
    }

    #[test]
    fn upper_address_space_reads_return_ff_when_disconnected() {
        let machine = Spectrum16K::new();

        // $8000-$FFFF is electrically disconnected on the 16K.
        assert_eq!(machine.read(0x8000), 0xff);
        assert_eq!(machine.read(0xc000), 0xff);
        assert_eq!(machine.read(0xffff), 0xff);
    }

    #[test]
    fn upper_address_space_writes_are_silently_dropped() {
        let mut machine = Spectrum16K::new();

        machine.write(0x8000, 0xaa);
        machine.write(0xc000, 0xbb);
        machine.write(0xffff, 0xcc);

        // Writes do not panic and do not bleed into lower RAM.
        assert_eq!(machine.read(0x8000), 0xff);
        assert_eq!(machine.read(0xffff), 0xff);
        assert_eq!(machine.read(0x4000), 0x00);
    }

    #[test]
    fn machine_loads_rom_image() {
        let mut machine = Spectrum16K::with_issue(BoardIssue::Issue3);
        let rom = [0xa5; 16 * 1024];

        machine
            .load_rom_bytes(&rom)
            .expect("16 KiB ROM image should load");

        assert_eq!(machine.read(0x0000), 0xa5);
        assert_eq!(machine.read(0x3fff), 0xa5);
    }

    #[test]
    fn machine_runs_frame_without_rom() {
        let mut machine = Spectrum16K::new();
        machine.run_frame();

        assert!(machine.z80().regs.pc > 0 || machine.z80().halt);
        assert_eq!(machine.hc(), 0);
    }

    #[test]
    fn contention_map_matches_48k_for_lower_ram_bank() {
        let machine = Spectrum16K::new();

        assert!(!machine.is_contended(0x3fff));
        assert!(machine.is_contended(0x4000));
        assert!(machine.is_contended(0x7fff));
        assert!(!machine.is_contended(0x8000));
        assert!(!machine.is_contended(0xffff));
    }

    #[test]
    fn machine_exposes_issue_specific_feedback() {
        let mut issue2 = Spectrum16K::with_issue(BoardIssue::Issue2);
        let mut issue3 = Spectrum16K::with_issue(BoardIssue::Issue3);

        issue2.write_fe(0x08);
        issue3.write_fe(0x08);

        assert_eq!(issue2.read_fe(0xfffe) & 0x40, 0x40);
        assert_eq!(issue3.read_fe(0xfffe) & 0x40, 0x00);
    }
}
