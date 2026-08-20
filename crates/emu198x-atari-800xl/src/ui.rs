//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Atari 800XL's first native window on the shared `emu198x-ui` harness:
//! wgpu video with `raw`/`lcd`/`crt` filters, framed POKEY audio, and
//! keyboard + gamepad input. The 800XL is a home computer, so the keyboard
//! types (letters/digits/symbols via [`UiSystem::map_keys`], feeding the POKEY
//! scan-code path) and the joystick is reached by a gamepad — or the host arrow
//! keys — through the console path ([`UiSystem::map_key`] +
//! [`UiSystem::button_map`]). The three console keys (Start/Select/Option) are
//! momentary named key events on F2/F3/F4. Compiled only with the `ui` Cargo
//! feature; `main.rs` routes here when no automation flag is given.
//!
//! Chip timing (ANTIC/GTIA/POKEY) matches the 5200 sibling: a frame is
//! `lines × 228` colour clocks; NTSC = 262 lines (~60 Hz), PAL = 312 (~50 Hz).

use std::env;
use std::path::PathBuf;
use std::time::Duration;

use emu198x_ui::{
    ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiError, UiSystem, VideoFilter,
};
use runtime_atari_800xl::{Atari800xlRuntime, Model};

const DEFAULT_SCALE: u32 = 3;
/// Colour clocks per frame — `lines × 228`, matching the headless runner.
const FRAME_TICKS_NTSC: u64 = 262 * 228;
const FRAME_TICKS_PAL: u64 = 312 * 228;
const NTSC_FRAME_HZ: f64 = 60.0;
const PAL_FRAME_HZ: f64 = 50.0;

/// Player-1 joystick: directions + fire, driven by a gamepad or the host arrow
/// keys. The runtime maps these names onto the PIA port-A controller bits.
const ATARI_800XL_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
]);

const USAGE: &str = "\
Usage: emu198x-atari-800xl [OPTIONS]

Options:
    --os PATH       16 KB OS ROM; default
                    ~/.emu198x/roms/atari-800xl/atarixl.rom (or EMU198X_A800XL_OS)
    --basic PATH    8 KB Atari BASIC ROM; default
                    ~/.emu198x/roms/atari-800xl/ataribas.rom (or EMU198X_A800XL_BASIC)
    --cart PATH     cartridge ROM (8 KB or 16 KB)
    --no-basic      hold OPTION at boot to disable the built-in BASIC
    --region MODE   ntsc | pal [default: ntsc]
    --scale N       integer window scale, default 3
    --video MODE    raw | lcd | crt [default: raw]
    --help, -h      show this help

Controls:
    Esc             quit
    F12             emulator hard reset
    A-Z 0-9 etc.    the Atari keyboard
    Enter / Space / Delete / Tab   the matching Atari keys
    Arrow keys      joystick (player 1)
    F2 / F3 / F4    Start / Select / Option console keys
    Gamepad         joystick + fire (player 1)

Examples:
    emu198x-atari-800xl
    emu198x-atari-800xl --cart game.bin --region pal --scale 4
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
            Self::Ntsc => Model::A800xlNtsc,
            Self::Pal => Model::A800xlPal,
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

/// The Atari 800XL as a [`UiSystem`]. The region is fixed at construction; a
/// hard reset rebuilds the machine from the OS / BASIC / cartridge the runtime
/// already holds.
struct Atari800xlSystem {
    region: Region,
}

impl UiSystem for Atari800xlSystem {
    type Runtime = Atari800xlRuntime;

    fn window_title(&self) -> String {
        "Emu198x Atari 800XL".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The 800XL drove a 4:3 TV; its GTIA framebuffer stretches to fill it.

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| (machine.framebuffer_width(), machine.framebuffer_height()))
            .unwrap_or((384, 240))
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        self.region.frame_ticks()
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / self.region.frame_hz())
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &ATARI_800XL_BUTTON_MAP
    }

    fn map_key(&self, code: KeyCode) -> Option<HostControl> {
        // Host arrow keys drive the joystick (the 800XL's own cursor movement
        // is Ctrl+key, not a plain scan code, so the arrows are best spent on
        // the joystick). Fire is a gamepad button — letters must stay typeable.
        Some(match code {
            KeyCode::ArrowUp => HostControl::Up,
            KeyCode::ArrowDown => HostControl::Down,
            KeyCode::ArrowLeft => HostControl::Left,
            KeyCode::ArrowRight => HostControl::Right,
            _ => return None,
        })
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
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
            KeyCode::Enter | KeyCode::NumpadEnter => &["return"],
            KeyCode::Space => &["space"],
            KeyCode::Backspace | KeyCode::Delete => &["delete"],
            KeyCode::Tab => &["tab"],
            // Console keys — momentary, distinct from the harness's Esc/F12.
            KeyCode::F2 => &["start"],
            KeyCode::F3 => &["select"],
            KeyCode::F4 => &["option"],
            _ => return None,
        })
    }
}

/// Parsed interactive CLI.
#[derive(Debug, PartialEq, Eq)]
pub struct Cli {
    os: Option<PathBuf>,
    basic: Option<PathBuf>,
    cart: Option<PathBuf>,
    basic_enabled: bool,
    region: Region,
    scale: u32,
    video: VideoFilter,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            os: None,
            basic: None,
            cart: None,
            basic_enabled: true,
            region: Region::Ntsc,
            scale: DEFAULT_SCALE,
            video: VideoFilter::Raw,
        }
    }
}

/// Build the runtime from the CLI and open the window.
pub fn run(cli: Cli) -> Result<(), String> {
    let os = cli
        .os
        .clone()
        .or_else(|| default_rom("EMU198X_A800XL_OS", "atarixl.rom"))
        .and_then(|p| std::fs::read(p).ok());
    let basic = cli
        .basic
        .clone()
        .or_else(|| default_rom("EMU198X_A800XL_BASIC", "ataribas.rom"))
        .and_then(|p| std::fs::read(p).ok());
    let cart = match &cli.cart {
        Some(p) => Some(
            std::fs::read(p)
                .map_err(|err| format!("failed to read --cart {}: {err}", p.display()))?,
        ),
        None => None,
    };
    if os.is_none() && cart.is_none() {
        return Err("no OS ROM found: pass --os PATH or stage atarixl.rom in \
             ~/.emu198x/roms/atari-800xl/ (or boot a cart with --cart)"
            .to_owned());
    }

    let runtime = Atari800xlRuntime::new(cli.region.model(), os, basic, cart, cli.basic_enabled)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;

    println!(
        "Controls: Esc quit, F12 reset, A-Z/0-9 keyboard, arrows joystick, \
         Z/X via gamepad fire, F2/F3/F4 Start/Select/Option."
    );
    emu198x_ui::run(
        Atari800xlSystem { region: cli.region },
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
            "--os" => cli.os = Some(PathBuf::from(next_arg(&mut iter, "--os"))),
            "--basic" => cli.basic = Some(PathBuf::from(next_arg(&mut iter, "--basic"))),
            "--cart" => cli.cart = Some(PathBuf::from(next_arg(&mut iter, "--cart"))),
            "--no-basic" => cli.basic_enabled = false,
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
            other => die(&format!("unknown flag: {other}")),
        }
    }
    cli
}

fn next_arg<I: Iterator<Item = String>>(iter: &mut I, flag: &str) -> String {
    iter.next()
        .unwrap_or_else(|| die(&format!("{flag} requires a value")))
}

fn die(message: &str) -> ! {
    eprintln!("error: {message}");
    eprintln!();
    eprintln!("{USAGE}");
    std::process::exit(2);
}

fn default_rom(env_key: &str, default_file: &str) -> Option<PathBuf> {
    if let Ok(p) = env::var(env_key)
        && !p.is_empty()
    {
        return Some(PathBuf::from(p));
    }
    let path = PathBuf::from(env::var("HOME").ok()?)
        .join(format!(".emu198x/roms/atari-800xl/{default_file}"));
    path.exists().then_some(path)
}
