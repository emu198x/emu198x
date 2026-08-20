//! Putting a cassette in the deck is not pressing PLAY.
//!
//! The ZX80's loader will not decode until the line has been quiet for its
//! leader countdown, and any high resets that countdown. The encoder puts
//! the quiet run at the front of the tape, so the tape has to start *after*
//! `LOAD` is typed. A runtime that threads it at load time spends the
//! lead-in during boot and typing, and the loader then waits forever.
//!
//! These tests pin the separation itself; the ROM-level proof that the
//! order matters lives in `machine-sinclair-zx80`'s `tape_load` suite.

use emu198x_shell::{
    ControlCommand, MachineCore, MediaImage, MediaKind, MediaSet, MediaTransportAction,
    MediaTransportCommand,
};
use runtime_sinclair_zx80::{Model, Zx80Runtime};

const TAPE_SLOT: &str = "tape-1";

/// A twelve-byte `.o`: the word at `$400A` has to name the image's own end.
fn image() -> Vec<u8> {
    let mut bytes = vec![0u8; 12];
    bytes[0x0A] = 0x0C;
    bytes[0x0B] = 0x40;
    bytes
}

fn runtime() -> Zx80Runtime {
    Zx80Runtime::new(Model::Zx80, vec![0u8; 4 * 1024]).expect("a 4 KB ROM should construct")
}

fn transport(action: MediaTransportAction) -> ControlCommand {
    ControlCommand::MediaTransport(MediaTransportCommand::new(TAPE_SLOT, action))
}

fn remaining(runtime: &Zx80Runtime) -> usize {
    runtime.machine().expect("machine").tape_remaining()
}

#[test]
fn loading_a_tape_does_not_start_it() {
    let mut runtime = runtime();
    let bytes = image();
    let mut media = MediaSet::new();
    media.push(MediaImage::new(TAPE_SLOT, MediaKind::Tape, &bytes));

    runtime.load_media(&media).expect("a valid .o should load");

    assert_eq!(
        remaining(&runtime),
        0,
        "the deck holds the tape; nothing should be playing yet"
    );
}

#[test]
fn pressing_play_threads_the_loaded_tape() {
    let mut runtime = runtime();
    let bytes = image();
    let mut media = MediaSet::new();
    media.push(MediaImage::new(TAPE_SLOT, MediaKind::Tape, &bytes));
    runtime.load_media(&media).expect("load");

    runtime
        .command(&transport(MediaTransportAction::Start))
        .expect("play should thread the loaded tape");
    assert!(
        remaining(&runtime) > 0,
        "pressing play should put pulses on the line"
    );

    runtime
        .command(&transport(MediaTransportAction::Stop))
        .expect("stop should lift the tape off");
    assert_eq!(remaining(&runtime), 0, "stop should leave nothing playing");
}

#[test]
fn pressing_play_with_an_empty_deck_is_an_error() {
    let mut runtime = runtime();

    runtime
        .command(&transport(MediaTransportAction::Start))
        .expect_err("there is no cassette to play");
}

#[test]
fn transport_on_an_unknown_slot_is_an_error() {
    let mut runtime = runtime();

    runtime
        .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
            "disk-1",
            MediaTransportAction::Start,
        )))
        .expect_err("a ZX80 has one cassette slot and no others");
}
