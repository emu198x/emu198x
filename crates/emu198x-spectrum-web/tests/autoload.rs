//! The browser's tape autoload on a machine that has already been used
//! (#1569), through the same `WebMachine` the page drives.
//!
//! ```text
//! cargo test -p emu198x-spectrum-web --test autoload -- --ignored
//! ```

use std::path::PathBuf;

use emu198x_shell::{FamilyRuntime, FirmwareImage, FirmwareSet, MediaKind, SessionDriver};
use emu198x_spectrum_web::{SpectrumWebMachine, autoload_tape, basic_tape};
use emu198x_web::WebMachine;
use runtime_sinclair_zx_spectrum::{
    DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, Model, SpectrumRuntimeKind, SpectrumSessionQueryProvider,
    tap_key,
};

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

fn screen_shows(machine: &SpectrumWebMachine, text: &str) -> bool {
    machine
        .query("screen.text.lines")
        .expect("screen text")
        .value
        .as_array()
        .expect("lines")
        .iter()
        .filter_map(|line| line.as_str())
        .any(|line| line.contains(text))
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

#[test]
#[ignore = "FIXTURE: needs the 48K Spectrum ROM — run with --ignored"]
fn choosing_a_tape_on_a_used_machine_loads_it() {
    let mut machine = machine();

    // What the issue's learner did: start the machine and run PRINT 1.
    machine
        .wait_for_boot(DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES)
        .expect("the 48K boots");
    tap_key(&mut machine, "enter").expect("open the editor");
    machine.run_frames(20).expect("run");
    for key in ["p", "1", "enter"] {
        tap_key(&mut machine, key).expect("type");
    }
    assert!(
        run_until_screen_shows(&mut machine, "0 OK, 0:1", 100),
        "PRINT 1 did not run"
    );

    let tape = basic_tape("10 PRINT 7", "probe").expect("tape");
    machine
        .load_media_bytes("tape-1", MediaKind::Tape, &tape)
        .expect("the tape inserts");
    autoload_tape(&mut machine, DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES)
        .expect("autoload works on a machine that has already been used");

    assert!(
        run_until_screen_shows(&mut machine, "0 OK, 10:1", 2_000),
        "the tape's program did not load and run"
    );
}

#[test]
#[ignore = "FIXTURE: needs the 48K Spectrum ROM — run with --ignored"]
fn a_tape_chosen_before_the_machine_runs_boots_it_once() {
    let mut machine = machine();
    let tape = basic_tape("10 PRINT 7", "probe").expect("tape");
    machine
        .load_media_bytes("tape-1", MediaKind::Tape, &tape)
        .expect("the tape inserts");

    let boot_frames = autoload_tape(&mut machine, DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES)
        .expect("autoload from power-on");
    assert!(boot_frames > 0, "a fresh machine waits for its own boot");

    assert!(
        run_until_screen_shows(&mut machine, "0 OK, 10:1", 2_000),
        "the tape's program did not load and run"
    );
}
