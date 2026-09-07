//! The Dragon 32/64 as a [`MachineApp`]: its flags, runtime, and the
//! routing between the shared script loop and the bring-up harness.
//!
//! The flag set is the union of what the window and the harness each
//! parsed for themselves: firmware and media (`--model`, `--rom`, `--rom64`,
//! `--tape`, `--cart`, `--disk`, `--bin`, `--snapshot`, `--autoload`) and
//! the harness's own surface (`--cycles`, `--type-command`, the trace
//! watches, the `--smoke-*` matrices, the `--xroar-*` references). The
//! harness flags select script mode on their own through
//! [`MachineApp::SCRIPT_FLAGS`], so `--rom dragon32.rom --smoke-root tapes`
//! runs headlessly without `--headless`, as the smoke pipeline has always
//! invoked it.

use std::path::PathBuf;

use emu198x_shell::launch::{Args, CommonCli, LaunchError, MachineApp, resolve_rom, script_report};
use emu198x_shell::{FirmwareImage, FirmwareSet, read_firmware_asset};
use machine_dragon_32::{AddressRange, DRAGON_FRAME_CYCLES, MatrixKey};
use runtime_dragon::{DragonRuntime, DragonSessionQueryProvider, Model};
use serde_json::{Map, Value};

use crate::script::{
    DEFAULT_CYCLES, DEFAULT_SMOKE_RUN_LIMIT, DEFAULT_TRACE_LIMIT, DEFAULT_XROAR_SETTLE_SECONDS,
    DEFAULT_XROAR_TIMEOUT_SECONDS, ScreenshotSource, SmokeJoystickAxisStep, SmokeJoystickAxisSweep,
    SmokeJoystickStep, SmokeScreenshotFormat, SmokeScreenshotPhase, parse_address_range,
    parse_dragon_key, parse_f32, parse_matrix_key, parse_model, parse_screenshot_format,
    parse_screenshot_phase, parse_screenshot_source, parse_smoke_joystick_axis_step,
    parse_smoke_joystick_axis_sweep, parse_smoke_joystick_step, parse_u32, parse_u64, parse_usize,
};

/// The MCP server's firmware: the Dragon 32 BASIC ROM from the
/// environment or the staged ROM directory.
const ROM_ENV: &str = "EMU198X_DRAGON32_ROM";
const ROM_RELATIVE: &str = "dragon/dragon32.rom";

/// The machine configuration the flags build up. Every field is a flag;
/// `screenshot` is the shared `--screenshot`, copied in from the launcher
/// when the harness runs so its capture options keep their meaning.
#[derive(Debug, Clone, PartialEq)]
pub struct Dragon {
    pub model: Model,
    pub rom: Option<PathBuf>,
    /// `--rom64`: the Dragon 64's 64-mode BASIC ROM.
    pub mode_rom: Option<PathBuf>,
    pub tape: Option<PathBuf>,
    pub cart: Option<PathBuf>,
    pub disk: Option<PathBuf>,
    pub bin: Option<PathBuf>,
    pub snapshot: Option<PathBuf>,
    pub autoload: bool,
    pub cycles: u64,
    pub type_command: Option<String>,
    pub trace_limit: usize,
    pub fetch_watch: Vec<AddressRange>,
    pub write_watch: Vec<AddressRange>,
    pub pressed_keys: Vec<MatrixKey>,
    pub dump_ram: Option<PathBuf>,
    pub disk_output: Option<PathBuf>,
    pub dump_text: bool,
    pub dump_text_png: Option<PathBuf>,
    pub screenshot: Option<PathBuf>,
    pub screenshot_format: SmokeScreenshotFormat,
    pub screenshot_phase: SmokeScreenshotPhase,
    pub screenshot_source: ScreenshotSource,
    pub smoke_root: Option<PathBuf>,
    pub bin_smoke_root: Option<PathBuf>,
    pub snapshot_smoke_root: Option<PathBuf>,
    pub disk_smoke_root: Option<PathBuf>,
    pub disk_smoke_launch: bool,
    pub smoke_run_limit: usize,
    pub smoke_report: Option<PathBuf>,
    pub smoke_screenshot_dir: Option<PathBuf>,
    pub smoke_screenshot_format: SmokeScreenshotFormat,
    pub smoke_audio_dir: Option<PathBuf>,
    pub smoke_joystick: Vec<SmokeJoystickStep>,
    pub smoke_joystick_axis: Vec<SmokeJoystickAxisStep>,
    pub smoke_joystick_axis_sweep: Vec<SmokeJoystickAxisSweep>,
    pub smoke_idle_after_start: u32,
    pub xroar_bin: Option<PathBuf>,
    pub xroar_reference_dir: Option<PathBuf>,
    pub xroar_snapshot_out: Option<PathBuf>,
    pub xroar_motoroff: Option<usize>,
    pub xroar_settle_seconds: f32,
    pub xroar_timeout_seconds: f32,
}

impl Default for Dragon {
    fn default() -> Self {
        Self {
            model: Model::Dragon32Pal,
            rom: None,
            mode_rom: None,
            tape: None,
            cart: None,
            disk: None,
            bin: None,
            snapshot: None,
            autoload: false,
            cycles: DEFAULT_CYCLES,
            type_command: None,
            trace_limit: DEFAULT_TRACE_LIMIT,
            fetch_watch: Vec::new(),
            write_watch: Vec::new(),
            pressed_keys: Vec::new(),
            dump_ram: None,
            disk_output: None,
            dump_text: false,
            dump_text_png: None,
            screenshot: None,
            screenshot_format: SmokeScreenshotFormat::Diagnostic,
            screenshot_phase: SmokeScreenshotPhase::Immediate,
            screenshot_source: ScreenshotSource::Beam,
            smoke_root: None,
            bin_smoke_root: None,
            snapshot_smoke_root: None,
            disk_smoke_root: None,
            disk_smoke_launch: false,
            smoke_run_limit: DEFAULT_SMOKE_RUN_LIMIT,
            smoke_report: None,
            smoke_screenshot_dir: None,
            smoke_screenshot_format: SmokeScreenshotFormat::Diagnostic,
            smoke_audio_dir: None,
            smoke_joystick: Vec::new(),
            smoke_joystick_axis: Vec::new(),
            smoke_joystick_axis_sweep: Vec::new(),
            smoke_idle_after_start: 0,
            xroar_bin: None,
            xroar_reference_dir: None,
            xroar_snapshot_out: None,
            xroar_motoroff: None,
            xroar_settle_seconds: DEFAULT_XROAR_SETTLE_SECONDS,
            xroar_timeout_seconds: DEFAULT_XROAR_TIMEOUT_SECONDS,
        }
    }
}

/// The harness parsers report a malformed value as a plain string; on the
/// command line that is a usage error.
fn usage(message: String) -> LaunchError {
    LaunchError::Usage(message)
}

impl MachineApp for Dragon {
    type Runtime = DragonRuntime;
    type Query = DragonSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-dragon";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    /// The harness's own flags. Their presence routes to script mode, so
    /// the smoke and XRoar pipelines run without `--headless`; the
    /// firmware and media flags are shared with the window and do not.
    const SCRIPT_FLAGS: &'static [&'static str] = &[
        "--cycles",
        "--type-command",
        "--trace-limit",
        "--watch-fetch",
        "--watch-write",
        "--press",
        "--press-matrix",
        "--dump-ram",
        "--disk-output",
        "--dump-text",
        "--dump-text-png",
        "--screenshot-format",
        "--screenshot-phase",
        "--screenshot-source",
        "--smoke-root",
        "--bin-smoke-root",
        "--snapshot-smoke-root",
        "--disk-smoke-root",
        "--disk-smoke-launch",
        "--smoke-run-limit",
        "--smoke-report",
        "--smoke-screenshot-dir",
        "--smoke-screenshot-format",
        "--smoke-audio-dir",
        "--smoke-joystick",
        "--smoke-joystick-axis",
        "--smoke-joystick-axis-sweep",
        "--smoke-idle-after-start",
        "--xroar-bin",
        "--xroar-reference-dir",
        "--xroar-snapshot-out",
        "--xroar-motoroff",
        "--xroar-settle-seconds",
        "--xroar-timeout-seconds",
        "--disk",
    ];
    const MACHINE_OPTIONS: &'static str = "    --model MODEL       dragon32 | dragon64 [default: dragon32]
    --rom PATH          Dragon 32 BASIC ROM, or Dragon 64 compatible-mode ROM; .zip archives are accepted
    --rom64 PATH        Dragon 64 64-mode BASIC ROM, required with --model dragon64
    --tape PATH         Dragon CAS tape image, or zip containing one .cas member (window)
    --cart PATH         Dragon cartridge ROM/DGN image; .zip archives are accepted
    --disk PATH         DragonDOS VDK disk image; .zip archives are accepted (headless)
    --bin PATH          DragonDOS .BIN program image; .zip archives are accepted
    --snapshot PATH     PC-Dragon PAK snapshot; .zip archives are accepted
    --autoload          type CLOAD/CLOADM, wait for load, then type RUN/EXEC (window)
    Without --video the window opens on the crt filter, not raw.

Headless harness (each of these selects headless mode; --frames is not accepted):
    --cycles N         maximum MC6809 bus cycles to run [default: 100000]
    --type-command S   boot through the runtime path, type a BASIC/DragonDOS command, then run --cycles
    --trace-limit N    number of recent instruction fetches to retain [default: 64]
    --watch-fetch A[-B]
                       retain opcode fetches in inclusive hex/decimal address range A..B; may be repeated
    --watch-write A[-B]
                       retain bus writes to inclusive hex/decimal address range A..B; may be repeated
    --press KEY        hold a named Dragon key closed; may be repeated
    --press-matrix R,C hold a raw keyboard matrix switch closed; may be repeated
    --dump-ram P       write the current 32 KiB RAM image as raw bytes
    --disk-output P    write the current mutated drive-1 VDK image to PATH
    --dump-text        print the current 32x16 MC6847 text snapshot
    --dump-text-png P  write the current border-inclusive MC6847 text framebuffer as a PNG
    --screenshot-format FORMAT
                       screenshot format: diagnostic | xroar-zoomed [default: diagnostic]
    --screenshot-phase PHASE
                       screenshot capture phase: immediate | completed-frame [default: immediate]
    --screenshot-source SOURCE
                       screenshot source: beam | static [default: beam]
    --smoke-root PATH  recursively scan .cas/.zip Dragon tape images
    --bin-smoke-root PATH
                       recursively scan .bin/.zip DragonDOS binary images
    --snapshot-smoke-root PATH
                       recursively scan .pak/.zip PC-Dragon snapshots
    --disk-smoke-root PATH
                       recursively scan .vdk/.zip DragonDOS disks and run DIR
    --disk-smoke-launch
                       with --disk-smoke-root, launch the first BASIC/BIN program instead of DIR
    --smoke-run-limit N
                       run real-ROM CLOAD/CLOADM or snapshot smoke for first N parsed media [default: 8]
    --smoke-report P   write smoke matrix JSON to PATH
    --smoke-screenshot-dir PATH
                       write load/start screenshots for runtime-smoked tapes
    --smoke-screenshot-format FORMAT
                       screenshot format: diagnostic | xroar-zoomed [default: diagnostic]
    --smoke-audio-dir PATH
                       write load/start WAV audio captures for runtime-smoked tapes
    --smoke-joystick PORT,CONTROL,FRAMES
                       after start, hold joystick control on port 1/2 for N frames;
                       CONTROL is up, down, left, right, fire, or idle; may be repeated
    --smoke-joystick-axis PORT,AXIS,VALUE,FRAMES
                       after start, hold analogue axis x/y on port 1/2 at VALUE for N frames;
                       VALUE is normalized from -1.0 to 1.0; may be repeated
    --smoke-joystick-axis-sweep PORT,AXIS,START,END,STEPS,FRAMES
                       after start, sweep analogue axis x/y over normalized START..END;
                       records whether each step changes visible output
    --smoke-idle-after-start FRAMES
                       after start, run N frames without extra input and capture idle output
    --xroar-bin PATH   patched XRoar binary used to write reference PNGs
    --xroar-reference-dir PATH
                       write patched-XRoar reference PNGs for runtime-smoked media
    --xroar-snapshot-out PATH
                       write the synthetic XRoar v2 snapshot used for reference comparison
    --xroar-motoroff N capture CAS reference on the Nth tape motor-off [default: auto]
    --xroar-settle-seconds N
                       wait N emulated seconds after CAS reference trigger before capture [default: 3];
                       snapshot references instead use the local screenshot cycle count
    --xroar-timeout-seconds N
                       hard XRoar run timeout in emulated seconds [default: 45]";
    const CONTROLS: &'static str = "    Esc              quit
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
    Machine menu     switch between Dragon 32 and Dragon 64 live";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--model" => self.model = parse_model(&args.value(flag)?).map_err(usage)?,
            "--rom" => self.rom = Some(args.path(flag)?),
            "--rom64" => self.mode_rom = Some(args.path(flag)?),
            "--tape" => self.tape = Some(args.path(flag)?),
            "--cart" => self.cart = Some(args.path(flag)?),
            "--disk" => self.disk = Some(args.path(flag)?),
            "--bin" => self.bin = Some(args.path(flag)?),
            "--snapshot" => self.snapshot = Some(args.path(flag)?),
            "--autoload" => self.autoload = true,
            "--cycles" => self.cycles = parse_u64(&args.value(flag)?, flag).map_err(usage)?,
            "--type-command" => self.type_command = Some(args.value(flag)?),
            "--trace-limit" => {
                self.trace_limit = parse_usize(&args.value(flag)?, flag).map_err(usage)?;
            }
            "--watch-fetch" => self
                .fetch_watch
                .push(parse_address_range(&args.value(flag)?, flag).map_err(usage)?),
            "--watch-write" => self
                .write_watch
                .push(parse_address_range(&args.value(flag)?, flag).map_err(usage)?),
            "--press" => {
                let key = parse_dragon_key(&args.value(flag)?).map_err(usage)?;
                self.pressed_keys.push(MatrixKey::from_dragon_key(key));
            }
            "--press-matrix" => self
                .pressed_keys
                .push(parse_matrix_key(&args.value(flag)?).map_err(usage)?),
            "--dump-ram" => self.dump_ram = Some(args.path(flag)?),
            "--disk-output" => self.disk_output = Some(args.path(flag)?),
            "--dump-text" => self.dump_text = true,
            "--dump-text-png" => self.dump_text_png = Some(args.path(flag)?),
            "--screenshot-format" => {
                self.screenshot_format =
                    parse_screenshot_format(&args.value(flag)?, flag).map_err(usage)?;
            }
            "--screenshot-phase" => {
                self.screenshot_phase =
                    parse_screenshot_phase(&args.value(flag)?, flag).map_err(usage)?;
            }
            "--screenshot-source" => {
                self.screenshot_source =
                    parse_screenshot_source(&args.value(flag)?, flag).map_err(usage)?;
            }
            "--smoke-root" => self.smoke_root = Some(args.path(flag)?),
            "--bin-smoke-root" => self.bin_smoke_root = Some(args.path(flag)?),
            "--snapshot-smoke-root" => self.snapshot_smoke_root = Some(args.path(flag)?),
            "--disk-smoke-root" => self.disk_smoke_root = Some(args.path(flag)?),
            "--disk-smoke-launch" => self.disk_smoke_launch = true,
            "--smoke-run-limit" => {
                self.smoke_run_limit = parse_usize(&args.value(flag)?, flag).map_err(usage)?;
            }
            "--smoke-report" => self.smoke_report = Some(args.path(flag)?),
            "--smoke-screenshot-dir" => self.smoke_screenshot_dir = Some(args.path(flag)?),
            "--smoke-screenshot-format" => {
                self.smoke_screenshot_format =
                    parse_screenshot_format(&args.value(flag)?, flag).map_err(usage)?;
            }
            "--smoke-audio-dir" => self.smoke_audio_dir = Some(args.path(flag)?),
            "--smoke-joystick" => self
                .smoke_joystick
                .push(parse_smoke_joystick_step(&args.value(flag)?).map_err(usage)?),
            "--smoke-joystick-axis" => self
                .smoke_joystick_axis
                .push(parse_smoke_joystick_axis_step(&args.value(flag)?).map_err(usage)?),
            "--smoke-joystick-axis-sweep" => self
                .smoke_joystick_axis_sweep
                .push(parse_smoke_joystick_axis_sweep(&args.value(flag)?).map_err(usage)?),
            "--smoke-idle-after-start" => {
                self.smoke_idle_after_start = parse_u32(&args.value(flag)?, flag).map_err(usage)?;
            }
            "--xroar-bin" => self.xroar_bin = Some(args.path(flag)?),
            "--xroar-reference-dir" => self.xroar_reference_dir = Some(args.path(flag)?),
            "--xroar-snapshot-out" => self.xroar_snapshot_out = Some(args.path(flag)?),
            "--xroar-motoroff" => {
                self.xroar_motoroff = Some(parse_usize(&args.value(flag)?, flag).map_err(usage)?);
            }
            "--xroar-settle-seconds" => {
                self.xroar_settle_seconds = parse_f32(&args.value(flag)?, flag).map_err(usage)?;
            }
            "--xroar-timeout-seconds" => {
                self.xroar_timeout_seconds = parse_f32(&args.value(flag)?, flag).map_err(usage)?;
            }
            _ if flag.starts_with('-') => return Ok(false),
            // The window takes one bare path as the ROM.
            _ => {
                if self.rom.is_some() {
                    return Err(LaunchError::Usage(
                        "only one positional ROM path is supported".to_owned(),
                    ));
                }
                self.rom = Some(PathBuf::from(flag));
            }
        }
        Ok(true)
    }

    /// One PAL VDG frame in MC6809 bus cycles — the machine's own constant,
    /// which the harness and the shared script loop already ran on.
    fn frame_ticks(&self) -> u64 {
        DRAGON_FRAME_CYCLES
    }

    fn query_provider(&self) -> DragonSessionQueryProvider {
        DragonSessionQueryProvider
    }

    /// The headless runtime: the harness's firmware loader (`--rom`
    /// required, exact ROM size, `.zip` accepted, `--rom64` with
    /// `--model dragon64`).
    fn build_runtime(&self) -> Result<DragonRuntime, LaunchError> {
        let firmware = crate::script::load_dragon_firmware(self)?;
        Ok(crate::script::runtime_from_firmware(&firmware)?)
    }

    /// The Dragon boot ROM is firmware, not loadable media, so it must be
    /// present at startup — the same way the Spectrum and Amiga MCP servers
    /// resolve their ROMs. The server always boots a Dragon 32 from the
    /// conventional location; media named on the command line is loaded by
    /// the launcher.
    fn build_mcp_runtime(&self) -> Result<DragonRuntime, LaunchError> {
        let rom_path = resolve_rom(None, ROM_ENV, ROM_RELATIVE)?;
        let rom = read_firmware_asset(&rom_path).map_err(|err| {
            LaunchError::Run(format!(
                "failed to load Dragon 32 ROM {}: {err}",
                rom_path.display()
            ))
        })?;
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new("dragon32-basic-rom", &rom.bytes));
        DragonRuntime::from_firmware(Model::Dragon32Pal, &firmware)
            .map_err(|err| LaunchError::Run(format!("failed to build Dragon runtime: {err}")))
    }

    /// The `--script` report is the shared `observations` and `time` alone.
    fn report(&self, _runtime: &DragonRuntime, _report: &mut Map<String, Value>) {}

    /// `--script` runs on the shared loop; anything else headless is the
    /// bring-up harness, which has its own run budget (`--cycles`) and has
    /// never accepted `--frames`.
    fn run_script(&self, common: &CommonCli, _raw_args: &[String]) -> Result<(), LaunchError> {
        if common.frames != 0 {
            return Err(LaunchError::Usage("unknown argument: --frames".to_owned()));
        }
        crate::script::validate(self).map_err(usage)?;
        if common.script.is_some() {
            let report = script_report(self, common)?;
            println!("{}", serde_json::to_string(&report).unwrap_or_default());
            return Ok(());
        }
        crate::script::run(self, common)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::launch::{Mode, Parsed, parse};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn parsed(list: &[&str]) -> (Dragon, CommonCli, Mode) {
        match parse::<Dragon>(&args(list)).expect("parses") {
            Parsed::Run { app, common, mode } => (app, common, mode),
            Parsed::Help => panic!("expected a run"),
        }
    }

    #[test]
    fn a_bare_rom_and_the_shared_media_flags_open_the_window() {
        let (app, common, mode) = parsed(&[
            "--rom",
            "dragon32.rom",
            "--model",
            "dragon32",
            "--tape",
            "program.cas",
            "--autoload",
            "--scale",
            "3",
            "--video",
            "raw",
        ]);
        assert_eq!(app.rom, Some(PathBuf::from("dragon32.rom")));
        assert_eq!(app.model, Model::Dragon32Pal);
        assert_eq!(app.tape, Some(PathBuf::from("program.cas")));
        assert!(app.autoload);
        assert_eq!(common.scale, Some(3));
        assert_eq!(common.video.as_deref(), Some("raw"));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn a_positional_rom_opens_the_window() {
        let (app, _, mode) = parsed(&["dragon32.rom"]);
        assert_eq!(app.rom, Some(PathBuf::from("dragon32.rom")));
        assert_eq!(mode, Mode::Ui);

        let err = parse::<Dragon>(&args(&["a.rom", "b.rom"])).expect_err("rejects");
        assert_eq!(
            err,
            LaunchError::Usage("only one positional ROM path is supported".to_owned())
        );
    }

    #[test]
    fn dragon64_takes_its_mode_rom() {
        let (app, ..) = parsed(&[
            "--model",
            "dragon64",
            "--rom",
            "dragon64-compat.rom",
            "--rom64",
            "dragon64.rom",
        ]);
        assert_eq!(app.model, Model::Dragon64Pal);
        assert_eq!(app.rom, Some(PathBuf::from("dragon64-compat.rom")));
        assert_eq!(app.mode_rom, Some(PathBuf::from("dragon64.rom")));
    }

    #[test]
    fn media_flags_parse_in_every_mode() {
        let (app, ..) = parsed(&[
            "--rom",
            "dragon32.rom",
            "--cart",
            "game.dgn",
            "--bin",
            "game.bin",
            "--snapshot",
            "game.pak",
            "--disk",
            "game.vdk",
        ]);
        assert_eq!(app.cart, Some(PathBuf::from("game.dgn")));
        assert_eq!(app.bin, Some(PathBuf::from("game.bin")));
        assert_eq!(app.snapshot, Some(PathBuf::from("game.pak")));
        assert_eq!(app.disk, Some(PathBuf::from("game.vdk")));
    }

    #[test]
    fn harness_flags_route_to_script_mode_without_headless() {
        for flags in [
            &["--rom", "dragon32.rom", "--smoke-root", "tapes"][..],
            &["--rom", "dragon32.rom", "--type-command", "PRINT 1"],
            &["--rom", "dragon32.rom", "--cycles", "0x20"],
            &["--rom", "dragon32.rom", "--dump-text"],
            &["--rom", "dragon32.rom", "--xroar-bin", "xroar"],
            &["--rom", "dragon32.rom", "--disk", "game.vdk"],
            &["--rom", "dragon32.rom", "--script", "steps.json"],
            &["--rom", "dragon32.rom", "--headless"],
        ] {
            let (.., mode) = parsed(flags);
            assert_eq!(mode, Mode::Script, "{flags:?} should be script");
        }
    }

    #[test]
    fn the_shared_script_flag_parses() {
        // `main.rs` has always routed `--script` here, and the harness's
        // parser once rejected it — so the flag selected the headless lane
        // and then died in it with "unknown argument". The Dragon was the
        // one frontend of thirty outside the shared session surface (#1073).
        let (_, common, mode) = parsed(&["--rom", "dragon32.rom", "--script", "steps.json"]);
        assert_eq!(common.script, Some(PathBuf::from("steps.json")));
        assert_eq!(mode, Mode::Script);
    }

    #[test]
    fn mcp_takes_precedence_over_harness_flags() {
        let (.., mode) = parsed(&["--mcp", "--smoke-root", "x"]);
        assert_eq!(mode, Mode::Mcp);
    }

    #[test]
    fn a_bad_model_is_a_usage_error() {
        let err = parse::<Dragon>(&args(&["--model", "coco"])).expect_err("rejects");
        assert_eq!(
            err,
            LaunchError::Usage("--model expects dragon32 or dragon64".to_owned())
        );
    }

    #[test]
    fn a_bad_harness_value_is_a_usage_error() {
        let err = parse::<Dragon>(&args(&["--cycles", "lots"])).expect_err("rejects");
        assert!(matches!(err, LaunchError::Usage(message) if message.contains("--cycles")));
    }

    #[test]
    fn frames_is_rejected_as_the_harness_always_did() {
        let app = Dragon {
            rom: Some(PathBuf::from("dragon32.rom")),
            ..Dragon::default()
        };
        let common = CommonCli {
            frames: 3,
            ..CommonCli::default()
        };
        assert_eq!(
            app.run_script(&common, &[]),
            Err(LaunchError::Usage("unknown argument: --frames".to_owned()))
        );
    }

    #[test]
    fn defaults_match_the_harness() {
        let app = Dragon::default();
        assert_eq!(app.cycles, 100_000);
        assert_eq!(app.trace_limit, 64);
        assert_eq!(app.smoke_run_limit, 8);
        assert_eq!(app.xroar_settle_seconds, 3.0);
        assert_eq!(app.xroar_timeout_seconds, 45.0);
        assert_eq!(app.frame_ticks(), DRAGON_FRAME_CYCLES);
    }
}
