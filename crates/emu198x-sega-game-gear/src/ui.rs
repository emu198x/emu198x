//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Sega Master System / Game Gear's first native window, on the shared
//! `emu198x-ui` harness: wgpu video with `raw`/`lcd`/`crt` filters, framed VDP
//! audio, and keyboard/gamepad input. The SMS is a console — its pad is the
//! harness's console path ([`UiSystem::map_key`] + [`UiSystem::button_map`]) —
//! plus the single Pause button, which the runtime takes as an
//! [`InputEvent::Key`] (`pause` on the SMS, `start` on the Game Gear), routed
//! through [`UiSystem::map_keys`]. Compiled only with the `ui` Cargo feature;
//! `main.rs` routes here when no automation flag is given.

use std::path::PathBuf;
use std::time::Duration;

use emu198x_ui::{
    ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiError, UiSystem, VideoFilter,
};
use runtime_sega_game_gear::{Model, SmsRuntime, with_cartridge};

const DEFAULT_SCALE: u32 = 3;
/// CPU clocks per frame — `228 × lines`, matching the headless runner.
const FRAME_TICKS: u64 = 228 * 262;
const FRAME_HZ: f64 = 60.0;

/// Player-1 control pad: directions plus the two face buttons. `south`/`east`
/// are the names the class runtime's `controller_bit` maps to the pad's
/// button 1 / button 2. A real gamepad reaches these through the same
/// map; the keyboard does via [`UiSystem::map_key`].
const SMS_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "south")),
    (HostControl::East, ButtonTarget::new(1, "east")),
]);

const USAGE: &str = "\
Usage: emu198x-sega-master-system [OPTIONS]

Options:
    --cart PATH     cartridge ROM (required)
    --variant KIND  game-gear [default: game-gear]
    --scale N       integer window scale, default 3
    --video MODE    raw | lcd | crt [default: raw]
    --help, -h      show this help

Automation:
    --script PATH   run a JSON session headlessly and print a report
    --headless      run without a window (implied by --script)
    --mcp           serve this machine over MCP on stdio

Controls:
    Esc             quit
    F12             emulator hard reset
    Arrow keys      d-pad (player 1)
    Z / X           buttons 1 and 2
    Enter           Start

Examples:
    emu198x-sega-game-gear --cart sonic.gg
    emu198x-sega-game-gear --cart sonic.gg --scale 4
";

/// The Game Gear shipped in one hardware configuration. Kept as a flag so an
/// invocation that used to read `emu198x-sega-master-system --variant
/// game-gear` migrates by changing only the binary name (#998).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Variant {
    GameGear,
}

impl Variant {
    fn model(self) -> Model {
        match self {
            Self::GameGear => Model::GameGear,
        }
    }

    fn frame_ticks(self) -> u64 {
        match self {
            Self::GameGear => FRAME_TICKS,
        }
    }

    fn frame_hz(self) -> f64 {
        match self {
            Self::GameGear => FRAME_HZ,
        }
    }
}

/// The Sega Game Gear as a [`UiSystem`] for the shared harness.
/// The variant is fixed at construction; a hard reset rebuilds the machine from
/// the cartridge the runtime already holds.
struct GameGearSystem {
    variant: Variant,
}

impl UiSystem for GameGearSystem {
    type Runtime = SmsRuntime;

    fn window_title(&self) -> String {
        "Emu198x Sega Game Gear".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The Game Gear is a square-pixel LCD, so it needs no aspect correction.

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| (machine.framebuffer_width(), machine.framebuffer_height()))
            .unwrap_or((160, 144))
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        self.variant.frame_ticks()
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / self.variant.frame_hz())
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &SMS_BUTTON_MAP
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
        // The single console button. The Game Gear labels it Start where the
        // Master System labels it Pause; the runtime takes it as a named key
        // event either way.
        match code {
            KeyCode::Enter | KeyCode::NumpadEnter => Some(&["start"]),
            _ => None,
        }
    }
}

/// Parsed interactive CLI.
#[derive(Debug, PartialEq, Eq)]
pub struct Cli {
    cart: Option<PathBuf>,
    variant: Variant,
    scale: u32,
    video: VideoFilter,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            cart: None,
            variant: Variant::GameGear,
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
    let runtime = with_cartridge(cli.variant.model(), cart);

    println!("Controls: Esc quit, F12 reset, arrows d-pad, Z/X buttons, Enter Pause/Start.");
    emu198x_ui::run(
        GameGearSystem {
            variant: cli.variant,
        },
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
            "--variant" => {
                cli.variant = match next_arg(&mut iter, "--variant").as_str() {
                    "game-gear" | "gg" => Variant::GameGear,
                    other => die(&format!("--variant expects game-gear, got {other}")),
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
    fn parse_cli_accepts_cart_variant_scale_video() {
        let cli = parse_cli([
            "--cart".to_owned(),
            "game.gg".to_owned(),
            "--variant".to_owned(),
            "game-gear".to_owned(),
            "--scale".to_owned(),
            "4".to_owned(),
            "--video".to_owned(),
            "crt".to_owned(),
        ]);
        assert_eq!(cli.cart, Some(PathBuf::from("game.gg")));
        assert_eq!(cli.variant, Variant::GameGear);
        assert_eq!(cli.scale, 4);
        assert_eq!(cli.video, VideoFilter::Crt);
    }

    #[test]
    fn parse_cli_accepts_positional_cart() {
        let cli = parse_cli(["sonic.gg".to_owned()]);
        assert_eq!(cli.cart, Some(PathBuf::from("sonic.gg")));
        assert_eq!(cli.variant, Variant::GameGear);
    }

    #[test]
    fn variant_frame_ticks_match() {
        assert_eq!(Variant::GameGear.frame_ticks(), 228 * 262);
    }

    /// The Game Gear labels the console button Start, where its Master System
    /// sibling labels it Pause.
    #[test]
    fn pad_maps_and_console_button_is_start() {
        let gg = GameGearSystem {
            variant: Variant::GameGear,
        };
        assert_eq!(gg.map_key(KeyCode::ArrowLeft), Some(HostControl::Left));
        assert_eq!(gg.map_key(KeyCode::KeyZ), Some(HostControl::South));
        assert_eq!(gg.map_key(KeyCode::KeyX), Some(HostControl::East));
        assert_eq!(gg.map_keys(KeyCode::Enter), Some(&["start"][..]));
    }
}
