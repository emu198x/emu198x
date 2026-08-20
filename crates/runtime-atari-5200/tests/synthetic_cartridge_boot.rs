//! Fixture-free proof that the Atari 5200 hands over and draws.
//!
//! This machine could not start with a cartridge at all until recently. The
//! reset vector lives in the BIOS socket, and the synthetic BIOS that proves
//! the CPU runs sets a colour and spins — it never reads the start address a
//! cartridge publishes at `$BFFE`. `atari-5200-bios-handover.rom` exists to do
//! only that, and this test is the pair working together.
//!
//! Nothing pokes a framebuffer. The cartridge programs ANTIC and GTIA and lets
//! the chips draw, so a pass says the BIOS jumped through `$BFFE`, cartridge
//! reads landed across `$4000`-`$BFFF`, ANTIC fetched a display list and a
//! font over DMA, and GTIA resolved the playfield registers.
//!
//! Built by `test-data/synthetic-cartridges/build-synthetic-cartridges.py`
//! and `test-data/synthetic-firmware/build-synthetic-firmware.py`.

use std::path::PathBuf;

use emu198x_shell::{
    HostIo, MachineCore, MachineTime, NullAudioSink, NullFrameSink, NullTraceSink,
};
use runtime_atari_5200::{Atari5200Runtime, Model};

const MODEL: Model = Model::A5200Ntsc;
/// Colour clocks in one NTSC frame, near enough for a sixty-frame settle.
const FRAME_TICKS: u64 = 114 * 262;

fn test_data(relative: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data")
        .join(relative);
    std::fs::read(path).unwrap_or_else(|_| panic!("missing {relative} — run its build script"))
}

#[test]
fn the_synthetic_cartridge_boots_through_the_handover_bios() {
    let cart = test_data("synthetic-cartridges/atari-5200-logo.bin");
    let bios = test_data("synthetic-firmware/atari-5200-bios-handover.rom");

    let mut runtime =
        Atari5200Runtime::new(MODEL, cart, bios).expect("cartridge and BIOS should construct");

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

    // Three colours, because that is what distinguishes a drawn plate from a
    // machine that merely got as far as setting a background. The old BIOS
    // does exactly the latter, and a one-colour screen is what this test
    // exists to tell apart from success.
    assert!(
        histogram.len() >= 3,
        "only {} colours on screen — the display list may never have been fetched",
        histogram.len()
    );

    let paper = histogram
        .iter()
        .max_by_key(|(_, count)| **count)
        .map(|(colour, _)| *colour)
        .expect("a frame was drawn");

    // The prefix cell carries the identity colour, and Emu198x's is a blue.
    // This machine's palette holds #1c4c78 against a target of #0d4a7d, so the
    // assertion can be tighter than the NES's: a real blue, not merely bluish.
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
