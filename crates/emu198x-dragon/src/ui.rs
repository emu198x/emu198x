//! Interactive UI mode — the Dragon 32/64 on the shared `emu198x-ui` harness.
//!
//! This replaces the former bespoke winit + wgpu runner with a thin
//! [`UiSystem`] descriptor over [`DragonRuntime`]. The harness owns the window,
//! video filters, framed audio, gamepad/keyboard plumbing, the native menu,
//! save-states, tape transport, and live variant switching; this file supplies
//! only the Dragon-specific knobs:
//!
//! - **Keyboard**: [`map_dragon_keys`] maps each physical host key to one or
//!   more Dragon-matrix key names (physical-layout-only — like the Spectrum, no
//!   logical/character path; shifted symbols are entered with the host's own
//!   Shift + the base key).
//! - **Gamepad**: the left analogue stick / d-pad drives Dragon joystick 1, and
//!   South/East fire — lifted verbatim from the bespoke runner.
//! - **Variants**: Dragon 32 and Dragon 64 as the Machine-menu radio;
//!   [`switch_variant`](UiSystem::switch_variant) rebuilds the runtime from the
//!   staged ROM bundle via `from_firmware`.
//! - **Tape**: F9/F10 transport + F11 turbo come free from the harness, gated on
//!   the `tape-1` slot; [`tape_playing`](UiSystem::tape_playing) drives turbo.
//!
//! Compiled only with the `ui` Cargo feature; `main.rs` routes here when no
//! headless-only flag is given.

use std::borrow::Cow;
use std::path::PathBuf;
use std::process;
use std::time::Duration;

use emu198x_shell::MachineCore;
use emu198x_shell::{
    FirmwareImage, FirmwareSet, HeadlessSession, InputEvent, MachineError, MediaImage, MediaKind,
    MediaSet, SessionError, read_firmware_asset, read_media_asset,
};
use emu198x_ui::{
    AxisInputMap, AxisTarget, ButtonInputMap, ButtonTarget, HostAxis, HostControl, KeyCode,
    UiSystem, VariantInfo, VideoFilter,
};
use motorola_vdg_6847::{VDG_PAL_OVERSCAN_FRAMEBUFFER_HEIGHT, VDG_PAL_OVERSCAN_FRAMEBUFFER_WIDTH};
use runtime_dragon::{DragonRuntime, DragonSessionQueryProvider, Model};
use thiserror::Error;

const DEFAULT_SCALE: u32 = 2;
const DRAGON_CPU_HZ: u64 = 894_886;
const DRAGON_FRAME_HZ: u64 = 50;
const DRAGON_FRAME_CYCLES: u64 = DRAGON_CPU_HZ / DRAGON_FRAME_HZ;
const INPUT_SLICES_PER_FRAME: u32 = 4;
const AUTOLOAD_BOOT_FRAMES: u32 = 100;
const AUTOLOAD_KEY_EDGE_FRAMES: u32 = 4;
const AUTOLOAD_START_SETTLE_FRAMES: u32 = 60;

const DRAGON32_ID: &str = "dragon32";
const DRAGON64_ID: &str = "dragon64";

// ---- Gamepad maps (lifted from the bespoke runner, unchanged) --------------

const DRAGON_GAMEPAD_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
    (HostControl::Start, ButtonTarget::new(1, "enter")),
    (HostControl::Select, ButtonTarget::new(1, "clear")),
]);
const DRAGON_GAMEPAD_AXIS_MAP: AxisInputMap = AxisInputMap::new(&[
    (HostAxis::LeftStickX, AxisTarget::new(1, "x")),
    (HostAxis::LeftStickY, AxisTarget::new(1, "y")),
]);

/// Maps one physical host key to one or more Dragon-matrix key names.
///
/// Physical-layout-only (no logical/character path): the host's own Shift plus
/// a base key produces the Dragon's shifted symbols, exactly as the Dragon
/// keyboard membrane does. Lifted from the bespoke `map_dragon_physical_fallback`,
/// reshaped to return a static slice. The numpad and platform-key aliases are
/// preserved.
fn map_dragon_keys(code: KeyCode) -> Option<&'static [&'static str]> {
    Some(match code {
        KeyCode::Digit0 | KeyCode::Numpad0 => &["0"],
        KeyCode::Digit1 | KeyCode::Numpad1 => &["1"],
        KeyCode::Digit2 | KeyCode::Numpad2 => &["2"],
        KeyCode::Digit3 | KeyCode::Numpad3 => &["3"],
        KeyCode::Digit4 | KeyCode::Numpad4 => &["4"],
        KeyCode::Digit5 | KeyCode::Numpad5 => &["5"],
        KeyCode::Digit6 | KeyCode::Numpad6 => &["6"],
        KeyCode::Digit7 | KeyCode::Numpad7 => &["7"],
        KeyCode::Digit8 | KeyCode::Numpad8 => &["8"],
        KeyCode::Digit9 | KeyCode::Numpad9 => &["9"],
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
        KeyCode::ArrowUp => &["up"],
        KeyCode::ArrowDown => &["down"],
        KeyCode::ArrowLeft => &["left"],
        KeyCode::ArrowRight => &["right"],
        KeyCode::Space => &["space"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["enter"],
        KeyCode::Backspace | KeyCode::Delete | KeyCode::NumpadBackspace | KeyCode::NumpadClear => {
            &["clear"]
        }
        KeyCode::F1 => &["break"],
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &["shift"],
        KeyCode::Comma | KeyCode::NumpadComma => &[","],
        KeyCode::Minus | KeyCode::NumpadSubtract => &["-"],
        KeyCode::Period | KeyCode::NumpadDecimal => &["."],
        KeyCode::Slash | KeyCode::NumpadDivide => &["/"],
        KeyCode::Semicolon => &[";"],
        KeyCode::Quote => &["@"],
        _ => return None,
    })
}

// ---- The UiSystem ----------------------------------------------------------

/// The Dragon as a [`UiSystem`]. Tracks the active model so the title and the
/// Machine-menu radio follow live switches.
struct DragonSystem {
    current: Model,
}

impl UiSystem for DragonSystem {
    type Runtime = DragonRuntime;

    fn window_title(&self) -> String {
        format!("Emu198x {}", self.current.display_name())
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    /// Same VDG as the Atom, so the same two pixels per clock period.
    fn pixel_aspect_ratio(&self, runtime: &Self::Runtime) -> Option<f32> {
        emu198x_shell::display::pixel_aspect_for_region(
            runtime.profile().region,
            motorola_vdg_6847::PAL_PIXEL_CLOCK_HZ,
            motorola_vdg_6847::NTSC_PIXEL_CLOCK_HZ,
        )
    }

    fn framebuffer_size(&self, _runtime: &Self::Runtime) -> (u32, u32) {
        (
            VDG_PAL_OVERSCAN_FRAMEBUFFER_WIDTH as u32,
            VDG_PAL_OVERSCAN_FRAMEBUFFER_HEIGHT as u32,
        )
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        DRAGON_FRAME_CYCLES
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / DRAGON_FRAME_HZ as f64)
    }

    fn input_slices_per_frame(&self) -> u32 {
        INPUT_SLICES_PER_FRAME
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &DRAGON_GAMEPAD_MAP
    }

    fn axis_map(&self) -> &'static AxisInputMap {
        &DRAGON_GAMEPAD_AXIS_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_dragon_keys(code)
    }

    fn tape_playing(&self, runtime: &Self::Runtime) -> bool {
        runtime.machine().cassette_motor_on()
    }

    fn variants(&self) -> Vec<VariantInfo> {
        vec![
            VariantInfo::new(DRAGON32_ID, Model::Dragon32Pal.display_name()),
            VariantInfo::new(DRAGON64_ID, Model::Dragon64Pal.display_name()),
        ]
    }

    fn current_variant(&self) -> Option<Cow<'static, str>> {
        Some(Cow::Borrowed(variant_id(self.current)))
    }

    fn switch_variant(
        &mut self,
        runtime: &mut Self::Runtime,
        variant: &str,
    ) -> Result<(), MachineError> {
        let model = model_for_variant(variant).ok_or(MachineError::UnsupportedOperation {
            operation: "unknown Dragon variant",
        })?;
        let firmware_images =
            resolve_variant_firmware(model).map_err(|reason| MachineError::Host {
                reason: format!("loading {} ROMs: {reason}", model.display_name()),
            })?;
        let mut firmware = FirmwareSet::new();
        for (id, bytes) in &firmware_images {
            firmware.push(FirmwareImage::new(*id, bytes));
        }
        *runtime = DragonRuntime::from_firmware(model, &firmware)?;
        self.current = model;
        Ok(())
    }
}

/// The stable variant id for a model (round-trips through [`model_for_variant`]).
fn variant_id(model: Model) -> &'static str {
    match model {
        Model::Dragon32Pal => DRAGON32_ID,
        Model::Dragon64Pal => DRAGON64_ID,
    }
}

/// Resolve a variant id from the Machine menu back to a [`Model`].
fn model_for_variant(variant: &str) -> Option<Model> {
    match variant {
        DRAGON32_ID => Some(Model::Dragon32Pal),
        DRAGON64_ID => Some(Model::Dragon64Pal),
        _ => None,
    }
}

// ---- Construction + CLI ----------------------------------------------------

/// Parsed interactive CLI (preserved from the bespoke runner).
#[derive(Debug, PartialEq, Eq)]
pub struct Cli {
    pub model: Model,
    pub rom: Option<PathBuf>,
    pub mode_rom: Option<PathBuf>,
    pub tape: Option<PathBuf>,
    pub cart: Option<PathBuf>,
    pub bin: Option<PathBuf>,
    pub snapshot: Option<PathBuf>,
    pub autoload: bool,
    pub scale: u32,
    pub video: VideoFilter,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            model: Model::Dragon32Pal,
            rom: None,
            mode_rom: None,
            tape: None,
            cart: None,
            bin: None,
            snapshot: None,
            autoload: false,
            scale: DEFAULT_SCALE,
            video: VideoFilter::Crt,
        }
    }
}

/// Setup-phase errors building the runtime from the CLI. Surfaced to `main.rs`
/// as a `String` via [`run`].
#[derive(Debug, Error)]
enum AppError {
    #[error(transparent)]
    Machine(#[from] MachineError),

    #[error(transparent)]
    Session(#[from] SessionError),

    #[error("{reason}")]
    Setup { reason: String },
}

const USAGE: &str = "\
Usage: emu198x-dragon [OPTIONS] --rom PATH

Options:
    --model MODEL    dragon32 | dragon64 [default: dragon32]
    --rom PATH       Dragon 32 BASIC ROM, or Dragon 64 compatible-mode ROM
    --rom64 PATH     Dragon 64 64-mode BASIC ROM, required with --model dragon64
    --tape PATH      Dragon CAS tape image, or zip containing one .cas member
    --cart PATH      Dragon cartridge ROM/DGN image, or zip containing one cartridge member
    --bin PATH       DragonDOS .BIN program, or zip containing one .bin member
    --snapshot PATH  PC-Dragon PAK snapshot, or zip containing one .pak member
    --autoload       type CLOAD/CLOADM, wait for load, then type RUN/EXEC
    --scale N        integer window scale, default 2
    --video MODE     raw | lcd | crt [default: crt]
    --help, -h       show this help

Controls:
    Esc              quit
    F9 / F10 / F11   start / stop tape, toggle fast-load (turbo)
    F12              hard reset
    Cmd/Ctrl+S / +L  quick save / load state
    A-Z, 0-9         Dragon keyboard keys
    @ : ; , - . /    Dragon punctuation keys (shifted symbols via host Shift)
    Arrows           Dragon arrow keys
    Enter            Dragon Enter
    Space            Dragon Space
    Shift            Dragon Shift
    Backspace        Dragon Clear
    F1               Dragon Break
    Gamepad          left stick / d-pad drives Dragon joystick 1; South/East fire
    Machine menu     switch between Dragon 32 and Dragon 64 live
";

/// Build the runtime from the CLI and open the window.
pub fn run(cli: Cli) -> Result<(), String> {
    println!(
        "Controls: Esc quit, F9/F10 tape start/stop, F11 fast-load, F12 reset, \
         Cmd/Ctrl+S/L save/load state; Dragon keys: A-Z, 0-9, punctuation, arrows, \
         Enter, Clear, Break, Shift, Space; Machine menu switches variant."
    );
    let model = cli.model;
    let runtime = build_runtime(&cli).map_err(|err| err.to_string())?;
    emu198x_ui::run(
        DragonSystem { current: model },
        runtime,
        cli.scale,
        cli.video,
    )
    .map_err(|err| err.to_string())
}

/// Build a [`DragonRuntime`] from the CLI's launch model and media workflow. A
/// temporary [`HeadlessSession`] is used for the media load/autoload (reusing
/// the shared helpers), then unwrapped into the bare runtime the harness drives.
fn build_runtime(cli: &Cli) -> Result<DragonRuntime, AppError> {
    if cli.autoload && cli.tape.is_none() {
        return Err(AppError::Setup {
            reason: "--autoload requires --tape PATH".to_owned(),
        });
    }

    let rom = cli.rom.as_ref().ok_or_else(|| AppError::Setup {
        reason: "provide --rom PATH".to_owned(),
    })?;
    let loaded = read_firmware_asset(rom).map_err(|err| AppError::Setup {
        reason: format!("failed to load Dragon ROM {}: {err}", rom.display()),
    })?;
    let loaded_mode_rom = if cli.model == Model::Dragon64Pal {
        let mode_rom = cli.mode_rom.as_ref().ok_or_else(|| AppError::Setup {
            reason: "--model dragon64 requires --rom64 PATH".to_owned(),
        })?;
        Some(
            read_firmware_asset(mode_rom).map_err(|err| AppError::Setup {
                reason: format!(
                    "failed to load Dragon 64 mode ROM {}: {err}",
                    mode_rom.display()
                ),
            })?,
        )
    } else {
        None
    };
    let mut firmware = FirmwareSet::new();
    match cli.model {
        Model::Dragon32Pal => {
            if cli.mode_rom.is_some() {
                return Err(AppError::Setup {
                    reason: "--rom64 requires --model dragon64".to_owned(),
                });
            }
            firmware.push(FirmwareImage::new("dragon32-basic-rom", &loaded.bytes));
        }
        Model::Dragon64Pal => {
            let loaded_mode_rom = loaded_mode_rom
                .as_ref()
                .expect("Dragon 64 mode ROM should be loaded before firmware construction");
            firmware.push(FirmwareImage::new("dragon64-compatible-rom", &loaded.bytes));
            firmware.push(FirmwareImage::new(
                "dragon64-basic-rom",
                &loaded_mode_rom.bytes,
            ));
        }
    }
    let runtime = DragonRuntime::from_firmware(cli.model, &firmware)?;
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        DRAGON_FRAME_CYCLES,
        DragonSessionQueryProvider,
    );

    if let Some(tape) = &cli.tape {
        let loaded = read_media_asset(tape, MediaKind::Tape).map_err(|err| AppError::Setup {
            reason: format!("failed to load Dragon tape {}: {err}", tape.display()),
        })?;
        let mut media = MediaSet::new();
        media.push(MediaImage::new("tape-1", MediaKind::Tape, &loaded.bytes));
        session.load_media(&media)?;
        if let Some(summary) = session.machine().tape_summary() {
            let name = summary.header_name.as_deref().unwrap_or("<no header>");
            println!(
                "Loaded tape: {name}, {} CAS blocks, checksums {}",
                summary.blocks,
                if summary.checksums_valid {
                    "valid"
                } else {
                    "invalid"
                }
            );
        }
    }

    if let Some(cart) = &cli.cart {
        let loaded =
            read_media_asset(cart, MediaKind::Cartridge).map_err(|err| AppError::Setup {
                reason: format!("failed to load Dragon cartridge {}: {err}", cart.display()),
            })?;
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            "cartridge-1",
            MediaKind::Cartridge,
            &loaded.bytes,
        ));
        session.load_media(&media)?;
        println!("Loaded cartridge: {} bytes", loaded.bytes.len());
    }

    if let Some(bin) = &cli.bin {
        let loaded = read_media_asset(bin, MediaKind::Program).map_err(|err| AppError::Setup {
            reason: format!(
                "failed to load Dragon binary program {}: {err}",
                bin.display()
            ),
        })?;
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            "program-1",
            MediaKind::Program,
            &loaded.bytes,
        ));
        session.load_media(&media)?;
        if let Some(summary) = session.machine().program_summary() {
            println!(
                "Loaded DragonDOS BIN: {} bytes at ${:04X}, exec ${:04X}",
                summary.len, summary.load_address, summary.exec_address
            );
        }
    }

    if let Some(snapshot) = &cli.snapshot {
        let loaded =
            read_media_asset(snapshot, MediaKind::Snapshot).map_err(|err| AppError::Setup {
                reason: format!(
                    "failed to load Dragon snapshot {}: {err}",
                    snapshot.display()
                ),
            })?;
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            "snapshot-1",
            MediaKind::Snapshot,
            &loaded.bytes,
        ));
        session.load_media(&media)?;
        println!("Loaded snapshot: {} bytes", loaded.bytes.len());
    }

    if cli.autoload {
        autoload_tape(&mut session)?;
    }

    Ok(session.into_machine())
}

// ---- Staged-ROM resolvers (for live variant switching) ---------------------

/// Resolve the firmware bundle for a model from the staged ROM paths
/// (`$EMU198X_*` overrides or `~/.emu198x/roms/dragon/`). The Dragon 32 needs
/// one image; the Dragon 64 needs the compatible-mode ROM plus the 64-mode ROM.
fn resolve_variant_firmware(model: Model) -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    match model {
        Model::Dragon32Pal => {
            let path = staged_dragon32_rom()
                .ok_or_else(|| "Dragon 32 ROM is not staged (~/.emu198x/roms/dragon/dragon32.rom or $EMU198X_DRAGON32_ROM)".to_owned())?;
            let bytes = read_firmware_asset(&path)
                .map_err(|err| format!("{}: {err}", path.display()))?
                .bytes
                .to_vec();
            Ok(vec![("dragon32-basic-rom", bytes)])
        }
        Model::Dragon64Pal => {
            let compat_path = staged_dragon64_compat_rom()
                .ok_or_else(|| "Dragon 64 compatible-mode ROM is not staged (~/.emu198x/roms/dragon/dragon64-compat.rom or $EMU198X_DRAGON64_COMPAT_ROM)".to_owned())?;
            let mode_path = staged_dragon64_mode_rom()
                .ok_or_else(|| "Dragon 64 64-mode ROM is not staged (~/.emu198x/roms/dragon/dragon64.rom or $EMU198X_DRAGON64_ROM)".to_owned())?;
            let compat = read_firmware_asset(&compat_path)
                .map_err(|err| format!("{}: {err}", compat_path.display()))?
                .bytes
                .to_vec();
            let mode = read_firmware_asset(&mode_path)
                .map_err(|err| format!("{}: {err}", mode_path.display()))?
                .bytes
                .to_vec();
            Ok(vec![
                ("dragon64-compatible-rom", compat),
                ("dragon64-basic-rom", mode),
            ])
        }
    }
}

fn staged_dragon32_rom() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("EMU198X_DRAGON32_ROM") {
        return existing_file(path);
    }
    existing_file(home_path(".emu198x/roms/dragon/dragon32.rom")?)
}

fn staged_dragon64_compat_rom() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("EMU198X_DRAGON64_COMPAT_ROM") {
        return existing_file(path);
    }
    existing_file(home_path(".emu198x/roms/dragon/dragon64-compat.rom")?)
}

fn staged_dragon64_mode_rom() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("EMU198X_DRAGON64_ROM") {
        return existing_file(path);
    }
    existing_file(home_path(".emu198x/roms/dragon/dragon64.rom")?)
}

fn home_path(relative: &str) -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var("HOME").ok()?).join(relative))
}

fn existing_file(path: impl Into<PathBuf>) -> Option<PathBuf> {
    let path = path.into();
    path.is_file().then_some(path)
}

// ---- Autoload machinery (lifted from the bespoke runner) -------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragonAutoloadKind {
    Basic,
    MachineCode,
}

impl DragonAutoloadKind {
    fn load_command(self) -> &'static str {
        match self {
            Self::Basic => "CLOAD",
            Self::MachineCode => "CLOADM",
        }
    }

    fn start_command(self) -> &'static str {
        match self {
            Self::Basic => "RUN",
            Self::MachineCode => "EXEC",
        }
    }
}

fn autoload_kind(runtime: &DragonRuntime) -> Result<DragonAutoloadKind, AppError> {
    let summary = runtime.tape_summary().ok_or_else(|| AppError::Setup {
        reason: "--autoload requires a mounted CAS tape".to_owned(),
    })?;
    match summary.header_file_type {
        Some("basic") => Ok(DragonAutoloadKind::Basic),
        Some("machine-code") => Ok(DragonAutoloadKind::MachineCode),
        Some(file_type) => Err(AppError::Setup {
            reason: format!("--autoload does not support Dragon CAS file type {file_type}"),
        }),
        None => Err(AppError::Setup {
            reason: "--autoload requires a Dragon CAS namefile header".to_owned(),
        }),
    }
}

fn autoload_tape(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
) -> Result<(), AppError> {
    let kind = autoload_kind(session.machine())?;
    let boot = session.wait_for_boot(AUTOLOAD_BOOT_FRAMES)?;
    session.run_frames(30)?;

    println!("Autoload: typing {}", kind.load_command());
    type_basic_command(session, kind.load_command())?;
    wait_for_tape_position_above(session, 0, 180)?;
    let load_wait_frames =
        load_wait_frame_budget(session.machine().machine().cassette_len_bits() as u64);
    wait_for_tape_load_stop(session, load_wait_frames)?;

    println!("Autoload: typing {}", kind.start_command());
    type_basic_command(session, kind.start_command())?;
    session.run_frames(AUTOLOAD_START_SETTLE_FRAMES)?;
    println!("Autoload complete after BASIC boot: {}", boot.reason);
    Ok(())
}

fn load_wait_frame_budget(tape_length_bits: u64) -> u32 {
    let scaled = tape_length_bits / 16;
    u32::try_from(scaled.clamp(4_500, 20_000)).map_or(20_000, |frames| frames)
}

fn wait_for_tape_position_above(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    position_bits: usize,
    max_frames: u32,
) -> Result<(), AppError> {
    for _ in 0..=max_frames {
        if session.machine().machine().cassette_position_bits() > position_bits {
            return Ok(());
        }
        session.run_frames(1)?;
    }
    Err(AppError::Setup {
        reason: format!("Dragon autoload did not start consuming tape within {max_frames} frames"),
    })
}

fn wait_for_tape_load_stop(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    max_frames: u32,
) -> Result<(), AppError> {
    for _ in 0..=max_frames {
        let machine = session.machine().machine();
        if !machine.cassette_motor_on() || machine.cassette_finished() {
            return Ok(());
        }
        session.run_frames(1)?;
    }
    Err(AppError::Setup {
        reason: format!("Dragon autoload did not finish loading within {max_frames} frames"),
    })
}

fn type_basic_command(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    command: &str,
) -> Result<(), AppError> {
    for ch in command.chars() {
        tap_key(session, &ch.to_ascii_lowercase().to_string())?;
    }
    tap_key(session, "enter")
}

fn tap_key(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    name: &str,
) -> Result<(), AppError> {
    session.queue_input(InputEvent::Key {
        name: name.to_owned().into(),
        pressed: true,
    });
    session.run_frames(AUTOLOAD_KEY_EDGE_FRAMES)?;
    session.queue_input(InputEvent::Key {
        name: name.to_owned().into(),
        pressed: false,
    });
    session.run_frames(AUTOLOAD_KEY_EDGE_FRAMES)?;
    Ok(())
}

// ---- CLI parsing -----------------------------------------------------------

/// Parses the interactive CLI. Exits the process on `--help` or a malformed flag.
pub fn parse_cli<I>(args: I) -> Cli
where
    I: IntoIterator<Item = String>,
{
    let mut cli = Cli::default();
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--model" => cli.model = parse_model(&next_arg(&mut iter, "--model")),
            "--rom" => cli.rom = Some(PathBuf::from(next_arg(&mut iter, "--rom"))),
            "--rom64" => cli.mode_rom = Some(PathBuf::from(next_arg(&mut iter, "--rom64"))),
            "--tape" => cli.tape = Some(PathBuf::from(next_arg(&mut iter, "--tape"))),
            "--cart" => cli.cart = Some(PathBuf::from(next_arg(&mut iter, "--cart"))),
            "--bin" => cli.bin = Some(PathBuf::from(next_arg(&mut iter, "--bin"))),
            "--snapshot" => cli.snapshot = Some(PathBuf::from(next_arg(&mut iter, "--snapshot"))),
            "--autoload" => cli.autoload = true,
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
                process::exit(0);
            }
            _ if arg.starts_with('-') => die(&format!("unknown flag: {arg}")),
            _ => {
                if cli.rom.is_none() {
                    cli.rom = Some(PathBuf::from(arg));
                } else {
                    die("only one positional ROM path is supported");
                }
            }
        }
    }

    cli
}

fn parse_model(value: &str) -> Model {
    match value {
        "dragon32" | "dragon-32" | "dragon-32-pal" => Model::Dragon32Pal,
        "dragon64" | "dragon-64" | "dragon-64-pal" => Model::Dragon64Pal,
        _ => die("--model expects dragon32 or dragon64"),
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
    process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_accepts_positional_rom_and_video() {
        let cli = parse_cli([
            "--scale".to_owned(),
            "3".to_owned(),
            "--video".to_owned(),
            "raw".to_owned(),
            "dragon32.rom".to_owned(),
        ]);

        assert_eq!(
            cli,
            Cli {
                model: Model::Dragon32Pal,
                rom: Some(PathBuf::from("dragon32.rom")),
                mode_rom: None,
                tape: None,
                cart: None,
                bin: None,
                snapshot: None,
                autoload: false,
                scale: 3,
                video: VideoFilter::Raw,
            }
        );
    }

    #[test]
    fn parse_cli_accepts_dragon64_model_and_mode_rom() {
        let cli = parse_cli([
            "--model".to_owned(),
            "dragon64".to_owned(),
            "--rom".to_owned(),
            "dragon64-compat.rom".to_owned(),
            "--rom64".to_owned(),
            "dragon64.rom".to_owned(),
        ]);

        assert_eq!(cli.model, Model::Dragon64Pal);
        assert_eq!(cli.rom, Some(PathBuf::from("dragon64-compat.rom")));
        assert_eq!(cli.mode_rom, Some(PathBuf::from("dragon64.rom")));
    }

    #[test]
    fn parse_cli_accepts_tape_path() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--tape".to_owned(),
            "program.cas".to_owned(),
        ]);

        assert_eq!(cli.model, Model::Dragon32Pal);
        assert_eq!(cli.rom, Some(PathBuf::from("dragon32.rom")));
        assert_eq!(cli.tape, Some(PathBuf::from("program.cas")));
        assert!(!cli.autoload);
    }

    #[test]
    fn parse_cli_accepts_cart_path() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--cart".to_owned(),
            "game.dgn".to_owned(),
        ]);

        assert_eq!(cli.rom, Some(PathBuf::from("dragon32.rom")));
        assert_eq!(cli.mode_rom, None);
        assert_eq!(cli.cart, Some(PathBuf::from("game.dgn")));
    }

    #[test]
    fn parse_cli_accepts_snapshot_path() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--snapshot".to_owned(),
            "game.pak".to_owned(),
        ]);

        assert_eq!(cli.rom, Some(PathBuf::from("dragon32.rom")));
        assert_eq!(cli.snapshot, Some(PathBuf::from("game.pak")));
    }

    #[test]
    fn parse_cli_accepts_bin_path() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--bin".to_owned(),
            "game.bin".to_owned(),
        ]);

        assert_eq!(cli.rom, Some(PathBuf::from("dragon32.rom")));
        assert_eq!(cli.bin, Some(PathBuf::from("game.bin")));
    }

    #[test]
    fn parse_cli_accepts_autoload_flag() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--tape".to_owned(),
            "program.cas".to_owned(),
            "--autoload".to_owned(),
        ]);

        assert_eq!(cli.rom, Some(PathBuf::from("dragon32.rom")));
        assert_eq!(cli.tape, Some(PathBuf::from("program.cas")));
        assert!(cli.autoload);
    }

    #[test]
    fn autoload_kind_commands_match_dragon_basic() {
        assert_eq!(DragonAutoloadKind::Basic.load_command(), "CLOAD");
        assert_eq!(DragonAutoloadKind::Basic.start_command(), "RUN");
        assert_eq!(DragonAutoloadKind::MachineCode.load_command(), "CLOADM");
        assert_eq!(DragonAutoloadKind::MachineCode.start_command(), "EXEC");
    }

    #[test]
    fn gamepad_map_targets_dragon_joystick_fire() {
        assert_eq!(
            DRAGON_GAMEPAD_MAP.event(HostControl::South, true),
            Some(InputEvent::Button {
                port: 1,
                name: "fire".into(),
                pressed: true,
            })
        );
        assert_eq!(
            DRAGON_GAMEPAD_MAP.event(HostControl::Right, true),
            Some(InputEvent::Button {
                port: 1,
                name: "right".into(),
                pressed: true,
            })
        );
    }

    #[test]
    fn gamepad_axis_map_targets_dragon_analogue_axes() {
        assert_eq!(
            DRAGON_GAMEPAD_AXIS_MAP.event(HostAxis::LeftStickX, -1.0),
            Some(InputEvent::Axis {
                port: 1,
                name: "x".into(),
                value: i16::MIN,
            })
        );
        assert_eq!(
            DRAGON_GAMEPAD_AXIS_MAP.event(HostAxis::LeftStickY, 1.0),
            Some(InputEvent::Axis {
                port: 1,
                name: "y".into(),
                value: i16::MAX,
            })
        );
    }

    #[test]
    fn map_keys_covers_letters_digits_and_named_keys() {
        assert_eq!(map_dragon_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_dragon_keys(KeyCode::Digit1), Some(&["1"][..]));
        assert_eq!(map_dragon_keys(KeyCode::Quote), Some(&["@"][..]));
        assert_eq!(map_dragon_keys(KeyCode::ArrowLeft), Some(&["left"][..]));
        assert_eq!(map_dragon_keys(KeyCode::Enter), Some(&["enter"][..]));
        assert_eq!(map_dragon_keys(KeyCode::Backspace), Some(&["clear"][..]));
        assert_eq!(map_dragon_keys(KeyCode::F1), Some(&["break"][..]));
        assert_eq!(map_dragon_keys(KeyCode::ShiftLeft), Some(&["shift"][..]));
    }

    #[test]
    fn map_keys_covers_numpad_and_platform_aliases() {
        assert_eq!(map_dragon_keys(KeyCode::Numpad1), Some(&["1"][..]));
        assert_eq!(map_dragon_keys(KeyCode::NumpadEnter), Some(&["enter"][..]));
        assert_eq!(map_dragon_keys(KeyCode::Delete), Some(&["clear"][..]));
        assert_eq!(map_dragon_keys(KeyCode::Semicolon), Some(&[";"][..]));
    }

    #[test]
    fn map_keys_ignores_unmapped_keys() {
        assert_eq!(map_dragon_keys(KeyCode::F5), None);
        assert_eq!(map_dragon_keys(KeyCode::Tab), None);
    }

    #[test]
    fn variant_ids_round_trip_through_models() {
        assert_eq!(model_for_variant(DRAGON32_ID), Some(Model::Dragon32Pal));
        assert_eq!(model_for_variant(DRAGON64_ID), Some(Model::Dragon64Pal));
        assert_eq!(model_for_variant("nonsense"), None);
        assert_eq!(variant_id(Model::Dragon32Pal), DRAGON32_ID);
        assert_eq!(variant_id(Model::Dragon64Pal), DRAGON64_ID);
    }
}
