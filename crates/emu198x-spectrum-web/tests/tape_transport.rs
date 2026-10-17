//! Stopping and restarting the tape in the browser (#1794), through the same
//! `WebMachine` the page drives.
//!
//! The workflow is the one a magazine POKE asked for: load a game's BASIC
//! loader without running it, stop the tape, add a line, `RUN`, and play the
//! rest of the tape.
//!
//! ```text
//! cargo test -p emu198x-spectrum-web --test tape_transport -- --ignored
//! ```

use std::path::PathBuf;

use emu198x_shell::{FamilyRuntime, FirmwareImage, FirmwareSet, MediaKind, SessionDriver};
use emu198x_spectrum_web::{SpectrumWebMachine, autoload_tape, tape_transport, tape_turbo};
use emu198x_web::WebMachine;
use format198x_sinclair_zx_spectrum_tap::{Header, HeaderKind, TapBlock, encode};
use runtime_sinclair_zx_spectrum::{
    DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, Model, SpectrumRuntimeKind, SpectrumSessionQueryProvider,
    tap_key,
};

/// A header parameter of 32768 or more means "no auto-run" for a program, and
/// "no second parameter" for code.
const NO_AUTORUN: u16 = 32768;
const CODE_AT: u16 = 40000;
const CODE_BYTE: u8 = 42;

fn rom_path() -> PathBuf {
    std::env::var_os("EMU198X_SPECTRUM_48K_ROM").map_or_else(
        || {
            PathBuf::from(std::env::var_os("HOME").expect("HOME"))
                .join(".emu198x/roms/sinclair-zx-spectrum-48k/48.rom")
        },
        PathBuf::from,
    )
}

fn machine() -> SpectrumWebMachine {
    let rom = std::fs::read(rom_path()).expect("needs the 48K ROM");
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new("sinclair-zx-spectrum-48k-rom", &rom));
    let runtime = SpectrumRuntimeKind::from_firmware(Model::Spectrum48KPal, &firmware)
        .expect("the 48K builds from its ROM");
    WebMachine::new_with_query_provider(runtime, SpectrumSessionQueryProvider)
}

/// A game tape in miniature: a loader that does not run itself, then a code
/// block the loader fetches.
fn loader_and_code_tape() -> Vec<u8> {
    let loader = format198x_sinclair_zx_spectrum_bas::tokenise_listing(&format!(
        "10 LOAD \"\"CODE\n30 PRINT PEEK {CODE_AT}"
    ))
    .expect("loader tokenises")
    .bytes;
    let length = u16::try_from(loader.len()).expect("small loader");
    encode(&[
        Header::new(HeaderKind::Program, "loader", length, NO_AUTORUN, length).block(),
        TapBlock::data(loader),
        Header::new(HeaderKind::Code, "code", 1, CODE_AT, NO_AUTORUN).block(),
        TapBlock::data(vec![CODE_BYTE]),
    ])
}

fn screen_lines(machine: &SpectrumWebMachine) -> Vec<String> {
    machine
        .query("screen.text.lines")
        .expect("screen text")
        .value
        .as_array()
        .expect("lines")
        .iter()
        .filter_map(|line| line.as_str().map(|text| text.trim_end().to_owned()))
        .collect()
}

fn screen_shows(machine: &SpectrumWebMachine, text: &str) -> bool {
    screen_lines(machine).iter().any(|line| line.contains(text))
}

fn run_until_screen_shows(machine: &mut SpectrumWebMachine, text: &str, frames: u32) -> bool {
    for _ in 0..frames {
        if screen_shows(machine, text) {
            return true;
        }
        machine.run_one_frame().expect("frame");
    }
    screen_shows(machine, text)
}

fn tape_playing(machine: &SpectrumWebMachine) -> bool {
    machine
        .query("tape.playing")
        .expect("tape.playing")
        .value
        .as_bool()
        .expect("bool")
}

fn tape_position(machine: &SpectrumWebMachine) -> u64 {
    machine
        .query("tape.span_index")
        .expect("tape.span_index")
        .value
        .as_u64()
        .expect("index")
}

#[test]
#[ignore = "FIXTURE: needs the 48K Spectrum ROM — run with --ignored"]
fn a_stopped_tape_waits_while_a_line_is_added_then_plays_the_rest() {
    let mut machine = machine();
    machine
        .load_media_bytes("tape-1", MediaKind::Tape, &loader_and_code_tape())
        .expect("the tape inserts");
    autoload_tape(&mut machine, DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES).expect("LOAD \"\" and play");

    // The loader arrives without running, and the tape is stopped there.
    assert!(
        run_until_screen_shows(&mut machine, "0 OK, 0:1", 2_000),
        "the loader did not load: {:?}",
        screen_lines(&machine)
    );
    tape_transport(&mut machine, "tape-1", false).expect("STOP");
    assert!(!tape_playing(&machine), "STOP left the tape running");

    // Ten seconds of a person reading the magazine. A tape left running
    // would carry the code block past the machine in this time.
    let stopped_at = tape_position(&machine);
    machine.run_frames(500).expect("wait");
    assert_eq!(
        tape_position(&machine),
        stopped_at,
        "the tape moved while stopped"
    );

    // Add a line, RUN, and press PLAY for the rest of the tape.
    for key in ["2", "0", "p", "7", "enter"] {
        tap_key(&mut machine, key).expect("type the line");
    }
    // ENTER lists the program; a key pressed while it redraws is lost.
    machine.run_frames(20).expect("listing");
    for key in ["r", "enter"] {
        tap_key(&mut machine, key).expect("RUN");
    }
    tape_transport(&mut machine, "tape-1", true).expect("PLAY");
    assert!(tape_playing(&machine), "PLAY did not start the tape");

    assert!(
        run_until_screen_shows(&mut machine, "0 OK, 30:1", 3_000),
        "the program did not finish after PLAY: {:?}",
        screen_lines(&machine)
    );
    let lines = screen_lines(&machine);
    let shown: Vec<&str> = lines
        .iter()
        .map(String::as_str)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(
        shown,
        ["Bytes: code", "7", &CODE_BYTE.to_string(), "0 OK, 30:1"],
        "the code block should load after PLAY, then the added line and the loader both run"
    );
}

#[test]
#[ignore = "FIXTURE: needs the 48K Spectrum ROM — run with --ignored"]
fn transport_on_a_slot_that_is_not_a_tape_is_refused() {
    let mut machine = machine();
    let error = tape_transport(&mut machine, "drive-1", true).expect_err("not a tape deck");
    assert!(
        error.contains("drive-1"),
        "the error should name the slot: {error}"
    );
}

#[test]
#[ignore = "FIXTURE: needs the 48K Spectrum ROM — run with --ignored"]
fn the_tape_runs_fast_only_while_the_rom_is_loading_from_it() {
    let mut machine = machine();
    machine
        .load_media_bytes("tape-1", MediaKind::Tape, &loader_and_code_tape())
        .expect("the tape inserts");
    autoload_tape(&mut machine, DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES).expect("LOAD \"\" and play");
    machine.run_frames(5).expect("into the pilot tone");
    assert!(tape_turbo(&machine), "the ROM is loading: run ahead");

    // The loader is in and the ROM is back in BASIC, but the tape plays on.
    // That stretch must run in real time, or a reader cannot stop it.
    assert!(
        run_until_screen_shows(&mut machine, "0 OK, 0:1", 2_000),
        "the loader did not load"
    );
    for _ in 0..50 {
        assert!(tape_playing(&machine), "the tape should still be playing");
        assert!(
            !tape_turbo(&machine),
            "the ROM is not loading, so the tape must run in real time"
        );
        machine.run_one_frame().expect("frame");
    }
}
