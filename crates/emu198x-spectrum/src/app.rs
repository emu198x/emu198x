//! The Spectrum as a [`MachineApp`]: its flags, runtime, and report fields.
//!
//! The shared launcher owns mode detection, the common flags, `--help`,
//! and the exit codes. Script mode and MCP mode keep their own bodies —
//! `script::runner` intercepts the family's steps before the shell
//! executor sees them and picks the boot variant from the script's first
//! portable snapshot, and `mcp` boots 48K eagerly and registers the
//! Spectrum tool set — so [`MachineApp::run_script`] and
//! [`MachineApp::run_mcp`] are overridden to call them.

use std::path::PathBuf;

use emu198x_shell::launch::{Args, CommonCli, LaunchError, MachineApp};
use emu198x_shell::mcp::ToolRegistry;
use emu198x_shell::{
    AssetLoadError, ControlCommand, FirmwareOverrides, HeadlessSession, MachineError, MediaImage,
    MediaKind, MediaSet, MediaTransportAction, MediaTransportCommand, NativeAudioError, QueryError,
    build_variant, read_media_asset,
};
use runtime_sinclair_zx_spectrum::{
    DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, DEFAULT_TAPE_AUTOLOAD_SLOT, Model, SpectrumLiveAccess,
    SpectrumRuntimeKind, SpectrumSessionQueryProvider, autoload_basic_tape,
};
use serde_json::{Map, Value};
use thiserror::Error;

use crate::mcp::tools::SpectrumSession;
use crate::script::runner::{ScriptInputs, run_script};

const DEFAULT_TAPE_SLOT: &str = "tape-1";

/// Error type used by the script runner, the MCP server, and the
/// portable-snapshot helpers. The launcher turns one into its run-time
/// failure (exit 1) through `Display`.
#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Asset(#[from] AssetLoadError),

    #[error(transparent)]
    Machine(#[from] MachineError),

    #[error(transparent)]
    Query(#[from] QueryError),

    #[error(transparent)]
    Session(#[from] emu198x_shell::SessionError),

    #[error(transparent)]
    SpectrumAutoload(#[from] runtime_sinclair_zx_spectrum::SpectrumAutoloadError),

    /// `NativeAudioError` lives in `emu198x-shell` (which always
    /// pulls cpal), so this arm is available regardless of the `ui`
    /// feature. The error itself is only constructed by the UI mode.
    #[error(transparent)]
    Audio(#[from] NativeAudioError),

    // `path` also carries a `FirmwareError`'s message on the boot paths
    // that flatten one into here, so the wording has to fit both a bare
    // path and a sentence. It used to say "no ROM supplied", which became
    // wrong the moment `--rom` could supply one (#842).
    #[error("Spectrum ROM unavailable: {path}")]
    MissingRom { path: String },

    #[error("tape transport requested without tape media")]
    MissingTape,

    /// `--machine` named an unknown variant, or one contradicted by the
    /// script's first portable snapshot.
    #[error("--machine: {reason}")]
    InvalidMachine {
        /// Why the requested variant was refused.
        reason: String,
    },

    #[error("--autoload-tape conflicts with --play-tape")]
    ConflictingTapeWorkflow,

    /// One script step is recognised by the shell vocabulary but not
    /// handled by this binary. `set_machine` was the last such step;
    /// it has been supported since #456 and now routes through
    /// `HeadlessSession::swap_machine`.
    #[error("script step `{step}` is unsupported: {reason}")]
    ScriptUnsupported {
        /// The step's serde tag (e.g. `"set_machine"`).
        step: &'static str,
        /// Human-readable reason for the binary's refusal.
        reason: String,
    },

    /// One script step's arguments were rejected by the active machine
    /// (e.g. a zero-length watch range, or an out-of-range address).
    #[error("script step `{step}` rejected: {reason}")]
    ScriptStepRejected {
        /// The step's serde tag (e.g. `"watch_memory_start"`).
        step: &'static str,
        /// Why the machine rejected the request.
        reason: String,
    },
}

impl From<AppError> for LaunchError {
    fn from(err: AppError) -> Self {
        Self::Run(err.to_string())
    }
}

/// The machine configuration the flags build up.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Spectrum {
    /// Raw `--rom` values, resolved against the boot variant's bundle
    /// once that variant is known, because `ID=PATH` is checked against
    /// it and a bare `PATH` only means anything on a single-ROM variant.
    /// Empty resolves the whole bundle under `~/.emu198x/roms`.
    pub rom: Vec<String>,
    /// `--machine ID`: the variant to boot, as a family variant id.
    /// `None` keeps the default 48K boot policy. Not validated at parse
    /// time — the boot path resolves it against `Model::from_variant_id`
    /// so the error carries the catalogue's list of accepted ids.
    pub machine: Option<String>,
    /// Tape media to load into `tape-1` before anything runs. A bare
    /// positional argument is the tape too, the UI's original spelling.
    pub tape: Option<PathBuf>,
    /// Start tape transport on `tape-1` immediately.
    pub play_tape: bool,
    /// Run the BASIC autoload sequence on `tape-1` once boot is detected.
    pub autoload_tape: bool,
    /// `--turbo-tape`: accepted for compatibility; fast-load is armed in
    /// the window with F11.
    pub turbo_tape: bool,
}

impl Spectrum {
    /// The variant `--machine` asks for, else 48K.
    ///
    /// # Errors
    ///
    /// A usage error naming the accepted identifiers when the id is not
    /// a variant.
    fn boot_model(&self) -> Result<Model, LaunchError> {
        match self.machine.as_deref() {
            Some(id) => Model::from_variant_id(id).ok_or_else(|| {
                LaunchError::Usage(format!(
                    "--machine: unknown machine id `{id}`; expected one of {}",
                    Model::VARIANT_IDS.join(", ")
                ))
            }),
            None => Ok(Model::Spectrum48KPal),
        }
    }
}

/// The `--rom` values as firmware pins against `model`'s bundle.
///
/// # Errors
///
/// The resolver's message for a bare path on a multi-ROM variant or a
/// malformed spec.
pub(crate) fn firmware_overrides(
    specs: &[String],
    model: Model,
) -> Result<FirmwareOverrides, String> {
    let sources = model.firmware_sources();
    let mut overrides = FirmwareOverrides::none();
    for spec in specs {
        overrides
            .add_spec(spec, model.variant_id(), &sources)
            .map_err(|err| err.to_string())?;
    }
    Ok(overrides)
}

impl MachineApp for Spectrum {
    type Runtime = SpectrumRuntimeKind;
    type Query = SpectrumSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-spectrum";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --rom PATH      ROM image or zip, for a single-ROM variant
    --rom ID=PATH   one entry of a multi-ROM variant's bundle; repeatable,
                    the rest still resolve under ~/.emu198x/roms. An ID
                    the variant does not have is an error, not a silent
                    fallback.
                    48K family:  sinclair-zx-spectrum-48k-rom
                    128K:        sinclair-zx-spectrum-128k-rom-{0,1}
                    +2:          sinclair-zx-spectrum-plus2-rom-{0,1}
                    +2A/+3:      sinclair-zx-spectrum-plus3-rom-{0..3}
    --machine ID    boot this variant instead of the default 48K.
                    One of: spectrum_16k, spectrum_48k, spectrum_plus,
                    spectrum_128k, spectrum_plus2, spectrum_plus2a,
                    spectrum_plus2b, spectrum_plus3, pentagon_128,
                    scorpion_zs256, timex_tc2048, timex_tc2068,
                    timex_ts2068. Use a mid-script
                    { \"action\": \"set_machine\" } step to swap variant
                    during a run.
    --tape PATH     TAP/TZX image or zip containing one tape candidate,
                    loaded into slot tape-1 (a bare PATH argument means
                    the same)
    --play-tape     start tape transport on tape-1 immediately
    --autoload-tape wait for boot, type LOAD \"\", and start tape-1
    --turbo-tape    (accepted; arm fast-load in the UI with F11)";
    const CONTROLS: &'static str = "    Esc                quit
    F9 / F10 / F11     start / stop tape, toggle fast-load (turbo)
    Cmd/Ctrl+Shift+E  export tape recording to a new .tap file
    Cmd/Ctrl+Shift+K  toggle Host / Original keyboard (also Machine → Keyboard)
    Home / Pause      EDIT / BREAK in Host keyboard mode
    F12                hard reset
    Cmd/Ctrl+S / +L    quick save / load state
    Arrow keys         Spectrum cursor keys (Caps Shift + 5/6/7/8)
    Alt                Symbol Shift
    Gamepad            Kempston on 16K/48K/+/128K/+2; IF2 on +2A/+2B/+3
    Machine menu       switch between the 13 variants live
    File > Open State  load a .sna / .z80 / .emu198x-state file";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            // Resolution needs the boot variant, which `--machine` may
            // not have supplied yet, so keep the raw spec and resolve
            // once the variant is known.
            "--rom" => self.rom.push(args.value(flag)?),
            "--machine" => self.machine = Some(args.value(flag)?),
            "--tape" => self.tape = Some(args.path(flag)?),
            "--play-tape" => self.play_tape = true,
            "--autoload-tape" => self.autoload_tape = true,
            "--turbo-tape" => self.turbo_tape = true,
            positional if !positional.starts_with('-') => {
                if self.tape.is_some() {
                    return Err(LaunchError::Usage(
                        "only one positional tape path is supported".to_owned(),
                    ));
                }
                self.tape = Some(PathBuf::from(positional));
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// The 48K frame, in master-clock half-cycles. Both headless bodies
    /// pace their session from the runtime they build
    /// (`native_frame_ticks`), which follows the variant; this stands in
    /// for the trait only.
    fn frame_ticks(&self) -> u64 {
        u64::from(common_sinclair_zx_spectrum::timing::TIMING_48K.halfcycles_per_frame)
    }

    fn query_provider(&self) -> SpectrumSessionQueryProvider {
        SpectrumSessionQueryProvider
    }

    /// Boot the variant and apply the tape workflow — the window's
    /// start-up. A temporary [`HeadlessSession`] is used for the tape
    /// load/autoload (reusing the shared helpers), then unwrapped into
    /// the bare runtime the harness drives.
    fn build_runtime(&self) -> Result<SpectrumRuntimeKind, LaunchError> {
        if self.play_tape && self.autoload_tape {
            return Err(LaunchError::Usage(
                "--play-tape and --autoload-tape are mutually exclusive".to_owned(),
            ));
        }
        let model = self.boot_model()?;
        let overrides = firmware_overrides(&self.rom, model)?;
        let runtime =
            build_variant::<SpectrumRuntimeKind>(model, &overrides).map_err(|e| e.to_string())?;

        let frame_ticks = u64::from(runtime.frame_halfcycles());
        let mut session = HeadlessSession::new_with_query_provider(
            runtime,
            frame_ticks,
            SpectrumSessionQueryProvider,
        );

        if let Some(tape_path) = &self.tape {
            let tape = read_media_asset(tape_path, MediaKind::Tape).map_err(|e| e.to_string())?;
            let mut media = MediaSet::new();
            media.push(MediaImage::new(
                DEFAULT_TAPE_SLOT,
                MediaKind::Tape,
                &tape.bytes,
            ));
            session.load_media(&media).map_err(|e| e.to_string())?;
        }

        if self.autoload_tape {
            if self.tape.is_none() {
                return Err(LaunchError::Usage(
                    "--autoload-tape needs a --tape".to_owned(),
                ));
            }
            autoload_basic_tape(
                &mut session,
                DEFAULT_TAPE_AUTOLOAD_SLOT,
                DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
            )
            .map_err(|e| e.to_string())?;
        } else if self.play_tape {
            if self.tape.is_none() {
                return Err(LaunchError::Usage("--play-tape needs a --tape".to_owned()));
            }
            session
                .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
                    DEFAULT_TAPE_SLOT,
                    MediaTransportAction::Start,
                )))
                .map_err(|e| e.to_string())?;
        }

        Ok(session.into_machine())
    }

    fn report(&self, runtime: &SpectrumRuntimeKind, report: &mut Map<String, Value>) {
        report.insert("tape_loaded".to_owned(), runtime.tape_is_loaded().into());
        report.insert("tape_playing".to_owned(), runtime.tape_is_playing().into());
    }

    /// Script mode keeps its own runner: it picks the boot variant from
    /// `--machine` or the script's first portable snapshot, intercepts
    /// the family's steps (`set_machine`, `autoload_tape`,
    /// `load_basic_program`, portable `load_snapshot`) before the shell
    /// executor, and routes the capture flags through the same steps a
    /// script would use.
    ///
    /// Output: the runner's report as JSON when a script file is
    /// supplied, or a one-line tape-state summary otherwise.
    fn run_script(&self, common: &CommonCli, _raw_args: &[String]) -> Result<(), LaunchError> {
        if (common.screenshot.is_some() || common.audio_capture.is_some())
            && common.frames == 0
            && common.script.is_none()
        {
            return Err(LaunchError::Usage(
                "capture requests require either --frames or --script so the machine emits output"
                    .to_owned(),
            ));
        }

        let report = run_script(ScriptInputs {
            script: common.script.clone(),
            frames: common.frames,
            screenshot: common.screenshot.clone(),
            audio_capture: common.audio_capture.clone(),
            machine: self.machine.clone(),
            tape: self.tape.clone(),
            play_tape: self.play_tape,
            autoload_tape: self.autoload_tape,
            rom: self.rom.clone(),
        })?;

        if common.script.is_some() {
            let json = serde_json::to_string(&report).map_err(|err| {
                LaunchError::Run(format!("failed to serialize runner report: {err}"))
            })?;
            println!("{json}");
        } else {
            println!(
                "Spectrum runtime: time={} tape_loaded={} tape_playing={}",
                report.time, report.tape_loaded, report.tape_playing
            );
        }
        Ok(())
    }

    /// MCP mode boots 48K eagerly and resolves `--rom` against that
    /// bundle, so the shared server (which would read `--rom` as
    /// cartridge media) does not apply; `mcp::run` is the body.
    fn run_mcp(&self, _raw_args: &[String]) -> Result<(), LaunchError> {
        crate::mcp::run(&self.rom).map_err(LaunchError::from)
    }

    fn register_mcp_tools(
        &self,
        registry: &mut ToolRegistry<SpectrumSession>,
        session: &SpectrumSession,
    ) {
        crate::mcp::register_full_surface(registry, session);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::launch::{Mode, Parsed, parse};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn parsed(list: &[&str]) -> (Spectrum, CommonCli, Mode) {
        match parse::<Spectrum>(&args(list)).expect("parses") {
            Parsed::Run { app, common, mode } => (app, common, mode),
            Parsed::Help => panic!("expected a run"),
        }
    }

    #[test]
    fn defaults_are_empty() {
        let (app, common, mode) = parsed(&[]);
        assert_eq!(app, Spectrum::default());
        assert_eq!(common, CommonCli::default());
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn flags_accept_rom_tape_scale_and_positional() {
        let (app, common, _) = parsed(&[
            "--rom",
            "48.rom",
            "--tape",
            "manic.zip",
            "--autoload-tape",
            "--scale",
            "3",
        ]);
        assert_eq!(app.rom, vec!["48.rom".to_owned()]);
        assert_eq!(app.tape, Some(PathBuf::from("manic.zip")));
        assert!(app.autoload_tape);
        assert_eq!(common.scale, Some(3));

        let (positional, _, _) = parsed(&["manic.zip"]);
        assert_eq!(positional.tape, Some(PathBuf::from("manic.zip")));
    }

    #[test]
    fn a_second_positional_tape_is_a_usage_error() {
        let err = parse::<Spectrum>(&args(&["a.tap", "b.tap"])).expect_err("rejects");
        assert_eq!(
            err,
            LaunchError::Usage("only one positional tape path is supported".to_owned())
        );
    }

    #[test]
    fn convenience_aliases_select_script_mode() {
        let (app, common, mode) = parsed(&[
            "--headless",
            "--tape",
            "manic.tzx",
            "--autoload-tape",
            "--script",
            "run.json",
        ]);
        assert_eq!(mode, Mode::Script);
        assert_eq!(common.script, Some(PathBuf::from("run.json")));
        assert_eq!(
            app,
            Spectrum {
                tape: Some(PathBuf::from("manic.tzx")),
                autoload_tape: true,
                ..Spectrum::default()
            }
        );
    }

    #[test]
    fn machine_id_is_kept_raw_and_not_validated() {
        // The boot path resolves it against `Model::from_variant_id` so
        // the error carries the catalogue's list of accepted ids.
        let (app, _, _) = parsed(&["--headless", "--machine", "spectrum_999k"]);
        assert_eq!(app.machine.as_deref(), Some("spectrum_999k"));
        assert!(app.boot_model().is_err());
        let (app, _, _) = parsed(&["--machine", "spectrum_128k"]);
        assert_eq!(app.boot_model().expect("known id"), Model::Spectrum128KPal);
    }

    #[test]
    fn play_tape_alone_is_supported() {
        let (app, _, _) = parsed(&["--tape", "demo.tap", "--play-tape"]);
        assert!(app.play_tape);
        assert!(!app.autoload_tape);
        assert_eq!(app.tape, Some(PathBuf::from("demo.tap")));
    }

    /// #1187: twenty-nine binaries took these and the Spectrum did not,
    /// so the machine the curriculum leads with was the only one that
    /// needed a JSON file to take a picture.
    #[test]
    fn the_capture_flags_parse() {
        let (_, common, mode) = parsed(&[
            "--headless",
            "--frames",
            "120",
            "--screenshot",
            "boot.png",
            "--audio-capture",
            "boot.wav",
        ]);
        assert_eq!(mode, Mode::Script);
        assert_eq!(common.frames, 120);
        assert_eq!(common.screenshot, Some(PathBuf::from("boot.png")));
        assert_eq!(common.audio_capture, Some(PathBuf::from("boot.wav")));
    }

    #[test]
    fn rom_is_repeatable() {
        let (app, _, _) = parsed(&[
            "--rom",
            "sinclair-zx-spectrum-128k-rom-0=/a/0.rom",
            "--rom",
            "sinclair-zx-spectrum-128k-rom-1=/a/1.rom",
        ]);
        assert_eq!(app.rom.len(), 2);
    }
}
