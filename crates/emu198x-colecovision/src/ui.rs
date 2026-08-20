//! Interactive UI mode — the default when no automation flag is present.
//!
//! The ColecoVision's first native window, on the shared `emu198x-ui` harness:
//! wgpu video with `raw`/`lcd`/`crt` filters, framed PSG audio, and
//! keyboard/gamepad input. The Coleco controller is a joystick + two fire
//! buttons + a 12-key numeric keypad: the joystick and fire go through the
//! harness's console path ([`UiSystem::map_key`] + [`UiSystem::button_map`]),
//! and the keypad digits / `*` / `#` are named key events on controller 1,
//! routed through [`UiSystem::map_keys`]. Compiled only with the `ui` Cargo
//! feature; `main.rs` routes here when no automation flag is given.

use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use emu198x_shell::MachineCore;
use emu198x_ui::Display;
use emu198x_ui::{
    ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiError, UiSystem, VideoFilter,
};
use runtime_coleco_colecovision::{CvRuntime, Model};

const DEFAULT_SCALE: u32 = 3;
/// CPU clocks per frame — `228 × lines`, matching the headless runner.
const FRAME_TICKS_NTSC: u64 = 228 * 262;
const FRAME_TICKS_PAL: u64 = 228 * 313;
const NTSC_FRAME_HZ: f64 = 60.0;
const PAL_FRAME_HZ: f64 = 50.0;
const BIOS_SIZE: usize = 8 * 1024;

/// Player-1 controller: joystick directions plus the two fire buttons.
/// `south`/`east` are the names `runtime-coleco-colecovision`'s `apply_button`
/// maps to the controller's left / right fire buttons. A real gamepad reaches
/// these through the same map; the keyboard does via [`UiSystem::map_key`].
const COLECO_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "south")),
    (HostControl::East, ButtonTarget::new(1, "east")),
]);

const USAGE: &str = "\
Usage: emu198x-colecovision [OPTIONS]

Options:
    --bios PATH     ColecoVision BIOS ROM (8 KB); default
                    ~/.emu198x/roms/coleco-colecovision/colecovision.rom
                    (or set EMU198X_COLECO_BIOS)
    --cart PATH     cartridge ROM image (optional — BIOS shows the splash)
    --region MODE   ntsc | pal [default: ntsc]
    --scale N       integer window scale, default 3
    --video MODE    raw | lcd | crt [default: raw]
    --help, -h      show this help

Controls:
    Esc             quit
    F12             emulator hard reset
    Arrow keys      joystick (player 1)
    Z / X           left and right fire buttons
    0-9             numeric keypad
    Numpad * / /    keypad * and # keys

Examples:
    emu198x-colecovision --cart donkeykong.col
    emu198x-colecovision --bios coleco.rom --cart game.col --scale 4
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
            Self::Ntsc => Model::CvNtsc,
            Self::Pal => Model::CvPal,
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

/// The ColecoVision as a [`UiSystem`] for the shared harness. The region is
/// fixed at construction; a hard reset rebuilds the machine from the BIOS and
/// cartridge the runtime already holds.
struct ColecoSystem {
    region: Region,
}

impl UiSystem for ColecoSystem {
    type Runtime = CvRuntime;

    fn window_title(&self) -> String {
        "Emu198x ColecoVision".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The Coleco's TMS9918 drove a 4:3 TV; its 288×240 framebuffer stretches to
    // fill it.

    /// The TMS9918 family drove a television through a colour-subcarrier
    /// crystal, so its dots are not square: 8:7 on the NTSC parts, about
    /// 1.382 on the PAL TMS9929A. Presenting the 288x240 framebuffer unstretched
    /// claimed otherwise.
    fn display(&self, runtime: &Self::Runtime) -> Option<Display> {
        Display::television_for_region(
            runtime.profile().region,
            ti_tms9918::PAL_DOT_CLOCK_HZ,
            ti_tms9918::NTSC_DOT_CLOCK_HZ,
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
            .unwrap_or((288, 240))
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        self.region.frame_ticks()
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / self.region.frame_hz())
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &COLECO_BUTTON_MAP
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
        // The 12-key numeric keypad — named key events on controller 1. Digits
        // come from both the top row and the numeric keypad; `*` and `#` from
        // the numpad operator keys.
        Some(match code {
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
    bios: Option<PathBuf>,
    cart: Option<PathBuf>,
    region: Region,
    scale: u32,
    video: VideoFilter,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            bios: None,
            cart: None,
            region: Region::Ntsc,
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
        .ok_or_else(|| "no BIOS: pass --bios PATH or set EMU198X_COLECO_BIOS".to_owned())?;
    let bios = read_rom(&bios_path, "BIOS", BIOS_SIZE)?;
    let mut runtime = CvRuntime::new(cli.region.model(), bios)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;
    if let Some(cart_path) = &cli.cart {
        let cart = std::fs::read(cart_path)
            .map_err(|err| format!("failed to read --cart {}: {err}", cart_path.display()))?;
        runtime.insert_cartridge(cart);
    }

    println!(
        "Controls: Esc quit, F12 reset, arrows joystick, Z/X fire, 0-9 keypad, Numpad */÷ for * and #."
    );
    emu198x_ui::run(
        ColecoSystem { region: cli.region },
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
            "--bios" => cli.bios = Some(PathBuf::from(next_arg(&mut iter, "--bios"))),
            "--cart" => cli.cart = Some(PathBuf::from(next_arg(&mut iter, "--cart"))),
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
    if let Ok(path) = env::var("EMU198X_COLECO_BIOS")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/coleco-colecovision/colecovision.rom"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_accepts_bios_cart_region_scale_video() {
        let cli = parse_cli([
            "--bios".to_owned(),
            "coleco.rom".to_owned(),
            "--cart".to_owned(),
            "dk.col".to_owned(),
            "--region".to_owned(),
            "pal".to_owned(),
            "--scale".to_owned(),
            "4".to_owned(),
            "--video".to_owned(),
            "crt".to_owned(),
        ]);
        assert_eq!(cli.bios, Some(PathBuf::from("coleco.rom")));
        assert_eq!(cli.cart, Some(PathBuf::from("dk.col")));
        assert_eq!(cli.region, Region::Pal);
        assert_eq!(cli.scale, 4);
        assert_eq!(cli.video, VideoFilter::Crt);
    }

    #[test]
    fn parse_cli_accepts_positional_cart() {
        let cli = parse_cli(["dk.col".to_owned()]);
        assert_eq!(cli.cart, Some(PathBuf::from("dk.col")));
    }

    #[test]
    fn region_frame_ticks_match() {
        assert_eq!(Region::Ntsc.frame_ticks(), 228 * 262);
        assert_eq!(Region::Pal.frame_ticks(), 228 * 313);
    }

    #[test]
    fn joystick_on_map_key_and_keypad_on_map_keys() {
        let sys = ColecoSystem {
            region: Region::Ntsc,
        };
        // Joystick + fire on the console path.
        assert_eq!(sys.map_key(KeyCode::ArrowLeft), Some(HostControl::Left));
        assert_eq!(sys.map_key(KeyCode::KeyZ), Some(HostControl::South));
        assert_eq!(sys.map_key(KeyCode::KeyX), Some(HostControl::East));
        // Keypad on the keyboard path — and not also a joystick control.
        assert_eq!(sys.map_keys(KeyCode::Digit5), Some(&["5"][..]));
        assert_eq!(sys.map_keys(KeyCode::Numpad5), Some(&["5"][..]));
        assert_eq!(sys.map_keys(KeyCode::NumpadMultiply), Some(&["*"][..]));
        assert_eq!(sys.map_keys(KeyCode::NumpadDivide), Some(&["#"][..]));
        assert_eq!(sys.map_key(KeyCode::Digit5), None);
        // Arrows are joystick, not keypad.
        assert_eq!(sys.map_keys(KeyCode::ArrowLeft), None);
    }
}
