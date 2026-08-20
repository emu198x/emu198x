//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Commodore PET's first native window, on the shared `emu198x-ui` harness:
//! wgpu video with `raw`/`lcd`/`crt` filters and the keyboard routed through
//! the harness's general-keyboard path ([`UiSystem::map_keys`]). The PET is
//! keyboard-only — no joystick, no sound — so it carries an empty button map
//! and routes every key through `map_keys`. Many PET symbols sit on dedicated
//! keys that are shifted on a modern host, so only the physically-unshifted
//! keys are mapped. Compiled only with the `ui` Cargo feature; `main.rs` routes
//! here when no automation flag is given.

use std::env;
use std::path::PathBuf;
use std::time::Duration;

use emu198x_ui::{ButtonInputMap, KeyCode, UiError, UiSystem, VideoFilter};
use runtime_commodore_pet::{Model, PetRuntime};

const DEFAULT_SCALE: u32 = 3;
/// 6502 @ 1 MHz, 50 Hz → 20,000 cycles/frame, matching the headless runner's
/// `FRAME_TICKS`.
const FRAME_TICKS: u64 = 20_000;
const FRAME_HZ: f64 = 50.0;
const KERNAL_SIZE: usize = 4096;
const BASIC_SIZE: usize = 8192;
const EDITOR_SIZE: usize = 2048;
const CHAR_SIZE: usize = 4096;

/// The PET has no joystick, but the harness still wants a button map — so an
/// empty one. Every key flows through [`UiSystem::map_keys`] instead.
const PET_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[]);

const USAGE: &str = "\
Usage: emu198x-commodore-pet [OPTIONS]

ROMs (defaults: $EMU198X_PET_{KERNAL,BASIC,EDITOR,CHAR}, then
~/.emu198x/roms/commodore-pet/{kernal,basic,editor,chargen}.rom):
    --kernal PATH   KERNAL ROM (4 KB)
    --basic PATH    BASIC ROM (8 KB)
    --editor PATH   editor ROM (2 KB)
    --char PATH     character ROM (4 KB)

Display:
    --columns N     40 or 80 [default: 40]
    --scale N       integer window scale, default 3
    --video MODE    raw | lcd | crt [default: raw]
    --help, -h      show this help

Controls:
    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the PET keyboard
    Enter           RETURN

Examples:
    emu198x-commodore-pet
    emu198x-commodore-pet --columns 80 --scale 2
";

/// The Commodore PET as a [`UiSystem`] for the shared harness. Keyboard-only; a
/// hard reset rebuilds the machine from the firmware the runtime already holds.
/// The column model is fixed at construction.
struct PetSystem;

impl UiSystem for PetSystem {
    type Runtime = PetRuntime;

    fn window_title(&self) -> String {
        "Emu198x Commodore PET".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The PET drove a 4:3 monochrome monitor; its framebuffer stretches to fill
    // it.
    //
    // Deliberately still on this hook rather than `pixel_aspect_ratio`, and the
    // only core that should be. The raster derivation asks how much of a
    // *broadcast* line a set displays, and a set overscans; a dedicated monitor
    // shows the whole framebuffer, so "stretch this buffer to fill 4:3" is not
    // a legacy approximation here but the correct model. The PET's profile says
    // `Region::Other` for the same reason, and the derivation would decline to
    // answer. See `knowledge/decisions/pixel-aspect-comes-from-the-raster.md`.
    fn display_aspect_ratio(&self) -> Option<f32> {
        Some(4.0 / 3.0)
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
            .unwrap_or((320, 200))
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        FRAME_TICKS
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / FRAME_HZ)
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &PET_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_pet_keys(code)
    }
}

/// Parsed interactive CLI.
#[derive(Debug, PartialEq, Eq)]
pub struct Cli {
    kernal: Option<PathBuf>,
    basic: Option<PathBuf>,
    editor: Option<PathBuf>,
    char_rom: Option<PathBuf>,
    columns: u32,
    scale: u32,
    video: VideoFilter,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            kernal: None,
            basic: None,
            editor: None,
            char_rom: None,
            columns: 40,
            scale: DEFAULT_SCALE,
            video: VideoFilter::Raw,
        }
    }
}

fn model_for(columns: u32) -> Model {
    match columns {
        80 => Model::Pet80Col,
        _ => Model::Pet40Col,
    }
}

/// Build the runtime from the CLI and open the window. Returns a string error
/// for the `main.rs` dispatcher.
pub fn run(cli: Cli) -> Result<(), String> {
    let kernal = read_required(
        cli.kernal.clone(),
        "KERNAL",
        "KERNAL",
        "kernal.rom",
        KERNAL_SIZE,
    )?;
    let basic = read_required(cli.basic.clone(), "BASIC", "BASIC", "basic.rom", BASIC_SIZE)?;
    let editor = read_required(
        cli.editor.clone(),
        "editor",
        "EDITOR",
        "editor.rom",
        EDITOR_SIZE,
    )?;
    let char_rom = read_required(
        cli.char_rom.clone(),
        "character",
        "CHAR",
        "chargen.rom",
        CHAR_SIZE,
    )?;
    let runtime = PetRuntime::new(model_for(cli.columns), kernal, basic, editor, char_rom)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;

    println!("Controls: Esc quit, F12 reset, A-Z/0-9 keyboard typed directly, Enter RETURN.");
    emu198x_ui::run(PetSystem, runtime, cli.scale, cli.video)
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
            "--kernal" => cli.kernal = Some(PathBuf::from(next_arg(&mut iter, "--kernal"))),
            "--basic" => cli.basic = Some(PathBuf::from(next_arg(&mut iter, "--basic"))),
            "--editor" => cli.editor = Some(PathBuf::from(next_arg(&mut iter, "--editor"))),
            "--char" => cli.char_rom = Some(PathBuf::from(next_arg(&mut iter, "--char"))),
            "--columns" => {
                cli.columns = next_arg(&mut iter, "--columns")
                    .parse()
                    .unwrap_or_else(|_| die("--columns expects 40 or 80"));
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

fn default_rom(kind: &str, default_file: &str) -> Option<PathBuf> {
    if let Ok(path) = env::var(format!("EMU198X_PET_{kind}"))
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(format!(".emu198x/roms/commodore-pet/{default_file}")))
}

fn read_required(
    explicit: Option<PathBuf>,
    kind: &str,
    env_kind: &str,
    default_file: &str,
    expected: usize,
) -> Result<Vec<u8>, String> {
    // `kind` is the human label for errors; `env_kind` is the env-var suffix
    // (EMU198X_PET_<env_kind>), which differs from `kind` for char/editor.
    let path = explicit
        .or_else(|| default_rom(env_kind, default_file))
        .ok_or_else(|| format!("no {kind} ROM: pass its flag or set EMU198X_PET_{env_kind}"))?;
    let bytes = std::fs::read(&path)
        .map_err(|err| format!("failed to read {kind} ROM {}: {err}", path.display()))?;
    if bytes.len() != expected {
        return Err(format!(
            "{kind} ROM at {} is {} bytes; expected {expected}",
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

/// Map a physical host key to its PET key name (matched by
/// `runtime-commodore-pet`'s `key_from_name`). The PET places many symbols on
/// dedicated keys that are shifted on a modern keyboard; without shift-symbol
/// synthesis only the physically-unshifted keys are mapped here — letters,
/// digits, the directly-typable punctuation, RETURN, space, and cursor-right.
fn map_pet_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Period => &["."],
        KeyCode::Comma => &[","],
        KeyCode::Semicolon => &[";"],
        KeyCode::Slash => &["/"],
        KeyCode::Quote => &["'"],
        KeyCode::Equal => &["="],
        KeyCode::Space => &["space"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["return"],
        KeyCode::ArrowRight => &["right"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_accepts_roms_columns_scale_video() {
        let cli = parse_cli([
            "--kernal".to_owned(),
            "k.rom".to_owned(),
            "--columns".to_owned(),
            "80".to_owned(),
            "--scale".to_owned(),
            "2".to_owned(),
            "--video".to_owned(),
            "crt".to_owned(),
        ]);
        assert_eq!(cli.kernal, Some(PathBuf::from("k.rom")));
        assert_eq!(cli.columns, 80);
        assert_eq!(cli.scale, 2);
        assert_eq!(cli.video, VideoFilter::Crt);
    }

    #[test]
    fn model_selects_by_columns() {
        assert_eq!(model_for(40), Model::Pet40Col);
        assert_eq!(model_for(80), Model::Pet80Col);
    }

    #[test]
    fn maps_letters_digits_and_return() {
        assert_eq!(map_pet_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_pet_keys(KeyCode::Digit5), Some(&["5"][..]));
        assert_eq!(map_pet_keys(KeyCode::Enter), Some(&["return"][..]));
        assert_eq!(map_pet_keys(KeyCode::ArrowRight), Some(&["right"][..]));
        // Keys with no directly-typable PET position are ignored.
        assert_eq!(map_pet_keys(KeyCode::Tab), None);
    }
}
