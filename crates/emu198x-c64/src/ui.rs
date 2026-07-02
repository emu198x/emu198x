//! Interactive UI mode — the Commodore 64 on the shared `emu198x-ui` harness.
//!
//! This replaces the former bespoke winit + wgpu runner with a thin
//! [`UiSystem`] descriptor over [`C64Runtime`]. The harness owns the window,
//! video filters, framed audio, gamepad/keyboard plumbing, the native menu,
//! save-states, tape transport, and live variant switching; this file supplies
//! only the C64-specific knobs:
//!
//! - **Keyboard**: [`map_c64_keys`] maps each physical host key to one or more
//!   C64-matrix key names (the cursor combos, the shifted function keys, the
//!   platform-key Commodore alias). In keyboard-joystick mode (toggled with
//!   Page Up) the arrow keys + Space fall through to the gameport-2 joystick.
//! - **Gamepad / keyboard-joystick**: [`C64_JOYSTICK_MAP`] drives gameport 2
//!   (port 0); every face button is the single C64 fire.
//! - **Variants**: PAL and NTSC breadbins as the Machine-menu radio. Both share
//!   the same KERNAL/BASIC/CHARGEN/1541 firmware, so
//!   [`switch_variant`](UiSystem::switch_variant) rebuilds the runtime from the
//!   stashed firmware bytes via `from_firmware`.
//! - **Tape**: F9/F10 transport + F11 turbo come free from the harness, gated on
//!   the `tape-1` slot; [`tape_playing`](UiSystem::tape_playing) drives turbo.
//!
//! Compiled only with the `ui` Cargo feature; `main.rs` routes here when no
//! headless-only flag is given.

use std::borrow::Cow;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::Duration;

use common_commodore_c64::timing::{C64Timing, TIMING_NTSC_BREADBIN, TIMING_PAL_BREADBIN};
use emu198x_shell::{
    BootArtifacts, ControlCommand, FirmwareImage, FirmwareSet, HeadlessSession, MachineError,
    MediaImage, MediaKind, MediaSet, MediaTransportAction, MediaTransportCommand, boot_machine,
    read_firmware_asset, read_media_asset, read_program_asset,
};
use emu198x_ui::{
    ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem, VariantInfo, VideoFilter,
};
use runtime_commodore_c64::{
    C64Runtime, C64SessionQueryProvider, DEFAULT_DISK_AUTOLOAD_SLOT,
    DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES, DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
    DEFAULT_TAPE_AUTOLOAD_SLOT, DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES, Model, autoload_basic_disk,
    autoload_basic_tape, file_loader::load_host_file,
};

const KERNAL_ID: &str = "commodore-c64-kernal-rom";
const BASIC_ID: &str = "commodore-c64-basic-rom";
const CHARACTER_ID: &str = "commodore-c64-character-rom";
const DRIVE1541_ID: &str = "commodore-1541-dos-rom";
const DEFAULT_SCALE: u32 = 2;
const DEFAULT_IMPORT_BOOT_FRAMES: u32 = 200;
const INPUT_SLICES_PER_FRAME: u32 = 8;
const DEFAULT_TAPE_SLOT: &str = "tape-1";
const DEFAULT_DISK_SLOT: &str = "drive-8";

const PAL_ID: &str = "pal";
const NTSC_ID: &str = "ntsc";
const C64C_PAL_ID: &str = "c64c-pal";
const C64C_NTSC_ID: &str = "c64c-ntsc";

/// A resolved firmware bundle: `(id, bytes)` per ROM image. Stashed on the
/// [`C64System`] so a live variant switch can rebuild without re-reading ROMs.
type FirmwareBundle = Vec<(String, Vec<u8>)>;

// Seam-2 input port convention: port 0 = C64 gameport 2 (CIA1 PA,
// the main "gameport"). See runtime-commodore-c64/src/input.rs for
// the full mapping rationale. The host gamepad's face buttons all
// route to FIRE — the C64 stick is single-fire.
const C64_JOYSTICK_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(0, "up")),
    (HostControl::Down, ButtonTarget::new(0, "down")),
    (HostControl::Left, ButtonTarget::new(0, "left")),
    (HostControl::Right, ButtonTarget::new(0, "right")),
    (HostControl::South, ButtonTarget::new(0, "fire")),
    (HostControl::East, ButtonTarget::new(0, "fire")),
    (HostControl::West, ButtonTarget::new(0, "fire")),
    (HostControl::North, ButtonTarget::new(0, "fire")),
]);

/// Maps one physical host key to one or more C64-matrix key names. Lifted
/// verbatim from the bespoke `map_c64_keys`.
fn map_c64_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::ShiftLeft => &["lshift"],
        KeyCode::ShiftRight => &["rshift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["ctrl"],
        KeyCode::AltLeft | KeyCode::AltRight | KeyCode::SuperLeft | KeyCode::SuperRight => {
            &["commodore"]
        }
        KeyCode::ArrowRight => &["right"],
        KeyCode::ArrowLeft => &["lshift", "right"],
        KeyCode::ArrowDown => &["down"],
        KeyCode::ArrowUp => &["lshift", "down"],
        KeyCode::Home => &["home"],
        KeyCode::F1 => &["f1"],
        KeyCode::F2 => &["lshift", "f1"],
        KeyCode::F3 => &["f3"],
        KeyCode::F4 => &["lshift", "f3"],
        KeyCode::F5 => &["f5"],
        KeyCode::F6 => &["lshift", "f5"],
        KeyCode::F7 => &["f7"],
        KeyCode::F8 => &["lshift", "f7"],
        KeyCode::Minus => &["minus"],
        KeyCode::Equal => &["equals"],
        KeyCode::Comma => &["comma"],
        KeyCode::Period => &["period"],
        KeyCode::Slash => &["slash"],
        KeyCode::Semicolon => &["semicolon"],
        KeyCode::Quote => &["colon"],
        KeyCode::BracketLeft => &["at"],
        KeyCode::BracketRight => &["asterisk"],
        KeyCode::Backslash => &["plus"],
        KeyCode::Backquote => &["leftarrow"],
        KeyCode::Tab => &["runstop"],
        _ => return None,
    })
}

/// The arrow keys + Space the keyboard-joystick mode steals from the keyboard
/// path and routes through the gameport-2 button map. Lifted from the bespoke
/// `map_c64_joystick_key`.
fn map_c64_joystick_key(code: KeyCode) -> Option<HostControl> {
    Some(match code {
        KeyCode::ArrowUp => HostControl::Up,
        KeyCode::ArrowDown => HostControl::Down,
        KeyCode::ArrowLeft => HostControl::Left,
        KeyCode::ArrowRight => HostControl::Right,
        KeyCode::Space => HostControl::South,
        _ => return None,
    })
}

// ---- The UiSystem ----------------------------------------------------------

/// The C64 as a [`UiSystem`]. Tracks the active model so the title and the
/// Machine-menu radio follow live switches, the resolved firmware (so a variant
/// switch can rebuild without re-reading ROMs), and whether the arrow keys /
/// Space currently drive the gameport-2 joystick (Page Up).
struct C64System {
    model: Model,
    firmware: FirmwareBundle,
    keyboard_joystick: bool,
}

impl C64System {
    fn timing(&self) -> &'static C64Timing {
        match self.model {
            Model::C64NtscBreadbin | Model::C64cNtsc => &TIMING_NTSC_BREADBIN,
            Model::C64PalBreadbin | Model::C64cPal => &TIMING_PAL_BREADBIN,
        }
    }
}

impl UiSystem for C64System {
    type Runtime = C64Runtime;

    fn window_title(&self) -> String {
        format!("Emu198x | Commodore 64 ({})", model_label(self.model))
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        let vic = runtime.machine().vic();
        (vic.framebuffer_width(), vic.framebuffer_height())
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        u64::from(self.timing().cycles_per_frame)
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        let timing = self.timing();
        Duration::from_secs_f64(f64::from(timing.cycles_per_frame) / timing.cpu_hz as f64)
    }

    fn input_slices_per_frame(&self) -> u32 {
        INPUT_SLICES_PER_FRAME
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &C64_JOYSTICK_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        // In keyboard-joystick mode the arrow keys + Space fall through to the
        // joystick path (returning `None` here so the harness routes them
        // through `map_key` + the button map instead).
        if self.keyboard_joystick && map_c64_joystick_key(code).is_some() {
            return None;
        }
        map_c64_keys(code)
    }

    fn map_key(&self, code: KeyCode) -> Option<HostControl> {
        // Only meaningful in keyboard-joystick mode; otherwise the arrow keys +
        // Space are handled as keyboard keys by `map_keys`.
        if self.keyboard_joystick {
            map_c64_joystick_key(code)
        } else {
            None
        }
    }

    fn handle_key(&mut self, _runtime: &mut Self::Runtime, code: KeyCode, pressed: bool) -> bool {
        if code == KeyCode::PageUp {
            if pressed {
                self.keyboard_joystick = !self.keyboard_joystick;
                eprintln!(
                    "input: keyboard joystick {}",
                    if self.keyboard_joystick {
                        "enabled on gameport 2"
                    } else {
                        "disabled"
                    }
                );
            }
            return true;
        }
        false
    }

    fn tape_playing(&self, runtime: &Self::Runtime) -> bool {
        runtime.machine().tape_is_playing()
    }

    fn variants(&self) -> Vec<VariantInfo> {
        vec![
            VariantInfo::new(PAL_ID, model_label(Model::C64PalBreadbin)),
            VariantInfo::new(NTSC_ID, model_label(Model::C64NtscBreadbin)),
            VariantInfo::new(C64C_PAL_ID, model_label(Model::C64cPal)),
            VariantInfo::new(C64C_NTSC_ID, model_label(Model::C64cNtsc)),
        ]
    }

    fn current_variant(&self) -> Option<Cow<'static, str>> {
        Some(Cow::Borrowed(variant_id(self.model)))
    }

    fn switch_variant(
        &mut self,
        runtime: &mut Self::Runtime,
        variant: &str,
    ) -> Result<(), MachineError> {
        let model = model_for_variant(variant).ok_or(MachineError::UnsupportedOperation {
            operation: "unknown Commodore 64 variant",
        })?;
        // All four variants (PAL/NTSC breadbin and C64C) share the same
        // KERNAL/BASIC/CHARGEN/1541 firmware — they differ only in region and
        // SID revision — so rebuild from the stashed bytes rather than re-reading
        // the ROM files. The harness re-paces and refreshes; state/media are not
        // preserved (a hardware swap).
        let mut firmware = FirmwareSet::new();
        for (id, bytes) in &self.firmware {
            firmware.push(FirmwareImage::new(id.clone(), bytes));
        }
        *runtime = C64Runtime::from_firmware(model, &firmware)?;
        self.model = model;
        Ok(())
    }
}

/// The Machine-menu label for a model (region + SID revision).
fn model_label(model: Model) -> &'static str {
    match model {
        Model::C64PalBreadbin => "PAL Breadbin (6581)",
        Model::C64NtscBreadbin => "NTSC Breadbin (6581)",
        Model::C64cPal => "PAL C64C (8580)",
        Model::C64cNtsc => "NTSC C64C (8580)",
    }
}

/// The stable variant id for a model (round-trips through [`model_for_variant`]).
fn variant_id(model: Model) -> &'static str {
    match model {
        Model::C64PalBreadbin => PAL_ID,
        Model::C64NtscBreadbin => NTSC_ID,
        Model::C64cPal => C64C_PAL_ID,
        Model::C64cNtsc => C64C_NTSC_ID,
    }
}

/// Resolve a variant id from the Machine menu back to a [`Model`].
fn model_for_variant(variant: &str) -> Option<Model> {
    match variant {
        PAL_ID => Some(Model::C64PalBreadbin),
        NTSC_ID => Some(Model::C64NtscBreadbin),
        C64C_PAL_ID => Some(Model::C64cPal),
        C64C_NTSC_ID => Some(Model::C64cNtsc),
        _ => None,
    }
}

// ---- Construction + CLI ----------------------------------------------------

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Cli {
    model: ModelArg,
    rom_dir: Option<PathBuf>,
    kernal: Option<PathBuf>,
    basic: Option<PathBuf>,
    chargen: Option<PathBuf>,
    load: Option<PathBuf>,
    disk: Option<PathBuf>,
    tape: Option<PathBuf>,
    autoload_disk: bool,
    autoload_tape: bool,
    start_tape: bool,
    turbo_tape: bool,
    georam_kb: Option<usize>,
    load_snapshot: Option<PathBuf>,
    scale: u32,
    video: VideoFilter,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum ModelArg {
    #[default]
    Pal,
    Ntsc,
    C64cPal,
    C64cNtsc,
}

impl ModelArg {
    const fn to_model(self) -> Model {
        match self {
            Self::Pal => Model::C64PalBreadbin,
            Self::Ntsc => Model::C64NtscBreadbin,
            Self::C64cPal => Model::C64cPal,
            Self::C64cNtsc => Model::C64cNtsc,
        }
    }
}

impl From<ModelArg> for Model {
    fn from(arg: ModelArg) -> Self {
        arg.to_model()
    }
}

#[derive(Debug)]
struct LoadedFirmware {
    id: &'static str,
    bytes: Vec<u8>,
}

#[derive(Debug)]
struct LoadedProgram {
    name: String,
    bytes: Vec<u8>,
}

const USAGE: &str = "\
Usage: emu198x-c64 [OPTIONS]

Options:
    --rom-dir DIR        directory containing Commodore ROM images
    --kernal PATH        override KERNAL ROM path
    --basic PATH         override BASIC ROM path
    --chargen PATH       override character ROM path
    --model MODEL        pal, ntsc, c64c-pal, or c64c-ntsc [default: pal]
                         (c64c models fit the MOS 8580 SID; breadbins the 6581)
    --load PATH          import a program after boot: .prg, .bas, .t64, .d64,
                         or .p00 (PC64 container)
    --disk PATH          insert one D64 image into drive-8 at startup
    --tape PATH          insert one TAP image into datasette slot at startup
    --autoload-disk      wait for READY. and type LOAD\"*\",8,1 for drive-8
    --autoload-tape      wait for READY., press SHIFT+RUN/STOP, and start tape-1
    --start-tape         start the inserted tape immediately at startup
    --turbo-tape         run unthrottled while the tape is playing
    --georam KB          attach a GeoRAM RAM expansion (512, 1024, or 2048 KiB)
    --load-snapshot PATH restore a runtime snapshot before starting
    --scale N            integer window scale, default 2
    --video MODE         raw | lcd | crt [default: raw]
    --help, -h           show this help

Controls:
    Esc                  quit
    F9 / F10 / F11       start / stop tape, toggle tape turbo
    F12                  hard reset
    Cmd/Ctrl+S / +L      quick save / load state
    Page Up              toggle arrow/space joystick mode for gameport 2
    Arrow keys           C64 cursor keys
    Arrow keys + Space   joystick gameport 2 when Page Up mode is enabled
    F1-F8                C64 function keys
    Alt / Command        Commodore key
    Tab                  Run/Stop
    Gamepad              maps to gameport 2
    Machine menu         switch between PAL and NTSC live

Examples:
    emu198x-c64 --rom-dir ~/.emu198x/roms/commodore-c64
    emu198x-c64 --rom-dir ~/.emu198x/roms/commodore-c64 --load demo.bas
    emu198x-c64 --rom-dir ~/.emu198x/roms/commodore-c64 --disk game.d64
    emu198x-c64 --rom-dir ~/.emu198x/roms/commodore-c64 --disk game.d64 --autoload-disk
    emu198x-c64 --rom-dir ~/.emu198x/roms/commodore-c64 --tape game.tap --autoload-tape
    emu198x-c64 --load-snapshot ready.c64.pst
";

/// Build the runtime from the CLI and open the window.
pub fn run(cli: Cli) -> Result<(), String> {
    println!(
        "Controls: Esc quit, F9/F10 tape start/stop, F11 tape turbo, F12 reset, \
         Cmd/Ctrl+S/L save/load state, Page Up toggles gameport-2 arrows/space, \
         gamepad maps to gameport 2; Machine menu switches PAL/NTSC."
    );
    let (runtime, firmware) = build_runtime(&cli)?;
    let model = cli.model.into();
    emu198x_ui::run(
        C64System {
            model,
            firmware,
            keyboard_joystick: false,
        },
        runtime,
        cli.scale,
        cli.video,
    )
    .map_err(|err| err.to_string())
}

/// Boot a [`C64Runtime`] and apply the CLI's media workflow. A temporary
/// [`HeadlessSession`] is used for media load/autoload (reusing the shared
/// helpers), then unwrapped into the bare runtime the harness drives. Returns
/// the runtime *and* the resolved firmware bytes, so the [`C64System`] can stash
/// them for live variant switching.
fn build_runtime(cli: &Cli) -> Result<(C64Runtime, FirmwareBundle), String> {
    if cli.autoload_disk && cli.autoload_tape {
        return Err("--autoload-disk conflicts with --autoload-tape".to_owned());
    }
    if cli.autoload_tape && cli.start_tape {
        return Err("--autoload-tape conflicts with --start-tape".to_owned());
    }
    if (cli.autoload_tape || cli.start_tape) && cli.tape.is_none() {
        return Err("--autoload-tape and --start-tape require --tape PATH".to_owned());
    }

    let firmware_storage = load_firmware_bytes(cli)?;
    let firmware_bytes: FirmwareBundle = firmware_storage
        .iter()
        .map(|image| (image.id.to_owned(), image.bytes.clone()))
        .collect();
    let mut firmware = FirmwareSet::new();
    for image in &firmware_storage {
        firmware.push(FirmwareImage::new(image.id, &image.bytes));
    }

    let snapshot_bytes = match &cli.load_snapshot {
        Some(path) => Some(
            fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?,
        ),
        None => None,
    };

    let model = cli.model.to_model();
    let mut machine = boot_machine(
        &BootArtifacts {
            firmware,
            snapshot: snapshot_bytes.as_deref(),
        },
        |firmware| C64Runtime::from_firmware(model, firmware),
        || C64Runtime::blank(model),
    )
    .map_err(|err| format!("boot failed: {err}"))?;

    // Attach a GeoRAM expansion only when requested, so a snapshot that
    // restored its own GeoRAM is left intact when the flag is absent.
    if let Some(kb) = cli.georam_kb {
        machine.set_georam(Some(kb));
    }

    let frame_ticks = u64::from(match cli.model {
        ModelArg::Pal | ModelArg::C64cPal => TIMING_PAL_BREADBIN.cycles_per_frame,
        ModelArg::Ntsc | ModelArg::C64cNtsc => TIMING_NTSC_BREADBIN.cycles_per_frame,
    });
    let mut session =
        HeadlessSession::new_with_query_provider(machine, frame_ticks, C64SessionQueryProvider);

    if let Some(path) = &cli.tape {
        let loaded = read_media_asset(path, MediaKind::Tape)
            .map_err(|err| format!("failed to load tape asset {}: {err}", path.display()))?;
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            DEFAULT_TAPE_SLOT,
            MediaKind::Tape,
            &loaded.bytes,
        ));
        session
            .load_media(&media)
            .map_err(|err| format!("tape load failed: {err}"))?;
    }

    if let Some(path) = &cli.disk {
        let loaded = read_media_asset(path, MediaKind::Disk)
            .map_err(|err| format!("failed to load disk asset {}: {err}", path.display()))?;
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            DEFAULT_DISK_SLOT,
            MediaKind::Disk,
            &loaded.bytes,
        ));
        session
            .load_media(&media)
            .map_err(|err| format!("disk load failed: {err}"))?;
    }

    if cli.autoload_tape {
        autoload_basic_tape(
            &mut session,
            DEFAULT_TAPE_AUTOLOAD_SLOT,
            DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
            DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES,
        )
        .map_err(|err| format!("tape autoload failed: {err}"))?;
    } else if cli.autoload_disk {
        autoload_basic_disk(
            &mut session,
            DEFAULT_DISK_AUTOLOAD_SLOT,
            DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
            DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES,
        )
        .map_err(|err| format!("disk autoload failed: {err}"))?;
    } else if cli.start_tape {
        session
            .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
                DEFAULT_TAPE_AUTOLOAD_SLOT,
                MediaTransportAction::Start,
            )))
            .map_err(|err| format!("failed to start tape transport: {err}"))?;
    }

    if let Some(path) = &cli.load {
        let _ = session
            .wait_for_boot(DEFAULT_IMPORT_BOOT_FRAMES)
            .map_err(|err| format!("wait for boot failed: {err}"))?;
        let loaded = load_program_bytes(path)?;
        let message = load_host_file(session.machine_mut(), &loaded.name, &loaded.bytes)?;
        println!("{message}");
    }

    Ok((session.into_machine(), firmware_bytes))
}

fn load_firmware_bytes(cli: &Cli) -> Result<Vec<LoadedFirmware>, String> {
    let rom_dir = resolve_rom_dir(cli)?;
    let entries = [
        (
            KERNAL_ID,
            resolve_rom_path(
                cli.kernal.as_deref(),
                rom_dir.as_deref(),
                &["kernal.rom", "c64-kernal.rom"],
            )?,
        ),
        (
            BASIC_ID,
            resolve_rom_path(
                cli.basic.as_deref(),
                rom_dir.as_deref(),
                &["basic.rom", "c64-basic.rom"],
            )?,
        ),
        (
            CHARACTER_ID,
            resolve_rom_path(
                cli.chargen.as_deref(),
                rom_dir.as_deref(),
                &["chargen.rom", "c64-chargen.rom"],
            )?,
        ),
        (
            DRIVE1541_ID,
            resolve_rom_path(
                None,
                rom_dir.as_deref(),
                &["1541.rom", "dos1541.rom", "c1541.rom"],
            )?,
        ),
    ];

    entries
        .into_iter()
        .filter_map(|(id, path)| path.map(|path| (id, path)))
        .map(|(id, path)| {
            read_firmware_asset(&path)
                .map(|loaded| LoadedFirmware {
                    id,
                    bytes: loaded.bytes,
                })
                .map_err(|err| {
                    format!(
                        "failed to read firmware {id} from {}: {err}",
                        path.display()
                    )
                })
        })
        .collect()
}

fn load_program_bytes(path: &Path) -> Result<LoadedProgram, String> {
    let loaded = read_program_asset(path)
        .map_err(|err| format!("failed to read program {}: {err}", path.display()))?;
    let name = loaded.archive_member.unwrap_or_else(|| {
        path.file_name()
            .and_then(|value| value.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| path.display().to_string())
    });

    Ok(LoadedProgram {
        name,
        bytes: loaded.bytes,
    })
}

fn resolve_rom_dir(cli: &Cli) -> Result<Option<PathBuf>, String> {
    if let Some(dir) = &cli.rom_dir {
        return Ok(Some(dir.clone()));
    }

    if let Ok(dir) = std::env::var("EMU198X_C64_ROM_DIR") {
        return Ok(Some(PathBuf::from(dir)));
    }

    let Some(home) = std::env::var_os("HOME") else {
        return Ok(None);
    };
    let commodore_dir = PathBuf::from(&home).join(".emu198x/roms/commodore-c64");
    if commodore_dir.exists() {
        return Ok(Some(commodore_dir));
    }

    let legacy_dir = PathBuf::from(home).join(".emu198x/roms/c64");
    if legacy_dir.exists() {
        return Ok(Some(legacy_dir));
    }

    if cli.kernal.is_some()
        || cli.basic.is_some()
        || cli.chargen.is_some()
        || cli.load_snapshot.is_some()
    {
        return Ok(None);
    }

    Err(
        "no C64 ROM directory found — pass --rom-dir DIR, set EMU198X_C64_ROM_DIR, or create ~/.emu198x/roms/commodore-c64".into(),
    )
}

fn resolve_rom_path(
    explicit: Option<&Path>,
    rom_dir: Option<&Path>,
    filenames: &[&str],
) -> Result<Option<PathBuf>, String> {
    if let Some(path) = explicit {
        return Ok(Some(path.to_path_buf()));
    }

    let Some(rom_dir) = rom_dir else {
        return Ok(None);
    };

    for filename in filenames {
        let candidate = rom_dir.join(filename);
        if candidate.exists() {
            return Ok(Some(candidate));
        }
    }

    Err(format!(
        "missing required ROM in {} (looked for {})",
        rom_dir.display(),
        filenames.join(", ")
    ))
}

/// Parses the interactive CLI. Exits the process on `--help` or a malformed
/// flag.
pub fn parse_cli<I>(args: I) -> Cli
where
    I: IntoIterator<Item = String>,
{
    let mut cli = Cli {
        scale: DEFAULT_SCALE,
        ..Cli::default()
    };
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--rom-dir" => cli.rom_dir = Some(PathBuf::from(next_arg(&mut iter, "--rom-dir"))),
            "--kernal" => cli.kernal = Some(PathBuf::from(next_arg(&mut iter, "--kernal"))),
            "--basic" => cli.basic = Some(PathBuf::from(next_arg(&mut iter, "--basic"))),
            "--chargen" => cli.chargen = Some(PathBuf::from(next_arg(&mut iter, "--chargen"))),
            "--model" => cli.model = parse_model_arg(&next_arg(&mut iter, "--model")),
            "--load" => cli.load = Some(PathBuf::from(next_arg(&mut iter, "--load"))),
            "--disk" => cli.disk = Some(PathBuf::from(next_arg(&mut iter, "--disk"))),
            "--tape" => cli.tape = Some(PathBuf::from(next_arg(&mut iter, "--tape"))),
            "--autoload-disk" => cli.autoload_disk = true,
            "--autoload-tape" => cli.autoload_tape = true,
            "--start-tape" => cli.start_tape = true,
            "--turbo-tape" => cli.turbo_tape = true,
            "--georam" => cli.georam_kb = Some(parse_georam_size(&next_arg(&mut iter, "--georam"))),
            "--load-snapshot" => {
                cli.load_snapshot = Some(PathBuf::from(next_arg(&mut iter, "--load-snapshot")));
            }
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
                process::exit(0);
            }
            _ => die(&format!("unknown flag: {arg}")),
        }
    }

    cli
}

fn parse_video_arg(video: &str) -> VideoFilter {
    video
        .parse()
        .unwrap_or_else(|_| die("--video expects raw, lcd, or crt"))
}

/// Parse a `--georam` size in KiB. Accepts the standard 512/1024/2048 units.
fn parse_georam_size(value: &str) -> usize {
    match value.parse::<usize>() {
        Ok(kb @ (512 | 1024 | 2048)) => kb,
        _ => die("--georam expects a size in KiB: 512, 1024, or 2048"),
    }
}

fn parse_model_arg(value: &str) -> ModelArg {
    match value {
        "pal" => ModelArg::Pal,
        "ntsc" => ModelArg::Ntsc,
        "c64c-pal" | "c64c" => ModelArg::C64cPal,
        "c64c-ntsc" => ModelArg::C64cNtsc,
        _ => die("--model expects pal, ntsc, c64c-pal, or c64c-ntsc"),
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
    eprintln!();
    eprintln!("{USAGE}");
    process::exit(2);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_accepts_expected_flags() {
        let cli = parse_cli([
            "--model".to_string(),
            "ntsc".to_string(),
            "--rom-dir".to_string(),
            "roms".to_string(),
            "--load".to_string(),
            "demo.bas".to_string(),
            "--load-snapshot".to_string(),
            "ready.c64.pst".to_string(),
            "--scale".to_string(),
            "3".to_string(),
        ]);

        assert_eq!(
            cli,
            Cli {
                model: ModelArg::Ntsc,
                rom_dir: Some(PathBuf::from("roms")),
                kernal: None,
                basic: None,
                chargen: None,
                load: Some(PathBuf::from("demo.bas")),
                disk: None,
                tape: None,
                autoload_disk: false,
                autoload_tape: false,
                start_tape: false,
                turbo_tape: false,
                georam_kb: None,
                load_snapshot: Some(PathBuf::from("ready.c64.pst")),
                scale: 3,
                video: VideoFilter::Raw,
            }
        );
    }

    #[test]
    fn parse_cli_accepts_tape_flags() {
        let cli = parse_cli([
            "--tape".to_string(),
            "game.tap".to_string(),
            "--autoload-tape".to_string(),
        ]);

        assert_eq!(
            cli,
            Cli {
                model: ModelArg::Pal,
                rom_dir: None,
                kernal: None,
                basic: None,
                chargen: None,
                load: None,
                disk: None,
                tape: Some(PathBuf::from("game.tap")),
                autoload_disk: false,
                autoload_tape: true,
                start_tape: false,
                turbo_tape: false,
                georam_kb: None,
                load_snapshot: None,
                scale: DEFAULT_SCALE,
                video: VideoFilter::Raw,
            }
        );
    }

    #[test]
    fn parse_cli_accepts_tape_turbo_flag() {
        let cli = parse_cli(["--turbo-tape".to_string()]);
        assert!(cli.turbo_tape);
    }

    #[test]
    fn parse_cli_accepts_georam_size() {
        let cli = parse_cli(["--georam".to_string(), "512".to_string()]);
        assert_eq!(cli.georam_kb, Some(512));
        assert_eq!(parse_georam_size("2048"), 2048);
    }

    #[test]
    fn parse_cli_accepts_video_filter() {
        let cli = parse_cli(["--video".to_string(), "crt".to_string()]);
        assert_eq!(cli.video, VideoFilter::Crt);
    }

    #[test]
    fn parse_cli_accepts_disk_flag() {
        let cli = parse_cli(["--disk".to_string(), "game.d64".to_string()]);
        assert_eq!(cli.disk, Some(PathBuf::from("game.d64")));
        assert_eq!(cli.tape, None);
    }

    #[test]
    fn parse_cli_accepts_disk_autoload_flag() {
        let cli = parse_cli(["--autoload-disk".to_string()]);
        assert!(cli.autoload_disk);
        assert!(!cli.autoload_tape);
    }

    #[test]
    fn key_map_covers_cursors_and_shifted_function_keys() {
        assert_eq!(
            map_c64_keys(KeyCode::ArrowLeft),
            Some(&["lshift", "right"][..])
        );
        assert_eq!(
            map_c64_keys(KeyCode::ArrowUp),
            Some(&["lshift", "down"][..])
        );
        assert_eq!(map_c64_keys(KeyCode::F2), Some(&["lshift", "f1"][..]));
        assert_eq!(map_c64_keys(KeyCode::F8), Some(&["lshift", "f7"][..]));
        assert_eq!(map_c64_keys(KeyCode::Tab), Some(&["runstop"][..]));
        assert_eq!(map_c64_keys(KeyCode::AltLeft), Some(&["commodore"][..]));
        // Page Up is never a C64 matrix key (it toggles keyboard-joystick mode).
        assert_eq!(map_c64_keys(KeyCode::PageUp), None);
    }

    #[test]
    fn joystick_key_map_is_host_only() {
        assert_eq!(
            map_c64_joystick_key(KeyCode::ArrowLeft),
            Some(HostControl::Left)
        );
        assert_eq!(
            map_c64_joystick_key(KeyCode::Space),
            Some(HostControl::South)
        );
        assert_eq!(map_c64_joystick_key(KeyCode::F8), None);
    }

    #[test]
    fn variant_ids_round_trip_through_models() {
        for model in [
            Model::C64PalBreadbin,
            Model::C64NtscBreadbin,
            Model::C64cPal,
            Model::C64cNtsc,
        ] {
            assert_eq!(model_for_variant(variant_id(model)), Some(model));
        }
        assert_eq!(model_for_variant("nonsense"), None);
    }

    #[test]
    fn model_arg_parses_all_variants() {
        assert_eq!(parse_model_arg("pal").to_model(), Model::C64PalBreadbin);
        assert_eq!(parse_model_arg("ntsc").to_model(), Model::C64NtscBreadbin);
        assert_eq!(parse_model_arg("c64c-pal").to_model(), Model::C64cPal);
        assert_eq!(parse_model_arg("c64c").to_model(), Model::C64cPal);
        assert_eq!(parse_model_arg("c64c-ntsc").to_model(), Model::C64cNtsc);
    }

    #[test]
    fn page_up_toggles_keyboard_joystick_on_keydown_only() {
        let mut system = C64System {
            model: Model::C64PalBreadbin,
            firmware: Vec::new(),
            keyboard_joystick: false,
        };
        // Key-down flips the mode and consumes the key.
        assert!(c64system_handle_pageup(&mut system, true));
        assert!(system.keyboard_joystick);
        // Key-up consumes the key but does not toggle again.
        assert!(c64system_handle_pageup(&mut system, false));
        assert!(system.keyboard_joystick);
        // A second key-down flips it back off.
        assert!(c64system_handle_pageup(&mut system, true));
        assert!(!system.keyboard_joystick);
    }

    /// Test helper: exercise `handle_key` for Page Up without a live runtime
    /// (the C64 `handle_key` ignores the runtime for the Page-Up toggle).
    fn c64system_handle_pageup(system: &mut C64System, pressed: bool) -> bool {
        // The runtime argument is unused by the Page-Up branch, so a null
        // pointer read is never reached; route through a fresh blank runtime to
        // satisfy the signature without booting firmware.
        let mut runtime = C64Runtime::blank(Model::C64PalBreadbin);
        system.handle_key(&mut runtime, KeyCode::PageUp, pressed)
    }

    #[test]
    fn keyboard_joystick_mode_steals_arrows_and_space_from_keyboard() {
        let mut system = C64System {
            model: Model::C64PalBreadbin,
            firmware: Vec::new(),
            keyboard_joystick: false,
        };
        // Off: arrows are keyboard keys, no host control.
        assert!(system.map_keys(KeyCode::ArrowUp).is_some());
        assert_eq!(system.map_key(KeyCode::ArrowUp), None);
        // On: arrows fall through to the joystick path.
        system.keyboard_joystick = true;
        assert_eq!(system.map_keys(KeyCode::ArrowUp), None);
        assert_eq!(system.map_key(KeyCode::ArrowUp), Some(HostControl::Up));
        assert_eq!(system.map_key(KeyCode::Space), Some(HostControl::South));
        // A non-joystick key is still a keyboard key in joystick mode.
        assert!(system.map_keys(KeyCode::KeyA).is_some());
    }
}
