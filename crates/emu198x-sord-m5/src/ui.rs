//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Sord M5's first native window, on the shared `emu198x-ui` harness: wgpu
//! video with `raw`/`lcd`/`crt` filters and the keyboard routed through the
//! harness's general-keyboard path ([`UiSystem::map_keys`]). The M5 is
//! keyboard-led; its joystick carries only the four directions (the action
//! buttons are keyboard keys), reached by a real gamepad through
//! [`UiSystem::button_map`]. Compiled only with the `ui` Cargo feature;
//! `main.rs` routes here when no automation flag is given.

use std::env;
use std::path::PathBuf;
use std::time::Duration;

use emu198x_shell::MachineCore;
use emu198x_ui::{
    ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiError, UiSystem, VideoFilter,
};
use runtime_sord_m5::{M5Runtime, Model};

const DEFAULT_SCALE: u32 = 3;
/// CPU clocks per frame — `228 × lines`, matching the headless runner.
const FRAME_TICKS_NTSC: u64 = 228 * 262;
const FRAME_TICKS_PAL: u64 = 228 * 313;
const NTSC_FRAME_HZ: f64 = 60.0;
const PAL_FRAME_HZ: f64 = 50.0;

/// Player-1 joystick: four directions only — the M5 has no joystick fire line
/// (action buttons are keyboard keys), so the button map carries no fire. A
/// real gamepad reaches the directions through this map.
const SORD_M5_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
]);

const USAGE: &str = "\
Usage: emu198x-sord-m5 [OPTIONS]

Options:
    --rom PATH      Sord M5 BIOS ROM (monitor + BASIC-I); default
                    ~/.emu198x/roms/sord-m5/sord-m5.rom (or set EMU198X_SORD_M5_ROM)
    --cart PATH     cartridge ROM (optional)
    --region MODE   ntsc | pal [default: ntsc]
    --scale N       integer window scale, default 3
    --video MODE    raw | lcd | crt [default: raw]
    --help, -h      show this help

Controls:
    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the M5 keyboard
    Shift / Ctrl    the M5 SHIFT / CONTROL keys (Tab = FUNC)
    Gamepad         joystick (player 1, directions)

Examples:
    emu198x-sord-m5
    emu198x-sord-m5 --cart game.rom --scale 4
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
            Self::Ntsc => Model::M5Ntsc,
            Self::Pal => Model::M5Pal,
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

/// The Sord M5 as a [`UiSystem`] for the shared harness. The region is fixed at
/// construction; a hard reset rebuilds the machine from the firmware the
/// runtime already holds.
struct SordM5System {
    region: Region,
}

impl UiSystem for SordM5System {
    type Runtime = M5Runtime;

    fn window_title(&self) -> String {
        "Emu198x Sord M5".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The M5's TMS9918 drove a 4:3 TV; its 288×240 framebuffer stretches to
    // fill it.

    /// The TMS9918 family drove a television through a colour-subcarrier
    /// crystal, so its dots are not square: 8:7 on the NTSC parts, about
    /// 1.382 on the PAL TMS9929A. Presenting the 288x240 framebuffer unstretched
    /// claimed otherwise.
    fn pixel_aspect_ratio(&self, runtime: &Self::Runtime) -> Option<f32> {
        emu198x_shell::display::pixel_aspect_for_region(
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
        &SORD_M5_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_m5_keys(code)
    }
}

/// Parsed interactive CLI.
#[derive(Debug, PartialEq, Eq)]
pub struct Cli {
    rom: Option<PathBuf>,
    cart: Option<PathBuf>,
    region: Region,
    scale: u32,
    video: VideoFilter,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            rom: None,
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
    let rom_path = cli
        .rom
        .clone()
        .or_else(default_rom_path)
        .ok_or_else(|| "no ROM: pass --rom PATH or set EMU198X_SORD_M5_ROM".to_owned())?;
    let rom = std::fs::read(&rom_path)
        .map_err(|err| format!("failed to read ROM {}: {err}", rom_path.display()))?;
    let mut runtime = M5Runtime::new(cli.region.model(), rom);
    if let Some(cart_path) = &cli.cart {
        let cart = std::fs::read(cart_path)
            .map_err(|err| format!("failed to read --cart {}: {err}", cart_path.display()))?;
        runtime.insert_cartridge(cart);
    }

    println!(
        "Controls: Esc quit, F12 reset, keyboard typed directly (Tab = FUNC), gamepad joystick directions."
    );
    emu198x_ui::run(
        SordM5System { region: cli.region },
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
            "--rom" => cli.rom = Some(PathBuf::from(next_arg(&mut iter, "--rom"))),
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

fn default_rom_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("EMU198X_SORD_M5_ROM")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/sord-m5/sord-m5.rom"))
}

fn next_arg<I: Iterator<Item = String>>(iter: &mut I, flag: &str) -> String {
    iter.next()
        .unwrap_or_else(|| die(&format!("missing value for {flag}")))
}

fn die(message: &str) -> ! {
    eprintln!("error: {message}");
    std::process::exit(1);
}

/// Map a physical host key to its M5 key name (matched by `runtime-sord-m5`'s
/// `key_to_matrix`). The M5 has no cursor keys on the keyboard — those are the
/// gamepad joystick — so only the matrix keys map here. Shifted symbols are
/// reached by holding SHIFT; host Tab is the M5's FUNC key.
fn map_m5_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Comma => &[","],
        KeyCode::Period => &["."],
        KeyCode::Slash => &["/"],
        KeyCode::BracketLeft => &["["],
        KeyCode::BracketRight => &["]"],
        KeyCode::Semicolon => &[":"],
        KeyCode::Quote => &["'"],
        KeyCode::Backslash => &["\\"],
        KeyCode::Space => &["space"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["enter"],
        KeyCode::Backspace | KeyCode::Delete => &["backspace"],
        KeyCode::Tab => &["func"],
        KeyCode::ShiftLeft => &["shift"],
        KeyCode::ShiftRight => &["rshift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["ctrl"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_accepts_rom_cart_region_scale_video() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "m5.rom".to_owned(),
            "--cart".to_owned(),
            "game.rom".to_owned(),
            "--region".to_owned(),
            "pal".to_owned(),
            "--scale".to_owned(),
            "4".to_owned(),
            "--video".to_owned(),
            "crt".to_owned(),
        ]);
        assert_eq!(cli.rom, Some(PathBuf::from("m5.rom")));
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
    fn maps_keys_func_and_shifts() {
        assert_eq!(map_m5_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_m5_keys(KeyCode::Enter), Some(&["enter"][..]));
        assert_eq!(map_m5_keys(KeyCode::Tab), Some(&["func"][..]));
        assert_eq!(map_m5_keys(KeyCode::ShiftRight), Some(&["rshift"][..]));
        assert_eq!(map_m5_keys(KeyCode::ControlLeft), Some(&["ctrl"][..]));
        // The M5 has no keyboard cursor keys — those are the gamepad joystick.
        assert_eq!(map_m5_keys(KeyCode::ArrowUp), None);
    }
}
