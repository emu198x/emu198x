//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Acorn Electron's first native window, on the shared `emu198x-ui`
//! harness: wgpu video with `raw`/`lcd`/`crt` filters, framed ULA audio, and
//! the keyboard routed through the harness's general-keyboard path
//! ([`UiSystem::map_keys`]). The Electron is keyboard-only — no joystick — so it
//! carries an empty button map and routes every key through `map_keys`.
//! Compiled only with the `ui` Cargo feature; `main.rs` routes here when no
//! automation flag is given.

use std::env;
use std::path::PathBuf;
use std::time::Duration;

use emu198x_shell::MachineCore;
use emu198x_ui::{ButtonInputMap, KeyCode, UiError, UiSystem, VideoFilter};
use runtime_acorn_electron::{ElectronRuntime, Model};

/// Framebuffer pixels per second.
const PIXEL_CLOCK_HZ: f64 = 16_000_000.0;

const DEFAULT_SCALE: u32 = 3;
/// 6502A @ 2 MHz nominal, 50 Hz → 40,000 cycles/frame, matching the headless
/// runner's `FRAME_TICKS_PAL`.
// Keep <= the machine's run_frame() size, or the harness runs two machine
// frames per displayed frame (~2x too fast). See docs/status/ui-boot-verification.
const FRAME_TICKS_PAL: u64 = 39_936;
const PAL_FRAME_HZ: f64 = 50.0;
const ROM_SIZE: usize = 16 * 1024;

/// The Electron has no joystick, but the harness still wants a button map — so
/// an empty one. Every key flows through [`UiSystem::map_keys`] instead.
const ELECTRON_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[]);

const USAGE: &str = "\
Usage: emu198x-acorn-electron [OPTIONS]

Options:
    --os PATH       Electron OS ROM (16 KB); default
                    ~/.emu198x/roms/acorn-electron/os.rom (or set EMU198X_ELECTRON_OS)
    --basic PATH    BBC BASIC II ROM (16 KB); default
                    ~/.emu198x/roms/acorn-electron/basic.rom
                    (or set EMU198X_ELECTRON_BASIC)
    --scale N       integer window scale, default 3
    --video MODE    raw | lcd | crt [default: raw]
    --help, -h      show this help

Controls:
    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the Electron keyboard (cursor keys are real Electron keys)
    Shift / Ctrl    the Electron SHIFT / CTRL keys; Alt = FUNC; End = COPY

Examples:
    emu198x-acorn-electron
    emu198x-acorn-electron --scale 2 --video crt
";

/// The Acorn Electron as a [`UiSystem`] for the shared harness. Keyboard-only,
/// single-model; a hard reset rebuilds the machine from the firmware the
/// runtime already holds.
struct ElectronSystem;

impl UiSystem for ElectronSystem {
    type Runtime = ElectronRuntime;

    fn window_title(&self) -> String {
        "Emu198x Acorn Electron".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The Electron drove a 4:3 TV; its framebuffer stretches to fill it.

    /// 16 MHz, as on the BBC — the ULA keeps the same dot rate and the core
    /// scales every mode into one 640-wide buffer.
    fn pixel_aspect_ratio(&self, runtime: &Self::Runtime) -> Option<f32> {
        emu198x_shell::display::pixel_aspect_for_region(
            runtime.profile().region,
            PIXEL_CLOCK_HZ,
            PIXEL_CLOCK_HZ,
        )
    }

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| (machine.framebuffer_width(), machine.framebuffer_height()))
            .unwrap_or((640, 256))
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        FRAME_TICKS_PAL
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / PAL_FRAME_HZ)
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &ELECTRON_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_electron_keys(code)
    }
}

/// Parsed interactive CLI.
#[derive(Debug, PartialEq, Eq)]
pub struct Cli {
    os: Option<PathBuf>,
    basic: Option<PathBuf>,
    scale: u32,
    video: VideoFilter,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            os: None,
            basic: None,
            scale: DEFAULT_SCALE,
            video: VideoFilter::Raw,
        }
    }
}

/// Build the runtime from the CLI and open the window. Returns a string error
/// for the `main.rs` dispatcher.
pub fn run(cli: Cli) -> Result<(), String> {
    let os = read_required(cli.os.clone(), "OS", "OS", "os.rom")?;
    let basic = read_required(cli.basic.clone(), "BASIC", "BASIC", "basic.rom")?;
    let runtime = ElectronRuntime::new(Model::Electron, os, basic)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;

    println!(
        "Controls: Esc quit, F12 reset, keyboard typed directly (cursor keys are Electron keys, Alt FUNC, End COPY)."
    );
    emu198x_ui::run(ElectronSystem, runtime, cli.scale, cli.video)
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
            "--os" => cli.os = Some(PathBuf::from(next_arg(&mut iter, "--os"))),
            "--basic" => cli.basic = Some(PathBuf::from(next_arg(&mut iter, "--basic"))),
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

fn default_rom_path(env_kind: &str, default_file: &str) -> Option<PathBuf> {
    if let Ok(path) = env::var(format!("EMU198X_ELECTRON_{env_kind}"))
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(format!(".emu198x/roms/acorn-electron/{default_file}")))
}

fn read_required(
    explicit: Option<PathBuf>,
    kind: &str,
    env_kind: &str,
    default_file: &str,
) -> Result<Vec<u8>, String> {
    let path = explicit
        .or_else(|| default_rom_path(env_kind, default_file))
        .ok_or_else(|| {
            format!("no {kind} ROM: pass its flag or set EMU198X_ELECTRON_{env_kind}")
        })?;
    let bytes = std::fs::read(&path)
        .map_err(|err| format!("failed to read {kind} ROM {}: {err}", path.display()))?;
    if bytes.len() != ROM_SIZE {
        return Err(format!(
            "{kind} ROM at {} is {} bytes; expected {ROM_SIZE}",
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

/// Map a physical host key to its Electron key name (matched by
/// `runtime-acorn-electron`'s `key_to_matrix`). The cursor keys are genuine
/// matrix cells, so they type. Host Alt is the Electron FUNC key and host End
/// is COPY. The Electron's own Escape key is unreachable — the harness owns Esc
/// for quit. Symbols shifted on a modern host are omitted.
fn map_electron_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Semicolon => &[";"],
        KeyCode::Comma => &[","],
        KeyCode::Period => &["."],
        KeyCode::Slash => &["/"],
        KeyCode::Backslash => &["\\"],
        KeyCode::Space => &["space"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["return"],
        KeyCode::Backspace | KeyCode::Delete => &["delete"],
        KeyCode::End => &["copy"],
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &["shift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["ctrl"],
        KeyCode::AltLeft | KeyCode::AltRight => &["func"],
        KeyCode::ArrowLeft => &["left"],
        KeyCode::ArrowRight => &["right"],
        KeyCode::ArrowUp => &["up"],
        KeyCode::ArrowDown => &["down"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_accepts_os_basic_scale_video() {
        let cli = parse_cli([
            "--os".to_owned(),
            "os.rom".to_owned(),
            "--basic".to_owned(),
            "basic.rom".to_owned(),
            "--scale".to_owned(),
            "2".to_owned(),
            "--video".to_owned(),
            "crt".to_owned(),
        ]);
        assert_eq!(cli.os, Some(PathBuf::from("os.rom")));
        assert_eq!(cli.basic, Some(PathBuf::from("basic.rom")));
        assert_eq!(cli.scale, 2);
        assert_eq!(cli.video, VideoFilter::Crt);
    }

    #[test]
    fn cursor_keys_type_and_func_copy_map() {
        assert_eq!(map_electron_keys(KeyCode::ArrowUp), Some(&["up"][..]));
        assert_eq!(map_electron_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_electron_keys(KeyCode::Enter), Some(&["return"][..]));
        assert_eq!(map_electron_keys(KeyCode::AltLeft), Some(&["func"][..]));
        assert_eq!(map_electron_keys(KeyCode::End), Some(&["copy"][..]));
        // Keys with no Electron position are ignored.
        assert_eq!(map_electron_keys(KeyCode::Tab), None);
    }
}
