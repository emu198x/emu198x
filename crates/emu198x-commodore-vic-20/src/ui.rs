//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Commodore VIC-20's first native window, on the shared `emu198x-ui`
//! harness: wgpu video with `raw`/`lcd`/`crt` filters, framed VIC audio, and
//! keyboard/gamepad input. The VIC-20 is keyboard-led; its two real cursor
//! keys are matrix cells, so they type, and the single joystick port is reached
//! by a real gamepad through [`UiSystem::button_map`]. Compiled only with the
//! `ui` Cargo feature; `main.rs` routes here when no automation flag is given.

use std::env;
use std::path::PathBuf;
use std::time::Duration;

use emu198x_shell::MachineCore;
use emu198x_ui::Display;
use emu198x_ui::{
    ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiError, UiSystem, VideoFilter,
};
use runtime_commodore_vic_20::{Model, Vic20Runtime};

const DEFAULT_SCALE: u32 = 3;
/// VIC cycles per frame — `cols × lines`, matching the headless runner.
const FRAME_TICKS_PAL: u64 = 71 * 312;
const FRAME_TICKS_NTSC: u64 = 65 * 261;
const NTSC_FRAME_HZ: f64 = 60.0;
const PAL_FRAME_HZ: f64 = 50.0;
const KERNAL_SIZE: usize = 8 * 1024;
const BASIC_SIZE: usize = 8 * 1024;
const CHAR_SIZE: usize = 4 * 1024;

/// The VIC-20's single control port: four directions plus fire, named as
/// `runtime-commodore-vic-20`'s controller mirror expects. The cursor keys are
/// keyboard cells, so a real gamepad reaches the joystick through this map.
const VIC20_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
]);

const USAGE: &str = "\
Usage: emu198x-commodore-vic-20 [OPTIONS]

ROMs (defaults: $EMU198X_VIC20_{KERNAL,BASIC,CHAR}, then
~/.emu198x/roms/commodore-vic-20/{kernal,basic,chargen}.rom):
    --kernal PATH   KERNAL ROM (8 KB)
    --basic PATH    BASIC ROM (8 KB)
    --char PATH     character ROM (4 KB)

Display / input:
    --region MODE   ntsc | pal [default: pal]
    --scale N       integer window scale, default 3
    --video MODE    raw | lcd | crt [default: raw]
    --help, -h      show this help

Controls:
    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the VIC-20 keyboard
    Right / Down    the two cursor keys; Tab = RUN/STOP, Alt = Commodore
    Gamepad         joystick (single control port)

Examples:
    emu198x-commodore-vic-20
    emu198x-commodore-vic-20 --region ntsc --scale 3
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
            Self::Ntsc => Model::Vic20Ntsc,
            Self::Pal => Model::Vic20Pal,
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

/// The Commodore VIC-20 as a [`UiSystem`] for the shared harness. The region is
/// fixed at construction; a hard reset rebuilds the machine from the firmware
/// the runtime already holds.
struct Vic20System {
    region: Region,
}

impl UiSystem for Vic20System {
    type Runtime = Vic20Runtime;

    fn window_title(&self) -> String {
        "Emu198x Commodore VIC-20".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The VIC-20 drove a 4:3 TV; its framebuffer stretches to fill it.

    /// Eight pixels per machine cycle. On NTSC that is the VIC-II's clock, so a
    /// VIC-20 and a C64 share a pixel shape there and diverge on PAL, where
    /// their cycle rates differ.
    fn display(&self, runtime: &Self::Runtime) -> Option<Display> {
        Display::television_for_region(
            runtime.profile().region,
            mos_vic_i::PAL_PIXEL_CLOCK_HZ,
            mos_vic_i::NTSC_PIXEL_CLOCK_HZ,
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
            .unwrap_or((232, 284))
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        self.region.frame_ticks()
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / self.region.frame_hz())
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &VIC20_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_vic20_keys(code)
    }
}

/// Parsed interactive CLI.
#[derive(Debug, PartialEq, Eq)]
pub struct Cli {
    kernal: Option<PathBuf>,
    basic: Option<PathBuf>,
    char_rom: Option<PathBuf>,
    region: Region,
    scale: u32,
    video: VideoFilter,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            kernal: None,
            basic: None,
            char_rom: None,
            region: Region::Pal,
            scale: DEFAULT_SCALE,
            video: VideoFilter::Raw,
        }
    }
}

/// Build the runtime from the CLI and open the window. Returns a string error
/// for the `main.rs` dispatcher.
pub fn run(cli: Cli) -> Result<(), String> {
    let kernal = read_required(
        cli.kernal.clone(),
        "KERNAL",
        "KERNAL",
        "kernal.rom",
        KERNAL_SIZE,
    )?;
    let basic = read_required(cli.basic.clone(), "BASIC", "BASIC", "basic.rom", BASIC_SIZE)?;
    let char_rom = read_required(
        cli.char_rom.clone(),
        "character",
        "CHAR",
        "chargen.rom",
        CHAR_SIZE,
    )?;
    let runtime = Vic20Runtime::new(cli.region.model(), kernal, basic, char_rom)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;

    println!(
        "Controls: Esc quit, F12 reset, keyboard typed directly (Right/Down cursor, Tab RUN/STOP, Alt CBM), gamepad joystick."
    );
    emu198x_ui::run(
        Vic20System { region: cli.region },
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
            "--kernal" => cli.kernal = Some(PathBuf::from(next_arg(&mut iter, "--kernal"))),
            "--basic" => cli.basic = Some(PathBuf::from(next_arg(&mut iter, "--basic"))),
            "--char" => cli.char_rom = Some(PathBuf::from(next_arg(&mut iter, "--char"))),
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

fn default_rom(env_kind: &str, default_file: &str) -> Option<PathBuf> {
    if let Ok(path) = env::var(format!("EMU198X_VIC20_{env_kind}"))
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(format!(".emu198x/roms/commodore-vic-20/{default_file}")))
}

fn read_required(
    explicit: Option<PathBuf>,
    kind: &str,
    env_kind: &str,
    default_file: &str,
    expected: usize,
) -> Result<Vec<u8>, String> {
    let path = explicit
        .or_else(|| default_rom(env_kind, default_file))
        .ok_or_else(|| format!("no {kind} ROM: pass its flag or set EMU198X_VIC20_{env_kind}"))?;
    let bytes = std::fs::read(&path)
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

/// Map a physical host key to its VIC-20 key name (matched by
/// `runtime-commodore-vic-20`'s `key_from_name`). The VIC-20 has only two
/// physical cursor keys (right and down — up/left are shifted), so only those
/// map; the joystick is the gamepad. Symbols that are shifted on a modern host
/// are omitted, like the other Commodore keyboards. Host Tab is RUN/STOP and
/// host Alt is the Commodore key.
fn map_vic20_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Semicolon => &[";"],
        KeyCode::Comma => &[","],
        KeyCode::Period => &["."],
        KeyCode::Slash => &["/"],
        KeyCode::NumpadAdd => &["+"],
        KeyCode::NumpadMultiply => &["*"],
        KeyCode::Space => &["space"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["return"],
        KeyCode::Backspace | KeyCode::Delete => &["delete"],
        KeyCode::Home => &["home"],
        KeyCode::Tab => &["stop"],
        KeyCode::ShiftLeft => &["shift"],
        KeyCode::ShiftRight => &["rshift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["ctrl"],
        KeyCode::AltLeft | KeyCode::AltRight => &["commodore"],
        KeyCode::F1 => &["f1"],
        KeyCode::F3 => &["f3"],
        KeyCode::F5 => &["f5"],
        KeyCode::F7 => &["f7"],
        // The VIC-20 has only right/down cursor keys (up/left are shifted).
        KeyCode::ArrowRight => &["crsr-right"],
        KeyCode::ArrowDown => &["crsr-down"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_accepts_roms_region_scale_video() {
        let cli = parse_cli([
            "--kernal".to_owned(),
            "k.rom".to_owned(),
            "--region".to_owned(),
            "ntsc".to_owned(),
            "--scale".to_owned(),
            "2".to_owned(),
            "--video".to_owned(),
            "crt".to_owned(),
        ]);
        assert_eq!(cli.kernal, Some(PathBuf::from("k.rom")));
        assert_eq!(cli.region, Region::Ntsc);
        assert_eq!(cli.scale, 2);
        assert_eq!(cli.video, VideoFilter::Crt);
    }

    #[test]
    fn default_region_is_pal() {
        assert_eq!(Cli::default().region, Region::Pal);
    }

    #[test]
    fn region_frame_ticks_match() {
        assert_eq!(Region::Pal.frame_ticks(), 71 * 312);
        assert_eq!(Region::Ntsc.frame_ticks(), 65 * 261);
    }

    #[test]
    fn keyboard_maps_cursor_keys_and_specials() {
        assert_eq!(map_vic20_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_vic20_keys(KeyCode::Enter), Some(&["return"][..]));
        assert_eq!(map_vic20_keys(KeyCode::Tab), Some(&["stop"][..]));
        assert_eq!(map_vic20_keys(KeyCode::AltLeft), Some(&["commodore"][..]));
        assert_eq!(
            map_vic20_keys(KeyCode::ArrowRight),
            Some(&["crsr-right"][..])
        );
        assert_eq!(map_vic20_keys(KeyCode::ArrowDown), Some(&["crsr-down"][..]));
        // Up/left have no unshifted cursor key on the VIC-20.
        assert_eq!(map_vic20_keys(KeyCode::ArrowUp), None);
    }
}
