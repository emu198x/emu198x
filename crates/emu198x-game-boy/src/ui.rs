//! Interactive UI mode — the default when no automation flag is present.
//!
//! A native Game Boy window built on the shared `emu198x-ui` harness: wgpu
//! video with `raw`/`lcd`/`crt` filters, framed APU audio, and keyboard/gamepad
//! joypad input. Compiled only with the `ui` Cargo feature; `main.rs` routes
//! here when no `--script`/`--mcp`/automation flag is given.
//!
//! Beyond the harness defaults the Game Boy adds per-system shortcuts (the
//! `0`-`8` APU channel debug controls) via [`UiSystem::handle_key`], and a
//! teardown that flushes the cartridge save image (RAM + RTC footer) to its
//! `.sav` sidecar via [`UiSystem::on_exit`].

use std::path::{Path, PathBuf};

use common_nintendo_game_boy::timing::MCYCLE_HZ;
use common_nintendo_game_boy::{MCYCLES_PER_FRAME, SCREEN_HEIGHT, SCREEN_WIDTH};
use emu198x_shell::{MachineCore, MediaImage, MediaKind, MediaSet, read_media_asset};
use emu198x_ui::{
    ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiError, UiSystem, VideoFilter,
};
use runtime_nintendo_game_boy::{ApuChannel, AudioControls, GameBoyRuntime, Model};

const DEFAULT_SCALE: u32 = 4;
const INPUT_SLICES_PER_FRAME: u32 = 4;

const GAME_BOY_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "a")),
    (HostControl::East, ButtonTarget::new(1, "b")),
    (HostControl::West, ButtonTarget::new(1, "b")),
    (HostControl::Start, ButtonTarget::new(1, "start")),
    (HostControl::Select, ButtonTarget::new(1, "select")),
]);

const USAGE: &str = "\
Usage: emu198x-game-boy [OPTIONS] [ROM]

Options:
    --rom PATH            Game Boy ROM image or zip containing one ROM candidate
    --model MODEL         dmg0 | dmg | mgb | sgb | sgb2 [default: dmg]
    --load-snapshot PATH  restore a runtime snapshot before starting
    --battery-save PATH   load/write cartridge battery RAM sidecar
    --no-battery-save     disable automatic .sav load/write
    --scale N             integer window scale, default 4
    --video MODE          raw | lcd | crt [default: raw]
    --help, -h            show this help

Controls:
    Esc                   quit
    F12                   hard reset
    1-4                   toggle audio channels: pulse1, pulse2, wave, noise
    5-8                   cycle channel gain: 100%, 50%, 25%, muted
    0                     reset audio channel controls
    Arrow keys            D-pad
    Z                     B
    X                     A
    Right Shift           Select
    Enter                 Start

Examples:
    emu198x-game-boy tetris.gb
    emu198x-game-boy --rom game.gb --model mgb
    emu198x-game-boy --load-snapshot ready.gb.pst
";

/// The Game Boy as a [`UiSystem`] for the shared harness. A hard reset keeps
/// the cartridge in the runtime, so the only state it carries is the
/// battery-save path (flushed on exit).
struct GameBoySystem {
    battery_save_path: Option<PathBuf>,
}

impl UiSystem for GameBoySystem {
    type Runtime = GameBoyRuntime;

    fn window_title(&self) -> String {
        "Emu198x Game Boy".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The runtime honours sub-frame targets, so finer slices cut input latency.
    fn input_slices_per_frame(&self) -> u32 {
        INPUT_SLICES_PER_FRAME
    }

    fn framebuffer_size(&self, _runtime: &Self::Runtime) -> (u32, u32) {
        (SCREEN_WIDTH, SCREEN_HEIGHT)
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        u64::from(MCYCLES_PER_FRAME)
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> std::time::Duration {
        std::time::Duration::from_secs_f64(f64::from(MCYCLES_PER_FRAME) / f64::from(MCYCLE_HZ))
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &GAME_BOY_BUTTON_MAP
    }

    fn map_key(&self, code: KeyCode) -> Option<HostControl> {
        map_game_boy_key(code)
    }

    /// The `0`-`8` digit row drives the APU debug controls; consume those keys
    /// so they aren't treated as joypad buttons.
    fn handle_key(&mut self, runtime: &mut Self::Runtime, code: KeyCode, pressed: bool) -> bool {
        let action = match code {
            KeyCode::Digit0 => AudioShortcut::Reset,
            KeyCode::Digit1 => AudioShortcut::Toggle(ApuChannel::Pulse1),
            KeyCode::Digit2 => AudioShortcut::Toggle(ApuChannel::Pulse2),
            KeyCode::Digit3 => AudioShortcut::Toggle(ApuChannel::Wave),
            KeyCode::Digit4 => AudioShortcut::Toggle(ApuChannel::Noise),
            KeyCode::Digit5 => AudioShortcut::Gain(ApuChannel::Pulse1),
            KeyCode::Digit6 => AudioShortcut::Gain(ApuChannel::Pulse2),
            KeyCode::Digit7 => AudioShortcut::Gain(ApuChannel::Wave),
            KeyCode::Digit8 => AudioShortcut::Gain(ApuChannel::Noise),
            _ => return false,
        };
        if pressed {
            action.apply(runtime);
        }
        true
    }

    /// Persist the cartridge save image (RAM + RTC footer) to its `.sav` on the
    /// way out. The footer stamps wall-clock so the clock keeps running across
    /// restarts.
    fn on_exit(&mut self, runtime: &mut Self::Runtime) -> Result<(), String> {
        let Some(path) = &self.battery_save_path else {
            return Ok(());
        };
        if !runtime.has_persistent_cartridge_state() {
            return Ok(());
        }
        let Some(image) = runtime.cartridge_save_image() else {
            return Ok(());
        };
        std::fs::write(path, image)
            .map_err(|err| format!("failed to write battery save {}: {err}", path.display()))
    }
}

/// An APU debug shortcut: reset all controls, mute/unmute a channel, or cycle
/// its gain.
enum AudioShortcut {
    Reset,
    Toggle(ApuChannel),
    Gain(ApuChannel),
}

impl AudioShortcut {
    fn apply(self, runtime: &mut GameBoyRuntime) {
        match self {
            Self::Reset => {
                runtime.set_audio_controls(AudioControls::default());
                eprintln!("audio: reset channel controls");
            }
            Self::Toggle(channel) => {
                let Some(controls) = runtime.audio_controls() else {
                    return;
                };
                let enabled = !controls.channel(channel).enabled();
                runtime.set_audio_channel_enabled(channel, enabled);
                eprintln!(
                    "audio: {} {}",
                    channel.label(),
                    if enabled { "enabled" } else { "muted" }
                );
            }
            Self::Gain(channel) => {
                let Some(controls) = runtime.audio_controls() else {
                    return;
                };
                let next = next_audio_gain(controls.channel(channel).gain());
                runtime.set_audio_channel_gain(channel, next);
                eprintln!("audio: {} gain {:.0}%", channel.label(), next * 100.0);
            }
        }
    }
}

/// Parsed interactive CLI.
#[derive(Debug, PartialEq, Eq)]
pub struct Cli {
    rom: Option<PathBuf>,
    model: Model,
    load_snapshot: Option<PathBuf>,
    battery_save: Option<PathBuf>,
    no_battery_save: bool,
    scale: u32,
    video: VideoFilter,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            rom: None,
            model: Model::Dmg,
            load_snapshot: None,
            battery_save: None,
            no_battery_save: false,
            scale: DEFAULT_SCALE,
            video: VideoFilter::Raw,
        }
    }
}

/// Build the runtime from the CLI and open the window. Returns a string error
/// for the `main.rs` dispatcher.
pub fn run(cli: Cli) -> Result<(), String> {
    if cli.no_battery_save && cli.battery_save.is_some() {
        return Err("--battery-save conflicts with --no-battery-save".to_owned());
    }
    if cli.rom.is_none() && cli.load_snapshot.is_none() {
        return Err("provide a ROM path or --load-snapshot PATH".to_owned());
    }

    let mut runtime = GameBoyRuntime::blank(cli.model);
    let battery_save_path = resolve_battery_save_path(&cli);
    if let Some(path) = &cli.load_snapshot {
        let bytes = std::fs::read(path)
            .map_err(|err| format!("failed to read snapshot {}: {err}", path.display()))?;
        runtime.restore(&bytes).map_err(|err| err.to_string())?;
    }
    if let Some(path) = &cli.rom {
        let loaded = read_media_asset(path, MediaKind::Cartridge)
            .map_err(|err| format!("failed to load ROM {}: {err}", path.display()))?;
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            "cartridge",
            MediaKind::Cartridge,
            &loaded.bytes,
        ));
        runtime.load_media(&media).map_err(|err| err.to_string())?;
    }
    if let Some(path) = &battery_save_path {
        load_battery_save(&mut runtime, path, cli.battery_save.is_some())?;
    }

    println!(
        "Controls: Esc quit, F12 reset, arrows/gamepad D-pad, Z/gamepad east B, X/gamepad south A, Shift Select, Enter Start. Audio: 1-4 toggle channels, 5-8 cycle channel gain, 0 reset audio."
    );
    let system = GameBoySystem { battery_save_path };
    emu198x_ui::run(system, runtime, cli.scale, cli.video).map_err(|err: UiError| err.to_string())
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
            "--model" => cli.model = parse_model_arg(&next_arg(&mut iter, "--model")),
            "--load-snapshot" => {
                cli.load_snapshot = Some(PathBuf::from(next_arg(&mut iter, "--load-snapshot")));
            }
            "--battery-save" => {
                cli.battery_save = Some(PathBuf::from(next_arg(&mut iter, "--battery-save")));
            }
            "--no-battery-save" => cli.no_battery_save = true,
            "--scale" => {
                cli.scale = next_arg(&mut iter, "--scale")
                    .parse()
                    .unwrap_or_else(|_| die("--scale requires a positive integer"));
            }
            "--video" => {
                cli.video = parse_video_arg(&next_arg(&mut iter, "--video"));
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            _ if arg.starts_with('-') => die(&format!("unknown flag: {arg}")),
            _ if cli.rom.is_none() => cli.rom = Some(PathBuf::from(arg)),
            _ => die("only one positional ROM path is supported"),
        }
    }

    cli
}

fn resolve_battery_save_path(cli: &Cli) -> Option<PathBuf> {
    if cli.no_battery_save {
        return None;
    }
    cli.battery_save
        .clone()
        .or_else(|| cli.rom.as_deref().map(default_battery_save_path))
}

fn default_battery_save_path(rom_path: &Path) -> PathBuf {
    let mut path = rom_path.to_path_buf();
    path.set_extension("sav");
    path
}

/// Load a battery `.sav` into the cartridge. A missing file is fine (fresh
/// save). An explicit `--battery-save` on a non-persistent cartridge is an
/// error; the default path is silently skipped.
fn load_battery_save(
    runtime: &mut GameBoyRuntime,
    path: &Path,
    explicit: bool,
) -> Result<(), String> {
    if !runtime.has_persistent_cartridge_state() {
        if explicit {
            return Err("loaded cartridge does not have battery-backed RAM".to_owned());
        }
        return Ok(());
    }
    match std::fs::read(path) {
        Ok(bytes) => runtime
            .restore_cartridge_save_image(&bytes)
            .map_err(|err| err.to_string()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!(
            "failed to read battery save {}: {err}",
            path.display()
        )),
    }
}

fn parse_video_arg(video: &str) -> VideoFilter {
    video
        .parse()
        .unwrap_or_else(|_| die("--video expects raw, lcd, or crt"))
}

fn parse_model_arg(model: &str) -> Model {
    match model {
        "dmg0" => Model::Dmg0,
        "dmg" => Model::Dmg,
        "mgb" => Model::Mgb,
        "sgb" => Model::Sgb,
        "sgb2" => Model::Sgb2,
        _ => die("--model expects dmg0, dmg, mgb, sgb, or sgb2"),
    }
}

fn next_arg<I>(iter: &mut I, flag: &str) -> String
where
    I: Iterator<Item = String>,
{
    iter.next()
        .unwrap_or_else(|| die(&format!("missing value for {flag}")))
}

fn die(message: &str) -> ! {
    eprintln!("error: {message}");
    std::process::exit(1);
}

fn next_audio_gain(gain: f32) -> f32 {
    if gain > 0.75 {
        0.5
    } else if gain > 0.375 {
        0.25
    } else if gain > 0.0 {
        0.0
    } else {
        1.0
    }
}

fn map_game_boy_key(code: KeyCode) -> Option<HostControl> {
    Some(match code {
        KeyCode::KeyX => HostControl::South,
        KeyCode::KeyZ => HostControl::East,
        KeyCode::ShiftRight => HostControl::Select,
        KeyCode::Enter | KeyCode::NumpadEnter => HostControl::Start,
        KeyCode::ArrowUp => HostControl::Up,
        KeyCode::ArrowDown => HostControl::Down,
        KeyCode::ArrowLeft => HostControl::Left,
        KeyCode::ArrowRight => HostControl::Right,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_accepts_positional_rom_and_model() {
        let cli = parse_cli([
            "--model".to_owned(),
            "mgb".to_owned(),
            "--scale".to_owned(),
            "5".to_owned(),
            "game.gb".to_owned(),
        ]);

        assert_eq!(
            cli,
            Cli {
                rom: Some(PathBuf::from("game.gb")),
                model: Model::Mgb,
                load_snapshot: None,
                battery_save: None,
                no_battery_save: false,
                scale: 5,
                video: VideoFilter::Raw,
            }
        );
    }

    #[test]
    fn parse_cli_accepts_video_filter() {
        let cli = parse_cli(["--video".to_owned(), "lcd".to_owned(), "game.gb".to_owned()]);
        assert_eq!(cli.video, VideoFilter::Lcd);
    }

    #[test]
    fn default_battery_save_path_replaces_rom_extension() {
        assert_eq!(
            default_battery_save_path(Path::new("game.gb")),
            PathBuf::from("game.sav")
        );
    }

    #[test]
    fn parse_cli_accepts_battery_save_controls() {
        let cli = parse_cli([
            "--battery-save".to_owned(),
            "slot.sav".to_owned(),
            "game.gb".to_owned(),
        ]);

        assert_eq!(cli.battery_save, Some(PathBuf::from("slot.sav")));
        assert_eq!(
            resolve_battery_save_path(&cli),
            Some(PathBuf::from("slot.sav"))
        );

        let cli = parse_cli(["--no-battery-save".to_owned(), "game.gb".to_owned()]);
        assert!(cli.no_battery_save);
        assert_eq!(resolve_battery_save_path(&cli), None);
    }

    #[test]
    fn maps_controls_to_joypad_buttons() {
        assert_eq!(map_game_boy_key(KeyCode::KeyX), Some(HostControl::South));
        assert_eq!(map_game_boy_key(KeyCode::KeyZ), Some(HostControl::East));
        assert_eq!(map_game_boy_key(KeyCode::Enter), Some(HostControl::Start));
        assert_eq!(
            map_game_boy_key(KeyCode::ArrowLeft),
            Some(HostControl::Left)
        );
        assert_eq!(map_game_boy_key(KeyCode::Digit1), None);
    }

    #[test]
    fn audio_gain_shortcut_cycles_down_then_restores() {
        assert_eq!(next_audio_gain(1.0), 0.5);
        assert_eq!(next_audio_gain(0.5), 0.25);
        assert_eq!(next_audio_gain(0.25), 0.0);
        assert_eq!(next_audio_gain(0.0), 1.0);
    }
}
