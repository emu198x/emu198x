//! Interactive UI mode — the default when no automation flag is present.
//!
//! The ZX80's first native window, on the shared `emu198x-ui` harness: wgpu
//! video with `raw`/`lcd`/`crt` filters and the membrane keyboard routed
//! through the harness's general-keyboard path ([`UiSystem::map_keys`]). Like
//! its ZX81 sibling the ZX80 is keyboard-only — no sound, joystick, or mouse.
//! Compiled only with the `ui` Cargo feature; `main.rs` routes here when no
//! automation flag is given.

use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use emu198x_ui::{ButtonInputMap, KeyCode, UiError, UiSystem, VideoFilter};
use runtime_sinclair_zx80::{Model, Zx80Runtime};

const DEFAULT_SCALE: u32 = 3;
/// Framebuffer pixels per second: two per 3.25 MHz T-state.
const PIXEL_CLOCK_HZ: f64 = 6_500_000.0;
/// Framebuffer lines a PAL set spreads over its full height. The core is
/// progressive, so this is the active line count itself.
const PAL_ACTIVE_LINES: f64 = 288.0;

/// PAL TV-clock ticks per frame (207 per line × 312 lines), matching the
/// headless runner's `FRAME_TICKS_PAL`.
const FRAME_TICKS_PAL: u64 = 207 * 312;
const PAL_FRAME_HZ: f64 = 50.0;
/// The ZX80's monitor ROM is 4 KB (half the ZX81's).
const ROM_SIZE: usize = 4 * 1024;

/// The ZX80 has no joystick; the harness still wants a button map, so an empty
/// one. Every key flows through [`UiSystem::map_keys`].
const ZX80_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[]);

const USAGE: &str = "\
Usage: emu198x-sinclair-zx80 [OPTIONS]

Options:
    --rom PATH      ZX80 monitor ROM (4 KB); default ~/.emu198x/roms/sinclair-zx80/zx80.rom
                    (or set EMU198X_ZX80_ROM)
    --scale N       integer window scale, default 3
    --video MODE    raw | lcd | crt [default: raw]
    --help, -h      show this help

Controls:
    Esc             quit
    F12             hard reset
    A-Z 0-9 . Space the ZX80 membrane keyboard
    Shift           SHIFT (the function/symbol layer — hold with another key)
    Enter           NEWLINE

Examples:
    emu198x-sinclair-zx80
    emu198x-sinclair-zx80 --rom zx80.rom --scale 4
";

/// The ZX80 as a [`UiSystem`] for the shared harness. Keyboard-only and
/// single-model, so it carries no state — a hard reset rebuilds the machine
/// from the firmware the runtime already holds.
struct Zx80System;

impl UiSystem for Zx80System {
    type Runtime = Zx80Runtime;

    fn window_title(&self) -> String {
        "Emu198x ZX80".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    /// Two pixels per 3.25 MHz T-state, filling PAL's 288 active lines once.
    /// Works out at about 1.14 — the ZX80/ZX81 raster puts 256 pixels of
    /// characters across roughly three quarters of the screen's width but
    /// only 192 lines down two thirds of its height, so the pixels are wider
    /// than they are tall. Showing the 320×240 framebuffer square renders the
    /// character area at 1.33:1 where a set gives 1.52:1.
    fn pixel_aspect_ratio(&self, _runtime: &Self::Runtime) -> Option<f32> {
        emu198x_shell::display::pixel_aspect_ratio(
            emu198x_shell::machine::Region::Pal,
            PIXEL_CLOCK_HZ,
            PAL_ACTIVE_LINES,
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
            .unwrap_or((256, 192))
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        FRAME_TICKS_PAL
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / PAL_FRAME_HZ)
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &ZX80_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_zx80_keys(code)
    }
}

/// Parsed interactive CLI.
#[derive(Debug, PartialEq, Eq)]
pub struct Cli {
    rom: Option<PathBuf>,
    scale: u32,
    video: VideoFilter,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            rom: None,
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
        .ok_or_else(|| "no ROM: pass --rom PATH or set EMU198X_ZX80_ROM".to_owned())?;
    let rom = read_rom(&rom_path)?;
    let runtime = Zx80Runtime::new(Model::Zx80, rom)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;

    println!(
        "Controls: Esc quit, F12 reset, A-Z/0-9/./Space keyboard, Shift SHIFT, Enter NEWLINE."
    );
    emu198x_ui::run(Zx80System, runtime, cli.scale, cli.video)
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
    if let Ok(path) = env::var("EMU198X_ZX80_ROM")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/sinclair-zx80/zx80.rom"))
}

fn read_rom(path: &Path) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(path)
        .map_err(|err| format!("failed to read ROM {}: {err}", path.display()))?;
    if bytes.len() != ROM_SIZE {
        return Err(format!(
            "ROM at {} is {} bytes; expected {ROM_SIZE}",
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

/// Map a physical host key to its ZX80 membrane key name. The ZX80's symbols
/// and keywords are Shift-layer combos reached by holding Shift, so only the
/// base keys need mapping.
fn map_zx80_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Space => &["space"],
        KeyCode::Period => &["."],
        KeyCode::Enter | KeyCode::NumpadEnter => &["newline"],
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &["shift"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_accepts_rom_scale_video() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "zx80.rom".to_owned(),
            "--scale".to_owned(),
            "4".to_owned(),
            "--video".to_owned(),
            "crt".to_owned(),
        ]);
        assert_eq!(cli.rom, Some(PathBuf::from("zx80.rom")));
        assert_eq!(cli.scale, 4);
        assert_eq!(cli.video, VideoFilter::Crt);
    }

    #[test]
    fn maps_membrane_keys_and_shift() {
        assert_eq!(map_zx80_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_zx80_keys(KeyCode::Digit5), Some(&["5"][..]));
        assert_eq!(map_zx80_keys(KeyCode::Enter), Some(&["newline"][..]));
        assert_eq!(map_zx80_keys(KeyCode::ShiftLeft), Some(&["shift"][..]));
        assert_eq!(map_zx80_keys(KeyCode::Tab), None);
    }
}
