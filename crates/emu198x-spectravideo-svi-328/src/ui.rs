//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Spectravideo SVI-328's first native window, on the shared `emu198x-ui`
//! harness: wgpu video with `raw`/`lcd`/`crt` filters and the full keyboard
//! routed through the harness's general-keyboard path ([`UiSystem::map_keys`]).
//! The SVI-328 is keyboard-led; its cursor keys are genuine matrix cells, so
//! they type rather than driving the stick. The joystick is reached by a real
//! gamepad through [`UiSystem::button_map`]. Compiled only with the `ui` Cargo
//! feature; `main.rs` routes here when no automation flag is given.

use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use emu198x_shell::MachineCore;
use emu198x_ui::Display;
use emu198x_ui::{
    ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiError, UiSystem, VideoFilter,
};
use runtime_spectravideo_svi_328::{Model, Svi328Runtime};

const DEFAULT_SCALE: u32 = 3;
/// CPU clocks per frame — `228 × lines`, matching the headless runner.
const FRAME_TICKS_NTSC: u64 = 228 * 262;
const FRAME_TICKS_PAL: u64 = 228 * 313;
const NTSC_FRAME_HZ: f64 = 60.0;
const PAL_FRAME_HZ: f64 = 50.0;
const BIOS_SIZE: usize = 32 * 1024;
/// The SVI-328's TMS9918 framebuffer (active + border), fixed by the VDP.
const FB_WIDTH: u32 = 288;
const FB_HEIGHT: u32 = 240;

/// Player-1 joystick: four directions plus fire, named as
/// `runtime-spectravideo-svi-328`'s controller mirror expects. The cursor keys
/// are keyboard cells, so a real gamepad reaches the stick through this map.
const SVI_328_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
]);

const USAGE: &str = "\
Usage: emu198x-spectravideo-svi-328 [OPTIONS]

Options:
    --bios PATH     32 KB system ROM (BASIC + OS); default
                    ~/.emu198x/roms/spectravideo-svi-328/svi-328.rom
                    (or set EMU198X_SVI_328_BIOS)
    --cart PATH     cartridge ROM (up to 16 KB at $8000-$BFFF)
    --region MODE   ntsc | pal [default: ntsc]
    --scale N       integer window scale, default 3
    --video MODE    raw | lcd | crt [default: raw]
    --help, -h      show this help

Controls:
    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the SVI-328 keyboard (cursor keys are real SVI keys)
    Shift / Ctrl    the SVI SHIFT / CTRL keys (Alt = GRAPH/CODE)
    Gamepad         joystick (player 1)

Examples:
    emu198x-spectravideo-svi-328
    emu198x-spectravideo-svi-328 --cart game.rom --scale 4
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
            Self::Ntsc => Model::Svi328Ntsc,
            Self::Pal => Model::Svi328Pal,
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

/// The Spectravideo SVI-328 as a [`UiSystem`] for the shared harness. The
/// region is fixed at construction; a hard reset rebuilds the machine from the
/// firmware the runtime already holds.
struct Svi328System {
    region: Region,
}

impl UiSystem for Svi328System {
    type Runtime = Svi328Runtime;

    fn window_title(&self) -> String {
        "Emu198x Spectravideo SVI-328".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The SVI-328's TMS9918 drove a 4:3 TV; its 288×240 framebuffer stretches
    // to fill it.

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

    fn framebuffer_size(&self, _runtime: &Self::Runtime) -> (u32, u32) {
        (FB_WIDTH, FB_HEIGHT)
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        self.region.frame_ticks()
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / self.region.frame_hz())
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &SVI_328_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_svi_keys(code)
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
        .ok_or_else(|| "no BIOS: pass --bios PATH or set EMU198X_SVI_328_BIOS".to_owned())?;
    let bios = read_rom(&bios_path, "BIOS", BIOS_SIZE)?;
    let mut runtime = Svi328Runtime::new(cli.region.model(), bios)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;
    if let Some(cart_path) = &cli.cart {
        let cart = std::fs::read(cart_path)
            .map_err(|err| format!("failed to read --cart {}: {err}", cart_path.display()))?;
        runtime
            .insert_cartridge(cart)
            .map_err(|err| format!("cartridge rejected: {err}"))?;
    }

    println!(
        "Controls: Esc quit, F12 reset, keyboard typed directly (cursor keys are SVI keys), gamepad joystick."
    );
    emu198x_ui::run(
        Svi328System { region: cli.region },
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
            _ => die(&format!("unknown flag: {arg}")),
        }
    }
    cli
}

fn default_bios_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("EMU198X_SVI_328_BIOS")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/spectravideo-svi-328/svi-328.rom"))
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

/// Map a physical host key to its SVI-328 key name (matched by
/// `runtime-spectravideo-svi-328`'s `key_to_matrix`). The cursor keys are
/// genuine matrix cells, so they map here rather than to the joystick. Shifted
/// symbols are reached by holding SHIFT; host Alt is the GRAPH/CODE key. The
/// SVI's own Escape key is unreachable — the harness owns Esc for quit.
fn map_svi_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Semicolon => &[";"],
        KeyCode::Quote => &["'"],
        KeyCode::Comma => &[","],
        KeyCode::Equal => &["="],
        KeyCode::Period => &["."],
        KeyCode::Slash => &["/"],
        KeyCode::Minus => &["-"],
        KeyCode::BracketLeft => &["["],
        KeyCode::Backslash => &["\\"],
        KeyCode::BracketRight => &["]"],
        KeyCode::Space => &["space"],
        KeyCode::Tab => &["tab"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["return"],
        KeyCode::Backspace => &["backspace"],
        KeyCode::Delete => &["delete"],
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &["shift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["ctrl"],
        KeyCode::AltLeft | KeyCode::AltRight => &["graph"],
        KeyCode::CapsLock => &["caps"],
        KeyCode::Home => &["home"],
        KeyCode::Insert => &["insert"],
        KeyCode::F1 => &["f1"],
        KeyCode::F2 => &["f2"],
        KeyCode::F3 => &["f3"],
        KeyCode::F4 => &["f4"],
        KeyCode::F5 => &["f5"],
        KeyCode::ArrowUp => &["up"],
        KeyCode::ArrowDown => &["down"],
        KeyCode::ArrowLeft => &["left"],
        KeyCode::ArrowRight => &["right"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_accepts_bios_cart_region_scale_video() {
        let cli = parse_cli([
            "--bios".to_owned(),
            "svi.rom".to_owned(),
            "--cart".to_owned(),
            "game.rom".to_owned(),
            "--region".to_owned(),
            "pal".to_owned(),
            "--scale".to_owned(),
            "4".to_owned(),
            "--video".to_owned(),
            "crt".to_owned(),
        ]);
        assert_eq!(cli.bios, Some(PathBuf::from("svi.rom")));
        assert_eq!(cli.cart, Some(PathBuf::from("game.rom")));
        assert_eq!(cli.region, Region::Pal);
        assert_eq!(cli.scale, 4);
        assert_eq!(cli.video, VideoFilter::Crt);
    }

    #[test]
    fn region_frame_ticks_match() {
        assert_eq!(Region::Ntsc.frame_ticks(), 228 * 262);
        assert_eq!(Region::Pal.frame_ticks(), 228 * 313);
    }

    #[test]
    fn cursor_keys_are_keyboard_cells_and_graph_maps() {
        // The SVI's cursor keys are genuine matrix cells, so they type.
        assert_eq!(map_svi_keys(KeyCode::ArrowUp), Some(&["up"][..]));
        assert_eq!(map_svi_keys(KeyCode::ArrowRight), Some(&["right"][..]));
        assert_eq!(map_svi_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_svi_keys(KeyCode::Enter), Some(&["return"][..]));
        assert_eq!(map_svi_keys(KeyCode::AltLeft), Some(&["graph"][..]));
        assert_eq!(map_svi_keys(KeyCode::F1), Some(&["f1"][..]));
        // Keys with no SVI position are ignored.
        assert_eq!(map_svi_keys(KeyCode::PageUp), None);
    }
}
