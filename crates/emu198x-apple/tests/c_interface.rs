//! The C interface behaves like `Fleet`, which the browser player drives.
//!
//! Each run is made twice, once through the exported functions and once
//! through `Fleet` directly, and the frame hashes and sample counts must
//! agree. The fixture is the repository's synthetic Game Boy cartridge, so
//! this runs without firmware; the Spectrum check runs where a ROM is staged.
#![allow(unsafe_code)]

mod common;

use std::path::PathBuf;

use common::{CMachine, c_string, empty_buffer, hash, last_error, take};
use emu198x_apple::{ffi, status};
use emu198x_fleet_web::Fleet;

const CARTRIDGE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../test-data/synthetic-cartridges/nintendo-game-boy-logo.gb"
));

/// Room for every sample a single frame produces.
const AUDIO_CAPACITY: usize = 1 << 16;

/// What one frame produced: its picture hash, how many samples it made and
/// their energy (as bits, so equal means bit-identical).
type Checkpoint = (u64, usize, u64);

fn checkpoint(pixels: &[u8], audio: &[f32]) -> Checkpoint {
    let energy: f64 = audio.iter().map(|sample| f64::from(sample.abs())).sum();
    (hash(pixels), audio.len(), energy.to_bits())
}

fn booted_game_boy() -> CMachine {
    let mut machine = CMachine::create("nintendo-game-boy", "dmg").expect("compiled variant");
    machine.boot().expect("needs no firmware");
    machine
        .load("cartridge", CARTRIDGE)
        .expect("synthetic cartridge");
    machine.configure_audio(48_000).expect("audio");
    machine
}

fn through_c(machine: &mut CMachine, frames: usize) -> Vec<Checkpoint> {
    (0..frames)
        .map(|_| {
            machine.step().expect("step");
            let audio = machine.audio(AUDIO_CAPACITY).expect("audio");
            checkpoint(&machine.pixels().expect("pixels"), &audio)
        })
        .collect()
}

fn through_fleet(fleet: &mut Fleet, frames: usize) -> Vec<Checkpoint> {
    (0..frames)
        .map(|_| {
            fleet.step().expect("step");
            let audio = fleet.audio().expect("audio");
            checkpoint(&fleet.pixels().expect("pixels"), &audio)
        })
        .collect()
}

#[test]
fn game_boy_frames_and_audio_match_fleet() {
    let mut machine = booted_game_boy();
    let mut fleet = Fleet::new("nintendo-game-boy", "dmg").expect("compiled variant");
    fleet.boot().expect("needs no firmware");
    fleet
        .load("cartridge", CARTRIDGE)
        .expect("synthetic cartridge");
    fleet.configure_audio(48_000).expect("audio");

    let (mut native, mut direct) = (Vec::new(), Vec::new());
    for pressed in [false, true, false] {
        machine.button(1, "a", pressed).expect("button");
        fleet.button("a", pressed).expect("button");
        native.extend(through_c(&mut machine, 20));
        direct.extend(through_fleet(&mut fleet, 20));
    }

    assert_eq!(native, direct);
    assert!(
        native.iter().all(|&(_, samples, _)| samples > 0),
        "every frame makes sound"
    );
    // The plate is static, so the hashes alone would also agree if both
    // sides copied a blank frame. The copied frame must carry the picture.
    let pixels = machine.pixels().expect("pixels");
    let colours: std::collections::BTreeSet<&[u8; 4]> = pixels.as_chunks::<4>().0.iter().collect();
    assert!(colours.len() > 1, "the plate has more than one shade");
}

#[test]
fn synthetic_spectrum_sound_and_picture_match_fleet() {
    let (rom_id, model) = ("sinclair-zx-spectrum-48k-rom", "spectrum_48k");
    let rom = common::beeper::beeper_rom();
    let mut machine = CMachine::create("sinclair-zx-spectrum", model).expect("compiled variant");
    machine.firmware(rom_id, &rom).expect("firmware");
    machine.boot().expect("boot");
    machine.configure_audio(48_000).expect("audio");
    let mut fleet = Fleet::new("sinclair-zx-spectrum", model).expect("compiled variant");
    fleet.firmware(rom_id, &rom).expect("firmware");
    fleet.boot().expect("boot");
    fleet.configure_audio(48_000).expect("audio");

    let native = through_c(&mut machine, 50);
    assert_eq!(native, through_fleet(&mut fleet, 50));
    assert!(
        native
            .iter()
            .all(|&(_, _, energy)| f64::from_bits(energy) > 0.0),
        "the beeper sounds in every frame"
    );
    let pictures: std::collections::BTreeSet<u64> = native.iter().map(|&(h, _, _)| h).collect();
    assert!(pictures.len() > 10, "the border stripes move");
}

#[test]
fn pixel_aspect_comes_from_the_runtime() {
    let rom = common::beeper::beeper_rom();
    let mut spectrum =
        CMachine::create("sinclair-zx-spectrum", "spectrum_48k").expect("compiled variant");
    spectrum
        .firmware("sinclair-zx-spectrum-48k-rom", &rom)
        .expect("firmware");
    spectrum.boot().expect("boot");
    let mut fleet = Fleet::new("sinclair-zx-spectrum", "spectrum_48k").expect("compiled variant");
    fleet
        .firmware("sinclair-zx-spectrum-48k-rom", &rom)
        .expect("firmware");
    fleet.boot().expect("boot");
    let aspect = spectrum.pixel_aspect().expect("aspect");
    assert_eq!(
        aspect.to_bits(),
        fleet.pixel_aspect_ratio().expect("aspect").to_bits()
    );
    assert!(
        aspect > 1.0,
        "a PAL television stretches Spectrum pixels: {aspect}"
    );
    assert!(
        (booted_game_boy().pixel_aspect().expect("aspect") - 1.0).abs() < f32::EPSILON,
        "an LCD has square pixels"
    );
}

/// Port 0 is the primary joystick, which on a Spectrum is Kempston
/// (`knowledge/decisions/joystick-port-numbering.md`). The test ROM copies
/// Kempston port $1F into the first attribute cell, so the picture changes
/// only when a press reaches that port.
#[test]
fn port_zero_drives_the_spectrum_kempston_joystick() {
    // 0000 IN A,($1F) / LD ($5800),A / JR $0000
    let mut rom = vec![0; 16 * 1024];
    rom[..7].copy_from_slice(&[0xDB, 0x1F, 0x32, 0x00, 0x58, 0x18, 0xF9]);
    let picture = |fire: Option<u8>| {
        let mut machine =
            CMachine::create("sinclair-zx-spectrum", "spectrum_48k").expect("compiled variant");
        machine
            .firmware("sinclair-zx-spectrum-48k-rom", &rom)
            .expect("firmware");
        machine.boot().expect("boot");
        if let Some(port) = fire {
            machine.button(port, "fire", true).expect("button");
        }
        for _ in 0..3 {
            machine.step().expect("step");
        }
        hash(&machine.pixels().expect("pixels"))
    };
    let idle = picture(None);
    assert_ne!(picture(Some(0)), idle, "port 0 fire reaches Kempston $1F");
    assert_eq!(
        picture(Some(1)),
        idle,
        "port 1 is Interface 2, not Kempston"
    );
}

#[test]
fn autoload_reaches_the_runtime_and_is_spectrum_only() {
    let mut spectrum =
        CMachine::create("sinclair-zx-spectrum", "spectrum_48k").expect("compiled variant");
    spectrum
        .firmware(
            "sinclair-zx-spectrum-48k-rom",
            &common::beeper::beeper_rom(),
        )
        .expect("firmware");
    spectrum.boot().expect("boot");
    let (code, message) = spectrum.autoload_tape(500).expect_err("no tape inserted");
    assert_eq!(code, status::ERROR);
    assert!(
        message.contains("tape"),
        "the runtime's own refusal: {message}"
    );

    let (code, message) = booted_game_boy()
        .autoload_tape(500)
        .expect_err("not a Spectrum");
    assert_eq!(code, status::ERROR);
    assert!(message.contains("only on the Spectrum"), "{message}");
}

#[test]
fn queries_answer_as_fleet_does() {
    let mut spectrum =
        CMachine::create("sinclair-zx-spectrum", "spectrum_48k").expect("compiled variant");
    spectrum
        .firmware(
            "sinclair-zx-spectrum-48k-rom",
            &common::beeper::beeper_rom(),
        )
        .expect("firmware");
    spectrum.boot().expect("boot");
    assert_eq!(spectrum.query("tape.playing").expect("known path"), "false");
    spectrum.load("tape-1", &print_seven_tape()).expect("tape");
    spectrum.transport("tape-1", true).expect("play");
    assert_eq!(spectrum.query("tape.playing").expect("known path"), "true");

    let (code, message) = spectrum.query("no.such.path").expect_err("unknown path");
    assert_eq!(code, status::ERROR);
    assert!(message.contains("no.such.path"), "{message}");
}

/// A two-block tape holding `10 PRINT 7`, as in the runtime's own autoload
/// test (`autoload_running_machine.rs`).
fn print_seven_tape() -> Vec<u8> {
    fn block(flag: u8, payload: &[u8]) -> Vec<u8> {
        let length = u16::try_from(payload.len() + 2).expect("small block");
        let mut block = length.to_le_bytes().to_vec();
        block.push(flag);
        block.extend_from_slice(payload);
        block.push(payload.iter().fold(flag, |sum, byte| sum ^ byte));
        block
    }
    let program = [
        0x00, 0x0A, 0x09, 0x00, 0xF5, b'7', 0x0E, 0x00, 0x00, 0x07, 0x00, 0x00, 0x0D,
    ];
    let length = u16::try_from(program.len()).expect("small program");
    let mut header = vec![0x00];
    header.extend_from_slice(b"probe     ");
    header.extend_from_slice(&length.to_le_bytes());
    header.extend_from_slice(&10u16.to_le_bytes());
    header.extend_from_slice(&length.to_le_bytes());
    let mut tape = block(0x00, &header);
    tape.extend(block(0xFF, &program));
    tape
}

#[test]
fn autoload_starts_a_tape_with_the_real_rom() {
    let Some(rom) = spectrum_rom() else {
        emu198x_test_skip::skip!("Spectrum 48K ROM not staged (EMU198X_SPECTRUM_48K_ROM)");
    };
    let mut machine =
        CMachine::create("sinclair-zx-spectrum", "spectrum_48k").expect("compiled variant");
    machine
        .firmware("sinclair-zx-spectrum-48k-rom", &rom)
        .expect("firmware");
    machine.boot().expect("boot");
    machine.load("tape-1", &print_seven_tape()).expect("tape");
    let boot_frames = machine.autoload_tape(500).expect("autoload");
    assert!(boot_frames > 0, "it waited for the ROM to boot");
}

#[test]
fn advance_runs_the_frames_wall_clock_time_owes() {
    let mut machine = booted_game_boy();
    let frame_ms = machine.frame_ms().expect("frame period");
    assert!(frame_ms > 0.0);
    assert_eq!(machine.advance(frame_ms * 3.5).expect("advance"), 3);
    assert_eq!(machine.advance(frame_ms * 0.6).expect("advance"), 1);
}

#[test]
fn a_saved_state_resumes_identically() {
    let mut machine = booted_game_boy();
    through_c(&mut machine, 30);
    let start = machine.save_state().expect("save");
    let first = through_c(&mut machine, 10);
    let after_first = machine.save_state().expect("save");
    machine.restore_state(&start).expect("restore");
    let second = through_c(&mut machine, 10);
    let after_second = machine.save_state().expect("save");
    assert_ne!(start, after_first, "ten frames change the state");
    assert_eq!(after_first, after_second);
    assert_eq!(first, second);
}

#[test]
fn audio_that_does_not_fit_waits_for_the_next_read() {
    let mut whole = booted_game_boy();
    let mut piecewise = booted_game_boy();
    let mut saw_full_read = false;
    for _ in 0..10 {
        whole.step().expect("step");
        piecewise.step().expect("step");
        let expected = whole.audio(AUDIO_CAPACITY).expect("audio");
        let mut collected = Vec::new();
        loop {
            let chunk = piecewise.audio(65).expect("audio");
            assert_eq!(chunk.len() % 2, 0, "reads keep stereo pairs together");
            if chunk.is_empty() {
                break;
            }
            saw_full_read |= chunk.len() == 64;
            collected.extend(chunk);
        }
        assert_eq!(collected, expected);
    }
    assert!(saw_full_read, "some reads were limited by the buffer");
}

#[test]
fn failures_report_a_status_and_a_message() {
    let unknown = CMachine::create("nintendo-game-boy", "no-such-model").err();
    let (code, message) = unknown.expect("unknown variant is refused");
    assert_eq!(code, status::ERROR);
    assert!(message.contains("unknown compiled variant"), "{message}");

    let mut unbooted = CMachine::create("nintendo-game-boy", "dmg").expect("compiled variant");
    let (code, message) = unbooted.step().expect_err("not booted");
    assert_eq!(code, status::ERROR);
    assert!(message.contains("boot the machine first"), "{message}");

    let mut machine = booted_game_boy();
    let (code, _) = machine.advance(f64::NAN).expect_err("not a duration");
    assert_eq!(code, status::INVALID_ARGUMENT);

    machine.step().expect("a frame to copy");
    let mut small = [0_u8; 16];
    let mut written = 0;
    // SAFETY: a live handle and a buffer valid for its stated length.
    let code = unsafe {
        ffi::emu198x_machine_pixels(
            machine.raw(),
            small.as_mut_ptr(),
            small.len(),
            &raw mut written,
        )
    };
    assert_eq!(code, status::INVALID_ARGUMENT);
    assert_eq!(written, 0);

    machine.step().expect("step");
    assert_eq!(last_error(), "", "success clears the message");
}

#[test]
fn null_arguments_are_refused_not_dereferenced() {
    let variant = c_string("dmg");
    let mut out = std::ptr::null_mut();
    // SAFETY: null is the case under test; the other pointers are valid.
    let code =
        unsafe { ffi::emu198x_machine_create(std::ptr::null(), variant.as_ptr(), &raw mut out) };
    assert_eq!(code, status::INVALID_ARGUMENT);
    assert!(out.is_null());

    // SAFETY: a null handle is the case under test.
    let code = unsafe { ffi::emu198x_machine_step(std::ptr::null_mut()) };
    assert_eq!(code, status::INVALID_ARGUMENT);

    let mut machine = booted_game_boy();
    // SAFETY: a live handle and a null name, which is the case under test.
    let code = unsafe { ffi::emu198x_machine_key(machine.raw(), std::ptr::null(), true) };
    assert_eq!(code, status::INVALID_ARGUMENT);
    machine.step().expect("the refusal left the machine usable");

    // SAFETY: null handles and buffers are documented as ignored.
    unsafe {
        ffi::emu198x_machine_free(std::ptr::null_mut());
        ffi::emu198x_buffer_free(empty_buffer());
    }
}

#[test]
fn the_catalogue_lists_compiled_variants() {
    let mut buffer = empty_buffer();
    // SAFETY: a writable buffer.
    common::check(unsafe { ffi::emu198x_catalogue(&raw mut buffer) }).expect("catalogue");
    let json = String::from_utf8(take(buffer)).expect("UTF-8");
    assert!(json.starts_with('['), "{json}");
    assert!(json.contains("\"family\":\"nintendo-game-boy\""));
    assert!(json.contains("\"id\":\"spectrum_48k\""));
}

#[test]
fn machines_tear_down_repeatedly() {
    for _ in 0..100 {
        let mut machine = booted_game_boy();
        machine.step().expect("step");
        machine.step().expect("step");
    }
}

fn spectrum_rom() -> Option<Vec<u8>> {
    let path = std::env::var_os("EMU198X_SPECTRUM_48K_ROM").map_or_else(
        || {
            std::env::var_os("HOME").map(|home| {
                PathBuf::from(home).join(".emu198x/roms/sinclair-zx-spectrum-48k/48.rom")
            })
        },
        |path| Some(PathBuf::from(path)),
    )?;
    std::fs::read(path).ok()
}

#[test]
fn spectrum_boots_and_matches_fleet() {
    let Some(rom) = spectrum_rom() else {
        emu198x_test_skip::skip!("Spectrum 48K ROM not staged (EMU198X_SPECTRUM_48K_ROM)");
    };
    let (rom_id, model) = ("sinclair-zx-spectrum-48k-rom", "spectrum_48k");
    let mut machine = CMachine::create("sinclair-zx-spectrum", model).expect("compiled variant");
    machine.firmware(rom_id, &rom).expect("firmware");
    machine.boot().expect("boot");
    machine.configure_audio(48_000).expect("audio");
    let mut fleet = Fleet::new("sinclair-zx-spectrum", model).expect("compiled variant");
    fleet.firmware(rom_id, &rom).expect("firmware");
    fleet.boot().expect("boot");
    fleet.configure_audio(48_000).expect("audio");

    let native = through_c(&mut machine, 150);
    machine.key("k", true).expect("key");
    let after_key = through_c(&mut machine, 5);
    assert_eq!(native, through_fleet(&mut fleet, 150));
    fleet.key("k", true).expect("key");
    assert_eq!(after_key, through_fleet(&mut fleet, 5));

    let pixels = machine.pixels().expect("pixels");
    let colours: std::collections::BTreeSet<&[u8; 4]> = pixels.as_chunks::<4>().0.iter().collect();
    assert!(colours.len() > 1, "the copyright screen has text on paper");
}
