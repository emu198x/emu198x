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
