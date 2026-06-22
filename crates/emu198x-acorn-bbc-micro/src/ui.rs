//! Interactive UI mode — the default when no automation flag is present.
//!
//! The BBC Micro's first native window, on the shared `emu198x-ui` harness:
//! wgpu video with `raw`/`lcd`/`crt` filters, framed SN76489 audio, and
//! keyboard/gamepad input. The BBC is keyboard-led; its cursor keys are genuine
//! matrix cells, so they type. The analogue joystick's fire button is reached
//! by a real gamepad through [`UiSystem::button_map`] (the proportional axes go
//! through a μPD7002 ADC path the harness gamepad doesn't drive yet). Compiled
//! only with the `ui` Cargo feature; `main.rs` routes here when no automation
//! flag is given.

use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use emu198x_ui::{
    ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiError, UiSystem, VideoFilter,
};
use runtime_acorn_bbc_micro::{BbcMicroRuntime, Model};

const DEFAULT_SCALE: u32 = 3;
/// 6502 @ 2 MHz, 50 Hz → 40,000 cycles/frame, matching the headless runner.
// Keep <= the machine's run_frame() size, or the harness runs two machine
// frames per displayed frame (~2x too fast). See docs/status/ui-boot-verification.
const FRAME_TICKS_PAL: u64 = 39_936;
const PAL_FRAME_HZ: f64 = 50.0;
const MOS_SIZE: usize = 16 * 1024;

/// The analogue joystick's fire button. The proportional X/Y axes are read
/// through the μPD7002 ADC (a separate `Axis` path the harness gamepad doesn't
/// drive yet), so only fire is mapped here.
const BBC_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
]);

const USAGE: &str = "\
Usage: emu198x-acorn-bbc-micro [OPTIONS]

Options:
    --mos PATH      BBC MOS ROM (16 KB); default
                    ~/.emu198x/roms/acorn-bbc-micro/mos.rom (or set EMU198X_BBC_MOS)
    --scale N       integer window scale, default 3
    --video MODE    raw | lcd | crt [default: raw]
    --help, -h      show this help

Controls:
    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the BBC keyboard (cursor keys are real BBC keys)
    Shift / Ctrl    the BBC SHIFT / CTRL keys; F1-F10 are the red f0-f9 keys
    Gamepad         joystick fire (player 1)

Examples:
    emu198x-acorn-bbc-micro
    emu198x-acorn-bbc-micro --mos mos.rom --scale 2
";

/// The BBC Micro as a [`UiSystem`] for the shared harness. Single-model; a hard
/// reset rebuilds the machine from the firmware the runtime already holds.
struct BbcSystem;

impl UiSystem for BbcSystem {
    type Runtime = BbcMicroRuntime;

    fn window_title(&self) -> String {
        "Emu198x BBC Micro".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The BBC drove a 4:3 TV / monitor; its framebuffer stretches to fill it.
    fn display_aspect_ratio(&self) -> Option<f32> {
        Some(4.0 / 3.0)
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
            .unwrap_or((640, 256))
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        FRAME_TICKS_PAL
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / PAL_FRAME_HZ)
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &BBC_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_bbc_keys(code)
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
        .ok_or_else(|| "no MOS ROM: pass --mos PATH or set EMU198X_BBC_MOS".to_owned())?;
    let mos = read_rom(&mos_path)?;
    let mut runtime = BbcMicroRuntime::new(Model::BbcModelB, mos)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;
    // Best-effort: install BASIC as the default language in the highest-priority
    // sideways bank (15) if a ROM is staged, so the machine boots to the BASIC
    // prompt rather than the bare MOS. Headless callers pass `--sideways`
    // explicitly instead.
    if let Some(basic_path) = default_basic_path()
        && let Ok(basic) = std::fs::read(&basic_path)
    {
        runtime.insert_sideways_rom(15, basic);
    }
    // Load the SAA5050 teletext character ROM (MODE 7) if one is available.
    if let Some(font_path) = default_font_path()
        && let Ok(font) = std::fs::read(&font_path)
    {
        runtime.set_teletext_font(font);
    }

    println!(
        "Controls: Esc quit, F12 reset, keyboard typed directly (cursor keys are BBC keys), gamepad fire."
    );
    emu198x_ui::run(BbcSystem, runtime, cli.scale, cli.video)
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
    if let Ok(path) = env::var("EMU198X_BBC_MOS")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/acorn-bbc-micro/mos.rom"))
}

fn default_basic_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("EMU198X_BBC_BASIC")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    let home = env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".emu198x/roms/acorn-bbc-micro/basic.rom");
    path.exists().then_some(path)
}

fn default_font_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("EMU198X_BBC_SAA5050")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    let home = env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".emu198x/roms/acorn-bbc-micro/saa5050.rom");
    path.exists().then_some(path)
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

/// Map a physical host key to its BBC key name (matched by
/// `runtime-acorn-bbc-micro`'s `key_to_matrix`). The cursor keys are genuine
/// matrix cells, so they type. The red function keys f0-f9 map from host
/// F1-F10. Symbols shifted on a modern host are omitted. The BBC's own Escape
/// key is unreachable — the harness owns Esc for quit.
fn map_bbc_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Semicolon => &[";"],
        KeyCode::Comma => &[","],
        KeyCode::Period => &["."],
        KeyCode::Slash => &["/"],
        KeyCode::BracketLeft => &["["],
        KeyCode::BracketRight => &["]"],
        KeyCode::Backslash => &["\\"],
        KeyCode::Space => &["space"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["return"],
        KeyCode::Tab => &["tab"],
        KeyCode::Backspace | KeyCode::Delete => &["delete"],
        KeyCode::End => &["copy"],
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &["shift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["ctrl"],
        KeyCode::CapsLock => &["caps"],
        // The red function keys f0-f9 sit on host F1-F10.
        KeyCode::F1 => &["f0"],
        KeyCode::F2 => &["f1"],
        KeyCode::F3 => &["f2"],
        KeyCode::F4 => &["f3"],
        KeyCode::F5 => &["f4"],
        KeyCode::F6 => &["f5"],
        KeyCode::F7 => &["f6"],
        KeyCode::F8 => &["f7"],
        KeyCode::F9 => &["f8"],
        KeyCode::F10 => &["f9"],
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
    fn parse_cli_accepts_mos_scale_video() {
        let cli = parse_cli([
            "--mos".to_owned(),
            "mos.rom".to_owned(),
            "--scale".to_owned(),
            "2".to_owned(),
            "--video".to_owned(),
            "crt".to_owned(),
        ]);
        assert_eq!(cli.mos, Some(PathBuf::from("mos.rom")));
        assert_eq!(cli.scale, 2);
        assert_eq!(cli.video, VideoFilter::Crt);
    }

    #[test]
    fn cursor_keys_type_and_function_keys_map_to_red_keys() {
        assert_eq!(map_bbc_keys(KeyCode::ArrowUp), Some(&["up"][..]));
        assert_eq!(map_bbc_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_bbc_keys(KeyCode::Enter), Some(&["return"][..]));
        // Host F1 is the BBC's red f0.
        assert_eq!(map_bbc_keys(KeyCode::F1), Some(&["f0"][..]));
        assert_eq!(map_bbc_keys(KeyCode::F10), Some(&["f9"][..]));
        assert_eq!(map_bbc_keys(KeyCode::End), Some(&["copy"][..]));
        // Keys with no BBC position are ignored.
        assert_eq!(map_bbc_keys(KeyCode::PageUp), None);
    }
}
