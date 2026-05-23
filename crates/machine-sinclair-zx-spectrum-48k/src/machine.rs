//! ZX Spectrum 48K machine, expressed as an alias of the shared 48K-class
//! composition.
//!
//! The hardware composition (Z80 + Ferranti ULA + beeper + tape) lives in
//! [`common_sinclair_zx_spectrum_48k_class::SpectrumMachineCore`], because
//! 16K, 48K, and Spectrum+ are electrically identical apart from memory
//! size and badge. This crate is the 48K-flavoured wrapper: a type alias
//! plus the [`ApplyInputEvent`] extension trait that maps host-boundary
//! `InputEvent`s onto the keyboard matrix. Host-boundary types stay out
//! of `common-sinclair-zx-spectrum` so the shared crate keeps its dep
//! graph hardware-only.

use common_sinclair_zx_spectrum::keyboard::SpectrumKey;
use common_sinclair_zx_spectrum::memory::{MemoryBus, Spectrum48kMemory};
use common_sinclair_zx_spectrum_48k_class::{
    Spectrum48kMarker, SpectrumMachineCore, Variant48kClass,
};
use emu198x_shell::InputEvent;
use peripheral_kempston_joystick::KempstonButton;

/// Machine-local state for a stock ZX Spectrum 48K.
pub type Spectrum48k = SpectrumMachineCore<Spectrum48kMemory, Spectrum48kMarker>;

/// Maps a host-boundary [`InputEvent`] onto the Spectrum keyboard matrix.
///
/// Implemented for every 48K-class core — 16K and Spectrum+ wrappers will
/// inherit this once they're scaffolded. Lives in the 48K crate (rather
/// than the layer or common crate) because `InputEvent` is a host-shell
/// type and `common-{family}` is hardware-only by convention.
pub trait ApplyInputEvent {
    /// Applies one host input event to the keyboard matrix.
    ///
    /// Returns `true` when the event maps to a physical key.
    fn apply_input_event(&mut self, event: &InputEvent) -> bool;
}

impl<M: MemoryBus, V: Variant48kClass> ApplyInputEvent for SpectrumMachineCore<M, V> {
    fn apply_input_event(&mut self, event: &InputEvent) -> bool {
        let InputEvent::Key { name, pressed } = event else {
            return false;
        };
        let Some(key) = SpectrumKey::from_name(name.as_ref()) else {
            return false;
        };
        self.keyboard_mut().set_key(key, *pressed);
        true
    }
}

/// Maps a typed [`KempstonButton`] event onto the 48K-class Kempston
/// peripheral.
///
/// Mirrors the runtime layer's `set_kempston_button` shape but takes
/// the typed [`KempstonButton`] enum directly, for callers (tests,
/// scripted-input bindings, direct machine manipulation) that already
/// hold a typed button reference. Flips the peripheral's `attached`
/// flag on first event — software that probes `$1F` for Kempston
/// detection sees the floating bus until the user touches the pad.
///
/// **Deliberately not implemented for the Amstrad class.** The +2A /
/// +2B / +3 broke the rear-connector pinout in 1987, so a Kempston
/// interface cannot physically attach. The trait bound on
/// `Variant48kClass` (paired with the matching `Variant128kClass`
/// impl in the 128K crate) keeps this enforced at compile time —
/// trying to call `apply_kempston_event` on an Amstrad-class machine
/// will fail to resolve the trait, surfacing the architectural
/// constraint at the type system rather than at runtime.
pub trait ApplyKempstonEvent {
    /// Applies one button state change to the Kempston joystick.
    fn apply_kempston_event(&mut self, button: KempstonButton, pressed: bool);
}

impl<M: MemoryBus, V: Variant48kClass> ApplyKempstonEvent for SpectrumMachineCore<M, V> {
    fn apply_kempston_event(&mut self, button: KempstonButton, pressed: bool) {
        let kempston = self.kempston_mut();
        kempston.attached = true;
        kempston.set_button(button, pressed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_sinclair_zx_spectrum::audio::SpeakerChannel;
    use common_sinclair_zx_spectrum::keyboard::SpectrumKey;
    use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH};
    use ferranti_ula_6c001e::UlaRevision;
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
    fn machine_applies_host_key_events() {
        let mut machine = Spectrum48k::new();
        let pressed = InputEvent::Key {
            name: "q".into(),
            pressed: true,
        };

        assert!(machine.apply_input_event(&pressed));
        assert_eq!(machine.read_fe(0xfbfe) & 0x01, 0x00);
    }

    #[test]
    fn apply_input_event_returns_false_for_non_key_events() {
        let mut machine = Spectrum48k::new();
        // Button events go through the Kempston path, not the keyboard
        // matrix; the trait declines them.
        let button = InputEvent::Button {
            port: 0,
            name: "fire".into(),
            pressed: true,
        };
        assert!(!machine.apply_input_event(&button));
        // Axis events similarly aren't keyboard input.
        let axis = InputEvent::Axis {
            port: 0,
            name: "horizontal".into(),
            value: 16_000,
        };
        assert!(!machine.apply_input_event(&axis));
    }

    #[test]
    fn apply_input_event_returns_false_for_unknown_key_names() {
        let mut machine = Spectrum48k::new();
        // The Spectrum doesn't have an "escape" or "F12" key; the
        // name lookup falls through to None and the trait returns
        // false without touching the matrix.
        for unknown in ["escape", "F12", "", "πø"] {
            let event = InputEvent::Key {
                name: unknown.into(),
                pressed: true,
            };
            assert!(
                !machine.apply_input_event(&event),
                "name {unknown:?} should not match any SpectrumKey",
            );
        }
        // Matrix must remain untouched (all keys released).
        assert_eq!(machine.read_fe(0xfffe) & 0x1F, 0x1F);
    }

    #[test]
    fn apply_input_event_releases_pressed_keys() {
        let mut machine = Spectrum48k::new();
        let q_press = InputEvent::Key {
            name: "q".into(),
            pressed: true,
        };
        let q_release = InputEvent::Key {
            name: "q".into(),
            pressed: false,
        };
        assert!(machine.apply_input_event(&q_press));
        assert_eq!(machine.read_fe(0xfbfe) & 0x01, 0x00);

        assert!(machine.apply_input_event(&q_release));
        // Q released — bit reads high again.
        assert_eq!(machine.read_fe(0xfbfe) & 0x01, 0x01);
    }

    #[test]
    fn apply_kempston_event_attaches_and_flips_bits() {
        let mut machine = Spectrum48k::new();
        // Defaults: unattached, all bits clear.
        assert!(!machine.kempston_mut().attached);
        assert_eq!(machine.kempston_mut().state, 0);

        machine.apply_kempston_event(KempstonButton::Fire, true);
        assert!(
            machine.kempston_mut().attached,
            "first event must attach the interface"
        );
        assert_eq!(machine.kempston_mut().state, 0b0001_0000, "fire bit");

        machine.apply_kempston_event(KempstonButton::Right, true);
        assert_eq!(machine.kempston_mut().state, 0b0001_0001, "fire + right");

        machine.apply_kempston_event(KempstonButton::Fire, false);
        assert_eq!(
            machine.kempston_mut().state,
            0b0000_0001,
            "only right after fire release"
        );
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
        let mut machine = Spectrum48k::new();
        machine.write_fe(0x10);
        machine.set_tape_connected(true);
        machine.set_tape_level(false);
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x40);

        machine.set_tape_level(true);
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x00);
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
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x40);

        machine.advance_halfcycles(3);
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x00);

        machine.advance_halfcycles(4);
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x40);

        machine.advance_halfcycles(8);
        assert!(!machine.tape_is_playing());
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x00);
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
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x00);

        machine.set_tape_connected(true);
        machine.set_tape_level(false);
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x40);
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
    #[ignore = "requires local 48K ROM at ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom"]
    fn boot_rom_populates_screen_memory() {
        let Some(rom_path) = spectrum_48k_rom_path() else {
            eprintln!("HOME is not set; skipping ROM-backed boot smoke test");
            return;
        };

        if !rom_path.is_file() {
            eprintln!("ROM not found at {}", rom_path.display());
            return;
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
