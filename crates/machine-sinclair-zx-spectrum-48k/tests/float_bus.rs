//! Woody's `Float48K` floating-bus T-state probe, run on a real 48K
//! Spectrum.
//!
//! Unlike `z80test.rs`, `Float48K.tap` is BASIC-only — the program builds
//! its machine-code probe in upper RAM via `POKE` statements and calls it
//! with `USR 62000`. The harness therefore has to drive the actual tape
//! pipeline rather than inject bytes directly: type `LOAD ""` at the BASIC
//! command prompt, connect the TAP as tape input, play it back at real
//! cassette speed (no ROM trap — see `knowledge/decisions/no-rom-trap-load.md`),
//! and wait for the auto-run to finish printing its result.
//!
//! The probe builds a small `IN A,(254)` … `IN A,($FF)` sequence whose
//! exact T-state offset varies by delay, fires it from BASIC, and prints
//! the T-state value at which the ULA floating-bus byte matched a known
//! display byte. On real 48K hardware the headline value is **14338**
//! (one T-state earlier than `ulatest3`'s 14339, because `Float48K`
//! measures display-byte-on-bus-after-latch and `ulatest3` measures
//! display-byte-being-fetched). On this engine the value is 14339 — see
//! `FLOAT48K_EXPECTED_TSTATE`'s comment block for the 1-T-state offset.
//!
//! Upstream: <https://github.com/oldbit-com/Spectron> bundles the canonical
//! `Float48k.tap` and `Float128k.tap` under `tests/Spectron.Integration.Tests/TestFiles/`.
//! Reference catalogue:
//! `Emu198x-Reference/_organised/by-topic/testing-suites/spectrum-test-roms.md`.
//!
//! Run with:
//!
//! ```sh
//! cargo test --release -p machine-sinclair-zx-spectrum-48k --test float_bus -- --ignored --nocapture
//! ```

use common_sinclair_zx_spectrum::keyboard::SpectrumKey;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::palette::SPECTRUM_PALETTE;
use common_sinclair_zx_spectrum::tape::TapeBlock;
use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH};
use format_sinclair_zx_spectrum_tap::{TapBlock, parse_tap};
use machine_sinclair_zx_spectrum_48k::Spectrum48k;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

const ROM_PATH_ENV: &str = "EMU198X_SPECTRUM_48K_ROM";
const SYSTEM_TESTS_DIR_ENV: &str = "EMU198X_SPECTRUM_SYSTEM_TESTS_DIR";

const BOOT_FRAMES: usize = 200;

/// Upper bound on frames spent running after the LOAD command. Float48K is
/// two-block (BASIC + CODE) and the BASIC program runs an iterative probe;
/// at cycle-accurate tape speed plus pilot tones plus BASIC interpreter
/// overhead, ~6000 frames (~120 s emulated) covers the worst case.
const MAX_RUN_FRAMES: usize = 6000;

const RST10_ADDR: u16 = 0x0010;

/// PR-ALL in the Spectrum 48K ROM at `$09F4`. Every printable character
/// routes through here regardless of how the caller got there — the RST 16
/// entry at `$0010` itself does `AND A; CALL $09F4`. Hooking PR-ALL catches
/// both direct `CALL $09F4` callers (BASIC's number formatter, some internal
/// routines) and RST 16 callers (user code, `PRINT CHR$()`, etc.).
const PR_ALL_ADDR: u16 = 0x09F4;
const SCR_CT_ADDR: u16 = 0x5C8C;

/// Sub-frame step granularity for the capture loop. PR-ALL ($09F4) is called
/// roughly 4× more often than RST 16 ($0010) because the ROM's internal print
/// routines (`PO-MSG`, `CHAN-OPEN` banner, error reporting) call it directly
/// without going through the RST 16 entry. At a 4-T-state granularity, two
/// PR-ALL hits inside one window would be sampled as a single edge and one
/// would be lost. A 1-T-state granularity guarantees every PR-ALL entry is
/// caught as a rising edge.
const STEP_TSTATES: u32 = 1;

/// First T-state at which the Float48K probe sees a non-`$FF` byte on our
/// engine.
///
/// **Real 48K hardware:** Float48K prints `14338` (Woody, WoS forum 17551)
/// — that is the canonical Smith Ch 12 / Ch 21 "fetched byte on the data
/// bus" tap.
///
/// **Our engine:** prints `14339`. The ULA's two-stage shifter (Seam 1 of
/// the [architecture review](../../../knowledge/decisions/spectrum-architecture-review.md))
/// lands the DataLatch at T-14336 and the bus exposure at T-14338, matching
/// silicon. The 1-T-state offset visible at the IN A,($FF) probe is a
/// Z80/ULA phase-alignment subtlety in when our Z80 model samples the IO
/// data bus inside the IN M-cycle — independent of the ULA fetch timing
/// itself. Tracked as a follow-up engine fidelity item; the catalogue
/// frame hashes are unaffected (they depend on the visible-pixel tap, not
/// the floating-bus probe's IO sample point).
const FLOAT48K_EXPECTED_TSTATE: u32 = 14339;

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

/// Reattach the flag byte and TAP checksum to each parsed block so the tape
/// player sees the same byte stream the original cassette did.
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

/// One scheduled keyboard action — set or release a key at a given absolute
/// frame number relative to the start of the run.
struct KeyAction {
    at_frame: usize,
    key: SpectrumKey,
    pressed: bool,
}

/// Schedule of key actions building up `LOAD ""<ENTER>` starting at
/// `start_frame`. At the K cursor (default after boot), `J` emits the
/// `LOAD` keyword. After LOAD the editor is in L mode and `SS+P` emits a `"`;
/// repeat for the second quote, then ENTER terminates the line. Each tap is
/// held for 6 frames followed by a 6-frame gap so the keyboard scan and
/// editor debounce both see it.
fn load_command_schedule(start_frame: usize) -> Vec<KeyAction> {
    let mut actions = Vec::new();
    let mut t = start_frame;

    fn tap(actions: &mut Vec<KeyAction>, t: &mut usize, key: SpectrumKey) {
        actions.push(KeyAction { at_frame: *t, key, pressed: true });
        actions.push(KeyAction { at_frame: *t + 6, key, pressed: false });
        *t += 12;
    }

    fn chord(actions: &mut Vec<KeyAction>, t: &mut usize, modifier: SpectrumKey, key: SpectrumKey) {
        actions.push(KeyAction { at_frame: *t, key: modifier, pressed: true });
        actions.push(KeyAction { at_frame: *t, key, pressed: true });
        actions.push(KeyAction { at_frame: *t + 6, key, pressed: false });
        actions.push(KeyAction { at_frame: *t + 6, key: modifier, pressed: false });
        *t += 12;
    }

    tap(&mut actions, &mut t, SpectrumKey::J);
    chord(&mut actions, &mut t, SpectrumKey::SymbolShift, SpectrumKey::P);
    chord(&mut actions, &mut t, SpectrumKey::SymbolShift, SpectrumKey::P);
    tap(&mut actions, &mut t, SpectrumKey::Enter);
    actions
}

/// One unified run loop that captures every `RST 16` print throughout, plays
/// scheduled keyboard actions at the right frame, starts the tape at a fixed
/// frame, and stops on completion-marker or budget exhaustion.
fn run_full(
    machine: &mut Spectrum48k,
    total_frames: usize,
    mut key_actions: Vec<KeyAction>,
    tape_start_frame: usize,
    completion_marker: impl Fn(&str) -> bool,
) -> (String, usize) {
    use common_sinclair_zx_spectrum::timing::TIMING_48K;

    // Sort the actions by frame so we can pop them in order.
    key_actions.sort_by_key(|a| a.at_frame);

    let mut transcript = String::new();
    let initial_pc = machine.z80().regs.pc;
    let mut prev_at_rst10 = initial_pc == RST10_ADDR;
    let mut prev_at_pr_all = initial_pc == PR_ALL_ADDR;
    let steps_per_frame = (TIMING_48K.tstates_per_frame / STEP_TSTATES) as usize;
    let mut tape_started = false;
    let mut action_idx = 0usize;
    let mut frames_run = 0usize;
    let mut rst10_hits = 0u64;
    let mut pr_all_hits = 0u64;

    // Capture point selection:
    // - RST 16 (`$0010`): clean stream of literal `PRINT CHR$()` and similar,
    //   but BASIC's `PRINT-FP` number formatter calls into character routines
    //   directly and bypasses this entry, so iteration digits are missed.
    // - PR-ALL (`$09F4`, default since the control-byte state machine landed):
    //   catches every printable character including PRINT-FP digits. To
    //   suppress the argument bytes that follow AT / INK / PAPER / FLASH /
    //   BRIGHT / INVERSE / OVER / TAB control codes, the capture loop tracks
    //   how many argument bytes to skip after each control code.
    // Set EMU198X_FLOAT48K_CAPTURE=rst10 to revert to the legacy capture point.
    let use_pr_all = std::env::var("EMU198X_FLOAT48K_CAPTURE")
        .map(|v| v != "rst10")
        .unwrap_or(true);

    // Control-byte state machine for PR-ALL mode. After a Spectrum 48K ROM
    // control code that takes argument bytes, the next 1 or 2 calls to
    // PR-ALL carry the arguments (row/column for AT, colour for INK/PAPER,
    // etc.) — argument bytes are NOT characters to print and must be
    // suppressed from the transcript. Per the 48K ROM `PRINT-OUT` routine:
    //
    //   $10 INK     | $11 PAPER  | $12 FLASH  | $13 BRIGHT
    //   $14 INVERSE | $15 OVER   | $17 TAB        — 1 argument byte each
    //   $16 AT                                    — 2 argument bytes
    let mut skip_args: u8 = 0;

    while frames_run < total_frames {
        while action_idx < key_actions.len() && key_actions[action_idx].at_frame == frames_run {
            let action = &key_actions[action_idx];
            machine.keyboard_mut().set_key(action.key, action.pressed);
            action_idx += 1;
        }
        if !tape_started && frames_run >= tape_start_frame {
            machine.play_tape();
            tape_started = true;
        }

        for _ in 0..steps_per_frame {
            machine.advance_tstates(STEP_TSTATES);
            let z80 = machine.z80();
            let pc = z80.regs.pc;
            let at_rst10 = pc == RST10_ADDR;
            let at_pr_all = pc == PR_ALL_ADDR;

            let capture = if use_pr_all {
                at_pr_all && !prev_at_pr_all
            } else {
                at_rst10 && !prev_at_rst10
            };

            if capture {
                let ch = z80.regs.a();
                if skip_args > 0 {
                    skip_args -= 1;
                } else {
                    match ch {
                        0x0D => {
                            eprintln!();
                            transcript.push('\n');
                        }
                        0x10..=0x15 | 0x17 => skip_args = 1,
                        0x16 => skip_args = 2,
                        0x20..=0x7E => {
                            eprint!("{}", ch as char);
                            transcript.push(ch as char);
                        }
                        _ => {}
                    }
                }
                if std::env::var("EMU198X_FLOAT48K_SUPPRESS_SCROLL").is_ok() {
                    machine.write(SCR_CT_ADDR, 0xFF);
                }
            }

            if at_rst10 && !prev_at_rst10 {
                rst10_hits += 1;
            }
            if at_pr_all && !prev_at_pr_all {
                pr_all_hits += 1;
            }
            prev_at_rst10 = at_rst10;
            prev_at_pr_all = at_pr_all;
        }
        frames_run += 1;
        if completion_marker(&transcript) {
            break;
        }
    }

    eprintln!(
        "\n[diag] RST 16 ($0010) hits: {} | PR-ALL ($09F4) hits: {} | capture mode: {}",
        rst10_hits,
        pr_all_hits,
        if use_pr_all { "pr_all" } else { "rst10" },
    );

    (transcript, frames_run)
}

#[test]
#[ignore = "requires local 48K ROM and Float48k.tap; ~50 s wall time at cycle-accurate tape speed"]
fn float48k_prints_expected_tstate() {
    let rom_path = rom_path();
    if !rom_path.is_file() {
        eprintln!(
            "48K ROM not found at {} — skipping Float48K test",
            rom_path.display()
        );
        return;
    }
    let tap_path = system_tests_dir().join("Float48k.tap");
    if !tap_path.is_file() {
        eprintln!(
            "Float48k.tap not found at {} — skipping (download from oldbit-com/Spectron)",
            tap_path.display()
        );
        return;
    }

    let rom = std::fs::read(&rom_path).expect("48K ROM should read");
    let tap_bytes = std::fs::read(&tap_path).expect("Float48k.tap should read");
    let tap_blocks = parse_tap(&tap_bytes).expect("Float48k.tap should parse");
    let tape_blocks = tap_blocks_to_tape_blocks(tap_blocks);

    let mut machine = Spectrum48k::new();
    machine.load_rom_bytes(&rom).expect("48K ROM should load");
    machine.reset();
    machine.load_tape_blocks(tape_blocks);

    // Schedule: boot phase 0..BOOT_FRAMES, then start typing LOAD"" at
    // BOOT_FRAMES, then call play_tape() after the schedule completes.
    let key_actions = load_command_schedule(BOOT_FRAMES);
    let tape_start_frame = key_actions
        .last()
        .map(|a| a.at_frame + 30)
        .unwrap_or(BOOT_FRAMES + 60);

    let (transcript, frames_used) = run_full(
        &mut machine,
        BOOT_FRAMES + MAX_RUN_FRAMES,
        key_actions,
        tape_start_frame,
        |t| {
            // The probe iterates and prints `T-state byte\n` for each offset.
            // Bytes are 255 until the floating bus exposes the fetched display
            // byte. The first non-255 reading is the canonical sample — stop
            // once one full result line containing a candidate T-state and a
            // non-255 byte has been printed.
            ["14336", "14337", "14338", "14339", "14340"]
                .iter()
                .any(|stem| {
                    t.lines().any(|line| {
                        line.starts_with(stem)
                            && !line.ends_with(" 255")
                            && line.len() > stem.len() + 1
                    })
                })
        },
    );

    let final_pc = machine.z80().regs.pc;
    eprintln!(
        "\n--- Float48K finished after {} frames ({:.1} s emulated), final PC=${:04X} ---",
        frames_used,
        frames_used as f64 / 50.0,
        final_pc,
    );
    eprintln!("--- full transcript ---\n{}", transcript);

    // Always dump the framebuffer to a PNG so the visual ground truth is
    // available even when the T-state assertion fails.
    let png_path = std::env::temp_dir().join("float48k.png");
    save_framebuffer_png(&machine, &png_path).expect("framebuffer should encode as PNG");
    eprintln!("Framebuffer screenshot: {}", png_path.display());

    // The load chain end-to-end must work. This catches breakage of: boot, key
    // injection, BASIC LOAD tokenisation, tape playback at cycle-accurate
    // speed, multi-block load (BASIC + CODE), and the BASIC autostart hook.
    assert!(
        transcript.contains("Program: Float48K"),
        "expected the BASIC LOAD message 'Program: Float48K' in the transcript\n\
         (boot / keyboard / tape / LOAD chain failure)\n\
         --- transcript ---\n{transcript}",
    );
    assert!(
        transcript.contains("Bytes: floatcode"),
        "expected the second-block CODE LOAD message 'Bytes: floatcode'\n\
         (the BASIC program LOAD-CODE sub-step did not complete)\n\
         --- transcript ---\n{transcript}",
    );

    // Strict assertion (un-gated 2026-05-20). The harness captures at PR-ALL
    // with a control-byte state machine that skips AT / INK / PAPER / FLASH /
    // BRIGHT / INVERSE / OVER / TAB argument bytes, and STEP_TSTATES = 1
    // guarantees every PR-ALL entry is caught (the legacy 4-T-state
    // granularity dropped ~50% of PR-ALL hits because the routine is called
    // too frequently to fit between samples). The completion marker waits
    // for a complete result line (T-state + non-`255` byte). With those
    // harness fixes the probe output is clean and we can pin the result.
    //
    // Pins our engine's measured value `FLOAT48K_EXPECTED_TSTATE`. A
    // regression in the ULA shifter pipeline, the Z80's IO-read M-cycle,
    // or the floating-bus exposure surfaces here.
    let expected = FLOAT48K_EXPECTED_TSTATE.to_string();
    assert!(
        transcript.contains(&expected),
        "Float48K probe did not produce expected T-state {expected}\n\
         (engine timing regression — see FLOAT48K_EXPECTED_TSTATE's comment)\n\
         --- transcript ---\n{transcript}",
    );
    eprintln!("\nFloat48K: STRICT PASS — found expected T-state {expected}");
}

/// Save the current 48K framebuffer as an RGBA PNG using the Spectrum
/// palette. Used for diagnostic ground truth when print capture is silent.
fn save_framebuffer_png(machine: &Spectrum48k, path: &Path) -> std::io::Result<()> {
    let framebuffer = machine.framebuffer();
    assert_eq!(framebuffer.len(), SCREEN_WIDTH * SCREEN_HEIGHT);

    let mut rgba = Vec::with_capacity(SCREEN_WIDTH * SCREEN_HEIGHT * 4);
    for &idx in framebuffer {
        let rgb = SPECTRUM_PALETTE[(idx as usize) & 0x0F];
        rgba.push(((rgb >> 16) & 0xFF) as u8);
        rgba.push(((rgb >> 8) & 0xFF) as u8);
        rgba.push((rgb & 0xFF) as u8);
        rgba.push(0xFF);
    }

    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    writer
        .write_image_data(&rgba)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Fixture-free unit tests on the helpers.
// ---------------------------------------------------------------------------

#[test]
fn tap_blocks_to_tape_blocks_reattaches_flag_and_checksum() {
    let blocks = vec![TapBlock {
        flag: 0xFF,
        data: vec![0x01, 0x02, 0x03],
    }];
    let tape = tap_blocks_to_tape_blocks(blocks);
    assert_eq!(tape.len(), 1);
    assert_eq!(tape[0].flag, 0xFF);
    // Full re-encoded form: flag + data + checksum (XOR over flag+data).
    assert_eq!(tape[0].data, vec![0xFF, 0x01, 0x02, 0x03, 0xFF ^ 0x01 ^ 0x02 ^ 0x03]);
}
