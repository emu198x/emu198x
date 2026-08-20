//! Fixture-free proof that the NES starts and draws.
//!
//! The machine will not run without a cartridge, and no commercial ROM can be
//! copied to a public runner — so "does it boot at all" was the claim CI could
//! least often make. This cartridge is ours from source.
//!
//! Nothing pokes the framebuffer. The cartridge runs on the 6502, waits out
//! the PPU's two-frame warm-up, writes a palette and a nametable through
//! `$2006`/`$2007`, and enables rendering. A pass therefore says the reset
//! vector was fetched, the iNES header parsed, PRG reads landed where NROM
//! puts them, CHR fetches reached the pattern tables, and the PPU produced a
//! frame.
//!
//! Built by `test-data/synthetic-cartridges/build-synthetic-cartridges.py`.

use std::path::PathBuf;

use emu198x_shell::{
    HostIo, MachineCore, MachineTime, MediaImage, MediaKind, MediaSet, NullAudioSink,
    NullFrameSink, NullTraceSink,
};
use runtime_nintendo_nes::{Model, NesRuntime};

const NTSC_FRAME_TICKS: u64 = 341 * 262;

fn cartridge() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/synthetic-cartridges/nintendo-nes-logo.nes");
    std::fs::read(path).expect("synthetic cartridge — run build-synthetic-cartridges.py")
}

#[test]
fn the_synthetic_cartridge_boots_and_draws_the_plate() {
    let bytes = cartridge();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &bytes));

    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    runtime
        .load_media(&media)
        .expect("a cartridge we built ourselves should load");

    let (mut frames, mut audio, mut trace) = (NullFrameSink, NullAudioSink, NullTraceSink);
    let mut host = HostIo {
        input_events: &[],
        frame_sink: &mut frames,
        audio_sink: &mut audio,
        trace_sink: &mut trace,
    };
    runtime
        .run_until(MachineTime::new(NTSC_FRAME_TICKS * 60), &mut host)
        .expect("sixty frames should run");

    // Unlike the Game Boy, whose framebuffer holds palette indices, this one
    // holds resolved ARGB. Both are called "framebuffer".
    let machine = runtime.machine().expect("the cartridge constructed one");
    let mut histogram: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for pixel in machine.framebuffer() {
        *histogram.entry(*pixel & 0x00FF_FFFF).or_default() += 1;
    }

    assert!(
        histogram.len() >= 3,
        "the plate is paper, a filled cell and ink — {} colours on screen",
        histogram.len()
    );

    let (paper, paper_count) = histogram
        .iter()
        .max_by_key(|(_, count)| **count)
        .map(|(colour, count)| (*colour, *count))
        .expect("a frame was drawn");
    assert!(
        paper_count > machine.framebuffer().len() / 2,
        "paper should dominate a 16x3 plate on a 32x30 screen"
    );

    // The identity rule, not a magic number: the prefix cell carries the
    // project colour, and Emu198x's is a blue. Asserting the hue rather than
    // the palette byte means a change of entry has to stay a blue to pass,
    // which is the constraint that actually matters — the nearest colour to
    // #0d4a7d in this palette is a teal that belongs to a sibling.
    let filled = histogram
        .iter()
        .filter(|(colour, _)| **colour != paper)
        .find(|(colour, _)| {
            let (r, g, b) = (
                (**colour >> 16) & 0xFF,
                (**colour >> 8) & 0xFF,
                **colour & 0xFF,
            );
            b > r + 32 && b > g + 32
        });
    assert!(
        filled.is_some(),
        "no blue on screen, so the prefix cell is not carrying the project colour: {histogram:?}"
    );
}
