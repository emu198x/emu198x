//! Interactive UI mode — the default when no automation flag is present.
//!
//! The MSX1's first native window, on the shared `emu198x-ui` harness: wgpu
//! video with `raw`/`lcd`/`crt` filters, framed PSG audio, and keyboard/gamepad
//! input. The MSX is keyboard-led; its cursor keys are genuine matrix cells, so
//! they type rather than driving the stick, and the joystick is reached by a
//! real gamepad through [`UiSystem::button_map`]. Compiled only with the `ui`
//! Cargo feature; `main.rs` routes here when no automation flag is given.

use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use emu198x_ui::{
    ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiError, UiSystem, VideoFilter,
};
use runtime_msx::{MapperType, Model, MsxRuntime};

const DEFAULT_SCALE: u32 = 3;
/// CPU clocks per frame — `228 × lines`, matching the headless runner.
const FRAME_TICKS_NTSC: u64 = 228 * 262;
const FRAME_TICKS_PAL: u64 = 228 * 313;
const NTSC_FRAME_HZ: f64 = 60.0;
const PAL_FRAME_HZ: f64 = 50.0;
const BIOS_SIZE: usize = 32 * 1024;

/// Player-1 joystick: four directions plus the trigger-A button, named as
/// `runtime-msx`'s controller mirror expects. The cursor keys are keyboard
/// cells, so a real gamepad reaches the stick through this map.
const MSX_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
]);

const USAGE: &str = "\
Usage: emu198x-msx [OPTIONS]

Options:
    --bios PATH     MSX1 BIOS ROM (32 KB); default
                    ~/.emu198x/roms/microsoft-msx/msx.rom (or set EMU198X_MSX_BIOS)
    --cart PATH     cartridge ROM (slot 1)
    --mapper KIND   cartridge mapper: plain | konami | konami-scc | ascii8 |
                    ascii16 [default: plain]
    --region MODE   ntsc | pal [default: ntsc]
    --scale N       integer window scale, default 3
    --video MODE    raw | lcd | crt [default: raw]
    --help, -h      show this help

Controls:
    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the MSX keyboard (cursor keys are real MSX keys)
    Shift / Ctrl    the MSX SHIFT / CTRL keys (Alt = GRAPH)
    Gamepad         joystick (player 1)

Examples:
    emu198x-msx
    emu198x-msx --cart nemesis.rom --mapper konami --scale 4
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
            Self::Ntsc => Model::Msx1Ntsc,
            Self::Pal => Model::Msx1Pal,
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

/// The MSX1 as a [`UiSystem`] for the shared harness. The region is fixed at
/// construction; a hard reset rebuilds the machine from the firmware and
/// cartridge the runtime already holds.
struct MsxSystem {
    region: Region,
}

impl UiSystem for MsxSystem {
    type Runtime = MsxRuntime;

    fn window_title(&self) -> String {
        "Emu198x MSX".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The MSX's TMS9918 drove a 4:3 TV; its 288×240 framebuffer stretches to
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
        self.region.frame_ticks()
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / self.region.frame_hz())
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &MSX_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_msx_keys(code)
    }
}

/// Parsed interactive CLI.
#[derive(Debug, PartialEq, Eq)]
pub struct Cli {
    bios: Option<PathBuf>,
    cart: Option<PathBuf>,
    mapper: MapperType,
    region: Region,
    scale: u32,
    video: VideoFilter,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            bios: None,
            cart: None,
            mapper: MapperType::Plain,
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
        .ok_or_else(|| "no BIOS: pass --bios PATH or set EMU198X_MSX_BIOS".to_owned())?;
    let bios = read_rom(&bios_path, "BIOS", BIOS_SIZE)?;
    let mut runtime = MsxRuntime::new(cli.region.model(), bios)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;
    if let Some(cart_path) = &cli.cart {
        let cart = std::fs::read(cart_path)
            .map_err(|err| format!("failed to read --cart {}: {err}", cart_path.display()))?;
        runtime.insert_cartridge1(cart, cli.mapper);
    }

    println!(
        "Controls: Esc quit, F12 reset, keyboard typed directly (cursor keys are MSX keys), gamepad joystick."
    );
    emu198x_ui::run(
        MsxSystem { region: cli.region },
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
            "--mapper" => cli.mapper = parse_mapper(&next_arg(&mut iter, "--mapper")),
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

fn parse_mapper(value: &str) -> MapperType {
    match value {
        "plain" => MapperType::Plain,
        "konami" => MapperType::Konami,
        "konami-scc" => MapperType::KonamiScc,
        "ascii8" => MapperType::Ascii8,
        "ascii16" => MapperType::Ascii16,
        other => die(&format!(
            "--mapper expects plain|konami|konami-scc|ascii8|ascii16, got {other}"
        )),
    }
}

fn default_bios_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("EMU198X_MSX_BIOS")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/microsoft-msx/msx.rom"))
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

/// Map a physical host key to its MSX key name (matched by `runtime-msx`'s
/// `key_to_matrix`). The cursor keys are genuine matrix cells, so they map here
/// rather than to the joystick. Shifted symbols are reached by holding SHIFT;
/// host Alt is the GRAPH key. The MSX's own Escape key is unreachable — the
/// harness owns Esc for quit.
fn map_msx_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Equal => &["="],
        KeyCode::Backslash => &["\\"],
        KeyCode::BracketLeft => &["["],
        KeyCode::BracketRight => &["]"],
        KeyCode::Semicolon => &[";"],
        KeyCode::Quote => &["'"],
        KeyCode::Backquote => &["`"],
        KeyCode::Comma => &[","],
        KeyCode::Period => &["."],
        KeyCode::Slash => &["/"],
        KeyCode::Space => &["space"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["enter"],
        KeyCode::Tab => &["tab"],
        KeyCode::Backspace => &["bs"],
        KeyCode::Delete => &["delete"],
        KeyCode::Insert => &["insert"],
        KeyCode::Home => &["home"],
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &["shift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["ctrl"],
        KeyCode::AltLeft | KeyCode::AltRight => &["graph"],
        KeyCode::CapsLock => &["caps"],
        KeyCode::F1 => &["f1"],
        KeyCode::F2 => &["f2"],
        KeyCode::F3 => &["f3"],
        KeyCode::F4 => &["f4"],
        KeyCode::F5 => &["f5"],
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
    fn parse_cli_accepts_bios_cart_mapper_region_scale_video() {
        let cli = parse_cli([
            "--bios".to_owned(),
            "msx.rom".to_owned(),
            "--cart".to_owned(),
            "nemesis.rom".to_owned(),
            "--mapper".to_owned(),
            "konami".to_owned(),
            "--region".to_owned(),
            "pal".to_owned(),
            "--scale".to_owned(),
            "4".to_owned(),
            "--video".to_owned(),
            "crt".to_owned(),
        ]);
        assert_eq!(cli.bios, Some(PathBuf::from("msx.rom")));
        assert_eq!(cli.cart, Some(PathBuf::from("nemesis.rom")));
        assert_eq!(cli.mapper, MapperType::Konami);
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
        assert_eq!(map_msx_keys(KeyCode::ArrowUp), Some(&["up"][..]));
        assert_eq!(map_msx_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_msx_keys(KeyCode::Enter), Some(&["enter"][..]));
        assert_eq!(map_msx_keys(KeyCode::AltLeft), Some(&["graph"][..]));
        assert_eq!(map_msx_keys(KeyCode::F1), Some(&["f1"][..]));
        // Keys with no MSX position are ignored.
        assert_eq!(map_msx_keys(KeyCode::PageUp), None);
    }
}
