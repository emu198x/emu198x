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
use runtime_sega_master_system::{Model, SmsRuntime, with_cartridge};

const DEFAULT_SCALE: u32 = 3;
/// CPU clocks per frame — `228 × lines`, matching the headless runner.
const FRAME_TICKS_NTSC: u64 = 228 * 262;
const FRAME_TICKS_PAL: u64 = 228 * 313;
const NTSC_FRAME_HZ: f64 = 60.0;
const PAL_FRAME_HZ: f64 = 50.0;

/// Player-1 control pad: directions plus the two face buttons. `south`/`east`
/// are the names `runtime-sega-master-system`'s `controller_bit` maps to the
/// SMS pad's button 1 / button 2. A real gamepad reaches these through the same
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
    --variant KIND  sms-ntsc | sms-pal | sms1-ntsc | sms1-pal [default: sms-ntsc]
                    sms1 selects the early 315-5124 VDP
    --scale N       integer window scale, default 3
    --video MODE    raw | lcd | crt [default: raw]
    --help, -h      show this help

Controls:
    Esc             quit
    F12             emulator hard reset
    Arrow keys      d-pad (player 1)
    Z / X           buttons 1 and 2
    Enter           Pause

Examples:
    emu198x-sega-master-system --cart sonic.sms
    emu198x-sega-master-system --cart sonic.sms --variant sms-pal --scale 4
";

/// Console variant — selects the model, frame tick budget and refresh rate.
/// The Game Gear ships from `emu198x-sega-game-gear` (#998).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Variant {
    SmsNtsc,
    SmsPal,
    Sms1Ntsc,
    Sms1Pal,
}

impl Variant {
    fn model(self) -> Model {
        match self {
            Self::SmsNtsc => Model::SmsNtsc,
            Self::SmsPal => Model::SmsPal,
            Self::Sms1Ntsc => Model::Sms1Ntsc,
            Self::Sms1Pal => Model::Sms1Pal,
        }
    }

    fn frame_ticks(self) -> u64 {
        match self {
            Self::SmsPal | Self::Sms1Pal => FRAME_TICKS_PAL,
            Self::SmsNtsc | Self::Sms1Ntsc => FRAME_TICKS_NTSC,
        }
    }

    fn frame_hz(self) -> f64 {
        match self {
            Self::SmsPal | Self::Sms1Pal => PAL_FRAME_HZ,
            Self::SmsNtsc | Self::Sms1Ntsc => NTSC_FRAME_HZ,
        }
    }
}

/// The Sega Master System as a [`UiSystem`] for the shared harness.
/// The variant is fixed at construction; a hard reset rebuilds the machine from
/// the cartridge the runtime already holds.
struct SmsSystem {
    variant: Variant,
}

impl UiSystem for SmsSystem {
    /// The Light Phaser lives in controller port 1, where every light-gun
    /// title expects it. Pointing the mouse aims it; the left button is the
    /// trigger, mapped through the ordinary button path.
    fn aim_port(&self) -> Option<u8> {
        Some(1)
    }

    type Runtime = SmsRuntime;

    fn window_title(&self) -> String {
        "Emu198x Sega Master System".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The SMS drove a 4:3 TV.

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| (machine.framebuffer_width(), machine.framebuffer_height()))
            .unwrap_or((280, 240))
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
        // The single console button. The runtime takes it as a named key
        // event.
        match code {
            KeyCode::Enter | KeyCode::NumpadEnter => Some(&["pause"]),
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
            variant: Variant::SmsNtsc,
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
        SmsSystem {
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
                    "sms-ntsc" | "sms" => Variant::SmsNtsc,
                    "sms-pal" => Variant::SmsPal,
                    "sms1-ntsc" | "sms1" => Variant::Sms1Ntsc,
                    "sms1-pal" => Variant::Sms1Pal,
                    other => die(&format!(
                        "--variant expects sms-ntsc|sms-pal|sms1-ntsc|sms1-pal, got {other}"
                    )),
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
            "sonic.sms".to_owned(),
            "--variant".to_owned(),
            "sms-pal".to_owned(),
            "--scale".to_owned(),
            "4".to_owned(),
            "--video".to_owned(),
            "crt".to_owned(),
        ]);
        assert_eq!(cli.cart, Some(PathBuf::from("sonic.sms")));
        assert_eq!(cli.variant, Variant::SmsPal);
        assert_eq!(cli.scale, 4);
        assert_eq!(cli.video, VideoFilter::Crt);
    }

    #[test]
    fn parse_cli_accepts_positional_cart() {
        let cli = parse_cli(["sonic.sms".to_owned()]);
        assert_eq!(cli.cart, Some(PathBuf::from("sonic.sms")));
        assert_eq!(cli.variant, Variant::SmsNtsc);
    }

    #[test]
    fn variant_frame_ticks_match() {
        assert_eq!(Variant::SmsNtsc.frame_ticks(), 228 * 262);
        assert_eq!(Variant::SmsPal.frame_ticks(), 228 * 313);
    }

    #[test]
    fn pad_maps_and_console_button() {
        let sms = SmsSystem {
            variant: Variant::SmsNtsc,
        };
        assert_eq!(sms.map_key(KeyCode::ArrowLeft), Some(HostControl::Left));
        assert_eq!(sms.map_key(KeyCode::KeyZ), Some(HostControl::South));
        assert_eq!(sms.map_key(KeyCode::KeyX), Some(HostControl::East));
        assert_eq!(sms.map_keys(KeyCode::Enter), Some(&["pause"][..]));
    }
}
