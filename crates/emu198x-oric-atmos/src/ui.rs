//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Oric-1 / Atmos's first native window, on the shared `emu198x-ui`
//! harness: wgpu video with `raw`/`lcd`/`crt` filters and the full 8×8 keyboard
//! routed through the harness's general-keyboard path ([`UiSystem::map_keys`]).
//! The Oric is keyboard-led; its IJK joystick is an add-on reached by a real
//! gamepad through [`UiSystem::button_map`] (the harness drains gamepad events
//! through the button map regardless of the keyboard path). The cursor keys are
//! genuine keyboard cells on the Oric, so they type — they don't drive the
//! stick. Compiled only with the `ui` Cargo feature; `main.rs` routes here when
//! no automation flag is given.

use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use emu198x_ui::{
    ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiError, UiSystem, VideoFilter,
};
use machine_oric_atmos::{FB_HEIGHT, FB_WIDTH};
use runtime_oric_atmos::{Model, OricRuntime};

const DEFAULT_SCALE: u32 = 3;
/// 6502 @ 1 MHz, 50 Hz PAL → 20,000 cycles/frame, matching the headless
/// runner's `FRAME_TICKS`.
// Keep <= the machine's run_frame() size, or the harness runs two machine
// frames per displayed frame (~2x too fast). See docs/status/ui-boot-verification.
const FRAME_TICKS: u64 = 19_968;
const PAL_FRAME_HZ: f64 = 50.0;
const ROM_SIZE: usize = 16 * 1024;

/// Player-1 IJK (left) stick: four directions plus fire, named as
/// `runtime-oric-atmos`'s controller mirror expects. A real gamepad reaches
/// these through the button map; keyboard cursor keys deliberately do not, so
/// they keep their Oric keyboard meaning.
const ORIC_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
]);

const USAGE: &str = "\
Usage: emu198x-oric-atmos [OPTIONS]

Options:
    --rom PATH      16 KB BASIC + OS ROM; default
                    ~/.emu198x/roms/oric/oric.rom (or set EMU198X_ORIC_ROM)
    --model NAME    oric-1 | atmos [default: atmos]
    --scale N       integer window scale, default 3
    --video MODE    raw | lcd | crt [default: raw]
    --help, -h      show this help

Controls:
    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the Oric keyboard (cursor keys are real Oric keys)
    Shift / Ctrl    the Oric shift / control keys
    Gamepad         IJK joystick (player 1, left stick)

Examples:
    emu198x-oric-atmos
    emu198x-oric-atmos --model oric-1 --scale 4
";

/// The Oric-1 / Atmos as a [`UiSystem`] for the shared harness. The model is
/// fixed at construction; a hard reset rebuilds the machine from the firmware
/// the runtime already holds.
struct OricSystem;

impl UiSystem for OricSystem {
    type Runtime = OricRuntime;

    fn window_title(&self) -> String {
        "Emu198x Oric Atmos".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The Oric drove a 4:3 TV; its 240×224 framebuffer stretches to fill it.
    fn display_aspect_ratio(&self) -> Option<f32> {
        Some(4.0 / 3.0)
    }

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, _runtime: &Self::Runtime) -> (u32, u32) {
        (FB_WIDTH, FB_HEIGHT)
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        FRAME_TICKS
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / PAL_FRAME_HZ)
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &ORIC_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_oric_keys(code)
    }
}

/// Parsed interactive CLI.
#[derive(Debug, PartialEq, Eq)]
pub struct Cli {
    rom: Option<PathBuf>,
    model: Model,
    scale: u32,
    video: VideoFilter,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            rom: None,
            model: Model::Atmos,
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
        .ok_or_else(|| "no ROM: pass --rom PATH or set EMU198X_ORIC_ROM".to_owned())?;
    let rom = read_rom(&rom_path)?;
    let runtime = OricRuntime::new(cli.model, rom)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;

    println!(
        "Controls: Esc quit, F12 reset, keyboard typed directly (cursor keys are Oric keys), gamepad IJK joystick."
    );
    emu198x_ui::run(OricSystem, runtime, cli.scale, cli.video)
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
            "--model" => {
                cli.model = match next_arg(&mut iter, "--model").as_str() {
                    "oric-1" | "oric1" => Model::Oric1,
                    "atmos" => Model::Atmos,
                    other => die(&format!("--model expects oric-1|atmos, got {other}")),
                };
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
    if let Ok(path) = env::var("EMU198X_ORIC_ROM")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/oric/oric.rom"))
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

/// Map a physical host key to its Oric key name (matched by
/// `runtime-oric-atmos`'s `key_to_matrix`). The cursor keys are real Oric
/// keyboard cells, so they map here rather than to the joystick. Shifted
/// symbols are reached by holding a shift with another key, so only the
/// unshifted legends need mapping.
fn map_oric_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Period => &["."],
        KeyCode::Semicolon => &[";"],
        KeyCode::Minus => &["-"],
        KeyCode::Quote => &["'"],
        KeyCode::Backslash => &["\\"],
        KeyCode::Slash => &["/"],
        KeyCode::Equal => &["="],
        KeyCode::BracketLeft => &["["],
        KeyCode::BracketRight => &["]"],
        KeyCode::Space => &["space"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["return"],
        KeyCode::ShiftLeft => &["shift"],
        KeyCode::ShiftRight => &["rshift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["ctrl"],
        KeyCode::Backspace | KeyCode::Delete => &["delete"],
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
    fn parse_cli_accepts_rom_model_scale_video() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "oric.rom".to_owned(),
            "--model".to_owned(),
            "oric-1".to_owned(),
            "--scale".to_owned(),
            "4".to_owned(),
            "--video".to_owned(),
            "crt".to_owned(),
        ]);
        assert_eq!(cli.rom, Some(PathBuf::from("oric.rom")));
        assert_eq!(cli.model, Model::Oric1);
        assert_eq!(cli.scale, 4);
        assert_eq!(cli.video, VideoFilter::Crt);
    }

    #[test]
    fn cursor_keys_are_keyboard_cells_not_joystick() {
        // The Oric's cursor keys are genuine keyboard cells, so they go
        // through the keyboard path and keep their Oric names.
        assert_eq!(map_oric_keys(KeyCode::ArrowLeft), Some(&["left"][..]));
        assert_eq!(map_oric_keys(KeyCode::ArrowUp), Some(&["up"][..]));
        assert_eq!(map_oric_keys(KeyCode::KeyH), Some(&["h"][..]));
        assert_eq!(map_oric_keys(KeyCode::Enter), Some(&["return"][..]));
        assert_eq!(map_oric_keys(KeyCode::ShiftRight), Some(&["rshift"][..]));
        assert_eq!(map_oric_keys(KeyCode::ControlLeft), Some(&["ctrl"][..]));
        // Keys with no Oric position are ignored.
        assert_eq!(map_oric_keys(KeyCode::Tab), None);
    }
}
