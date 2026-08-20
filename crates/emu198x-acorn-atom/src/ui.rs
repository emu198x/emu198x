//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Acorn Atom's first native window, on the shared `emu198x-ui` harness:
//! wgpu video with `raw`/`lcd`/`crt` filters and the keyboard routed through
//! the harness's general-keyboard path ([`UiSystem::map_keys`]). The Atom is
//! keyboard-only — no joystick or mouse — so it carries an empty button map and
//! routes every key through `map_keys`. Compiled only with the `ui` Cargo
//! feature; `main.rs` routes here when no automation flag is given.

use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use emu198x_shell::MachineCore;
use emu198x_ui::{ButtonInputMap, KeyCode, UiError, UiSystem, VideoFilter};
use runtime_acorn_atom::{AtomRuntime, Model};

const DEFAULT_SCALE: u32 = 3;
/// 6502 @ 1 MHz, 50 Hz → 20,000 cycles/frame, matching the headless runner's
/// `FRAME_TICKS`.
const FRAME_TICKS: u64 = 20_000;
const FRAME_HZ: f64 = 50.0;
const ROM_SIZE: usize = 24 * 1024;

/// The Atom has no joystick, but the harness still wants a button map — so an
/// empty one. Every key flows through [`UiSystem::map_keys`] instead.
const ATOM_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[]);

const USAGE: &str = "\
Usage: emu198x-acorn-atom [OPTIONS]

Options:
    --rom PATH      24 KB combined ROM (BASIC1 + FP + BASIC2 + OS); default
                    ~/.emu198x/roms/acorn-atom/atom.rom
                    (or set EMU198X_ACORN_ATOM_ROM)
    --ram-kb N      base RAM in KB (~2, or >=12 for a fully-expanded 32K) [default: 2]
    --scale N       integer window scale, default 3
    --video MODE    raw | lcd | crt [default: raw]
    --help, -h      show this help

Controls:
    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the Atom keyboard
    Enter           RETURN

Examples:
    emu198x-acorn-atom
    emu198x-acorn-atom --rom atom.rom --scale 4
";

/// The Acorn Atom as a [`UiSystem`] for the shared harness. Keyboard-only; a
/// hard reset rebuilds the machine from the firmware the runtime already holds.
/// The RAM size is fixed at construction.
struct AtomSystem;

impl UiSystem for AtomSystem {
    type Runtime = AtomRuntime;

    fn window_title(&self) -> String {
        "Emu198x Acorn Atom".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The Atom's MC6847 drove a 4:3 TV; its framebuffer stretches to fill it.

    /// Two pixels per 3.58 MHz clock period, which the VDG's own documented
    /// figures give: 128 active clock periods carrying 256 pixels.
    fn pixel_aspect_ratio(&self, runtime: &Self::Runtime) -> Option<f32> {
        emu198x_shell::display::pixel_aspect_for_region(
            runtime.profile().region,
            motorola_vdg_6847::PAL_PIXEL_CLOCK_HZ,
            motorola_vdg_6847::NTSC_PIXEL_CLOCK_HZ,
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
            .unwrap_or((372, 243))
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        FRAME_TICKS
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / FRAME_HZ)
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &ATOM_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_atom_keys(code)
    }
}

/// Parsed interactive CLI.
#[derive(Debug, PartialEq, Eq)]
pub struct Cli {
    rom: Option<PathBuf>,
    ram_kb: usize,
    scale: u32,
    video: VideoFilter,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            rom: None,
            ram_kb: 2,
            scale: DEFAULT_SCALE,
            video: VideoFilter::Raw,
        }
    }
}

fn model_for(ram_kb: usize) -> Model {
    if ram_kb >= 12 {
        Model::AtomFull
    } else {
        Model::AtomBase
    }
}

/// Build the runtime from the CLI and open the window. Returns a string error
/// for the `main.rs` dispatcher.
pub fn run(cli: Cli) -> Result<(), String> {
    let rom_path = cli
        .rom
        .clone()
        .or_else(default_rom_path)
        .ok_or_else(|| "no ROM: pass --rom PATH or set EMU198X_ACORN_ATOM_ROM".to_owned())?;
    let rom = read_rom(&rom_path)?;
    let runtime = AtomRuntime::new(model_for(cli.ram_kb), rom)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;

    println!("Controls: Esc quit, F12 reset, A-Z/0-9/keyboard typed directly, Enter RETURN.");
    emu198x_ui::run(AtomSystem, runtime, cli.scale, cli.video)
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
            "--ram-kb" => {
                cli.ram_kb = next_arg(&mut iter, "--ram-kb")
                    .parse()
                    .unwrap_or_else(|_| die("--ram-kb requires a non-negative integer"));
            }
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
    if let Ok(path) = env::var("EMU198X_ACORN_ATOM_ROM")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/acorn-atom/atom.rom"))
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

/// Map a physical host key to its Atom key name (matched by
/// `runtime-acorn-atom`'s `key_from_name`). The Atom's keyboard is uppercase,
/// so the unshifted letter and digit keys cover ordinary typing; only the keys
/// the runtime scans are mapped here.
fn map_atom_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Comma => &[","],
        KeyCode::Semicolon => &[";"],
        KeyCode::Period => &["."],
        KeyCode::Slash => &["/"],
        KeyCode::Quote => &["@"],
        KeyCode::Space => &["space"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["return"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_accepts_rom_ram_scale_video() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "atom.rom".to_owned(),
            "--ram-kb".to_owned(),
            "12".to_owned(),
            "--scale".to_owned(),
            "4".to_owned(),
            "--video".to_owned(),
            "crt".to_owned(),
        ]);
        assert_eq!(cli.rom, Some(PathBuf::from("atom.rom")));
        assert_eq!(cli.ram_kb, 12);
        assert_eq!(cli.scale, 4);
        assert_eq!(cli.video, VideoFilter::Crt);
    }

    #[test]
    fn model_selects_by_ram() {
        assert_eq!(model_for(2), Model::AtomBase);
        assert_eq!(model_for(12), Model::AtomFull);
    }

    #[test]
    fn maps_supported_keys() {
        assert_eq!(map_atom_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_atom_keys(KeyCode::Digit3), Some(&["3"][..]));
        assert_eq!(map_atom_keys(KeyCode::Enter), Some(&["return"][..]));
        assert_eq!(map_atom_keys(KeyCode::Quote), Some(&["@"][..]));
        // Keys the Atom runtime doesn't scan are ignored.
        assert_eq!(map_atom_keys(KeyCode::Tab), None);
    }
}
