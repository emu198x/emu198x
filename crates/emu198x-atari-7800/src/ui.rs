//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Atari 7800's first native window, on the shared `emu198x-ui` harness:
//! wgpu video with `raw`/`lcd`/`crt` filters, framed TIA audio, and
//! keyboard/gamepad input. The 7800 pad is a digital joystick + two fire
//! buttons — the harness's console path ([`UiSystem::map_key`] +
//! [`UiSystem::button_map`]) — plus the three console switches (Reset / Select
//! / Pause), which the runtime takes as named key events, routed through
//! [`UiSystem::map_keys`]. Compiled only with the `ui` Cargo feature; `main.rs`
//! routes here when no automation flag is given.

use std::path::PathBuf;
use std::time::Duration;

use emu198x_shell::MachineCore;
use emu198x_ui::Display;
use emu198x_ui::{
    ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiError, UiSystem, VideoFilter,
};
use runtime_atari_7800::{Atari7800Runtime, Model};

const DEFAULT_SCALE: u32 = 3;
/// CPU clocks per frame — `lines × 228`, matching the headless runner.
const FRAME_TICKS_NTSC: u64 = 262 * 228;
const FRAME_TICKS_PAL: u64 = 312 * 228;
const NTSC_FRAME_HZ: f64 = 60.0;
const PAL_FRAME_HZ: f64 = 50.0;

/// Player-1 control: joystick directions, the two fire buttons, and the two
/// gamepad menu buttons mapped to the console Select / Reset switches. The
/// names are the ones `runtime-atari-7800`'s `set_control` understands.
const ATARI_7800_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire2")),
    (HostControl::Start, ButtonTarget::new(1, "select")),
    (HostControl::Select, ButtonTarget::new(1, "reset")),
]);

const USAGE: &str = "\
Usage: emu198x-atari-7800 [OPTIONS]

Options:
    --cart PATH     Atari 7800 cartridge ROM (.a78 / .bin) (required)
    --region MODE   ntsc | pal [default: ntsc]
    --scale N       integer window scale, default 3
    --video MODE    raw | lcd | crt [default: raw]
    --help, -h      show this help

Controls:
    Esc             quit
    F12             emulator hard reset
    Arrow keys      joystick (player 1)
    Z / X           fire buttons 1 and 2
    Enter           console Select
    Backspace       console Reset
    Delete          console Pause

Examples:
    emu198x-atari-7800 --cart ballblazer.a78
    emu198x-atari-7800 game.bin --region pal --scale 4
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
            Self::Ntsc => Model::A7800Ntsc,
            Self::Pal => Model::A7800Pal,
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

/// The Atari 7800 as a [`UiSystem`] for the shared harness. The region is fixed
/// at construction; a hard reset rebuilds the machine from the cartridge the
/// runtime already holds.
struct Atari7800System {
    region: Region,
}

impl UiSystem for Atari7800System {
    type Runtime = Atari7800Runtime;

    fn window_title(&self) -> String {
        "Emu198x Atari 7800".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The 7800 drove a 4:3 TV; its MARIA framebuffer stretches to fill it.

    /// MARIA keeps the colour clock its predecessors used, so 6:7 on NTSC.
    fn display(&self, runtime: &Self::Runtime) -> Option<Display> {
        Display::television_for_region(
            runtime.profile().region,
            atari_maria::PAL_PIXEL_CLOCK_HZ,
            atari_maria::NTSC_PIXEL_CLOCK_HZ,
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
            .unwrap_or((320, 240))
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        self.region.frame_ticks()
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / self.region.frame_hz())
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &ATARI_7800_BUTTON_MAP
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
        // The three console switches — named key events, distinct from the
        // harness's own Esc-quit / F12-reset.
        Some(match code {
            KeyCode::Enter | KeyCode::NumpadEnter => &["select"],
            KeyCode::Backspace => &["reset"],
            KeyCode::Delete => &["pause"],
            _ => return None,
        })
    }
}

/// Parsed interactive CLI.
#[derive(Debug, PartialEq, Eq)]
pub struct Cli {
    cart: Option<PathBuf>,
    region: Region,
    scale: u32,
    video: VideoFilter,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
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
    let cart_path = cli
        .cart
        .as_ref()
        .ok_or_else(|| "provide a cartridge with --cart PATH".to_owned())?;
    let cart = std::fs::read(cart_path)
        .map_err(|err| format!("failed to read --cart {}: {err}", cart_path.display()))?;
    let runtime = Atari7800Runtime::new(cli.region.model(), cart)
        .map_err(|err| format!("failed to start cart {}: {err}", cart_path.display()))?;

    println!(
        "Controls: Esc quit, F12 reset, arrows joystick, Z/X fire, Enter Select, Backspace Reset, Delete Pause."
    );
    emu198x_ui::run(
        Atari7800System { region: cli.region },
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
    fn parse_cli_accepts_cart_region_scale_video() {
        let cli = parse_cli([
            "--cart".to_owned(),
            "game.a78".to_owned(),
            "--region".to_owned(),
            "pal".to_owned(),
            "--scale".to_owned(),
            "4".to_owned(),
            "--video".to_owned(),
            "crt".to_owned(),
        ]);
        assert_eq!(cli.cart, Some(PathBuf::from("game.a78")));
        assert_eq!(cli.region, Region::Pal);
        assert_eq!(cli.scale, 4);
        assert_eq!(cli.video, VideoFilter::Crt);
    }

    #[test]
    fn parse_cli_accepts_positional_cart() {
        let cli = parse_cli(["game.a78".to_owned()]);
        assert_eq!(cli.cart, Some(PathBuf::from("game.a78")));
    }

    #[test]
    fn region_frame_ticks_match() {
        assert_eq!(Region::Ntsc.frame_ticks(), 262 * 228);
        assert_eq!(Region::Pal.frame_ticks(), 312 * 228);
    }

    #[test]
    fn pad_on_map_key_and_console_switches_on_map_keys() {
        let sys = Atari7800System {
            region: Region::Ntsc,
        };
        assert_eq!(sys.map_key(KeyCode::ArrowLeft), Some(HostControl::Left));
        assert_eq!(sys.map_key(KeyCode::KeyZ), Some(HostControl::South));
        assert_eq!(sys.map_key(KeyCode::KeyX), Some(HostControl::East));
        assert_eq!(sys.map_keys(KeyCode::Enter), Some(&["select"][..]));
        assert_eq!(sys.map_keys(KeyCode::Backspace), Some(&["reset"][..]));
        assert_eq!(sys.map_keys(KeyCode::Delete), Some(&["pause"][..]));
        // No double-routing.
        assert_eq!(sys.map_keys(KeyCode::ArrowLeft), None);
        assert_eq!(sys.map_key(KeyCode::Enter), None);
    }
}
