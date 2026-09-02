//! The generic host layer against a real machine.
//!
//! ```text
//! cargo test -p emu198x-web --test real_runtime -- --ignored
//! ```
//!
//! Gated `#[ignore]` because it needs the 48K ROM at
//! `~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom`.
//!
//! The unit tests in this crate drive a pacer and a frame sink in isolation,
//! which proves the arithmetic but not the wiring: a synthetic fixture agrees
//! with whatever the code assumes. This one boots a real Spectrum through
//! `WebMachine` and checks the two things a browser host has to get right —
//! that a second of wall time runs a second of machine, and that frames come
//! out looking like the machine rather than like an empty buffer.

use std::fs;
use std::path::PathBuf;

use emu198x_shell::{FamilyRuntime, FirmwareImage, FirmwareSet};
use emu198x_web::WebMachine;
use runtime_sinclair_zx_spectrum::{Model, SpectrumRuntimeKind};

fn rom() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".emu198x/roms/sinclair-zx-spectrum-48k/48.rom");
    fs::read(path).ok()
}

fn spectrum(rom: &[u8]) -> SpectrumRuntimeKind {
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new("sinclair-zx-spectrum-48k-rom", rom));
    SpectrumRuntimeKind::from_firmware(Model::Spectrum48KPal, &firmware)
        .expect("the 48K builds from its ROM")
}

#[test]
#[ignore = "FIXTURE: needs the 48K Spectrum ROM — run with --ignored"]
fn a_second_of_wall_time_runs_a_second_of_machine() {
    let rom = rom().expect("needs ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom");
    let mut machine = WebMachine::new(spectrum(&rom));

    // The PAL Spectrum frame is 19.968 ms, so ~50.08 frames per second.
    assert!(
        (19.9..20.1).contains(&machine.frame_ms()),
        "frame length came out as {} ms",
        machine.frame_ms()
    );

    // Sixty animation callbacks, one second of wall time.
    let mut frames = 0;
    for _ in 0..60 {
        frames += machine.advance(1000.0 / 60.0).expect("the machine runs");
    }

    assert!(
        (50..=51).contains(&frames),
        "one second of 60 Hz callbacks ran {frames} machine frames; \
         a Spectrum runs ~50. Running 60 would be a 20% fast machine."
    );
}

#[test]
#[ignore = "FIXTURE: needs the 48K Spectrum ROM — run with --ignored"]
fn the_booted_machine_produces_a_real_picture() {
    let rom = rom().expect("needs ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom");
    let mut machine = WebMachine::new(spectrum(&rom));

    for _ in 0..250 {
        machine.run_one_frame().expect("the machine runs");
    }

    let (width, height) = machine.frame_size();
    assert_eq!(
        (width, height),
        (352, 296),
        "the 48K frame is 352x296 including borders"
    );

    let pixels = machine.frame_rgba();
    assert_eq!(
        pixels.len(),
        (width as usize) * (height as usize) * 4,
        "RGBA is four bytes a pixel"
    );

    let (rgba, remainder) = pixels.as_chunks::<4>();
    assert!(
        remainder.is_empty(),
        "the buffer is not a whole number of pixels"
    );

    // A boot screen has a light paper area and dark copyright text, so a
    // uniform buffer means the frame sink produced nothing real.
    let first = rgba[0];
    assert!(
        rgba.iter().any(|px| *px != first),
        "every pixel is identical, so no picture was drawn"
    );

    // Alpha must be opaque throughout, or the canvas composites the page
    // through the machine's picture.
    assert!(
        rgba.iter().all(|px| px[3] == 0xFF),
        "a transparent pixel reached the canvas"
    );
}

#[test]
#[ignore = "FIXTURE: needs the 48K Spectrum ROM — run with --ignored"]
fn every_mapped_dom_code_names_a_key_the_spectrum_has() {
    let rom = rom().expect("needs ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom");
    let machine = WebMachine::new(spectrum(&rom));

    // The mapper is generic and the machine is the authority, so the pairing
    // is only correct if the real keyboard accepts everything it emits. A
    // mapping that produces a plausible-looking name the machine rejects
    // would fail silently as a dead key.
    let codes = [
        "KeyA",
        "KeyM",
        "KeyZ",
        "Digit0",
        "Digit7",
        "Digit9",
        "Numpad3",
        "Space",
        "Enter",
        "NumpadEnter",
        "ArrowUp",
        "ArrowDown",
        "ArrowLeft",
        "ArrowRight",
        "Backspace",
    ];

    for code in codes {
        let name = emu198x_web::dom_code_to_key_name(code)
            .unwrap_or_else(|| panic!("{code} should map to a key name"));
        assert!(
            machine.accepts_key(name),
            "{code} maps to {name:?}, which the Spectrum does not recognise"
        );
    }
}

#[test]
#[ignore = "FIXTURE: needs the 48K Spectrum ROM — run with --ignored"]
fn a_keypress_reaches_the_machine_once_and_only_once() {
    let rom = rom().expect("needs ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom");
    let mut machine = WebMachine::new(spectrum(&rom));

    assert!(machine.key_event("KeyA", true), "A is a Spectrum key");
    assert_eq!(machine.pending_input().len(), 1);

    machine.run_one_frame().expect("the machine runs");

    assert!(
        machine.pending_input().is_empty(),
        "a queued key that is not drained replays on every later frame, \
         so one keypress becomes a held key"
    );
}

#[test]
#[ignore = "FIXTURE: needs the 48K Spectrum ROM — run with --ignored"]
fn keys_the_machine_does_not_have_are_refused_rather_than_queued() {
    let rom = rom().expect("needs ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom");
    let mut machine = WebMachine::new(spectrum(&rom));

    // No machine-neutral name.
    assert!(!machine.key_event("F13", true));
    // A name no Spectrum key answers to.
    assert!(!machine.queue_key("Meta", true));

    assert!(
        machine.pending_input().is_empty(),
        "a refused key must queue nothing"
    );

    // But the machine's own compound names do work, which is how a binding
    // reaches CapsShift and SymbolShift.
    assert!(
        machine.queue_key("CapsShift", true),
        "CapsShift is a Spectrum key"
    );
    assert_eq!(machine.pending_input().len(), 1);
}

#[test]
#[ignore = "FIXTURE: needs the 48K Spectrum ROM — run with --ignored"]
fn a_cursor_key_becomes_the_chord_the_hardware_needs() {
    let rom = rom().expect("needs ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom");
    let mut machine = WebMachine::new(spectrum(&rom));

    // The Spectrum has no cursor keys: Up is CapsShift + 7. Queueing the bare
    // name would be accepted by nothing and the key would simply be dead, so
    // the host has to expand it into the chord the machine actually scans.
    assert!(machine.key_event("ArrowUp", true), "ArrowUp is reachable");
    assert!(
        machine.pending_input().len() > 1,
        "Up queued {} event(s); a compound key must expand into its chord",
        machine.pending_input().len()
    );

    machine.run_one_frame().expect("the machine runs");
    assert!(machine.pending_input().is_empty());

    // Releasing unwinds the chord in the opposite order, as a hand would.
    assert!(machine.key_event("ArrowUp", false));
    assert!(machine.pending_input().len() > 1);
}

#[test]
#[ignore = "FIXTURE: needs the 48K Spectrum ROM — run with --ignored"]
fn a_second_of_machine_produces_a_second_of_audio() {
    let rom = rom().expect("needs ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom");
    let mut machine = WebMachine::new(spectrum(&rom));

    // Deep enough that nothing is dropped, so the count means what it says.
    machine.configure_audio(48_000, 1, 1_000_000);

    // ~50 frames is one second of Spectrum.
    for _ in 0..50 {
        machine.run_one_frame().expect("the machine runs");
    }

    let samples = machine.audio_drain();
    assert_eq!(
        machine.audio().dropped(),
        0,
        "the buffer was too shallow to measure"
    );

    // One second at 48 kHz mono. The machine's own rate is resampled to the
    // graph's, so a wrong conversion shows up here as audio that would play
    // at the wrong pitch rather than as an error.
    let expected = 48_000_f64;
    let ratio = samples.len() as f64 / expected;
    assert!(
        (0.95..1.05).contains(&ratio),
        "one second of machine produced {} samples, expected ~{expected} \
         (ratio {ratio:.3}) — audio would play at the wrong pitch",
        samples.len()
    );
}

#[test]
#[ignore = "FIXTURE: needs the 48K Spectrum ROM — run with --ignored"]
fn a_muted_machine_buffers_nothing() {
    let rom = rom().expect("needs ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom");
    let mut machine = WebMachine::new(spectrum(&rom));
    machine.set_audio_enabled(false);

    for _ in 0..50 {
        machine.run_one_frame().expect("the machine runs");
    }

    assert!(
        machine.audio().is_empty(),
        "a muted machine buffered {} samples",
        machine.audio().len()
    );
}
