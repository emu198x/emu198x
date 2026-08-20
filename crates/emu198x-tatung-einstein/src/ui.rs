//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Tatung Einstein's first native window, on the shared `emu198x-ui`
//! harness: wgpu video with `raw`/`lcd`/`crt` filters and the keyboard routed
//! through the harness's general-keyboard path ([`UiSystem::map_keys`]). The
//! Einstein is keyboard-led; its joysticks are analogue (pot-per-axis) and are
//! reached by a real gamepad through [`UiSystem::button_map`] (the harness
//! drains gamepad events through the button map, which the runtime snaps to the
//! pot extremes). Compiled only with the `ui` Cargo feature; `main.rs` routes
//! here when no automation flag is given.

use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use emu198x_shell::MachineCore;
use emu198x_ui::Display;
use emu198x_ui::{
    ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiError, UiSystem, VideoFilter,
};
use runtime_tatung_einstein::{EinsteinRuntime, Model};

const DEFAULT_SCALE: u32 = 3;
/// Z80 @ 4 MHz, 50 Hz PAL → 80,000 t-states/frame, matching the headless
/// runner's `FRAME_TICKS_PAL`.
// Keep <= the machine's run_frame() size, or the harness runs two machine
// frames per displayed frame (~2x too fast). See docs/status/ui-boot-verification.
const FRAME_TICKS_PAL: u64 = 79_700;
const PAL_FRAME_HZ: f64 = 50.0;
const MOS_SIZE: usize = 8 * 1024;

/// Player-1 joystick: four directions plus fire, named as
/// `runtime-tatung-einstein`'s controller mirror expects (the digital
/// directions snap the analogue pots to their extremes). A real gamepad reaches
/// these through the button map.
const EINSTEIN_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
]);

const USAGE: &str = "\
Usage: emu198x-tatung-einstein [OPTIONS]

Options:
    --mos PATH      Einstein MOS ROM (8 KB); default
                    ~/.emu198x/roms/tatung-einstein/mos.rom
                    (or set EMU198X_EINSTEIN_MOS)
    --scale N       integer window scale, default 3
    --video MODE    raw | lcd | crt [default: raw]
    --help, -h      show this help

Controls:
    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the Einstein keyboard
    Shift / Ctrl    the Einstein SHIFT / CONTROL keys
    Gamepad         joystick (player 1)

Examples:
    emu198x-tatung-einstein
    emu198x-tatung-einstein --mos mos.rom --scale 4
";

/// The Tatung Einstein as a [`UiSystem`] for the shared harness. Single-model;
/// a hard reset rebuilds the machine from the firmware the runtime holds.
struct EinsteinSystem;

impl UiSystem for EinsteinSystem {
    type Runtime = EinsteinRuntime;

    fn window_title(&self) -> String {
        "Emu198x Tatung Einstein".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The Einstein's TMS9929 drove a 4:3 TV; its 288×240 framebuffer stretches
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

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| (machine.framebuffer_width(), machine.framebuffer_height()))
            .unwrap_or((288, 240))
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        FRAME_TICKS_PAL
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / PAL_FRAME_HZ)
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &EINSTEIN_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_einstein_keys(code)
    }
}

/// Parsed interactive CLI.
#[derive(Debug, PartialEq, Eq)]
pub struct Cli {
    mos: Option<PathBuf>,
    scale: u32,
    video: VideoFilter,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            mos: None,
            scale: DEFAULT_SCALE,
            video: VideoFilter::Raw,
        }
    }
}

/// Build the runtime from the CLI and open the window. Returns a string error
/// for the `main.rs` dispatcher.
pub fn run(cli: Cli) -> Result<(), String> {
    let mos_path = cli
        .mos
        .clone()
        .or_else(default_mos_path)
        .ok_or_else(|| "no MOS ROM: pass --mos PATH or set EMU198X_EINSTEIN_MOS".to_owned())?;
    let mos = read_rom(&mos_path)?;
    let runtime = EinsteinRuntime::new(Model::Einstein, mos)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;

    println!(
        "Controls: Esc quit, F12 reset, keyboard typed directly, Shift/Ctrl modifier keys, gamepad joystick."
    );
    emu198x_ui::run(EinsteinSystem, runtime, cli.scale, cli.video)
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
            "--mos" => cli.mos = Some(PathBuf::from(next_arg(&mut iter, "--mos"))),
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

fn default_mos_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("EMU198X_EINSTEIN_MOS")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/tatung-einstein/mos.rom"))
}

fn read_rom(path: &Path) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(path)
        .map_err(|err| format!("failed to read MOS ROM {}: {err}", path.display()))?;
    if bytes.len() != MOS_SIZE {
        return Err(format!(
            "MOS ROM at {} is {} bytes; expected {MOS_SIZE}",
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

/// Map a physical host key to its Einstein key name (matched by
/// `runtime-tatung-einstein`'s `key_to_matrix` / `key_to_modifier`). SHIFT and
/// CONTROL are status-port modifiers, not matrix cells, but the runtime routes
/// them by name so they map here too. Shifted symbols are reached by holding a
/// shift with another key, so only the unshifted legends need mapping. The
/// Einstein's own Escape key is unreachable — the harness owns Esc for quit.
fn map_einstein_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Comma => &[","],
        KeyCode::Period => &["."],
        KeyCode::Slash => &["/"],
        KeyCode::Equal => &["="],
        KeyCode::Space => &["space"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["return"],
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &["shift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["ctrl"],
        KeyCode::AltLeft | KeyCode::AltRight => &["graph"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_accepts_mos_scale_video() {
        let cli = parse_cli([
            "--mos".to_owned(),
            "mos.rom".to_owned(),
            "--scale".to_owned(),
            "4".to_owned(),
            "--video".to_owned(),
            "crt".to_owned(),
        ]);
        assert_eq!(cli.mos, Some(PathBuf::from("mos.rom")));
        assert_eq!(cli.scale, 4);
        assert_eq!(cli.video, VideoFilter::Crt);
    }

    #[test]
    fn maps_keys_modifiers_and_graph() {
        assert_eq!(map_einstein_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_einstein_keys(KeyCode::Digit5), Some(&["5"][..]));
        assert_eq!(map_einstein_keys(KeyCode::Enter), Some(&["return"][..]));
        assert_eq!(map_einstein_keys(KeyCode::ShiftLeft), Some(&["shift"][..]));
        assert_eq!(map_einstein_keys(KeyCode::ControlLeft), Some(&["ctrl"][..]));
        assert_eq!(map_einstein_keys(KeyCode::AltLeft), Some(&["graph"][..]));
        // Keys with no Einstein position are ignored.
        assert_eq!(map_einstein_keys(KeyCode::Tab), None);
    }
}
