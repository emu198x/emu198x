//! Fixture-free proof that the Atari 7800 starts and draws.
//!
//! Nothing pokes a framebuffer. The cartridge builds MARIA's data structures
//! in ROM and lets the chip fetch them, so a pass says the reset vector was
//! read, cartridge reads landed across `$4000`-`$FFFF`, MARIA walked a display
//! list list and its display lists over DMA, and a frame reached the screen.
//!
//! Built by `test-data/synthetic-cartridges/build-synthetic-cartridges.py`.

use std::path::PathBuf;

use emu198x_shell::{
    HostIo, MachineCore, MachineTime, NullAudioSink, NullFrameSink, NullTraceSink,
};
use runtime_atari_7800::{Atari7800Runtime, Model};

const MODEL: Model = Model::A7800Ntsc;
/// Colour clocks in one NTSC frame, near enough for a sixty-frame settle.
const FRAME_TICKS: u64 = 114 * 262;

fn cartridge() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/synthetic-cartridges/atari-7800-logo.bin");
    std::fs::read(path).expect("synthetic cartridge — run build-synthetic-cartridges.py")
}

#[test]
fn the_synthetic_cartridge_boots_and_draws_the_plate() {
    let mut runtime =
        Atari7800Runtime::new(MODEL, cartridge()).expect("the cartridge should construct");

    let (mut frames, mut audio, mut trace) = (NullFrameSink, NullAudioSink, NullTraceSink);
    let mut host = HostIo {
        input_events: &[],
        frame_sink: &mut frames,
        audio_sink: &mut audio,
        trace_sink: &mut trace,
    };
    runtime
        .run_until(MachineTime::new(FRAME_TICKS * 60), &mut host)
        .expect("sixty frames should run");

    let machine = runtime.machine().expect("a machine was constructed");
    let mut histogram: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for pixel in machine.framebuffer() {
        *histogram.entry(*pixel & 0x00FF_FFFF).or_default() += 1;
    }

    // Three colours distinguishes a drawn plate from a machine that reached
    // its background register and stopped — which is what a display list
    // MARIA never walked would leave on screen.
    assert!(
        histogram.len() >= 3,
        "only {} colours on screen — MARIA may never have walked the list",
        histogram.len()
    );

    let paper = histogram
        .iter()
        .max_by_key(|(_, count)| **count)
        .map(|(colour, _)| *colour)
        .expect("a frame was drawn");
    let filled = histogram
        .iter()
        .filter(|(c, _)| **c != paper)
        .find(|(c, _)| {
            let (r, g, b) = ((**c >> 16) & 0xFF, (**c >> 8) & 0xFF, **c & 0xFF);
            b > r + 48 && b > g + 32
        });
    assert!(
        filled.is_some(),
        "no blue on screen, so the prefix cell is not carrying the project colour"
    );
}
