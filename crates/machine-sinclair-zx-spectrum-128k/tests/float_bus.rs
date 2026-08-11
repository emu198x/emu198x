//! Woody's `Float128K` floating-bus T-state probe, run on a real Sinclair
//! 128K (Sinclair 7K010E ULA).
//!
//! The 128K equivalent of `crates/machine-sinclair-zx-spectrum-48k/tests/
//! float_bus.rs`. Shape is identical — load `Float128k.tap`, drive the
//! native autoload path, capture the probe's printed result and assert
//! against the engine's pinned T-state value.
//!
//! Boot sequence differs from the 48K's `LOAD ""` typing path: the 128K's
//! firmware boots into a four-option menu (Tape Loader / 128 BASIC /
//! Calculator / 48 BASIC) and pressing ENTER selects the highlighted
//! Tape Loader entry, which internally calls `LOAD ""` and switches
//! to ROM 1 (48 BASIC) for the running program. The probe then executes
//! in 48 BASIC mode and uses the same `PR-ALL` / `RST 16` entry points
//! as the 48K.
//!
//! Engine timing differs: 228 T-states per line (vs 48K's 224),
//! `cpu_divisor = 5`, 311 lines per frame. Smith Ch 12 + Ch 21 give the
//! canonical 128K first-display-byte-on-bus sample point as **T=14364**
//! (28 T-states later than the 48K's T=14338, matching the wider line and
//! different scan-0 alignment).
//!
//! Upstream: <https://github.com/oldbit-com/Spectron> ships `Float128k.tap`
//! alongside `Float48k.tap`. Reference catalogue:
//! `Emu198x-Reference/_organised/by-topic/testing-suites/spectrum-test-roms.md`.
//!
//! Run with:
//!
//! ```sh
//! cargo test --release -p machine-sinclair-zx-spectrum-128k --test float_bus -- --ignored --nocapture
//! ```

use common_sinclair_zx_spectrum::keyboard::SpectrumKey;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::palette::SPECTRUM_PALETTE;
use common_sinclair_zx_spectrum::tape::TapeBlock;
use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH};
use format_sinclair_zx_spectrum_tap::{TapBlock, parse_tap};
use machine_sinclair_zx_spectrum_128k::Spectrum128K;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

const ROM0_PATH_ENV: &str = "EMU198X_SPECTRUM_128K_ROM0";
const ROM1_PATH_ENV: &str = "EMU198X_SPECTRUM_128K_ROM1";
const SYSTEM_TESTS_DIR_ENV: &str = "EMU198X_SPECTRUM_SYSTEM_TESTS_DIR";

const BOOT_FRAMES: usize = 200;

/// Upper bound on frames spent running after the ENTER press. Float128K is
/// two-block (BASIC + CODE) and the BASIC program runs an iterative probe;
/// at cycle-accurate tape speed plus pilot tones plus BASIC interpreter
/// overhead plus the 128K's slightly wider frame, ~6000 frames (~120 s
/// emulated) covers the worst case — matches the 48K budget.
const MAX_RUN_FRAMES: usize = 6000;

const RST10_ADDR: u16 = 0x0010;

/// PR-ALL at `$09F4` in the 48 BASIC ROM (ROM 1 on the 128K). The Tape
/// Loader switches to ROM 1 before handing control to the loaded program,
/// so the probe's `PRINT` statements route through the standard PR-ALL
/// entry — same address as the 48K equivalent.
const PR_ALL_ADDR: u16 = 0x09F4;
const SCR_CT_ADDR: u16 = 0x5C8C;

/// Sub-frame step granularity for the capture loop. See the 48K test's
/// comment block for the full rationale — PR-ALL is called frequently
/// enough that the legacy 4-T-state granularity dropped ~50% of hits.
const STEP_TSTATES: u32 = 1;

/// First T-state at which the Float128K probe sees a non-`$FF` byte on
/// our engine.
///
/// Long-established Fuse/community reference coordinate. Primary hardware
/// capture provenance remains incomplete; this is an implementation target,
/// not a claim of a new direct hardware measurement.
///
/// **Reached by derivation, not by fitting — this is what closes #851.**
/// The engine read 14363 when that issue was filed and 14362 once the
/// Z80's bus pins were corrected to Zilog's waveforms, because the
/// 128K-class core carried its own `SAMPLE_LEAD = 3` against an origin of
/// 14363 and neither number came from anywhere.
///
/// Both are gone. The core now applies one shared, derived constant —
/// `zilog_z80::IO_READ_DATA_LATCH_LEAD_TSTATES`, the two T-states between
/// the `/IORQ` edge and the CPU's data latch, re-derived from the recorded
/// I/O-read waveform by `zilog-z80`'s `bus_pin_waveform` — onto an origin
/// that is libspectrum's `top_left_pixel` for this ULA,
/// `timings_frame_ferranti_7c` = 14362. The 48K-class core applies the
/// same two constants against `timings_frame_ferranti_5c_6c` = 14336 and
/// comes out unchanged, which is the check that this is a rule rather than
/// a second fit.
///
/// So the number is load-bearing in both directions now: it is what the
/// derivation predicts, and it was not available to be tuned to. If it
/// moves, the sample instant or the origin moved — do not re-bless it.
const FLOAT128K_EXPECTED_TSTATE: u32 = 14364;

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").expect("HOME must be set"))
}

fn rom0_path() -> PathBuf {
    std::env::var_os(ROM0_PATH_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".emu198x/roms/sinclair-zx-spectrum-128k/128-0.rom"))
}

fn rom1_path() -> PathBuf {
    std::env::var_os(ROM1_PATH_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".emu198x/roms/sinclair-zx-spectrum-128k/128-1.rom"))
}

fn system_tests_dir() -> PathBuf {
    std::env::var_os(SYSTEM_TESTS_DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".emu198x/test-data/spectrum-system-tests"))
}

/// Reattach the flag byte and TAP checksum to each parsed block so the
/// tape player sees the same byte stream the original cassette did.
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

/// Set one Spectrum key's pressed state on the 128K's raw keyboard matrix
/// (no `KeyboardMatrix` wrapper — the 128K-class core exposes a bare
/// `[u8; 8]` field for direct serde compatibility).
fn set_key(keyboard: &mut [u8; 8], key: SpectrumKey, pressed: bool) {
    let (row, bit) = key.row_bit();
    let mask = 1u8 << bit;
    if pressed {
        keyboard[row] &= !mask;
    } else {
        keyboard[row] |= mask;
    }
}

/// One scheduled keyboard action — set or release a key at a given
/// absolute frame number relative to the start of the run.
struct KeyAction {
    at_frame: usize,
    key: SpectrumKey,
    pressed: bool,
}

/// Schedule of key actions building up an ENTER press starting at
/// `start_frame`. The 128K firmware boots into a menu with "Tape Loader"
/// highlighted; pressing ENTER selects it, which internally calls
/// `LOAD ""` and switches to ROM 1. Six-frame press/release matches the
/// 48K's `LOAD` typing schedule so the keyboard scan and editor
/// debounce both see it.
fn tape_loader_schedule(start_frame: usize) -> Vec<KeyAction> {
    vec![
        KeyAction {
            at_frame: start_frame,
            key: SpectrumKey::Enter,
            pressed: true,
        },
        KeyAction {
            at_frame: start_frame + 6,
            key: SpectrumKey::Enter,
            pressed: false,
        },
    ]
}

/// One unified run loop that captures every PR-ALL print throughout, plays
/// scheduled keyboard actions at the right frame, starts the tape at a
/// fixed frame, and stops on completion-marker or budget exhaustion.
fn run_full(
    machine: &mut Spectrum128K,
    total_frames: usize,
    mut key_actions: Vec<KeyAction>,
    tape_start_frame: usize,
    completion_marker: impl Fn(&str) -> bool,
) -> (String, usize) {
    use common_sinclair_zx_spectrum::timing::TIMING_128K;

    key_actions.sort_by_key(|a| a.at_frame);

    let mut transcript = String::new();
    let initial_pc = machine.z80.regs.pc;
    let mut prev_at_rst10 = initial_pc == RST10_ADDR;
    let mut prev_at_pr_all = initial_pc == PR_ALL_ADDR;
    let steps_per_frame = (TIMING_128K.tstates_per_frame / STEP_TSTATES) as usize;
    let mut tape_started = false;
    let mut action_idx = 0usize;
    let mut frames_run = 0usize;
    let mut rst10_hits = 0u64;
    let mut pr_all_hits = 0u64;

    // Capture mode mirrors the 48K test: PR-ALL by default with the
    // control-byte state machine; opt in to legacy RST 16 via
    // EMU198X_FLOAT48K_CAPTURE=rst10.
    let use_pr_all = std::env::var("EMU198X_FLOAT48K_CAPTURE")
        .map(|v| v != "rst10")
        .unwrap_or(true);

    // Control-byte state machine: AT / INK / PAPER / FLASH / BRIGHT /
    // INVERSE / OVER / TAB take 1 or 2 argument bytes that must not be
    // appended to the transcript.
    let mut skip_args: u8 = 0;

    while frames_run < total_frames {
        while action_idx < key_actions.len() && key_actions[action_idx].at_frame == frames_run {
            let action = &key_actions[action_idx];
            set_key(&mut machine.keyboard, action.key, action.pressed);
            action_idx += 1;
        }
        if !tape_started && frames_run >= tape_start_frame {
            machine.tape_play();
            tape_started = true;
        }

        for _ in 0..steps_per_frame {
            machine.advance_tstates(STEP_TSTATES);
            let pc = machine.z80.regs.pc;
            let at_rst10 = pc == RST10_ADDR;
            let at_pr_all = pc == PR_ALL_ADDR;

            let capture = if use_pr_all {
                at_pr_all && !prev_at_pr_all
            } else {
                at_rst10 && !prev_at_rst10
            };

            if capture {
                let ch = machine.z80.regs.a();
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
                    machine.memory.write(SCR_CT_ADDR, 0xFF);
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
#[ignore = "needs local 128K ROMs and Float128k.tap; ~50 s at cycle-accurate \
           tape speed"]
fn float128k_prints_expected_tstate() {
    let rom0_path = rom0_path();
    let rom1_path = rom1_path();
    if !rom0_path.is_file() || !rom1_path.is_file() {
        emu198x_test_skip::skip!(
            "128K ROMs not found at {} / {}",
            rom0_path.display(),
            rom1_path.display()
        );
    }
    let tap_path = system_tests_dir().join("Float128k.tap");
    if !tap_path.is_file() {
        emu198x_test_skip::skip!("Float128k.tap not found at {}", tap_path.display());
    }

    let rom0 = std::fs::read(&rom0_path).expect("128K ROM 0 should read");
    let rom1 = std::fs::read(&rom1_path).expect("128K ROM 1 should read");
    let tap_bytes = std::fs::read(&tap_path).expect("Float128k.tap should read");
    let tap_blocks = parse_tap(&tap_bytes).expect("Float128k.tap should parse");
    let tape_blocks = tap_blocks_to_tape_blocks(tap_blocks);

    let mut machine = Spectrum128K::new();
    machine.memory.load_roms(&rom0, &rom1);
    machine.reset();
    machine.load_tape_blocks(tape_blocks);

    // Schedule: boot phase 0..BOOT_FRAMES, then press ENTER to select
    // Tape Loader from the 128K menu, then call tape_play() shortly after.
    let key_actions = tape_loader_schedule(BOOT_FRAMES);
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
            // The probe iterates and prints `T-state byte\n` for each
            // offset. Stop once one full result line containing a
            // candidate T-state and a non-255 byte has been printed.
            ["14362", "14363", "14364", "14365", "14366", "14367"]
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

    let final_pc = machine.z80.regs.pc;
    eprintln!(
        "\n--- Float128K finished after {} frames ({:.1} s emulated), final PC=${:04X} ---",
        frames_used,
        frames_used as f64 / 50.0,
        final_pc,
    );
    eprintln!("--- full transcript ---\n{}", transcript);

    // Always dump the framebuffer to a PNG so the visual ground truth is
    // available even when the T-state assertion fails.
    let png_path = std::env::temp_dir().join("float128k.png");
    save_framebuffer_png(&machine, &png_path).expect("framebuffer should encode as PNG");
    eprintln!("Framebuffer screenshot: {}", png_path.display());

    // The load chain end-to-end must work. This catches breakage of: boot,
    // 128K menu navigation, ENTER selection of Tape Loader, tape playback
    // at cycle-accurate speed, multi-block load (BASIC + CODE), and the
    // BASIC autostart hook on the 128K-class core.
    //
    // Note: the 128 BASIC ROM's print routine drops the first character
    // of each message line through our PR-ALL hook (the boot menu and
    // Tape Loader prompts route through a different code path in ROM 0
    // than the standard 48 BASIC `PR-ALL` at $09F4). The LOAD progress
    // messages from BASIC's `LD-NAME` routine end up garbled the same
    // way — "Program: Float128K" arrives as "rogram: Float128K". The
    // substring check tolerates this cosmetic loss.
    assert!(
        transcript.contains("Float128K") || transcript.contains("Float128"),
        "expected the BASIC LOAD message to mention 'Float128K' in the transcript\n\
         (boot / menu / tape / LOAD chain failure)\n\
         --- transcript ---\n{transcript}",
    );
    assert!(
        transcript.contains("floatcode") || transcript.contains("loatcode"),
        "expected the second-block CODE LOAD message to mention 'floatcode'\n\
         (the BASIC program LOAD-CODE sub-step did not complete)\n\
         --- transcript ---\n{transcript}",
    );

    let first_non_ff = transcript.lines().find_map(|line| {
        let mut fields = line.split_ascii_whitespace();
        let tstate = fields.next()?.parse::<u32>().ok()?;
        let value = fields.next()?.parse::<u16>().ok()?;
        (value != 255).then_some(tstate)
    });
    assert_eq!(
        first_non_ff,
        Some(FLOAT128K_EXPECTED_TSTATE),
        "Float128K first non-255 reading drifted\n\
         (see FLOAT128K_EXPECTED_TSTATE's evidence note)\n\
         --- transcript ---\n{transcript}",
    );
    eprintln!(
        "\nFloat128K: STRICT PASS — first non-255 reading at {}",
        FLOAT128K_EXPECTED_TSTATE
    );
}

/// Save the current 128K framebuffer as an RGBA PNG using the Spectrum
/// palette. Used for diagnostic ground truth when print capture is silent.
fn save_framebuffer_png(machine: &Spectrum128K, path: &Path) -> std::io::Result<()> {
    let framebuffer = &machine.framebuffer;
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
    assert_eq!(
        tape[0].data,
        vec![0xFF, 0x01, 0x02, 0x03, 0xFF ^ 0x01 ^ 0x02 ^ 0x03]
    );
}

#[test]
fn set_key_press_and_release_match_active_low_matrix() {
    let mut kb: [u8; 8] = [0xFF; 8];
    set_key(&mut kb, SpectrumKey::Enter, true);
    // Enter is (row 6, bit 0).
    assert_eq!(kb[6] & 0x01, 0, "Enter press must clear row 6 bit 0");
    set_key(&mut kb, SpectrumKey::Enter, false);
    assert_eq!(kb[6] & 0x01, 0x01, "Enter release must set row 6 bit 0");
}
