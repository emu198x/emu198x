//! The Commodore 64 as a [`MachineApp`]: its flags, firmware resolution,
//! runtime construction, and MCP tool set.
//!
//! Script mode is the bespoke runner in `script.rs`, reached through
//! [`MachineApp::run_script`]: its report carries boot detection, printed
//! queries, screen text and traces that the shared loop has no shape for.
//! MCP mode is the launcher's default server with the C64 tools registered
//! on top. Windowed mode builds its runtime here — firmware, snapshot,
//! expansions, media, autoload, program import — and `ui.rs` supplies the
//! window.

use std::fs;
use std::path::{Path, PathBuf};

use common_commodore_c64::timing::{C64Timing, TIMING_NTSC_BREADBIN, TIMING_PAL_BREADBIN};
use emu198x_shell::launch::{Args, CommonCli, LaunchError, MachineApp};
use emu198x_shell::mcp::ToolRegistry;
use emu198x_shell::mcp_tools::{register_base_tools, register_keyboard_tools};
use emu198x_shell::{
    BootArtifacts, ControlCommand, FirmwareImage, FirmwareSet, HeadlessSession, MediaImage,
    MediaKind, MediaSet, MediaTransportAction, MediaTransportCommand, boot_machine,
    read_firmware_asset, read_media_asset, read_program_asset,
};
use runtime_commodore_c64::{
    C64Runtime, C64SessionQueryProvider, DEFAULT_DISK_AUTOLOAD_SLOT,
    DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES, DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
    DEFAULT_TAPE_AUTOLOAD_SLOT, DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES, Model, autoload_basic_disk,
    autoload_basic_disk_and_run, autoload_basic_tape, file_loader::load_host_file,
};
use serde_json::{Map, Value};

use crate::mcp_tools::register_c64_tools;

pub(crate) const KERNAL_ID: &str = "commodore-c64-kernal-rom";
pub(crate) const BASIC_ID: &str = "commodore-c64-basic-rom";
pub(crate) const CHARACTER_ID: &str = "commodore-c64-character-rom";
pub(crate) const DRIVE1541_ID: &str = "commodore-1541-dos-rom";
pub(crate) const DRIVE1571_ID: &str = "commodore-1571-dos-rom";
pub(crate) const DRIVE1581_ID: &str = "commodore-1581-dos-rom";
pub(crate) const DEFAULT_IMPORT_BOOT_FRAMES: u32 = 200;
pub(crate) const DEFAULT_TRACE_LIMIT: usize = 512;
pub(crate) const DEFAULT_TAPE_SLOT: &str = "tape-1";
pub(crate) const DEFAULT_DISK_SLOT: &str = "drive-8";

/// Baud the emulated modem answers at unless `--esp-at-baud` says otherwise.
///
/// A user-port modem is whatever rate the two ends agree on, and clients differ:
/// the Rachel C64 client bit-bangs 2400, its VIC-20 sibling 9600. Guessing wrong
/// does not fail loudly — the line simply decodes as garbage.
const ESP_AT_DEFAULT_BAUD: u64 = 9600;

/// RUBP's fixed frame size, which the bridge reassembles TCP reads into.
const ESP_AT_FRAME_SIZE: usize = 64;

/// A resolved firmware bundle: `(id, bytes)` per ROM image. Stashed on the
/// window driver so a live variant switch can rebuild without re-reading ROMs.
#[cfg(feature = "ui")]
pub(crate) type FirmwareBundle = Vec<(String, Vec<u8>)>;

/// `--model`: region and SID revision.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ModelArg {
    #[default]
    Pal,
    Ntsc,
    C64cPal,
    C64cNtsc,
}

impl ModelArg {
    /// Parse a `--model` value; `None` for an unknown one.
    pub(crate) fn from_id(value: &str) -> Option<Self> {
        Some(match value {
            "pal" => Self::Pal,
            "ntsc" => Self::Ntsc,
            "c64c-pal" | "c64c" => Self::C64cPal,
            "c64c-ntsc" => Self::C64cNtsc,
            _ => return None,
        })
    }

    pub(crate) const fn to_model(self) -> Model {
        match self {
            Self::Pal => Model::C64PalBreadbin,
            Self::Ntsc => Model::C64NtscBreadbin,
            Self::C64cPal => Model::C64cPal,
            Self::C64cNtsc => Model::C64cNtsc,
        }
    }

    pub(crate) const fn timing(self) -> &'static C64Timing {
        match self {
            Self::Pal | Self::C64cPal => &TIMING_PAL_BREADBIN,
            Self::Ntsc | Self::C64cNtsc => &TIMING_NTSC_BREADBIN,
        }
    }
}

impl From<ModelArg> for Model {
    fn from(arg: ModelArg) -> Self {
        arg.to_model()
    }
}

#[derive(Debug)]
pub(crate) struct LoadedFirmware {
    pub(crate) id: &'static str,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Debug)]
pub(crate) struct LoadedProgram {
    pub(crate) name: String,
    pub(crate) bytes: Vec<u8>,
}

/// The machine configuration the flags build up.
#[derive(Debug, PartialEq, Eq)]
pub struct C64 {
    pub model: ModelArg,
    pub rom_dir: Option<PathBuf>,
    pub kernal: Option<PathBuf>,
    pub basic: Option<PathBuf>,
    pub chargen: Option<PathBuf>,
    pub load: Option<PathBuf>,
    pub disk: Option<PathBuf>,
    pub tape: Option<PathBuf>,
    pub autoload_disk: bool,
    pub autoload_run: bool,
    pub autoload_tape: bool,
    pub start_tape: bool,
    /// Accepted for CLI compatibility with the rest of the fleet, but not a
    /// startup switch: tape fast-load (turbo) is a runtime toggle (F11) shared by
    /// every harness system via [`emu198x_ui`], not an initial state. Arming it
    /// from the CLI would mean a new parameter on the shared `emu198x_ui::run`
    /// (all 28 callers) — deliberately declined here, matching the Spectrum.
    pub turbo_tape: bool,
    pub georam_kb: Option<usize>,
    pub reu_kb: Option<usize>,
    pub mouse_1351_port: Option<u8>,
    pub esp_at_tcp: bool,
    pub esp_at_baud: Option<u64>,
    pub ultimate_net: bool,
    pub load_snapshot: Option<PathBuf>,
    pub save_snapshot: Option<PathBuf>,
    pub wait_for_boot: Option<u32>,
    pub wait_for_tape_stop: Option<u32>,
    pub print_queries: Vec<String>,
    pub print_screen_text: bool,
    pub trace_vic_colours: bool,
    pub trace_drive_rom_window: Option<(u16, u16)>,
    pub trace_limit: usize,
}

impl Default for C64 {
    fn default() -> Self {
        Self {
            model: ModelArg::default(),
            rom_dir: None,
            kernal: None,
            basic: None,
            chargen: None,
            load: None,
            disk: None,
            tape: None,
            autoload_disk: false,
            autoload_run: false,
            autoload_tape: false,
            start_tape: false,
            turbo_tape: false,
            georam_kb: None,
            reu_kb: None,
            mouse_1351_port: None,
            esp_at_tcp: false,
            esp_at_baud: None,
            ultimate_net: false,
            load_snapshot: None,
            save_snapshot: None,
            wait_for_boot: None,
            wait_for_tape_stop: None,
            print_queries: Vec::new(),
            print_screen_text: false,
            trace_vic_colours: false,
            trace_drive_rom_window: None,
            trace_limit: DEFAULT_TRACE_LIMIT,
        }
    }
}

impl MachineApp for C64 {
    type Runtime = C64Runtime;
    type Query = C64SessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-c64";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --rom-dir DIR        directory containing Commodore ROM images; default
                         EMU198X_C64_ROM_DIR, ~/.emu198x/roms/commodore-c64, or
                         ~/.emu198x/roms/c64, holding kernal.rom, basic.rom,
                         chargen.rom and the optional 1541/1571/1581 DOS ROMs
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
    --autoload-run       after --autoload-disk loads, wait for it and type RUN
    --autoload-tape      wait for READY., press SHIFT+RUN/STOP, and start tape-1
    --start-tape         start the inserted tape immediately at startup
    --turbo-tape         (accepted; arm tape fast-load in the UI with F11)
    --georam KB          attach a GeoRAM RAM expansion (512, 1024, or 2048 KiB)
    --reu KB             attach a 17xx REU RAM expansion (128, 256, or 512 KiB)
    --mouse-1351 PORT    plug a 1351 proportional mouse into control port 1 or 2
    --esp-at-tcp         plug a WiFi modem into the user port; dialling opens a
                         real TCP connection (64-byte frame reassembly). Speaks
                         both ESP-AT (AT+CIPSTART) and Hayes (ATD), latching
                         whichever the client uses first
    --esp-at-baud N      user-port line rate [default: 9600]
    --ultimate-net       fit an Ultimate Command Interface, giving the machine
                         the buffered network device a 1541 Ultimate-II or
                         Ultimate 64 provides. Preferred over --esp-at-tcp: no
                         line rate, no framing
    --load-snapshot PATH restore a runtime snapshot before starting
    --save-snapshot PATH write a runtime snapshot after a headless run
    --wait-for-boot N    headless: run up to N frames until boot.detected is true
    --wait-for-tape-stop N
                         headless: run up to N frames until c64.tape.playing has
                         started and then stops
    --print-query PATH   headless: resolve one query path after running (repeatable)
    --print-screen-text  headless: print decoded screen-text lines after running
    --trace-vic-colours  headless: trace D020/D021 changes during autoload and the
                         --frames run
    --trace-drive-rom S E
                         headless: trace drive-8 ROM activity for the inclusive hex
                         window S..E during autoload and the --frames run
    --trace-limit N      maximum traced events to retain [default: 512]";
    const CONTROLS: &'static str = "    Esc                  quit
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
    Machine menu         switch between PAL and NTSC live";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--rom-dir" => self.rom_dir = Some(args.path(flag)?),
            "--kernal" => self.kernal = Some(args.path(flag)?),
            "--basic" => self.basic = Some(args.path(flag)?),
            "--chargen" => self.chargen = Some(args.path(flag)?),
            "--model" => {
                let value = args.value(flag)?;
                self.model = ModelArg::from_id(&value).ok_or_else(|| {
                    LaunchError::Usage(
                        "--model expects pal, ntsc, c64c-pal, or c64c-ntsc".to_owned(),
                    )
                })?;
            }
            "--load" => self.load = Some(args.path(flag)?),
            "--disk" => self.disk = Some(args.path(flag)?),
            "--tape" => self.tape = Some(args.path(flag)?),
            "--autoload-disk" => self.autoload_disk = true,
            "--autoload-run" => self.autoload_run = true,
            "--autoload-tape" => self.autoload_tape = true,
            "--start-tape" => self.start_tape = true,
            "--turbo-tape" => self.turbo_tape = true,
            "--georam" => self.georam_kb = Some(parse_georam_size(&args.value(flag)?)?),
            "--reu" => self.reu_kb = Some(parse_reu_size(&args.value(flag)?)?),
            "--mouse-1351" => self.mouse_1351_port = Some(parse_mouse_port(&args.value(flag)?)?),
            "--esp-at-tcp" => self.esp_at_tcp = true,
            "--esp-at-baud" => self.esp_at_baud = Some(args.parse(flag, "a positive integer")?),
            "--ultimate-net" => self.ultimate_net = true,
            "--load-snapshot" => self.load_snapshot = Some(args.path(flag)?),
            "--save-snapshot" => self.save_snapshot = Some(args.path(flag)?),
            "--wait-for-boot" => {
                self.wait_for_boot = Some(args.parse(flag, "a non-negative integer")?);
            }
            "--wait-for-tape-stop" => {
                self.wait_for_tape_stop = Some(args.parse(flag, "a non-negative integer")?);
            }
            "--print-query" => self.print_queries.push(args.value(flag)?),
            "--print-screen-text" => self.print_screen_text = true,
            "--trace-vic-colours" => self.trace_vic_colours = true,
            "--trace-drive-rom" => {
                let start = parse_hex_u16(&args.value(flag)?).ok_or_else(|| {
                    LaunchError::Usage(
                        "--trace-drive-rom start must be a hexadecimal address".to_owned(),
                    )
                })?;
                let end = parse_hex_u16(&args.value(flag)?).ok_or_else(|| {
                    LaunchError::Usage(
                        "--trace-drive-rom end must be a hexadecimal address".to_owned(),
                    )
                })?;
                if start > end {
                    return Err(LaunchError::Usage(
                        "--trace-drive-rom start must be <= end".to_owned(),
                    ));
                }
                self.trace_drive_rom_window = Some((start, end));
            }
            "--trace-limit" => {
                self.trace_limit = args.parse(flag, "a non-negative integer")?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        u64::from(self.model.timing().cycles_per_frame)
    }

    fn query_provider(&self) -> C64SessionQueryProvider {
        C64SessionQueryProvider
    }

    /// The runtime for the window: booted, media inserted, and the autoload
    /// or program import already run so the machine is at the state the
    /// flags asked for when the window opens. Script mode does not use this;
    /// [`run_script`](Self::run_script) boots through [`Self::boot_runtime`]
    /// and drives the media workflow itself so its frames are observable.
    fn build_runtime(&self) -> Result<C64Runtime, LaunchError> {
        self.check_media_flags()?;
        if self.autoload_run && !self.autoload_disk {
            return Err(LaunchError::Run(
                "--autoload-run requires --autoload-disk".to_owned(),
            ));
        }
        if (self.autoload_tape || self.start_tape) && self.tape.is_none() {
            return Err(LaunchError::Run(
                "--autoload-tape and --start-tape require --tape PATH".to_owned(),
            ));
        }

        // A temporary session is used for media load/autoload (reusing the
        // shared helpers), then unwrapped into the bare runtime the harness
        // drives.
        let machine = self.boot_runtime()?;
        let mut session = HeadlessSession::new_with_query_provider(
            machine,
            self.frame_ticks(),
            C64SessionQueryProvider,
        );
        self.insert_media(&mut session)?;

        if self.autoload_tape {
            autoload_basic_tape(
                &mut session,
                DEFAULT_TAPE_AUTOLOAD_SLOT,
                DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
                DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES,
            )
            .map_err(|err| format!("tape autoload failed: {err}"))?;
        } else if self.autoload_disk {
            let autoload = if self.autoload_run {
                autoload_basic_disk_and_run
            } else {
                autoload_basic_disk
            };
            autoload(
                &mut session,
                DEFAULT_DISK_AUTOLOAD_SLOT,
                DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
                DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES,
            )
            .map_err(|err| format!("disk autoload failed: {err}"))?;
        } else if self.start_tape {
            session
                .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
                    DEFAULT_TAPE_AUTOLOAD_SLOT,
                    MediaTransportAction::Start,
                )))
                .map_err(|err| format!("failed to start tape transport: {err}"))?;
        }

        if let Some(path) = &self.load {
            let _ = session
                .wait_for_boot(DEFAULT_IMPORT_BOOT_FRAMES)
                .map_err(|err| format!("wait for boot failed: {err}"))?;
            let loaded = load_program_bytes(path)?;
            let message = load_host_file(session.machine_mut(), &loaded.name, &loaded.bytes)?;
            println!("{message}");
        }

        Ok(session.into_machine())
    }

    /// The boot ROMs are firmware, not loadable media, so they must be
    /// present at startup — the same way the Spectrum and Amiga MCP servers
    /// resolve their ROMs. A client (or Claude) then drives the session to
    /// debug it; media named on the command line is loaded by the launcher.
    fn build_mcp_runtime(&self) -> Result<C64Runtime, LaunchError> {
        self.boot_runtime()
    }

    /// The C64's report is built by `script::run`, which
    /// [`run_script`](Self::run_script) calls; the shared loop and this hook
    /// never run for this machine.
    fn report(&self, _runtime: &C64Runtime, _report: &mut Map<String, Value>) {}

    fn run_script(&self, common: &CommonCli, _raw_args: &[String]) -> Result<(), LaunchError> {
        crate::script::run(self, common)
    }

    fn register_mcp_tools(
        &self,
        registry: &mut ToolRegistry<HeadlessSession<C64Runtime, C64SessionQueryProvider>>,
    ) {
        register_base_tools(registry);
        // The C64 has a keyboard, so the shared press_key / type_string apply.
        register_keyboard_tools(registry);
        register_c64_tools(registry);
    }
}

impl C64 {
    /// The media flags that contradict each other in every mode.
    pub(crate) fn check_media_flags(&self) -> Result<(), LaunchError> {
        if self.autoload_disk && self.autoload_tape {
            return Err(LaunchError::Run(
                "--autoload-disk conflicts with --autoload-tape".to_owned(),
            ));
        }
        if self.autoload_tape && self.start_tape {
            return Err(LaunchError::Run(
                "--autoload-tape conflicts with --start-tape".to_owned(),
            ));
        }
        Ok(())
    }

    /// Boot a [`C64Runtime`] from the resolved firmware (or the snapshot
    /// named by `--load-snapshot`) and fit the hardware the flags ask for:
    /// RAM expansions, the 1351 mouse, and the user-port network devices.
    pub(crate) fn boot_runtime(&self) -> Result<C64Runtime, LaunchError> {
        let firmware_storage = self.load_firmware_bytes()?;
        let mut firmware = FirmwareSet::new();
        for image in &firmware_storage {
            firmware.push(FirmwareImage::new(image.id, &image.bytes));
        }

        let snapshot_bytes = match &self.load_snapshot {
            Some(path) => Some(
                fs::read(path)
                    .map_err(|err| format!("failed to read {}: {err}", path.display()))?,
            ),
            None => None,
        };

        let model = self.model.to_model();
        let mut machine = boot_machine(
            &BootArtifacts {
                firmware,
                snapshot: snapshot_bytes.as_deref(),
            },
            |firmware| C64Runtime::from_firmware(model, firmware),
            || C64Runtime::blank(model),
        )
        .map_err(|err| format!("boot failed: {err}"))?;

        // Attach expansions only when requested, so a snapshot that restored its
        // own expansion RAM is left intact when the flag is absent.
        if let Some(kb) = self.georam_kb {
            machine.set_georam(Some(kb));
        }
        if let Some(kb) = self.reu_kb {
            machine.set_reu(Some(kb));
        }
        if let Some(port) = self.mouse_1351_port {
            machine.set_mouse_1351(Some(port));
        }

        if self.esp_at_tcp {
            // The modem keeps real baud time while the C64's phi2 clock differs by
            // region, so the bit period is a cycle count rather than a constant.
            let cpu_hz = self.model.timing().cpu_hz;
            let baud = self.esp_at_baud.unwrap_or(ESP_AT_DEFAULT_BAUD);
            if baud == 0 {
                return Err(LaunchError::Run(
                    "--esp-at-baud must be greater than zero".to_owned(),
                ));
            }
            let cycles_per_bit = u32::try_from(cpu_hz / baud)
                .map_err(|_| "modem bit period does not fit in a cycle count".to_owned())?;
            machine.attach_esp_at_tcp_bridge(cycles_per_bit, ESP_AT_FRAME_SIZE);
        }
        if self.ultimate_net {
            machine.attach_ultimate_uci();
        }

        Ok(machine)
    }

    /// Insert the `--tape` and `--disk` images into their slots.
    pub(crate) fn insert_media(
        &self,
        session: &mut HeadlessSession<C64Runtime, C64SessionQueryProvider>,
    ) -> Result<(), LaunchError> {
        if let Some(path) = &self.tape {
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

        if let Some(path) = &self.disk {
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
        Ok(())
    }

    pub(crate) fn load_firmware_bytes(&self) -> Result<Vec<LoadedFirmware>, LaunchError> {
        let rom_dir = self.resolve_rom_dir()?;
        let entries = [
            (
                KERNAL_ID,
                resolve_rom_path(
                    self.kernal.as_deref(),
                    rom_dir.as_deref(),
                    &["kernal.rom", "c64-kernal.rom"],
                )?,
            ),
            (
                BASIC_ID,
                resolve_rom_path(
                    self.basic.as_deref(),
                    rom_dir.as_deref(),
                    &["basic.rom", "c64-basic.rom"],
                )?,
            ),
            (
                CHARACTER_ID,
                resolve_rom_path(
                    self.chargen.as_deref(),
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
            // The 1571 and 1581 DOS ROMs are optional: loaded when present so the
            // per-port drive selector can offer those models, absent otherwise
            // (`resolve_rom_path` returns `None`, and the profile marks both
            // optional). No CLI override — they live beside the 1541 in the ROM dir.
            (
                DRIVE1571_ID,
                resolve_rom_path(
                    None,
                    rom_dir.as_deref(),
                    &["1571.rom", "dos1571.rom", "c1571.rom"],
                )?,
            ),
            (
                DRIVE1581_ID,
                resolve_rom_path(
                    None,
                    rom_dir.as_deref(),
                    &["1581.rom", "dos1581.rom", "c1581.rom"],
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
                        LaunchError::Run(format!(
                            "failed to read firmware {id} from {}: {err}",
                            path.display()
                        ))
                    })
            })
            .collect()
    }

    /// The ROM directory, first match wins: `--rom-dir`, `EMU198X_C64_ROM_DIR`,
    /// `~/.emu198x/roms/commodore-c64`, `~/.emu198x/roms/c64`. `None` when
    /// there is none but the boot does not need one (explicit ROM paths or a
    /// snapshot).
    fn resolve_rom_dir(&self) -> Result<Option<PathBuf>, LaunchError> {
        if let Some(dir) = &self.rom_dir {
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

        if self.kernal.is_some()
            || self.basic.is_some()
            || self.chargen.is_some()
            || self.load_snapshot.is_some()
        {
            return Ok(None);
        }

        Err(LaunchError::Run(
            "no C64 ROM directory found — pass --rom-dir DIR, set EMU198X_C64_ROM_DIR, or create ~/.emu198x/roms/commodore-c64".to_owned(),
        ))
    }
}

pub(crate) fn resolve_rom_path(
    explicit: Option<&Path>,
    rom_dir: Option<&Path>,
    filenames: &[&str],
) -> Result<Option<PathBuf>, LaunchError> {
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

    Err(LaunchError::Run(format!(
        "missing required ROM in {} (looked for {})",
        rom_dir.display(),
        filenames.join(", ")
    )))
}

pub(crate) fn load_program_bytes(path: &Path) -> Result<LoadedProgram, LaunchError> {
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

/// Parse a `--georam` size in KiB. Accepts the standard 512/1024/2048 units.
fn parse_georam_size(value: &str) -> Result<usize, LaunchError> {
    match value.parse::<usize>() {
        Ok(kb @ (512 | 1024 | 2048)) => Ok(kb),
        _ => Err(LaunchError::Usage(
            "--georam expects a size in KiB: 512, 1024, or 2048".to_owned(),
        )),
    }
}

/// Parse a `--reu` size in KiB. Accepts the standard 128/256/512 REU units.
fn parse_reu_size(value: &str) -> Result<usize, LaunchError> {
    match value.parse::<usize>() {
        Ok(kb @ (128 | 256 | 512)) => Ok(kb),
        _ => Err(LaunchError::Usage(
            "--reu expects a size in KiB: 128, 256, or 512".to_owned(),
        )),
    }
}

/// Parse a `--mouse-1351` control-port number. The C64 has two control ports.
fn parse_mouse_port(value: &str) -> Result<u8, LaunchError> {
    match value.parse::<u8>() {
        Ok(port @ (1 | 2)) => Ok(port),
        _ => Err(LaunchError::Usage(
            "--mouse-1351 expects a control port: 1 or 2".to_owned(),
        )),
    }
}

fn parse_hex_u16(value: &str) -> Option<u16> {
    let trimmed = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    u16::from_str_radix(trimmed, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::launch::{Mode, Parsed, parse};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn parsed(list: &[&str]) -> (C64, CommonCli, Mode) {
        match parse::<C64>(&args(list)).expect("parses") {
            Parsed::Run { app, common, mode } => (app, common, mode),
            Parsed::Help => panic!("expected a run"),
        }
    }

    #[test]
    fn flags_cover_snapshot_boot_and_capture() {
        let (app, common, mode) = parsed(&[
            "--model",
            "ntsc",
            "--rom-dir",
            "roms",
            "--load-snapshot",
            "in.c64.pst",
            "--save-snapshot",
            "out.c64.pst",
            "--wait-for-boot",
            "180",
            "--print-screen-text",
            "--frames",
            "12",
            "--screenshot",
            "ready.png",
        ]);

        assert_eq!(
            app,
            C64 {
                model: ModelArg::Ntsc,
                rom_dir: Some(PathBuf::from("roms")),
                load_snapshot: Some(PathBuf::from("in.c64.pst")),
                save_snapshot: Some(PathBuf::from("out.c64.pst")),
                wait_for_boot: Some(180),
                print_screen_text: true,
                ..C64::default()
            }
        );
        assert_eq!(common.frames, 12);
        assert_eq!(common.screenshot, Some(PathBuf::from("ready.png")));
        assert_eq!(mode, Mode::Script);
    }

    #[test]
    fn flags_cover_the_window_media_workflow() {
        let (app, common, mode) = parsed(&[
            "--rom-dir",
            "roms",
            "--load",
            "demo.bas",
            "--disk",
            "game.d64",
            "--autoload-disk",
            "--autoload-run",
            "--scale",
            "3",
        ]);
        assert_eq!(app.load, Some(PathBuf::from("demo.bas")));
        assert_eq!(app.disk, Some(PathBuf::from("game.d64")));
        assert!(app.autoload_disk);
        assert!(app.autoload_run);
        assert!(!app.autoload_tape);
        assert_eq!(common.scale, Some(3));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn flags_cover_tape_autoload_and_the_tape_stop_wait() {
        let (app, _, _) = parsed(&[
            "--tape",
            "game.tap",
            "--autoload-tape",
            "--wait-for-tape-stop",
            "12000",
            "--turbo-tape",
        ]);
        assert_eq!(app.tape, Some(PathBuf::from("game.tap")));
        assert!(app.autoload_tape);
        assert!(app.turbo_tape);
        assert_eq!(app.wait_for_tape_stop, Some(12000));
        assert_eq!(app.trace_limit, DEFAULT_TRACE_LIMIT);
    }

    #[test]
    fn flags_cover_the_drive_rom_trace_window() {
        let (app, _, _) = parsed(&["--trace-drive-rom", "EC20", "ECA0", "--trace-limit", "9"]);
        assert_eq!(app.trace_drive_rom_window, Some((0xEC20, 0xECA0)));
        assert_eq!(app.trace_limit, 9);

        let err = parse::<C64>(&args(&["--trace-drive-rom", "ECA0", "EC20"])).expect_err("rejects");
        assert_eq!(
            err,
            LaunchError::Usage("--trace-drive-rom start must be <= end".to_owned())
        );
    }

    #[test]
    fn flags_cover_expansions_and_the_mouse() {
        let (app, _, _) = parsed(&[
            "--georam",
            "512",
            "--reu",
            "512",
            "--mouse-1351",
            "1",
            "--esp-at-tcp",
            "--esp-at-baud",
            "2400",
            "--ultimate-net",
        ]);
        assert_eq!(app.georam_kb, Some(512));
        assert_eq!(app.reu_kb, Some(512));
        assert_eq!(app.mouse_1351_port, Some(1));
        assert!(app.esp_at_tcp);
        assert_eq!(app.esp_at_baud, Some(2400));
        assert!(app.ultimate_net);

        assert_eq!(parse_georam_size("2048"), Ok(2048));
        assert_eq!(parse_reu_size("128"), Ok(128));
        assert_eq!(parse_mouse_port("2"), Ok(2));
        assert!(parse_georam_size("640").is_err());
        assert!(parse_reu_size("1024").is_err());
        assert!(parse_mouse_port("3").is_err());
    }

    #[test]
    fn a_bad_model_is_a_usage_error() {
        let err = parse::<C64>(&args(&["--model", "secam"])).expect_err("rejects");
        assert_eq!(
            err,
            LaunchError::Usage("--model expects pal, ntsc, c64c-pal, or c64c-ntsc".to_owned())
        );
    }

    #[test]
    fn model_arg_parses_all_variants() {
        assert_eq!(ModelArg::from_id("pal"), Some(ModelArg::Pal));
        assert_eq!(ModelArg::from_id("ntsc"), Some(ModelArg::Ntsc));
        assert_eq!(ModelArg::from_id("c64c-pal"), Some(ModelArg::C64cPal));
        assert_eq!(ModelArg::from_id("c64c"), Some(ModelArg::C64cPal));
        assert_eq!(ModelArg::from_id("c64c-ntsc"), Some(ModelArg::C64cNtsc));
        assert_eq!(ModelArg::C64cNtsc.to_model(), Model::C64cNtsc);
    }

    #[test]
    fn model_timing_follows_the_region() {
        assert_eq!(
            C64 {
                model: ModelArg::C64cPal,
                ..C64::default()
            }
            .frame_ticks(),
            u64::from(TIMING_PAL_BREADBIN.cycles_per_frame)
        );
        assert_eq!(
            C64 {
                model: ModelArg::Ntsc,
                ..C64::default()
            }
            .frame_ticks(),
            u64::from(TIMING_NTSC_BREADBIN.cycles_per_frame)
        );
    }

    #[test]
    fn build_runtime_rejects_autoload_run_without_disk() {
        let (app, _, _) = parsed(&["--autoload-run"]);
        let err = app
            .build_runtime()
            .map(|_| ())
            .expect_err("autoload-run needs autoload-disk");
        assert!(
            err.to_string()
                .contains("--autoload-run requires --autoload-disk")
        );
    }

    #[test]
    fn resolve_rom_path_prefers_explicit_override() {
        let resolved = resolve_rom_path(
            Some(Path::new("override/kernal.rom")),
            Some(Path::new("roms")),
            &["kernal.rom", "c64-kernal.rom"],
        )
        .expect("explicit ROM path should resolve");

        assert_eq!(resolved, Some(PathBuf::from("override/kernal.rom")));
    }

    /// The native firmware loader picks up the optional 1571 and 1581 DOS ROMs
    /// when present, so the per-port drive selector can offer those models. This
    /// is the enabling piece for the native-UI drive-type chooser: without the
    /// ROMs retained, `set_port_drive` would reject 1571/1581 as MissingFirmware.
    #[test]
    #[ignore = "FIXTURE: requires local C64 + 1541/1571/1581 DOS ROMs at ~/.emu198x/roms/commodore-c64/"]
    fn build_runtime_loads_the_optional_1571_and_1581_dos_roms() {
        use runtime_commodore_c64::DriveKind;

        let rom_dir = format!(
            "{}/.emu198x/roms/commodore-c64",
            std::env::var("HOME").expect("HOME set")
        );
        let (app, _, _) = parsed(&["--rom-dir", &rom_dir]);
        let mut runtime = app
            .build_runtime()
            .expect("build a runtime from the local ROM directory");

        // The ROMs were retained iff selecting those models on a port succeeds.
        runtime
            .set_port_drive(10, Some(DriveKind::C1571))
            .expect("1571 DOS ROM should have been loaded");
        runtime
            .set_port_drive(11, Some(DriveKind::C1581))
            .expect("1581 DOS ROM should have been loaded");
        assert_eq!(runtime.port_drive_kind(10), Some(DriveKind::C1571));
        assert_eq!(runtime.port_drive_kind(11), Some(DriveKind::C1581));
    }
}
