//! ZX Spectrum 48K machine, expressed as an alias of the shared 48K-class
//! composition.
//!
//! The hardware composition (Z80 + Ferranti ULA + beeper + tape) lives in
//! [`common_sinclair_zx_spectrum_48k_class::SpectrumMachineCore`], because
//! 16K, 48K, and Spectrum+ are electrically identical apart from memory
//! size and badge. This crate is the 48K-flavoured wrapper: a type alias
//! with input adapters supplied by the family runtime. Host-boundary types
//! stay out of the hardware-only board implementation.

use common_sinclair_zx_spectrum::memory::Spectrum48kMemory;
use common_sinclair_zx_spectrum_48k_class::{
    Spectrum48kIssue1Marker, Spectrum48kIssue2Marker, Spectrum48kMarker, SpectrumMachineCore,
};

/// Machine-local state for a stock ZX Spectrum 48K.
pub type Spectrum48k = SpectrumMachineCore<Spectrum48kMemory, Spectrum48kMarker>;

/// A ZX Spectrum 48K on the Issue 1 board: 5C ULA, no spider, so every I/O
/// cycle contends. Build it with the 5C ULA, `UlaRevision::Ferranti5C`.
pub type Spectrum48kIssue1 = SpectrumMachineCore<Spectrum48kMemory, Spectrum48kIssue1Marker>;

/// A ZX Spectrum 48K on the Issue 2 board: 5C ULA, with the spider. Build it
/// with `UlaRevision::Ferranti5C`.
pub type Spectrum48kIssue2 = SpectrumMachineCore<Spectrum48kMemory, Spectrum48kIssue2Marker>;

#[cfg(test)]
mod tests {
    use super::*;
    use common_sinclair_zx_spectrum::audio::SpeakerChannel;
    use common_sinclair_zx_spectrum::keyboard::SpectrumKey;
    use common_sinclair_zx_spectrum::memory::MemoryBus;
    use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH};
    use emu198x_ferranti_ula_6c001e::UlaRevision;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn machine_defaults_to_ferranti6c() {
        let machine = Spectrum48k::new();

        assert_eq!(machine.revision(), UlaRevision::Ferranti6C);
        assert_eq!(machine.border_color(), 7);
        assert_eq!(machine.framebuffer().len(), SCREEN_WIDTH * SCREEN_HEIGHT);
        assert_eq!(machine.read_fe(0xfffe), 0xbf);
    }

    #[test]
    fn machine_loads_rom_and_exposes_memory_bus() {
        let mut machine = Spectrum48k::with_revision(UlaRevision::Ferranti6C);
        let rom = [0xa5; 16 * 1024];

        machine
            .load_rom_bytes(&rom)
            .expect("16 KiB ROM image should load");
        machine.write(0x8000, 0x42);

        assert_eq!(machine.read(0x0000), 0xa5);
        assert_eq!(machine.read(0x3fff), 0xa5);
        assert_eq!(machine.read(0x8000), 0x42);
    }

    #[test]
    fn machine_runs_frame_without_rom() {
        let mut machine = Spectrum48k::new();
        machine.run_frame();

        assert!(machine.z80().regs.pc > 0 || machine.z80().halt);
        assert_eq!(machine.hc(), 0);
    }

    #[test]
    fn machine_exposes_revision_specific_feedback() {
        let mut ferranti5c = Spectrum48k::with_revision(UlaRevision::Ferranti5C);
        let mut ferranti6c = Spectrum48k::with_revision(UlaRevision::Ferranti6C);

        ferranti5c.write_fe(0x08);
        ferranti6c.write_fe(0x08);

        assert_eq!(ferranti5c.read_fe(0xfffe) & 0x40, 0x40);
        assert_eq!(ferranti6c.read_fe(0xfffe) & 0x40, 0x00);
    }

    #[test]
    fn connected_tape_input_overrides_feedback() {
        // MIC alone reads back as 1 on an Issue 2 ULA, so a low tape
        // input visibly overrides it.
        let mut machine = Spectrum48k::with_revision(UlaRevision::Ferranti5C);
        machine.write_fe(0x08);
        machine.set_tape_connected(true);
        machine.set_tape_level(false);
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x00);

        machine.set_tape_level(true);
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x40);
    }

    #[test]
    fn stopped_tape_does_not_override_ula_feedback() {
        let mut machine = Spectrum48k::new();

        machine.write_fe(0x10);
        machine.load_tape_pulses(vec![1, 1, 1]);
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x40);

        machine.write_fe(0x00);
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x00);
    }

    #[test]
    fn emulated_tape_advances_on_tstate_boundaries() {
        let mut machine = Spectrum48k::new();

        machine.load_tape_pulses(vec![1, 1, 2]);
        machine.play_tape();
        assert!(machine.tape_is_loaded());
        assert!(machine.tape_is_playing());
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x00);

        machine.advance_halfcycles(3);
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x40);

        machine.advance_halfcycles(4);
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x00);

        machine.advance_halfcycles(8);
        assert!(!machine.tape_is_playing());
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x40);
    }

    #[test]
    fn advance_tstates_tracks_frame_tstate_position() {
        let mut machine = Spectrum48k::new();

        machine.advance_tstates(7);
        assert_eq!(machine.tstate_in_frame(), 7);

        machine.advance_tstates(5);
        assert_eq!(machine.tstate_in_frame(), 12);
    }

    #[test]
    fn external_tape_input_overrides_emulated_tape() {
        let mut machine = Spectrum48k::new();

        machine.load_tape_pulses(vec![1, 2]);
        machine.play_tape();
        machine.advance_halfcycles(3);
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x40);

        machine.set_tape_connected(true);
        machine.set_tape_level(false);
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x00);
    }

    #[test]
    fn beeper_audio_is_emitted_per_frame() {
        let mut machine = Spectrum48k::new();

        machine.write_fe(0x10);
        machine.run_frame();

        assert!(machine.audio_samples_per_frame() > 0);
        assert!(machine.audio_frame().iter().any(|&sample| sample > 0.0));
    }

    #[test]
    fn audio_controls_proxy_to_beeper() {
        let mut machine = Spectrum48k::new();

        machine.set_audio_channel_enabled(SpeakerChannel::Speaker, false);
        machine.set_audio_channel_gain(SpeakerChannel::Speaker, 0.25);

        let controls = machine.audio_controls();
        assert!(!controls.channel(SpeakerChannel::Speaker).enabled());
        assert_eq!(controls.channel(SpeakerChannel::Speaker).gain(), 0.25);
    }

    #[test]
    fn machine_allows_direct_keyboard_access_for_tests() {
        let mut machine = Spectrum48k::new();
        machine.keyboard_mut().press_key(SpectrumKey::Enter);

        assert_eq!(machine.read_fe(0xbffe) & 0x01, 0x00);
    }

    #[test]
    #[ignore = "FIXTURE: requires local 48K ROM at ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom"]
    fn boot_rom_populates_screen_memory() {
        let Some(rom_path) = spectrum_48k_rom_path() else {
            emu198x_test_skip::skip!("HOME is not set; skipping ROM-backed boot smoke test");
        };

        if !rom_path.is_file() {
            emu198x_test_skip::skip!("48K ROM not staged at {}", rom_path.display());
        }

        let rom = match fs::read(&rom_path) {
            Ok(rom) => rom,
            Err(err) => panic!("failed to read {}: {err}", rom_path.display()),
        };

        let mut machine = Spectrum48k::new();
        machine
            .load_rom_bytes(&rom)
            .expect("48K ROM path should contain a 16 KiB image");
        machine.reset();

        for _ in 0..200 {
            machine.run_frame();
        }

        let pixel_non_zero = (0x4000u16..=0x57ff)
            .filter(|&addr| machine.read(addr) != 0)
            .count();
        let attribute_non_zero = (0x5800u16..=0x5aff)
            .filter(|&addr| machine.read(addr) != 0)
            .count();

        assert!(pixel_non_zero > 0, "expected boot ROM to draw pixel data");
        assert!(
            attribute_non_zero > 0,
            "expected boot ROM to program attribute memory"
        );
    }

    fn spectrum_48k_rom_path() -> Option<PathBuf> {
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(".emu198x/roms/sinclair-zx-spectrum-48k/48.rom"))
    }
}
