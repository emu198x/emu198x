//! Fixture-free proof that the Game Boy starts and draws.
//!
//! This machine will not run without a cartridge, and no commercial ROM can be
//! copied to a public runner — so the claim that matters most, "does it boot at
//! all", was the one CI could least often check. The cartridge here is ours
//! from source, which is what makes it committable.
//!
//! A pass means more than a picture appearing. Nothing pokes the framebuffer:
//! the cartridge runs on the CPU, uploads tiles to VRAM, writes a tile map,
//! programs the palette and switches the LCD on. So a pass says the reset
//! vector was fetched, the header parsed, ROM reads landed where the memory map
//! says, the PPU took its programming, and a frame reached the screen.
//!
//! Built by `test-data/synthetic-cartridges/build-synthetic-cartridges.py`.

mod common;

use std::path::PathBuf;

use emu198x_shell::{HostIo, MachineCore, MachineTime, MediaImage, MediaKind, MediaSet};
use runtime_nintendo_game_boy::{GameBoyRuntime, Model};

use common::null_host_buffers;

/// The three tones the plate uses, as Game Boy *palette indices*.
///
/// This machine's framebuffer holds indices, not resolved colour — the
/// `emu198x-native-video` stage maps them through `BGP` later. So these are
/// 0/2/3 rather than the greys a screenshot shows, which is the trap worth
/// naming: 0xFF, 0x55 and 0x00 all look plausible and none of them match.
const PAPER: u8 = 0;
const FILL: u8 = 2;
const INK: u8 = 3;

fn cartridge() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/synthetic-cartridges/nintendo-game-boy-logo.gb");
    std::fs::read(path).expect("synthetic cartridge — run build-synthetic-cartridges.py")
}

#[test]
fn the_synthetic_cartridge_boots_and_draws_the_plate() {
    let bytes = cartridge();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &bytes));

    let mut runtime = GameBoyRuntime::blank(Model::Dmg);
    runtime
        .load_media(&media)
        .expect("a cartridge we built ourselves should load");

    let (mut frame_sink, mut audio_sink, mut trace_sink) = null_host_buffers();
    let mut host = HostIo {
        input_events: &[],
        frame_sink: &mut frame_sink,
        audio_sink: &mut audio_sink,
        trace_sink: &mut trace_sink,
    };
    runtime
        .run_until(MachineTime::new(70_224 * 60), &mut host)
        .expect("sixty frames should run");

    let machine = runtime.machine().expect("the cartridge constructed one");
    let mut paper = 0usize;
    let mut fill = 0usize;
    let mut ink = 0usize;
    for &pixel in machine.framebuffer() {
        match pixel {
            PAPER => paper += 1,
            FILL => fill += 1,
            INK => ink += 1,
            _ => {}
        }
    }

    // All three tones have to be present, and that is the point of checking
    // tones rather than "is anything drawn". The plate is a filled prefix cell
    // beside a paper one: a cartridge that drew the frame and lost the fill,
    // or programmed the palette wrongly so both cells came out the same,
    // would still put plenty of ink on screen while losing the mark.
    assert!(ink > 0, "no frame or lettering was drawn");
    assert!(
        fill > 0,
        "the prefix cell is not filled — the plate has one cell"
    );
    assert!(
        paper > 0,
        "the screen is entirely covered; the map did not clear"
    );

    // The plate is 16x3 tiles of a 20x18 screen, so paper should dominate.
    // The upper bound catches a runaway map write, which looks like a
    // perfectly good picture until you count.
    assert!(
        paper > ink + fill,
        "the plate is a banner, not a fill: {paper} paper against {ink} ink and {fill} filled"
    );
}
