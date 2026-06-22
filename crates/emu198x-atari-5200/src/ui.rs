//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Atari 5200's first native window, on the shared `emu198x-ui` harness:
//! wgpu video with `raw`/`lcd`/`crt` filters, framed POKEY audio, and
//! keyboard/gamepad input. The 5200 controller is an analogue stick + fire +
//! a 16-key keypad. The stick and fire go through the harness's console path
//! ([`UiSystem::map_key`] + [`UiSystem::button_map`]) — the runtime snaps the
//! digital directions to the POKEY pot extremes — and the keypad keys
//! (`start`/`pause`/`reset`/`0`-`9`/`*`/`#`) are momentary named key events,
//! routed through [`UiSystem::map_keys`]. Compiled only with the `ui` Cargo
//! feature; `main.rs` routes here when no automation flag is given.

use std::env;
use std::path::PathBuf;
use std::time::Duration;

use emu198x_ui::{
    ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiError, UiSystem, VideoFilter,
};
use runtime_atari_5200::{Atari5200Runtime, Model};

const DEFAULT_SCALE: u32 = 3;
/// CPU clocks per frame — `lines × 228`, matching the headless runner.
const FRAME_TICKS_NTSC: u64 = 262 * 228;
const FRAME_TICKS_PAL: u64 = 312 * 228;
const NTSC_FRAME_HZ: f64 = 60.0;
const PAL_FRAME_HZ: f64 = 50.0;

/// Player-1 controller: stick directions plus fire. The runtime snaps the
/// digital directions to the analogue pot extremes; `fire` drives the trigger.
/// A real gamepad reaches these through the same map; the keyboard does via
/// [`UiSystem::map_key`].
const ATARI_5200_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
]);

const USAGE: &str = "\
Usage: emu198x-atari-5200 [OPTIONS]

Options:
    --cart PATH     Atari 5200 cartridge ROM (required)
    --bios PATH     Atari 5200 BIOS ROM (2 KB); default
                    ~/.emu198x/roms/atari-5200/bios.rom (or set EMU198X_A5200_BIOS)
    --region MODE   ntsc | pal [default: ntsc]
    --scale N       integer window scale, default 3
    --video MODE    raw | lcd | crt [default: raw]
    --help, -h      show this help

Controls:
    Esc             quit
    F12             emulator hard reset
    Arrow keys      analogue stick (player 1)
    Z / X           fire
    Enter           Start    Backspace  Pause    Delete  Reset (keypad)
    0-9             keypad digits
    Numpad * / /    keypad * and # keys

Examples:
    emu198x-atari-5200 --cart galaxian.a52
    emu198x-atari-5200 --cart game.bin --region pal --scale 4
";

/// Display region — selects the model, frame tick budget, and refresh rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Region {
    Ntsc,
    Pal,
}

impl Region {
    fn model(self) -> Model {
        match self {
            Self::Ntsc => Model::A5200Ntsc,
            Self::Pal => Model::A5200Pal,
        }
    }

    fn frame_ticks(self) -> u64 {
        match self {
            Self::Ntsc => FRAME_TICKS_NTSC,
            Self::Pal => FRAME_TICKS_PAL,
        }
    }

    fn frame_hz(self) -> f64 {
        match self {
            Self::Ntsc => NTSC_FRAME_HZ,
            Self::Pal => PAL_FRAME_HZ,
        }
    }
}

/// The Atari 5200 as a [`UiSystem`] for the shared harness. The region is fixed
/// at construction; a hard reset rebuilds the machine from the cartridge and
/// BIOS the runtime already holds.
struct Atari5200System {
    region: Region,
}

impl UiSystem for Atari5200System {
    type Runtime = Atari5200Runtime;

    fn window_title(&self) -> String {
        "Emu198x Atari 5200".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The 5200 drove a 4:3 TV; its GTIA framebuffer stretches to fill it.
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
            .unwrap_or((376, 240))
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        self.region.frame_ticks()
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / self.region.frame_hz())
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &ATARI_5200_BUTTON_MAP
    }

    fn map_key(&self, code: KeyCode) -> Option<HostControl> {
        Some(match code {
            KeyCode::ArrowUp => HostControl::Up,
            KeyCode::ArrowDown => HostControl::Down,
            KeyCode::ArrowLeft => HostControl::Left,
            KeyCode::ArrowRight => HostControl::Right,
            KeyCode::KeyZ => HostControl::South,
            KeyCode::KeyX => HostControl::East,
            _ => return None,
        })
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        // The 16-key keypad — momentary named key events. The three console
        // keys (Start / Pause / Reset) are keypad keys on the 5200, distinct
        // from the harness's own Esc-quit / F12-reset.
        Some(match code {
            KeyCode::Enter | KeyCode::NumpadEnter => &["start"],
            KeyCode::Backspace => &["pause"],
            KeyCode::Delete => &["reset"],
            KeyCode::Digit0 | KeyCode::Numpad0 => &["0"],
            KeyCode::Digit1 | KeyCode::Numpad1 => &["1"],
            KeyCode::Digit2 | KeyCode::Numpad2 => &["2"],
            KeyCode::Digit3 | KeyCode::Numpad3 => &["3"],
            KeyCode::Digit4 | KeyCode::Numpad4 => &["4"],
            KeyCode::Digit5 | KeyCode::Numpad5 => &["5"],
            KeyCode::Digit6 | KeyCode::Numpad6 => &["6"],
            KeyCode::Digit7 | KeyCode::Numpad7 => &["7"],
            KeyCode::Digit8 | KeyCode::Numpad8 => &["8"],
            KeyCode::Digit9 | KeyCode::Numpad9 => &["9"],
            KeyCode::NumpadMultiply => &["*"],
            KeyCode::NumpadDivide => &["#"],
            _ => return None,
        })
    }
}

/// Parsed interactive CLI.
#[derive(Debug, PartialEq, Eq)]
pub struct Cli {
    cart: Option<PathBuf>,
    bios: Option<PathBuf>,
    region: Region,
    scale: u32,
    video: VideoFilter,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            cart: None,
            bios: None,
            region: Region::Ntsc,
            scale: DEFAULT_SCALE,
            video: VideoFilter::Raw,
        }
    }
}

/// Build the runtime from the CLI and open the window. Returns a string error
/// for the `main.rs` dispatcher.
pub fn run(cli: Cli) -> Result<(), String> {
    let cart_path = cli
        .cart
        .as_ref()
        .ok_or_else(|| "provide a cartridge with --cart PATH".to_owned())?;
    let cart = std::fs::read(cart_path)
        .map_err(|err| format!("failed to read --cart {}: {err}", cart_path.display()))?;
    // The 5200 BIOS is optional — best-effort read like the headless runner.
    let bios = cli
        .bios
        .clone()
        .or_else(default_bios_path)
        .and_then(|path| std::fs::read(path).ok())
        .unwrap_or_default();
    let runtime = Atari5200Runtime::new(cli.region.model(), cart, bios)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;

    println!(
        "Controls: Esc quit, F12 reset, arrows stick, Z/X fire, Enter Start, 0-9 keypad, Numpad */÷ for * and #."
    );
    emu198x_ui::run(
        Atari5200System { region: cli.region },
        runtime,
        cli.scale,
        cli.video,
    )
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
            "--cart" => cli.cart = Some(PathBuf::from(next_arg(&mut iter, "--cart"))),
            "--bios" => cli.bios = Some(PathBuf::from(next_arg(&mut iter, "--bios"))),
            "--region" => {
                cli.region = match next_arg(&mut iter, "--region").as_str() {
                    "ntsc" => Region::Ntsc,
                    "pal" => Region::Pal,
                    other => die(&format!("--region expects ntsc|pal, got {other}")),
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
            _ if arg.starts_with('-') => die(&format!("unknown flag: {arg}")),
            _ if cli.cart.is_none() => cli.cart = Some(PathBuf::from(arg)),
            _ => die("only one positional cart path is supported"),
        }
    }
    cli
}

fn default_bios_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("EMU198X_A5200_BIOS")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/atari-5200/bios.rom"))
}

fn next_arg<I: Iterator<Item = String>>(iter: &mut I, flag: &str) -> String {
    iter.next()
        .unwrap_or_else(|| die(&format!("missing value for {flag}")))
}

fn die(message: &str) -> ! {
    eprintln!("error: {message}");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_accepts_cart_bios_region_scale_video() {
        let cli = parse_cli([
            "--cart".to_owned(),
            "game.a52".to_owned(),
            "--bios".to_owned(),
            "5200.rom".to_owned(),
            "--region".to_owned(),
            "pal".to_owned(),
            "--scale".to_owned(),
            "4".to_owned(),
            "--video".to_owned(),
            "crt".to_owned(),
        ]);
        assert_eq!(cli.cart, Some(PathBuf::from("game.a52")));
        assert_eq!(cli.bios, Some(PathBuf::from("5200.rom")));
        assert_eq!(cli.region, Region::Pal);
        assert_eq!(cli.scale, 4);
        assert_eq!(cli.video, VideoFilter::Crt);
    }

    #[test]
    fn parse_cli_accepts_positional_cart() {
        let cli = parse_cli(["game.a52".to_owned()]);
        assert_eq!(cli.cart, Some(PathBuf::from("game.a52")));
    }

    #[test]
    fn region_frame_ticks_match() {
        assert_eq!(Region::Ntsc.frame_ticks(), 262 * 228);
        assert_eq!(Region::Pal.frame_ticks(), 312 * 228);
    }

    #[test]
    fn stick_on_map_key_and_keypad_on_map_keys() {
        let sys = Atari5200System {
            region: Region::Ntsc,
        };
        assert_eq!(sys.map_key(KeyCode::ArrowLeft), Some(HostControl::Left));
        assert_eq!(sys.map_key(KeyCode::KeyZ), Some(HostControl::South));
        assert_eq!(sys.map_keys(KeyCode::Enter), Some(&["start"][..]));
        assert_eq!(sys.map_keys(KeyCode::Digit5), Some(&["5"][..]));
        assert_eq!(sys.map_keys(KeyCode::NumpadMultiply), Some(&["*"][..]));
        // No double-routing: stick keys aren't keypad keys and vice versa.
        assert_eq!(sys.map_keys(KeyCode::ArrowLeft), None);
        assert_eq!(sys.map_key(KeyCode::Digit5), None);
    }
}
