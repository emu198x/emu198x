//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Amstrad CPC's first native window, on the shared `emu198x-ui` harness:
//! wgpu video with `raw`/`lcd`/`crt` filters and the keyboard routed through
//! the harness's general-keyboard path ([`UiSystem::map_keys`]). Compiled only
//! with the `ui` Cargo feature; `main.rs` routes here when no automation flag
//! is given.
//!
//! The CPC's joystick is not a separate device: it is row 9 of the keyboard
//! matrix, which is why a gamepad here maps to the same key names the keyboard
//! path uses rather than to a controller mirror.

use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use emu198x_shell::MachineCore;
use emu198x_shell::{MediaImage, MediaKind, MediaSet};
use emu198x_ui::{
    ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiError, UiSystem, VideoFilter,
};
use machine_amstrad_cpc::{FB_HEIGHT, FB_WIDTH};
use runtime_amstrad_cpc::{AmstradCpcRuntime, Model};

const DEFAULT_SCALE: u32 = 2;

/// One PAL frame: 64 character clocks per line x 312 lines x 4 T-states.
/// Must match the headless runner's budget and not exceed the machine's own
/// `run_frame`, or the harness runs two machine frames per displayed frame.
const FRAME_TICKS_PAL: u64 = 64 * 312 * 4;

/// 64 x 312 microseconds is 19,968 µs, so ~50.08 Hz — the CPC's actual
/// refresh rather than a round 50.
const PAL_FRAME_HZ: f64 = 1_000_000.0 / (64.0 * 312.0);

const FIRMWARE_SIZE: usize = 32 * 1024;

/// Joystick 0, which on a CPC is row 9 of the keyboard matrix. The names are
/// the ones `runtime-amstrad-cpc`'s input layer resolves.
const CPC_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "joyup")),
    (HostControl::Down, ButtonTarget::new(1, "joydown")),
    (HostControl::Left, ButtonTarget::new(1, "joyleft")),
    (HostControl::Right, ButtonTarget::new(1, "joyright")),
    (HostControl::South, ButtonTarget::new(1, "joyfire1")),
    (HostControl::East, ButtonTarget::new(1, "joyfire2")),
]);

const USAGE: &str = "\
Usage: emu198x-amstrad-cpc [OPTIONS]

Options:
    --rom PATH      CPC464 firmware (32 KB: 16 KB OS + 16 KB BASIC); default
                    ~/.emu198x/roms/amstrad-cpc/cpc464.rom
                    (or set EMU198X_CPC464_ROM)
    --tape PATH     .cdt cassette image to insert at start
    --scale N       integer window scale, default 2
    --video MODE    raw | lcd | crt [default: raw]
    --help, -h      show this help

Controls:
    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the CPC keyboard
    Shift / Ctrl    the CPC SHIFT / CONTROL keys
    Arrows          the CPC cursor keys
    Gamepad         joystick 0

Loading a tape:
    Insert it with --tape, then in BASIC type RUN\" and press RETURN twice.
    The CPC drives the cassette motor itself, so playback follows the
    firmware rather than a host transport control.

Examples:
    emu198x-amstrad-cpc
    emu198x-amstrad-cpc --rom cpc464.rom --tape game.cdt --scale 3
";

/// The Amstrad CPC as a [`UiSystem`] for the shared harness. Single-model; a
/// hard reset rebuilds the machine from the firmware the runtime holds, and
/// keeps the cassette in the deck.
struct CpcSystem;

impl UiSystem for CpcSystem {
    type Runtime = AmstradCpcRuntime;

    fn window_title(&self) -> String {
        "Emu198x Amstrad CPC464".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    /// The beam locks to the CRTC's sync pulses, so a partial frame can hold a
    /// half-drawn picture; advance whole frames.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime.machine().map_or((FB_WIDTH, FB_HEIGHT), |machine| {
            (machine.framebuffer_width(), machine.framebuffer_height())
        })
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        FRAME_TICKS_PAL
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / PAL_FRAME_HZ)
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &CPC_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_cpc_keys(code)
    }
}

/// Parsed interactive CLI.
#[derive(Debug, PartialEq, Eq)]
pub struct Cli {
    rom: Option<PathBuf>,
    tape: Option<PathBuf>,
    scale: u32,
    video: VideoFilter,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            rom: None,
            tape: None,
            scale: DEFAULT_SCALE,
            video: VideoFilter::Raw,
        }
    }
}

/// Build the runtime from the CLI and open the window. Returns a string error
/// for the `main.rs` dispatcher.
pub fn run(cli: Cli) -> Result<(), String> {
    let rom_path = cli
        .rom
        .clone()
        .or_else(default_rom_path)
        .ok_or_else(|| "no firmware: pass --rom PATH or set EMU198X_CPC464_ROM".to_owned())?;
    let firmware = read_firmware(&rom_path)?;
    let mut runtime = AmstradCpcRuntime::new(Model::Cpc464, firmware)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;

    if let Some(path) = &cli.tape {
        let bytes = std::fs::read(path)
            .map_err(|err| format!("failed to read tape {}: {err}", path.display()))?;
        let mut media = MediaSet::new();
        media.push(MediaImage::new("tape-1", MediaKind::Tape, &bytes));
        runtime
            .load_media(&media)
            .map_err(|err| format!("tape load failed: {err}"))?;
        println!(
            "Tape inserted: {}. In BASIC, type RUN\" then press RETURN twice.",
            path.display()
        );
    }

    println!(
        "Controls: Esc quit, F12 reset, keyboard typed directly, Shift/Ctrl modifier keys, arrows for cursors, gamepad joystick."
    );
    emu198x_ui::run(CpcSystem, runtime, cli.scale, cli.video)
        .map_err(|err: UiError| err.to_string())
}

/// Parse the interactive CLI. Exits the process on `--help` or a malformed flag.
pub fn parse_cli<I>(args: I) -> Cli
where
    I: IntoIterator<Item = String>,
{
    let mut cli = Cli::default();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--rom" => cli.rom = Some(PathBuf::from(next_arg(&mut iter, "--rom"))),
            "--tape" => cli.tape = Some(PathBuf::from(next_arg(&mut iter, "--tape"))),
            "--scale" => {
                cli.scale = next_arg(&mut iter, "--scale")
                    .parse()
                    .unwrap_or_else(|_| die("--scale requires a positive integer"));
            }
            "--video" => {
                cli.video = next_arg(&mut iter, "--video")
                    .parse()
                    .unwrap_or_else(|_| die("--video expects raw, lcd, or crt"));
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            _ => die(&format!("unknown flag: {arg}")),
        }
    }
    cli
}

fn default_rom_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("EMU198X_CPC464_ROM")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/amstrad-cpc/cpc464.rom"))
}

fn read_firmware(path: &Path) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(path)
        .map_err(|err| format!("failed to read firmware {}: {err}", path.display()))?;
    if bytes.len() != FIRMWARE_SIZE {
        return Err(format!(
            "firmware at {} is {} bytes; expected {FIRMWARE_SIZE} (16 KB OS + 16 KB BASIC)",
            path.display(),
            bytes.len()
        ));
    }
    Ok(bytes)
}

fn next_arg<I: Iterator<Item = String>>(iter: &mut I, flag: &str) -> String {
    iter.next()
        .unwrap_or_else(|| die(&format!("missing value for {flag}")))
}

fn die(message: &str) -> ! {
    eprintln!("error: {message}");
    std::process::exit(1);
}

/// Map a physical host key to its CPC key name, as
/// `runtime-amstrad-cpc`'s `key_for_name` resolves them.
///
/// Only the unshifted legends are mapped: the harness reports Shift as its own
/// key, so a host `Shift+1` arrives as `shift` plus `1` and the matrix does
/// the rest — exactly as the hardware does. The CPC's own `ESC` is
/// unreachable, because the harness owns `Esc` for quit; `Tab` stands in for
/// it, since the CPC's `TAB` is otherwise the less-used of the two.
fn map_cpc_keys(code: KeyCode) -> Option<&'static [&'static str]> {
    Some(match code {
        KeyCode::KeyA => &["a"],
        KeyCode::KeyB => &["b"],
        KeyCode::KeyC => &["c"],
        KeyCode::KeyD => &["d"],
        KeyCode::KeyE => &["e"],
        KeyCode::KeyF => &["f"],
        KeyCode::KeyG => &["g"],
        KeyCode::KeyH => &["h"],
        KeyCode::KeyI => &["i"],
        KeyCode::KeyJ => &["j"],
        KeyCode::KeyK => &["k"],
        KeyCode::KeyL => &["l"],
        KeyCode::KeyM => &["m"],
        KeyCode::KeyN => &["n"],
        KeyCode::KeyO => &["o"],
        KeyCode::KeyP => &["p"],
        KeyCode::KeyQ => &["q"],
        KeyCode::KeyR => &["r"],
        KeyCode::KeyS => &["s"],
        KeyCode::KeyT => &["t"],
        KeyCode::KeyU => &["u"],
        KeyCode::KeyV => &["v"],
        KeyCode::KeyW => &["w"],
        KeyCode::KeyX => &["x"],
        KeyCode::KeyY => &["y"],
        KeyCode::KeyZ => &["z"],
        KeyCode::Digit0 => &["0"],
        KeyCode::Digit1 => &["1"],
        KeyCode::Digit2 => &["2"],
        KeyCode::Digit3 => &["3"],
        KeyCode::Digit4 => &["4"],
        KeyCode::Digit5 => &["5"],
        KeyCode::Digit6 => &["6"],
        KeyCode::Digit7 => &["7"],
        KeyCode::Digit8 => &["8"],
        KeyCode::Digit9 => &["9"],
        KeyCode::Minus => &["-"],
        KeyCode::Equal => &["^"],
        KeyCode::BracketLeft => &["@"],
        KeyCode::BracketRight => &["["],
        KeyCode::Backslash => &["]"],
        KeyCode::Semicolon => &[";"],
        KeyCode::Quote => &[":"],
        KeyCode::Comma => &[","],
        KeyCode::Period => &["."],
        KeyCode::Slash => &["/"],
        KeyCode::Backquote => &["\\"],
        KeyCode::Space => &["space"],
        KeyCode::Enter => &["return"],
        KeyCode::NumpadEnter => &["enter"],
        KeyCode::Backspace | KeyCode::Delete => &["del"],
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &["shift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["control"],
        KeyCode::CapsLock => &["capslock"],
        KeyCode::Tab => &["escape"],
        KeyCode::Home => &["clr"],
        KeyCode::Insert => &["copy"],
        KeyCode::ArrowUp => &["up"],
        KeyCode::ArrowDown => &["down"],
        KeyCode::ArrowLeft => &["left"],
        KeyCode::ArrowRight => &["right"],
        KeyCode::F1 => &["f1"],
        KeyCode::F2 => &["f2"],
        KeyCode::F3 => &["f3"],
        KeyCode::F4 => &["f4"],
        KeyCode::F5 => &["f5"],
        KeyCode::F6 => &["f6"],
        KeyCode::F7 => &["f7"],
        KeyCode::F8 => &["f8"],
        KeyCode::F9 => &["f9"],
        KeyCode::F10 => &["f0"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use runtime_amstrad_cpc::key_for_name;

    #[test]
    fn parse_cli_accepts_rom_tape_scale_video() {
        let cli = parse_cli(
            [
                "--rom",
                "cpc464.rom",
                "--tape",
                "game.cdt",
                "--scale",
                "4",
                "--video",
                "crt",
            ]
            .map(ToOwned::to_owned),
        );
        assert_eq!(cli.rom, Some(PathBuf::from("cpc464.rom")));
        assert_eq!(cli.tape, Some(PathBuf::from("game.cdt")));
        assert_eq!(cli.scale, 4);
        assert_eq!(cli.video, VideoFilter::Crt);
    }

    #[test]
    fn parse_cli_defaults_to_raw_at_scale_two() {
        let cli = parse_cli(Vec::<String>::new());
        assert_eq!(cli, Cli::default());
        assert_eq!(cli.scale, DEFAULT_SCALE);
        assert_eq!(cli.video, VideoFilter::Raw);
    }

    #[test]
    fn every_mapped_key_name_reaches_a_matrix_cell() {
        // A name the runtime cannot resolve is a key that does nothing, and
        // the input layer drops it without complaint — the failure would only
        // show up as a dead key in the window. Walk the whole KeyCode space so
        // a typo cannot hide in a rarely-pressed key.
        let mut mapped = 0;
        for code in ALL_KEY_CODES {
            let Some(names) = map_cpc_keys(*code) else {
                continue;
            };
            mapped += 1;
            for name in names {
                assert!(
                    key_for_name(name).is_some(),
                    "{code:?} maps to {name:?}, which no matrix cell matches"
                );
            }
        }
        assert!(mapped >= 70, "only {mapped} keys mapped; the table shrank");
    }

    #[test]
    fn the_gamepad_reaches_row_nine() {
        // Joystick 0 is row 9 of the keyboard matrix, not a separate device.
        for name in [
            "joyup", "joydown", "joyleft", "joyright", "joyfire1", "joyfire2",
        ] {
            let (row, _) = key_for_name(name).unwrap_or_else(|| panic!("{name} unmapped"));
            assert_eq!(row, 9, "{name}");
        }
    }

    #[test]
    fn the_frame_budget_matches_the_headless_runner() {
        // Two constants for one fact; if they drift, the window and the
        // screenshot pipeline disagree about how fast the machine runs.
        assert_eq!(FRAME_TICKS_PAL, 79_872);
        // ~50.08 Hz, not a round 50.
        assert!((PAL_FRAME_HZ - 50.08).abs() < 0.01, "{PAL_FRAME_HZ}");
    }

    /// Every `KeyCode` the map could plausibly name. Listed rather than
    /// iterated because `KeyCode` is non-exhaustive.
    const ALL_KEY_CODES: &[KeyCode] = &[
        KeyCode::KeyA,
        KeyCode::KeyB,
        KeyCode::KeyC,
        KeyCode::KeyD,
        KeyCode::KeyE,
        KeyCode::KeyF,
        KeyCode::KeyG,
        KeyCode::KeyH,
        KeyCode::KeyI,
        KeyCode::KeyJ,
        KeyCode::KeyK,
        KeyCode::KeyL,
        KeyCode::KeyM,
        KeyCode::KeyN,
        KeyCode::KeyO,
        KeyCode::KeyP,
        KeyCode::KeyQ,
        KeyCode::KeyR,
        KeyCode::KeyS,
        KeyCode::KeyT,
        KeyCode::KeyU,
        KeyCode::KeyV,
        KeyCode::KeyW,
        KeyCode::KeyX,
        KeyCode::KeyY,
        KeyCode::KeyZ,
        KeyCode::Digit0,
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
        KeyCode::Minus,
        KeyCode::Equal,
        KeyCode::BracketLeft,
        KeyCode::BracketRight,
        KeyCode::Backslash,
        KeyCode::Semicolon,
        KeyCode::Quote,
        KeyCode::Comma,
        KeyCode::Period,
        KeyCode::Slash,
        KeyCode::Backquote,
        KeyCode::Space,
        KeyCode::Enter,
        KeyCode::NumpadEnter,
        KeyCode::Backspace,
        KeyCode::Delete,
        KeyCode::ShiftLeft,
        KeyCode::ShiftRight,
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::CapsLock,
        KeyCode::Tab,
        KeyCode::Home,
        KeyCode::Insert,
        KeyCode::ArrowUp,
        KeyCode::ArrowDown,
        KeyCode::ArrowLeft,
        KeyCode::ArrowRight,
        KeyCode::F1,
        KeyCode::F2,
        KeyCode::F3,
        KeyCode::F4,
        KeyCode::F5,
        KeyCode::F6,
        KeyCode::F7,
        KeyCode::F8,
        KeyCode::F9,
        KeyCode::F10,
        // Deliberately unmapped, so the test proves the filter works.
        KeyCode::F11,
        KeyCode::PageUp,
        KeyCode::End,
    ];
}
