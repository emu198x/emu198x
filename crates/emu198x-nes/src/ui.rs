//! Interactive UI mode — the default when no automation flag is present.
//!
//! A native NES window built on the shared `emu198x-ui` harness: wgpu video
//! with `raw`/`lcd`/`crt` filters, framed APU audio, and keyboard/gamepad
//! controller input. Compiled only with the `ui` Cargo feature; `main.rs`
//! routes here when no `--script`/`--mcp`/automation flag is given.
//!
//! Beyond the harness defaults the NES adds two hooks: per-system shortcuts
//! (the `1`-`5` / `6`-`0` APU channel debug controls) via
//! [`UiSystem::handle_key`], and a teardown that flushes cartridge battery RAM
//! to its `.sav` sidecar via [`UiSystem::on_exit`].

use std::path::{Path, PathBuf};

use emu198x_shell::{MachineCore, MachineError, MediaImage, MediaKind, MediaSet, read_media_asset};
use emu198x_ui::{
    ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiError, UiSystem, VideoFilter,
};
use machine_nintendo_nes::{FB_HEIGHT, FB_WIDTH};
use runtime_nintendo_nes::{ApuChannel, Model, NesRuntime};

const DEFAULT_SCALE: u32 = 3;
const INPUT_SLICES_PER_FRAME: u32 = 4;
const NES_FRAME_TICKS: u64 = 341 * 262;
const NES_PPU_DOT_HZ: f64 = 5_369_318.0;

const NES_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
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
Usage: emu198x-nes [OPTIONS] [ROM]

Options:
    --rom PATH      iNES/NES 2.0 ROM image or zip containing one ROM candidate
    --scale N       integer window scale, default 3
    --video MODE    raw | lcd | crt [default: raw]
    --battery-save PATH  load/write cartridge battery RAM sidecar (default <rom>.sav)
    --no-battery-save    disable automatic .sav load/write
    --help, -h      show this help

Controls:
    Esc             quit
    F12             hard reset
    Arrow keys      D-pad
    Z               B
    X               A
    Right Shift     Select
    Enter           Start
    1-5             toggle Pulse 1, Pulse 2, Triangle, Noise, DMC
    6-0             cycle Pulse 1, Pulse 2, Triangle, Noise, DMC gain

Examples:
    emu198x-nes smb.nes
    emu198x-nes --rom nestest.nes --scale 2
";

/// The NES as a [`UiSystem`] for the shared harness. Holds the cartridge bytes
/// (to re-insert on a hard reset) and the battery-save path (to flush on exit).
struct NesSystem {
    cartridge_media: Vec<u8>,
    battery_save_path: Option<PathBuf>,
}

impl UiSystem for NesSystem {
    type Runtime = NesRuntime;

    fn window_title(&self) -> String {
        "Emu198x NES".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The runtime honours sub-frame targets, so finer slices cut input latency.
    fn input_slices_per_frame(&self) -> u32 {
        INPUT_SLICES_PER_FRAME
    }

    /// 8:7 — the ratio the NES is usually described by, and one the 256x240
    /// framebuffer cannot express on its own. This core is NTSC-only; the PAL
    /// 2C07 clock is wired up for when #80 lands.
    fn pixel_aspect_ratio(&self, runtime: &Self::Runtime) -> Option<f32> {
        emu198x_shell::display::pixel_aspect_for_region(
            runtime.profile().region,
            ricoh_ppu_2c02::PAL_DOT_CLOCK_HZ,
            ricoh_ppu_2c02::NTSC_DOT_CLOCK_HZ,
        )
    }

    fn framebuffer_size(&self, _runtime: &Self::Runtime) -> (u32, u32) {
        (FB_WIDTH, FB_HEIGHT)
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        NES_FRAME_TICKS
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> std::time::Duration {
        std::time::Duration::from_secs_f64(NES_FRAME_TICKS as f64 / NES_PPU_DOT_HZ)
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &NES_BUTTON_MAP
    }

    fn map_key(&self, code: KeyCode) -> Option<HostControl> {
        map_nes_key(code)
    }

    /// A hard reset drops the cartridge; re-insert it so the machine reboots.
    fn after_reset(&mut self, runtime: &mut Self::Runtime) -> Result<(), MachineError> {
        runtime.load_media(&cartridge_media_set(&self.cartridge_media))
    }

    /// The `1`-`5` / `6`-`0` digit row toggles / cycles APU channels for audio
    /// debugging; consume those keys so they aren't treated as buttons.
    fn handle_key(&mut self, runtime: &mut Self::Runtime, code: KeyCode, pressed: bool) -> bool {
        let action = match code {
            KeyCode::Digit1 => ApuShortcut::Toggle(ApuChannel::Pulse1),
            KeyCode::Digit2 => ApuShortcut::Toggle(ApuChannel::Pulse2),
            KeyCode::Digit3 => ApuShortcut::Toggle(ApuChannel::Triangle),
            KeyCode::Digit4 => ApuShortcut::Toggle(ApuChannel::Noise),
            KeyCode::Digit5 => ApuShortcut::Toggle(ApuChannel::Dmc),
            KeyCode::Digit6 => ApuShortcut::Gain(ApuChannel::Pulse1),
            KeyCode::Digit7 => ApuShortcut::Gain(ApuChannel::Pulse2),
            KeyCode::Digit8 => ApuShortcut::Gain(ApuChannel::Triangle),
            KeyCode::Digit9 => ApuShortcut::Gain(ApuChannel::Noise),
            KeyCode::Digit0 => ApuShortcut::Gain(ApuChannel::Dmc),
            _ => return false,
        };
        if pressed {
            action.apply(runtime);
        }
        true
    }

    /// Persist the cartridge's battery PRG-RAM to its `.sav` on the way out.
    fn on_exit(&mut self, runtime: &mut Self::Runtime) -> Result<(), String> {
        let Some(path) = &self.battery_save_path else {
            return Ok(());
        };
        let Some(ram) = runtime.cartridge_ram() else {
            return Ok(());
        };
        std::fs::write(path, ram)
            .map_err(|err| format!("failed to write battery save {}: {err}", path.display()))
    }
}

/// An APU debug shortcut: mute/unmute a channel, or cycle its gain.
enum ApuShortcut {
    Toggle(ApuChannel),
    Gain(ApuChannel),
}

impl ApuShortcut {
    fn apply(self, runtime: &mut NesRuntime) {
        match self {
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

/// The single-cartridge media set the NES boots from.
fn cartridge_media_set(bytes: &[u8]) -> MediaSet<'_> {
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, bytes));
    media
}

/// Parsed interactive CLI.
#[derive(Debug, PartialEq, Eq)]
pub struct Cli {
    rom: Option<PathBuf>,
    scale: u32,
    video: VideoFilter,
    battery_save: Option<PathBuf>,
    no_battery_save: bool,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            rom: None,
            scale: DEFAULT_SCALE,
            video: VideoFilter::Raw,
            battery_save: None,
            no_battery_save: false,
        }
    }
}

/// Build the runtime from the CLI and open the window. Returns a string error
/// for the `main.rs` dispatcher.
pub fn run(cli: Cli) -> Result<(), String> {
    let Some(path) = &cli.rom else {
        return Err("provide a ROM path with --rom PATH or as a positional argument".to_owned());
    };
    let loaded = read_media_asset(path, MediaKind::Cartridge)
        .map_err(|err| format!("failed to load ROM {}: {err}", path.display()))?;
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    runtime
        .load_media(&cartridge_media_set(&loaded.bytes))
        .map_err(|err| format!("failed to start ROM {}: {err}", path.display()))?;

    // Load a battery .sav sidecar (default <rom>.sav) into the cartridge's
    // PRG-RAM before the first frame runs.
    let battery_save_path = resolve_battery_save_path(&cli, path);
    if let Some(save_path) = &battery_save_path {
        load_battery_save(&mut runtime, save_path, cli.battery_save.is_some())?;
    }

    println!(
        "Controls: Esc quit, F12 reset, arrows/gamepad D-pad, Z/gamepad east B, X/gamepad south A, Shift Select, Enter Start, 1-5 toggle APU channels, 6-0 cycle channel gain."
    );
    let system = NesSystem {
        cartridge_media: loaded.bytes,
        battery_save_path,
    };
    emu198x_ui::run(system, runtime, cli.scale, cli.video).map_err(|err: UiError| err.to_string())
}

/// Resolve the battery `.sav` sidecar path: `None` when disabled, an explicit
/// `--battery-save` path, or `<rom>.sav` next to the cartridge.
fn resolve_battery_save_path(cli: &Cli, rom_path: &Path) -> Option<PathBuf> {
    if cli.no_battery_save {
        return None;
    }
    cli.battery_save
        .clone()
        .or_else(|| Some(default_battery_save_path(rom_path)))
}

fn default_battery_save_path(rom_path: &Path) -> PathBuf {
    let mut path = rom_path.to_path_buf();
    path.set_extension("sav");
    path
}

/// Load a battery `.sav` into the cartridge's PRG-RAM. A missing file is fine
/// (fresh save). An explicit `--battery-save` on a non-battery cartridge is an
/// error; the default path is silently skipped.
fn load_battery_save(runtime: &mut NesRuntime, path: &Path, explicit: bool) -> Result<(), String> {
    if !runtime.has_battery_backed_ram() {
        if explicit {
            return Err("loaded cartridge does not have battery-backed RAM".to_owned());
        }
        return Ok(());
    }
    match std::fs::read(path) {
        Ok(bytes) => runtime
            .restore_cartridge_ram(&bytes)
            .map_err(|err| err.to_string()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!(
            "failed to read battery save {}: {err}",
            path.display()
        )),
    }
}

/// Parse the interactive CLI (`--rom`, `--scale`, `--video`, battery flags,
/// positional ROM). Exits the process on `--help` or a malformed flag.
pub fn parse_cli<I>(args: I) -> Cli
where
    I: IntoIterator<Item = String>,
{
    let mut cli = Cli::default();
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--rom" => cli.rom = Some(PathBuf::from(next_arg(&mut iter, "--rom"))),
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
            "--battery-save" => {
                cli.battery_save = Some(PathBuf::from(next_arg(&mut iter, "--battery-save")));
            }
            "--no-battery-save" => cli.no_battery_save = true,
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

fn map_nes_key(code: KeyCode) -> Option<HostControl> {
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
    fn parse_cli_accepts_positional_rom_and_scale() {
        let cli = parse_cli(["--scale".to_owned(), "2".to_owned(), "game.nes".to_owned()]);

        assert_eq!(
            cli,
            Cli {
                rom: Some(PathBuf::from("game.nes")),
                scale: 2,
                video: VideoFilter::Raw,
                battery_save: None,
                no_battery_save: false,
            }
        );
    }

    #[test]
    fn default_battery_save_path_replaces_rom_extension() {
        assert_eq!(
            default_battery_save_path(Path::new("zelda.nes")),
            PathBuf::from("zelda.sav")
        );
    }

    #[test]
    fn parse_cli_accepts_battery_save_controls() {
        let cli = parse_cli([
            "--battery-save".to_owned(),
            "slot.sav".to_owned(),
            "game.nes".to_owned(),
        ]);
        assert_eq!(cli.battery_save, Some(PathBuf::from("slot.sav")));
        assert_eq!(
            resolve_battery_save_path(&cli, Path::new("game.nes")),
            Some(PathBuf::from("slot.sav"))
        );

        let cli = parse_cli(["--no-battery-save".to_owned(), "game.nes".to_owned()]);
        assert!(cli.no_battery_save);
        assert_eq!(resolve_battery_save_path(&cli, Path::new("game.nes")), None);

        // Default: <rom>.sav.
        let cli = parse_cli(["game.nes".to_owned()]);
        assert_eq!(
            resolve_battery_save_path(&cli, Path::new("game.nes")),
            Some(PathBuf::from("game.sav"))
        );
    }

    #[test]
    fn parse_cli_accepts_video_filter() {
        let cli = parse_cli([
            "--video".to_owned(),
            "crt".to_owned(),
            "game.nes".to_owned(),
        ]);
        assert_eq!(cli.video, VideoFilter::Crt);
    }

    #[test]
    fn maps_controls_to_controller_buttons() {
        assert_eq!(map_nes_key(KeyCode::KeyX), Some(HostControl::South));
        assert_eq!(map_nes_key(KeyCode::KeyZ), Some(HostControl::East));
        assert_eq!(map_nes_key(KeyCode::Enter), Some(HostControl::Start));
        assert_eq!(map_nes_key(KeyCode::ArrowLeft), Some(HostControl::Left));
        assert_eq!(map_nes_key(KeyCode::Digit1), None);
    }

    #[test]
    fn audio_gain_cycles_through_debug_levels() {
        assert_eq!(next_audio_gain(1.0), 0.5);
        assert_eq!(next_audio_gain(0.5), 0.25);
        assert_eq!(next_audio_gain(0.25), 0.0);
        assert_eq!(next_audio_gain(0.0), 1.0);
    }
}
