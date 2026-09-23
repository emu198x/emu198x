//! Spectrum 48K diagnostic screens compared with external Spectron oracles.
//!
//! Each test boots, types `LOAD ""`, plays its local TAP through the
//! cycle-accurate pipeline and runs to the diagnostic screen. HALT2INT,
//! btime, EIHALT and the completed FloatSpy self-test compare the entire
//! 256×192 active screen after palette normalisation and border alignment.
//! HALT2INT also asserts the decoded `HALT: Early` and `Float: Early` text.
//!
//! The FloatSpy menu, btime and ptime retain self-locked golden screenshots
//! as change detectors. FloatSpy requires `T` and a longer run to reach its
//! external-oracle result. There is no upstream 48K ptime reference.
//!
//! Required local fixtures:
//! - `EMU198X_SPECTRUM_48K_ROM`, defaulting to
//!   `~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom`.
//! - `EMU198X_SPECTRUM_SYSTEM_TESTS_DIR/<name>.tap`, defaulting to
//!   `~/.emu198x/test-data/spectrum-system-tests/<name>.tap`.
//!
//! Missing ROMs/tapes are reported through `emu198x_test_skip`. Spectron PNGs
//! are checked in; `EMU198X_SPECTRON_RESULTS_DIR` overrides their location.
//! Missing reference PNGs fail rather than skip. See the reference inventory
//! in `test-data/spectrum/spectron-results/README.md` for tape provenance.

use common_sinclair_zx_spectrum::keyboard::SpectrumKey;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::tape::TapeBlock;
use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH};
use format198x_sinclair_zx_spectrum_tap::{TapBlock, decode};
use machine_sinclair_zx_spectrum_48k::Spectrum48k;
use std::path::{Path, PathBuf};

#[path = "../../common-sinclair-zx-spectrum/test-support/spectron.rs"]
mod spectron;
use spectron::{assert_screen_matches_spectron, write_indexed_png};

const ROM_PATH_ENV: &str = "EMU198X_SPECTRUM_48K_ROM";
const SYSTEM_TESTS_DIR_ENV: &str = "EMU198X_SPECTRUM_SYSTEM_TESTS_DIR";
const BOOT_FRAMES: usize = 200;
const RUN_BUDGET_FRAMES: usize = 5_000;

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").expect("HOME must be set"))
}

fn rom_path() -> PathBuf {
    std::env::var_os(ROM_PATH_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".emu198x/roms/sinclair-zx-spectrum-48k/48.rom"))
}

fn system_tests_dir() -> PathBuf {
    std::env::var_os(SYSTEM_TESTS_DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".emu198x/test-data/spectrum-system-tests"))
}

fn tap_blocks_to_tape_blocks(blocks: Vec<TapBlock>) -> Vec<TapeBlock> {
    blocks
        .into_iter()
        .map(|block| {
            let mut full = Vec::with_capacity(block.data.len() + 2);
            full.push(block.flag);
            full.extend_from_slice(&block.data);
            let checksum = full.iter().fold(0u8, |acc, &byte| acc ^ byte);
            full.push(checksum);
            TapeBlock {
                flag: block.flag,
                data: full,
            }
        })
        .collect()
}

/// Type `LOAD ""<ENTER>` at the BASIC command prompt. At the K cursor
/// `J` emits the LOAD keyword; `SS+P` emits `"`; ENTER terminates.
fn type_load_command(machine: &mut Spectrum48k, start_frame: usize) -> usize {
    let mut frame = start_frame;
    let press = |m: &mut Spectrum48k, k: SpectrumKey, on: bool| m.keyboard_mut().set_key(k, on);

    let tap = |m: &mut Spectrum48k, frame: &mut usize, k: SpectrumKey| {
        for _ in 0..6 {
            *frame += 1;
            m.run_frame();
        }
        press(m, k, true);
        for _ in 0..6 {
            *frame += 1;
            m.run_frame();
        }
        press(m, k, false);
    };

    let chord = |m: &mut Spectrum48k, frame: &mut usize, mo: SpectrumKey, k: SpectrumKey| {
        for _ in 0..6 {
            *frame += 1;
            m.run_frame();
        }
        press(m, mo, true);
        press(m, k, true);
        for _ in 0..6 {
            *frame += 1;
            m.run_frame();
        }
        press(m, k, false);
        press(m, mo, false);
    };

    tap(machine, &mut frame, SpectrumKey::J);
    chord(
        machine,
        &mut frame,
        SpectrumKey::SymbolShift,
        SpectrumKey::P,
    );
    chord(
        machine,
        &mut frame,
        SpectrumKey::SymbolShift,
        SpectrumKey::P,
    );
    tap(machine, &mut frame, SpectrumKey::Enter);
    frame
}

fn goldens_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goldens")
}

fn read_indexed_png(path: &Path) -> Vec<u8> {
    let file = std::fs::File::open(path).expect("open golden");
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = decoder.read_info().expect("decode png header");
    let mut buf = vec![
        0u8;
        reader
            .output_buffer_size()
            .expect("png buffer size fits in usize")
    ];
    let info = reader.next_frame(&mut buf).expect("decode png frame");
    buf.truncate(info.buffer_size());
    assert_eq!(
        info.color_type,
        png::ColorType::Indexed,
        "golden {} should be indexed PNG",
        path.display()
    );
    assert_eq!(
        (info.width as usize, info.height as usize),
        (SCREEN_WIDTH, SCREEN_HEIGHT),
        "golden {} dimensions {}×{} don't match expected {}×{}",
        path.display(),
        info.width,
        info.height,
        SCREEN_WIDTH,
        SCREEN_HEIGHT,
    );
    buf
}

/// Decodes the ULA bitmap into its 24×32 ROM-font text cells.
///
/// HALT2INT prints its classifications through the Spectrum ROM, so matching
/// screen cells against the loaded ROM's glyph table gives the test a stable
/// semantic assertion without locking unrelated diagnostic fields.
fn screen_text_lines(machine: &Spectrum48k) -> Vec<String> {
    common_sinclair_zx_spectrum::screen_text::decode_screen_text(
        |addr| machine.read(addr),
        |addr| machine.read(addr),
    )
}

fn compare_or_update(test_name: &str, framebuffer: &[u8]) {
    let path = goldens_dir().join(format!("{test_name}.png"));
    let updating = std::env::var_os("UPDATE_GOLDENS").is_some();
    let missing = !path.exists();

    if updating || missing {
        write_indexed_png(&path, framebuffer);
        if missing && !updating {
            panic!(
                "golden {} did not exist — wrote it now, re-run to verify",
                path.display()
            );
        }
        return;
    }

    let expected = read_indexed_png(&path);
    if expected == framebuffer {
        return;
    }
    let differing = expected
        .iter()
        .zip(framebuffer.iter())
        .filter(|(a, b)| a != b)
        .count();
    let live_path = std::env::temp_dir().join(format!("{test_name}-live.png"));
    write_indexed_png(&live_path, framebuffer);
    panic!(
        "framebuffer for {test_name} differs from {} ({differing} of {} pixels). \
         Live frame written to {} for visual diff. \
         Re-run with UPDATE_GOLDENS=1 to refresh after eyeballing the change.",
        path.display(),
        framebuffer.len(),
        live_path.display(),
    );
}

/// Common runner: load TAP, boot, type LOAD"", play tape, run for the
/// budget, compare framebuffer to the locked golden.
fn run_and_compare(test_name: &str) {
    run_and_compare_with_spectron(test_name, None);
}

/// As `run_and_compare`, but when `spectron_png` is `Some`, also assert
/// the captured screen content is byte-equal to Spectron's reference of
/// that name (gated on `EMU198X_SPECTRON_RESULTS_DIR`).
fn run_and_compare_with_spectron(test_name: &str, spectron_png: Option<&str>) {
    let Some(machine) = run_to_completion(test_name) else {
        emu198x_test_skip::skip!("Spectrum 48K ROM or {test_name}.tap not staged");
    };

    compare_or_update(test_name, machine.framebuffer());
    if let Some(reference) = spectron_png {
        assert_screen_matches_spectron(reference, machine.framebuffer());
    }
}

/// Loads and runs one system-test TAP to its fixed capture budget.
fn run_to_completion(test_name: &str) -> Option<Spectrum48k> {
    let rom_path = rom_path();
    if !rom_path.is_file() {
        emu198x_test_skip::record(&format!(
            "48K ROM not found at {} — skipping",
            rom_path.display()
        ));
        return None;
    }
    let tap_path = system_tests_dir().join(format!("{test_name}.tap"));
    if !tap_path.is_file() {
        emu198x_test_skip::record(&format!(
            "{}.tap not found at {} — skipping",
            test_name,
            tap_path.display()
        ));
        return None;
    }

    let rom = std::fs::read(&rom_path).expect("48K ROM should read");
    let tap_bytes = std::fs::read(&tap_path).unwrap_or_else(|e| panic!("{test_name}.tap: {e}"));
    let tap_blocks = decode(&tap_bytes).unwrap_or_else(|e| panic!("{test_name}.tap parse: {e}"));
    let tape_blocks = tap_blocks_to_tape_blocks(tap_blocks);

    let mut machine = Spectrum48k::new();
    machine.load_rom_bytes(&rom).expect("48K ROM should load");
    machine.reset();
    machine.load_tape_blocks(tape_blocks);

    for _ in 0..BOOT_FRAMES {
        machine.run_frame();
    }
    let after_typing = type_load_command(&mut machine, BOOT_FRAMES);
    for _ in 0..30 {
        machine.run_frame();
    }
    machine.play_tape();

    for _ in (after_typing + 30)..(after_typing + 30 + RUN_BUDGET_FRAMES) {
        machine.run_frame();
    }

    Some(machine)
}

#[test]
#[ignore = "FIXTURE: requires local 48K ROM and floatspy.tap; ~100 s wall time at cycle-accurate tape speed"]
fn floatspy_runs_to_completion() {
    // Captures floatspy's interactive menu. After the +3 floating-bus
    // phase fix (#62) the menu's IN() BYTE reads 0, matching Spectron's
    // floatspy_48.png. Compared to the self-locked menu golden here; the
    // self-test-to-completion compare lives in `floatspy_selftest_ok`.
    run_and_compare("floatspy");
}

/// Drive floatspy's self-test (`T`) to completion and byte-compare the
/// finished "Floating bus OK" screen to Spectron's `floatspy_48.png`. This
/// is the end-to-end proof of the floating-bus read-phase fix (#62): the
/// IN() BYTE reads 0 and floatspy reports OK, byte-equal to the oracle.
/// Gated on `EMU198X_SPECTRON_RESULTS_DIR`; ~370 s at cycle-accurate speed.
#[test]
#[ignore = "FIXTURE: requires local 48K ROM, floatspy.tap, and EMU198X_SPECTRON_RESULTS_DIR; ~370 s"]
fn floatspy_selftest_ok() {
    let rom_path = rom_path();
    if !rom_path.is_file() {
        emu198x_test_skip::skip!("Spectrum 48K ROM not staged (EMU198X_SPECTRUM_48K_ROM)");
    }
    let tap_path = system_tests_dir().join("floatspy.tap");
    if !tap_path.is_file() {
        emu198x_test_skip::skip!("floatspy.tap not staged (EMU198X_SPECTRUM_SYSTEM_TESTS_DIR)");
    }
    let rom = std::fs::read(&rom_path).expect("read 48K ROM");
    let tape_blocks = tap_blocks_to_tape_blocks(
        decode(&std::fs::read(&tap_path).expect("read floatspy.tap")).expect("parse floatspy.tap"),
    );
    let mut machine = Spectrum48k::new();
    machine.load_rom_bytes(&rom).expect("load 48K ROM bytes");
    machine.reset();
    machine.load_tape_blocks(tape_blocks);
    for _ in 0..BOOT_FRAMES {
        machine.run_frame();
    }
    let after = type_load_command(&mut machine, BOOT_FRAMES);
    for _ in 0..30 {
        machine.run_frame();
    }
    machine.play_tape();
    for _ in (after + 30)..(after + 30 + RUN_BUDGET_FRAMES) {
        machine.run_frame();
    }
    machine.keyboard_mut().set_key(SpectrumKey::T, true);
    for _ in 0..4 {
        machine.run_frame();
    }
    machine.keyboard_mut().set_key(SpectrumKey::T, false);
    for _ in 0..40_000 {
        machine.run_frame();
    }
    let dump = std::env::temp_dir().join("floatspy-selftest.png");
    write_indexed_png(&dump, machine.framebuffer());
    eprintln!("floatspy self-test screen written to {}", dump.display());
    assert_screen_matches_spectron("floatspy_48.png", machine.framebuffer());
}

#[test]
#[ignore = "FIXTURE: requires local 48K ROM and halt2int.tap; ~100 s wall time"]
fn halt2int_runs_to_completion() {
    let Some(machine) = run_to_completion("halt2int") else {
        emu198x_test_skip::skip!(
            "Spectrum 48K ROM or tape image not staged (EMU198X_SPECTRUM_48K_ROM)"
        );
    };
    let lines = screen_text_lines(&machine);

    assert!(
        lines.iter().any(|line| line.contains("HALT: Early")),
        "HALT2INT should classify the HALT profile as Early; decoded screen:\n{}",
        lines.join("\n"),
    );

    // HALT2INT's other classification, which is the one that was wrong.
    // It decides this by stamping `$5800` and reading the floating bus at
    // a fixed instant: at the old read origin the read missed the
    // attribute slot and the suite printed `Float: Unknown` (#940).
    assert!(
        lines.iter().any(|line| line.contains("Float: Early")),
        "HALT2INT should classify the floating bus as Early; decoded screen:\n{}",
        lines.join("\n"),
    );

    // And hold the whole 256x192 to the oracle, because the decoded-text
    // checks above pass on a screen that is wrong everywhere the text is
    // right. `halt2int_48.png` sat unused in Spectron's results until #10.
    //
    // This was a scored ratchet at 49104 of 49152 while #940 stood — the
    // 48 pixels of that one word. It is an exact match now, so it is
    // asserted as one.
    assert_screen_matches_spectron("halt2int_48.png", machine.framebuffer());
}

#[test]
#[ignore = "FIXTURE: requires local 48K ROM and btime.tap; ~100 s wall time"]
fn btime_runs_to_completion() {
    run_and_compare_with_spectron("btime", Some("btime_48.png"));
}

#[test]
#[ignore = "FIXTURE: requires local 48K ROM and ptime.tap; ~100 s wall time"]
fn ptime_runs_to_completion() {
    // Spectron ships only a 128K ptime reference (`ptime_128.png`); there
    // is no 48K one to validly compare this 48K run against, so it stays
    // on its self-locked golden. (#10)
    run_and_compare("ptime");
}

// Super HALT Invaders Test is 128K-only; see the 128K crate's
// `tape_smoke.rs` for its test wiring.

#[test]
#[ignore = "FIXTURE: requires local 48k ROM and eihalt.tap"]
fn eihalt48k_matches_spectron() {
    let Some(machine) = run_to_completion("eihalt") else {
        emu198x_test_skip::skip!("Spectrum 48k ROM or eihalt.tap not staged");
    };
    assert_screen_matches_spectron("eihalt_49.png", machine.framebuffer());
}
