//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Mattel Aquarius's first native window, on the shared `emu198x-ui`
//! harness: wgpu video with `raw`/`lcd`/`crt` filters and input on both of the
//! harness's paths. The Aquarius is a home computer with a hand controller, so
//! it uses the keyboard path ([`UiSystem::map_keys`]) for its 8×6 matrix *and*
//! the console path ([`UiSystem::map_key`] + [`UiSystem::button_map`]) for the
//! Mini Expander hand controller. Arrow keys and Alt aren't on the keyboard
//! matrix, so they fall through to the joystick path without clashing. Compiled
//! only with the `ui` Cargo feature; `main.rs` routes here when no automation
//! flag is given.

use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use emu198x_ui::{
    ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiError, UiSystem, VideoFilter,
};
use runtime_mattel_aquarius::{AquariusRuntime, Model};

const DEFAULT_SCALE: u32 = 3;
/// Z80 @ ~3.58 MHz, ~50 Hz PAL → 71,590 t-states/frame, matching the headless
/// runner's `FRAME_TICKS_PAL`.
const FRAME_TICKS_PAL: u64 = 71_590;
const PAL_FRAME_HZ: f64 = 50.0;
const BIOS_SIZE: usize = 8 * 1024;

/// Player-1 hand controller on the Mini Expander: four disc directions plus the
/// first side button, named as `runtime-mattel-aquarius`'s controller mirror
/// expects. A real gamepad reaches these through the same map.
const AQUARIUS_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
]);

const USAGE: &str = "\
Usage: emu198x-mattel-aquarius [OPTIONS]

Options:
    --bios PATH        Aquarius BASIC ROM (8 KB); default
                       ~/.emu198x/roms/mattel-aquarius/aquarius.rom
                       (or set EMU198X_AQUARIUS_BIOS)
    --char PATH        Aquarius character ROM (2 KB); default
                       ~/.emu198x/roms/mattel-aquarius/aquarius-char.rom
                       (or set EMU198X_AQUARIUS_CHAR)
    --cart PATH        cartridge ROM (mapped at $E000-$FFFF, up to 8 KB)
    --expansion-kb N   RAM expansion in KB (0..=16) [default: 0]
    --scale N          integer window scale, default 3
    --video MODE       raw | lcd | crt [default: raw]
    --help, -h         show this help

Controls:
    Esc                quit
    F12                hard reset
    A-Z 0-9 etc.       the Aquarius keyboard
    Shift / Ctrl       the two Aquarius shift keys
    Arrow keys         hand controller disc (player 1)
    Alt                hand controller fire

Examples:
    emu198x-mattel-aquarius
    emu198x-mattel-aquarius --cart astrosmash.bin --scale 4
";

/// The Mattel Aquarius as a [`UiSystem`] for the shared harness. Single-model;
/// a hard reset rebuilds the machine from the firmware the runtime holds. The
/// cartridge and RAM expansion are fixed at construction.
struct AquariusSystem;

impl UiSystem for AquariusSystem {
    type Runtime = AquariusRuntime;

    fn window_title(&self) -> String {
        "Emu198x Mattel Aquarius".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The Aquarius drove a 4:3 TV; its 320×192 framebuffer stretches to fill it.

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| (machine.framebuffer_width(), machine.framebuffer_height()))
            .unwrap_or((352, 232))
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        FRAME_TICKS_PAL
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / PAL_FRAME_HZ)
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &AQUARIUS_BUTTON_MAP
    }

    fn map_key(&self, code: KeyCode) -> Option<HostControl> {
        Some(match code {
            KeyCode::ArrowUp => HostControl::Up,
            KeyCode::ArrowDown => HostControl::Down,
            KeyCode::ArrowLeft => HostControl::Left,
            KeyCode::ArrowRight => HostControl::Right,
            KeyCode::AltLeft | KeyCode::AltRight => HostControl::South,
            _ => return None,
        })
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_aquarius_keys(code)
    }
}

/// Parsed interactive CLI.
#[derive(Debug, PartialEq, Eq)]
pub struct Cli {
    bios: Option<PathBuf>,
    char_rom: Option<PathBuf>,
    cart: Option<PathBuf>,
    expansion_kb: usize,
    scale: u32,
    video: VideoFilter,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            bios: None,
            char_rom: None,
            cart: None,
            expansion_kb: 0,
            scale: DEFAULT_SCALE,
            video: VideoFilter::Raw,
        }
    }
}

/// Build the runtime from the CLI and open the window. Returns a string error
/// for the `main.rs` dispatcher.
pub fn run(cli: Cli) -> Result<(), String> {
    let bios_path = cli
        .bios
        .clone()
        .or_else(default_bios_path)
        .ok_or_else(|| "no BIOS: pass --bios PATH or set EMU198X_AQUARIUS_BIOS".to_owned())?;
    let bios = read_rom(&bios_path, "BIOS", BIOS_SIZE)?;
    let char_path = cli
        .char_rom
        .clone()
        .or_else(default_char_path)
        .ok_or_else(|| {
            "no character ROM: pass --char PATH or set EMU198X_AQUARIUS_CHAR".to_owned()
        })?;
    let char_rom = std::fs::read(&char_path).map_err(|err| {
        format!(
            "failed to read character ROM {}: {err}",
            char_path.display()
        )
    })?;

    let mut runtime = AquariusRuntime::new(Model::Aquarius, bios)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;
    runtime
        .set_char_rom(char_rom)
        .map_err(|err| format!("character ROM rejected: {err}"))?;
    runtime.set_expansion_kb(cli.expansion_kb);
    if let Some(cart_path) = &cli.cart {
        let rom = std::fs::read(cart_path)
            .map_err(|err| format!("failed to read --cart {}: {err}", cart_path.display()))?;
        runtime.insert_cartridge(rom);
    }

    println!(
        "Controls: Esc quit, F12 reset, keyboard typed directly, Shift/Ctrl shift keys, arrows + Alt hand controller."
    );
    emu198x_ui::run(AquariusSystem, runtime, cli.scale, cli.video)
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
            "--bios" => cli.bios = Some(PathBuf::from(next_arg(&mut iter, "--bios"))),
            "--char" => cli.char_rom = Some(PathBuf::from(next_arg(&mut iter, "--char"))),
            "--cart" => cli.cart = Some(PathBuf::from(next_arg(&mut iter, "--cart"))),
            "--expansion-kb" => {
                cli.expansion_kb = next_arg(&mut iter, "--expansion-kb")
                    .parse()
                    .unwrap_or_else(|_| die("--expansion-kb requires a non-negative integer"));
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

fn default_bios_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("EMU198X_AQUARIUS_BIOS")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/mattel-aquarius/aquarius.rom"))
}

fn default_char_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("EMU198X_AQUARIUS_CHAR")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/mattel-aquarius/aquarius-char.rom"))
}

fn read_rom(path: &Path, kind: &str, expected: usize) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(path)
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

/// Map a physical host key to its Aquarius key name (matched by
/// `runtime-mattel-aquarius`'s `key_to_matrix`). The Aquarius's symbols are
/// Shift/Ctrl-layer combos reached by holding a shift with another key, so only
/// the base keys need mapping here. Arrow and Alt keys are deliberately absent
/// — they drive the hand controller through [`UiSystem::map_key`] instead.
fn map_aquarius_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Enter | KeyCode::NumpadEnter => &["return"],
        KeyCode::Backspace => &["backspace"],
        KeyCode::Minus => &["-"],
        KeyCode::Equal => &["="],
        KeyCode::Slash => &["/"],
        KeyCode::Semicolon => &[";"],
        KeyCode::Comma => &[","],
        KeyCode::Period => &["."],
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &["shift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["ctrl"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_accepts_bios_char_cart_expansion_scale_video() {
        let cli = parse_cli([
            "--bios".to_owned(),
            "aq.rom".to_owned(),
            "--char".to_owned(),
            "aq-char.rom".to_owned(),
            "--cart".to_owned(),
            "game.bin".to_owned(),
            "--expansion-kb".to_owned(),
            "16".to_owned(),
            "--scale".to_owned(),
            "4".to_owned(),
            "--video".to_owned(),
            "crt".to_owned(),
        ]);
        assert_eq!(cli.bios, Some(PathBuf::from("aq.rom")));
        assert_eq!(cli.char_rom, Some(PathBuf::from("aq-char.rom")));
        assert_eq!(cli.cart, Some(PathBuf::from("game.bin")));
        assert_eq!(cli.expansion_kb, 16);
        assert_eq!(cli.scale, 4);
        assert_eq!(cli.video, VideoFilter::Crt);
    }

    #[test]
    fn keyboard_and_controller_paths_do_not_clash() {
        let sys = AquariusSystem;
        // Keyboard keys go through map_keys…
        assert_eq!(sys.map_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(sys.map_keys(KeyCode::Enter), Some(&["return"][..]));
        assert_eq!(sys.map_keys(KeyCode::ShiftLeft), Some(&["shift"][..]));
        assert_eq!(sys.map_keys(KeyCode::ControlLeft), Some(&["ctrl"][..]));
        // …and are not also controller controls.
        assert_eq!(sys.map_key(KeyCode::KeyA), None);
        // Controller keys go through map_key and are not keyboard keys.
        assert_eq!(sys.map_key(KeyCode::ArrowLeft), Some(HostControl::Left));
        assert_eq!(sys.map_key(KeyCode::AltLeft), Some(HostControl::South));
        assert_eq!(sys.map_keys(KeyCode::ArrowLeft), None);
        assert_eq!(sys.map_keys(KeyCode::AltLeft), None);
    }
}
