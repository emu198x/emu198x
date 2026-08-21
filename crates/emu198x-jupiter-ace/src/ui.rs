//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Jupiter Ace's first native window, on the shared `emu198x-ui` harness:
//! wgpu video with `raw`/`lcd`/`crt` filters and the full keyboard routed
//! through the harness's general-keyboard path ([`UiSystem::map_keys`]). The
//! Ace is keyboard-only — no joystick or mouse, and its sole sound is the
//! one-bit beeper — so it sits alongside the Sinclair machines as a pure
//! keyboard-computer consumer of the harness. Compiled only with the `ui`
//! Cargo feature; `main.rs` routes here when no automation flag is given.

use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use emu198x_ui::{ButtonInputMap, KeyCode, UiError, UiSystem, VideoFilter};
use runtime_jupiter_ace::{JupiterAceRuntime, Model};

const DEFAULT_SCALE: u32 = 3;
/// Z80A @ 3.25 MHz, ~50 Hz PAL → ~65,000 t-states/frame, matching the
/// headless runner's `FRAME_TICKS`.
// Keep <= the machine's run_frame() size, or the harness runs two machine
// frames per displayed frame (~2x too fast). See docs/status/ui-boot-verification.
const FRAME_TICKS: u64 = 64_584;
const PAL_FRAME_HZ: f64 = 50.0;
const ROM_SIZE: usize = 8192;

/// The Ace has no joystick, but the harness still wants a button map — so an
/// empty one. Every key flows through [`UiSystem::map_keys`] instead.
const ACE_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[]);

const USAGE: &str = "\
Usage: emu198x-jupiter-ace [OPTIONS]

Options:
    --rom PATH      Jupiter Ace Forth ROM (8 KB); default
                    ~/.emu198x/roms/jupiter-ace/ace.rom
                    (or set EMU198X_JUPITER_ACE_ROM)
    --ram-kb N      base RAM in KB (3 / 16 / 48) [default: 3]
    --scale N       integer window scale, default 3
    --video MODE    raw | lcd | crt [default: raw]
    --help, -h      show this help

Controls:
    Esc             quit
    F12             hard reset
    A-Z 0-9 Space   the Ace keyboard
    Shift           CAPS SHIFT (hold with another key)
    Ctrl            SYMBOL SHIFT (the red symbol layer)
    Enter           ENTER

Examples:
    emu198x-jupiter-ace
    emu198x-jupiter-ace --rom ace.rom --scale 4
";

/// The Jupiter Ace as a [`UiSystem`] for the shared harness. Keyboard-only, so
/// it carries no state — a hard reset rebuilds the machine from the firmware
/// the runtime already holds. The RAM size is fixed at construction.
struct JupiterAceSystem;

impl UiSystem for JupiterAceSystem {
    type Runtime = JupiterAceRuntime;

    fn window_title(&self) -> String {
        "Emu198x Jupiter Ace".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
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
            .unwrap_or((320, 288))
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        FRAME_TICKS
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / PAL_FRAME_HZ)
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &ACE_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_ace_keys(code)
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
            ram_kb: 3,
            scale: DEFAULT_SCALE,
            video: VideoFilter::Raw,
        }
    }
}

fn model_for(ram_kb: usize) -> Model {
    match ram_kb {
        n if n >= 48 => Model::Ace48k,
        n if n >= 16 => Model::Ace16k,
        _ => Model::Ace3k,
    }
}

/// Build the runtime from the CLI and open the window. Returns a string error
/// for the `main.rs` dispatcher.
pub fn run(cli: Cli) -> Result<(), String> {
    let rom_path = cli
        .rom
        .clone()
        .or_else(default_rom_path)
        .ok_or_else(|| "no ROM: pass --rom PATH or set EMU198X_JUPITER_ACE_ROM".to_owned())?;
    let rom = read_rom(&rom_path)?;
    let runtime = JupiterAceRuntime::new(model_for(cli.ram_kb), rom)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;

    println!(
        "Controls: Esc quit, F12 reset, A-Z/0-9/Space keyboard, Shift CAPS SHIFT, Ctrl SYMBOL SHIFT, Enter ENTER."
    );
    emu198x_ui::run(JupiterAceSystem, runtime, cli.scale, cli.video)
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
    if let Ok(path) = env::var("EMU198X_JUPITER_ACE_ROM")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/jupiter-ace/ace.rom"))
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

/// Map a physical host key to its Jupiter Ace key name (matched by
/// `runtime-jupiter-ace`'s `key_from_name`). The Ace's two shift keys mirror
/// the Spectrum's: CAPS SHIFT (host Shift) and the red SYMBOL SHIFT (host
/// Ctrl); symbols and keywords are Shift-layer combos reached by holding a
/// shift with another key, so only the base keys need mapping here.
fn map_ace_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Enter | KeyCode::NumpadEnter => &["enter"],
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &["shift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["symbol"],
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
            "ace.rom".to_owned(),
            "--ram-kb".to_owned(),
            "16".to_owned(),
            "--scale".to_owned(),
            "4".to_owned(),
            "--video".to_owned(),
            "crt".to_owned(),
        ]);
        assert_eq!(cli.rom, Some(PathBuf::from("ace.rom")));
        assert_eq!(cli.ram_kb, 16);
        assert_eq!(cli.scale, 4);
        assert_eq!(cli.video, VideoFilter::Crt);
    }

    #[test]
    fn model_selects_by_ram() {
        assert_eq!(model_for(3), Model::Ace3k);
        assert_eq!(model_for(16), Model::Ace16k);
        assert_eq!(model_for(48), Model::Ace48k);
    }

    #[test]
    fn maps_keys_and_both_shifts() {
        assert_eq!(map_ace_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_ace_keys(KeyCode::Digit5), Some(&["5"][..]));
        assert_eq!(map_ace_keys(KeyCode::Enter), Some(&["enter"][..]));
        assert_eq!(map_ace_keys(KeyCode::Space), Some(&["space"][..]));
        assert_eq!(map_ace_keys(KeyCode::ShiftLeft), Some(&["shift"][..]));
        assert_eq!(map_ace_keys(KeyCode::ControlLeft), Some(&["symbol"][..]));
        // Keys with no Ace position are ignored.
        assert_eq!(map_ace_keys(KeyCode::Tab), None);
    }
}
