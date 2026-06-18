//! Interactive UI mode — the default when no automation flag is present.
//!
//! A native Atari 2600 window built on the shared `emu198x-ui` harness: wgpu
//! video with `raw`/`lcd`/`crt` filters, framed TIA audio, and keyboard/gamepad
//! input. Compiled only with the `ui` Cargo feature; `main.rs` routes here when
//! no `--script`/`--mcp`/automation flag is given.

use std::path::PathBuf;

use emu198x_shell::{MediaKind, Region, read_media_asset};
use emu198x_ui::{
    ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiError, UiSystem, VideoFilter,
};
use runtime_atari_2600::{Atari2600Runtime, Model};

const DEFAULT_SCALE: u32 = 3;
const CLOCKS_PER_LINE: u64 = 228;
const NTSC_LINES: u64 = 262;
const PAL_LINES: u64 = 312;
const NTSC_COLOUR_HZ: f64 = 3_579_545.0;
const PAL_COLOUR_HZ: f64 = 3_546_894.0;

/// Joystick directions + fire (port 1) and the console RESET/SELECT switches.
/// The runtime ignores the port on the console switches, so port 1 is fine.
const ATARI_2600_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
    (HostControl::Start, ButtonTarget::new(1, "reset")),
    (HostControl::Select, ButtonTarget::new(1, "select")),
]);

const USAGE: &str = "\
Usage: emu198x-atari-2600 [OPTIONS] [CART]

Options:
    --cart PATH     cartridge ROM (.a26/.bin, or a .zip). A multi-entry zip
                    (e.g. a merged MAME software list) loads its root parent;
                    append #NAME or #INDEX to pick another, e.g. game.zip#poleposc
    --scale N       integer window scale, default 3
    --video MODE    raw | lcd | crt [default: raw]
    --region MODE   ntsc | pal [default: ntsc]
    --help, -h      show this help

Controls:
    Esc             quit
    F12             emulator hard reset
    Arrow keys      joystick (player 1)
    X / Z / Space   fire
    Enter           console RESET switch
    Right Shift     console SELECT switch

Examples:
    emu198x-atari-2600 frogger2.a26
    emu198x-atari-2600 --cart pitfall2.zip --scale 4 --video crt
";

/// The Atari 2600 as a [`UiSystem`] for the shared harness.
struct Atari2600System;

impl UiSystem for Atari2600System {
    type Runtime = Atari2600Runtime;

    fn window_title(&self) -> String {
        "Emu198x Atari 2600".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The 2600's 160 visible pixels span a 4:3 picture over ~192 active lines,
    // so each pixel is ~1.6× wider than tall ((4/3) × 192/160). Without this the
    // square-pixel framebuffer looks too narrow / vertically stretched.
    fn pixel_aspect_ratio(&self) -> f32 {
        1.6
    }

    // The runtime advances in whole frames, so a sub-frame target would
    // overshoot — run exactly one frame per slice.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| {
                // Visible width (160) — the runtime crops the HBLANK margin out
                // of the frames it presents, so the window must match that.
                (
                    machine.visible_framebuffer_width(),
                    machine.framebuffer_height(),
                )
            })
            .unwrap_or((160, NTSC_LINES as u32))
    }

    fn frame_ticks(&self, runtime: &Self::Runtime) -> u64 {
        let lines = match runtime.model().region() {
            Region::Pal => PAL_LINES,
            _ => NTSC_LINES,
        };
        lines * CLOCKS_PER_LINE
    }

    fn frame_duration(&self, runtime: &Self::Runtime) -> std::time::Duration {
        let hz = match runtime.model().region() {
            Region::Pal => PAL_COLOUR_HZ,
            _ => NTSC_COLOUR_HZ,
        };
        std::time::Duration::from_secs_f64(self.frame_ticks(runtime) as f64 / hz)
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &ATARI_2600_BUTTON_MAP
    }

    fn map_key(&self, code: KeyCode) -> Option<HostControl> {
        Some(match code {
            KeyCode::ArrowUp => HostControl::Up,
            KeyCode::ArrowDown => HostControl::Down,
            KeyCode::ArrowLeft => HostControl::Left,
            KeyCode::ArrowRight => HostControl::Right,
            KeyCode::KeyX | KeyCode::KeyZ | KeyCode::Space => HostControl::South,
            KeyCode::Enter | KeyCode::NumpadEnter => HostControl::Start,
            KeyCode::ShiftRight => HostControl::Select,
            _ => return None,
        })
    }

    /// Report a halted 6507 — a JAM/stop-code, almost always a corrupted ROM
    /// dump (a bad bank decodes a stop-code that hangs the CPU). F12 resets to
    /// clear it.
    fn halt_status(&self, runtime: &Self::Runtime) -> Option<String> {
        let cpu = runtime.machine()?.cpu();
        cpu.halted.then(|| {
            format!(
                "CPU halted (JAM) at ${:04X} — likely a bad ROM dump",
                cpu.regs.pc
            )
        })
    }
}

/// Parsed interactive CLI.
#[derive(Debug, PartialEq, Eq)]
pub struct Cli {
    cart: Option<PathBuf>,
    scale: u32,
    video: VideoFilter,
    region: Region,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            cart: None,
            scale: DEFAULT_SCALE,
            video: VideoFilter::Raw,
            region: Region::Ntsc,
        }
    }
}

/// Parse the interactive CLI. Exits the process on `--help` or a bad flag.
pub fn parse_cli(args: Vec<String>) -> Cli {
    let mut cli = Cli::default();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--cart" => cli.cart = Some(PathBuf::from(next_arg(&mut iter, "--cart"))),
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
            "--region" => {
                cli.region = match next_arg(&mut iter, "--region").as_str() {
                    "ntsc" => Region::Ntsc,
                    "pal" => Region::Pal,
                    other => die(&format!("--region expects ntsc|pal, got {other}")),
                };
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

/// Build the runtime from the CLI and open the window. Returns a string error
/// for the `main.rs` dispatcher.
pub fn run(cli: Cli) -> Result<(), String> {
    let Some(cart_path) = &cli.cart else {
        return Err("provide a cartridge with --cart PATH or as a positional argument".to_owned());
    };
    let loaded = read_media_asset(cart_path, MediaKind::Cartridge)
        .map_err(|err| format!("failed to load cart {}: {err}", cart_path.display()))?;
    let model = match cli.region {
        Region::Pal => Model::Vcs2600Pal,
        _ => Model::Vcs2600Ntsc,
    };
    let runtime = Atari2600Runtime::new(model, loaded.bytes)
        .map_err(|err| format!("failed to start cart {}: {err}", cart_path.display()))?;

    println!(
        "Controls: Esc quit, F12 reset, arrows joystick, X/Z/Space fire, Enter console RESET, Right Shift SELECT."
    );
    emu198x_ui::run(Atari2600System, runtime, cli.scale, cli.video)
        .map_err(|err: UiError| err.to_string())
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
    fn parse_cli_accepts_positional_cart_and_scale() {
        let cli = parse_cli(vec![
            "--scale".to_owned(),
            "2".to_owned(),
            "game.a26".to_owned(),
        ]);
        assert_eq!(cli.cart, Some(PathBuf::from("game.a26")));
        assert_eq!(cli.scale, 2);
        assert_eq!(cli.video, VideoFilter::Raw);
        assert_eq!(cli.region, Region::Ntsc);
    }

    #[test]
    fn parse_cli_accepts_region_and_video() {
        let cli = parse_cli(vec![
            "--region".to_owned(),
            "pal".to_owned(),
            "--video".to_owned(),
            "crt".to_owned(),
            "game.a26".to_owned(),
        ]);
        assert_eq!(cli.region, Region::Pal);
        assert_eq!(cli.video, VideoFilter::Crt);
    }

    #[test]
    fn system_frame_ticks_match_region() {
        let sys = Atari2600System;
        let ntsc = Atari2600Runtime::blank(Model::Vcs2600Ntsc);
        let pal = Atari2600Runtime::blank(Model::Vcs2600Pal);
        assert_eq!(sys.frame_ticks(&ntsc), 262 * 228);
        assert_eq!(sys.frame_ticks(&pal), 312 * 228);
    }

    #[test]
    fn maps_joystick_and_console_keys() {
        let sys = Atari2600System;
        assert_eq!(sys.map_key(KeyCode::ArrowLeft), Some(HostControl::Left));
        assert_eq!(sys.map_key(KeyCode::KeyX), Some(HostControl::South));
        assert_eq!(sys.map_key(KeyCode::Enter), Some(HostControl::Start));
        assert_eq!(sys.map_key(KeyCode::ShiftRight), Some(HostControl::Select));
        assert_eq!(sys.map_key(KeyCode::KeyQ), None);
    }
}
