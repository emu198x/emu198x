//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Memotech MTX's first native window, on the shared `emu198x-ui` harness:
//! wgpu video with `raw`/`lcd`/`crt` filters and the full keyboard routed
//! through the harness's general-keyboard path ([`UiSystem::map_keys`]). The
//! MTX is keyboard-led; its joysticks share the keyboard matrix and are reached
//! by a real gamepad through [`UiSystem::button_map`] (the harness drains
//! gamepad events through the button map regardless of the keyboard path). The
//! cursor keys are genuine matrix cells, so they type rather than driving the
//! stick. Compiled only with the `ui` Cargo feature; `main.rs` routes here when
//! no automation flag is given.

use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use emu198x_ui::{
    ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiError, UiSystem, VideoFilter,
};
use runtime_memotech_mtx::{Model, MtxRuntime};

const DEFAULT_SCALE: u32 = 3;
/// Z80 @ 4 MHz, 50 Hz PAL → 80,000 t-states/frame, matching the headless
/// runner's `FRAME_TICKS_PAL`.
// Keep <= the machine's run_frame() size, or the harness runs two machine
// frames per displayed frame (~2x too fast). See docs/status/ui-boot-verification.
const FRAME_TICKS_PAL: u64 = 79_700;
const PAL_FRAME_HZ: f64 = 50.0;
/// 8 KB OS plus paged ROMs: any multiple of 8 KB, at least 16 KB.
const MIN_ROM_SIZE: usize = 16 * 1024;
const ROM_PAGE: usize = 0x2000;

/// Player-1 joystick: four directions plus fire, named as
/// `runtime-memotech-mtx`'s controller mirror expects. The MTX joystick shares
/// the keyboard matrix, so a real gamepad reaches it through the button map;
/// keyboard cursor keys deliberately do not, so they keep their MTX meaning.
const MTX_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
]);

const USAGE: &str = "\
Usage: emu198x-memotech-mtx [OPTIONS]

Options:
    --rom PATH      MTX ROM: 8 KB OS + paged ROMs (BASIC, ASSEM…); default
                    ~/.emu198x/roms/memotech-mtx/mtx.rom (or set EMU198X_MTX_ROM)
    --model KIND    mtx500 | mtx512 [default: mtx500]
    --scale N       integer window scale, default 3
    --video MODE    raw | lcd | crt [default: raw]
    --help, -h      show this help

Controls:
    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the MTX keyboard (cursor keys are real MTX keys)
    Shift / Ctrl    the MTX shift / control keys
    Gamepad         joystick (player 1)

Examples:
    emu198x-memotech-mtx
    emu198x-memotech-mtx --model mtx512 --scale 4
";

/// The Memotech MTX as a [`UiSystem`] for the shared harness. The model is
/// fixed at construction; a hard reset rebuilds the machine from the firmware
/// the runtime already holds.
struct MtxSystem;

impl UiSystem for MtxSystem {
    type Runtime = MtxRuntime;

    fn window_title(&self) -> String {
        "Emu198x Memotech MTX".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The MTX's TMS9918 drove a 4:3 TV; its 288×240 framebuffer stretches to
    // fill it.

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| (machine.framebuffer_width(), machine.framebuffer_height()))
            .unwrap_or((288, 240))
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        FRAME_TICKS_PAL
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / PAL_FRAME_HZ)
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &MTX_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_mtx_keys(code)
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
            model: Model::Mtx500,
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
        .ok_or_else(|| "no ROM: pass --rom PATH or set EMU198X_MTX_ROM".to_owned())?;
    let rom = read_rom(&rom_path)?;
    let runtime = MtxRuntime::new(cli.model, rom)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;

    println!(
        "Controls: Esc quit, F12 reset, keyboard typed directly (cursor keys are MTX keys), gamepad joystick."
    );
    emu198x_ui::run(MtxSystem, runtime, cli.scale, cli.video)
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
                    "mtx500" => Model::Mtx500,
                    "mtx512" => Model::Mtx512,
                    other => die(&format!("--model expects mtx500|mtx512, got {other}")),
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
    if let Ok(path) = env::var("EMU198X_MTX_ROM")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/memotech-mtx/mtx.rom"))
}

fn read_rom(path: &Path) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(path)
        .map_err(|err| format!("failed to read ROM {}: {err}", path.display()))?;
    if bytes.len() < MIN_ROM_SIZE || !bytes.len().is_multiple_of(ROM_PAGE) {
        return Err(format!(
            "ROM at {} is {} bytes; expected the 8 KB OS plus 8 KB paged ROMs \
             (a multiple of 8192, ≥ {MIN_ROM_SIZE})",
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

/// Map a physical host key to its MTX key name (matched by
/// `runtime-memotech-mtx`'s `key_from_name`). The cursor keys are genuine
/// matrix cells, so they map here rather than to the joystick. Shifted symbols
/// are reached by holding a shift with another key, so only the unshifted
/// legends need mapping. The MTX's own Escape key is unreachable — the harness
/// owns Esc for quit.
fn map_mtx_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Backslash => &["\\"],
        KeyCode::BracketLeft => &["["],
        KeyCode::BracketRight => &["]"],
        KeyCode::Semicolon => &[";"],
        KeyCode::Comma => &[","],
        KeyCode::Period => &["."],
        KeyCode::Slash => &["/"],
        KeyCode::Space => &["space"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["enter"],
        KeyCode::Tab => &["tab"],
        KeyCode::ShiftLeft => &["shift"],
        KeyCode::ShiftRight => &["rshift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["ctrl"],
        KeyCode::CapsLock => &["caps"],
        KeyCode::Delete => &["delete"],
        KeyCode::Backspace => &["backspace"],
        KeyCode::Home => &["home"],
        KeyCode::Insert => &["insert"],
        KeyCode::F1 => &["f1"],
        KeyCode::F2 => &["f2"],
        KeyCode::F3 => &["f3"],
        KeyCode::F4 => &["f4"],
        KeyCode::F5 => &["f5"],
        KeyCode::F6 => &["f6"],
        KeyCode::F7 => &["f7"],
        KeyCode::F8 => &["f8"],
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
            "mtx.rom".to_owned(),
            "--model".to_owned(),
            "mtx512".to_owned(),
            "--scale".to_owned(),
            "4".to_owned(),
            "--video".to_owned(),
            "crt".to_owned(),
        ]);
        assert_eq!(cli.rom, Some(PathBuf::from("mtx.rom")));
        assert_eq!(cli.model, Model::Mtx512);
        assert_eq!(cli.scale, 4);
        assert_eq!(cli.video, VideoFilter::Crt);
    }

    #[test]
    fn cursor_keys_are_keyboard_cells_not_joystick() {
        // The MTX's cursor keys are genuine matrix cells, so they go through
        // the keyboard path and keep their MTX names.
        assert_eq!(map_mtx_keys(KeyCode::ArrowDown), Some(&["down"][..]));
        assert_eq!(map_mtx_keys(KeyCode::ArrowUp), Some(&["up"][..]));
        assert_eq!(map_mtx_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_mtx_keys(KeyCode::Enter), Some(&["enter"][..]));
        assert_eq!(map_mtx_keys(KeyCode::ShiftRight), Some(&["rshift"][..]));
        assert_eq!(map_mtx_keys(KeyCode::F1), Some(&["f1"][..]));
        // Keys with no MTX position are ignored.
        assert_eq!(map_mtx_keys(KeyCode::PageUp), None);
    }
}
