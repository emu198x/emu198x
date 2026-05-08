//! Script execution loop.
//!
//! Boots the eager 48K runtime, wraps it in a `HeadlessSession`, and
//! iterates the script. System-specific steps (`SetMachine`,
//! `AutoloadTape`) are intercepted before the shell executor sees
//! them; everything else delegates to `ScriptStep::execute_collect`.
//!
//! `SetMachine` is **not yet supported** in this commit — it errors
//! with a clear message. Mid-script runtime swaps need an
//! enum-of-sessions wrapper (each variant has a different concrete
//! `HeadlessSession<M, Q>` type) which is its own commit. Code198x's
//! existing scripts don't need it; eager 48K covers them.

use std::path::PathBuf;

use common_sinclair_zx_spectrum::timing::TIMING_48K;
use emu198x_shell::{
    ControlCommand, FirmwareImage, FirmwareSet, HeadlessScript, HeadlessSession, MediaImage,
    MediaKind, MediaSet, MediaTransportAction, MediaTransportCommand, ScriptError,
    ScriptObservation, ScriptStep, read_firmware_asset, read_media_asset,
};
use format_sinclair_zx_spectrum_bas::tokenise;
use runtime_sinclair_zx_spectrum::{
    DEFAULT_BASIC_LOADER_BOOT_FRAMES, DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, Spectrum48kRuntime,
    SpectrumSessionQueryProvider, autoload_basic_tape, load_basic_program,
};
use serde::Serialize;

use crate::AppError;
use crate::machine::{MachineKind, rom_root, variant_rom_bundle};

const DEFAULT_TAPE_SLOT: &str = "tape-1";

/// Inputs passed from `script::run` into the script runner.
#[derive(Debug, Default)]
pub struct ScriptInputs {
    /// Optional JSON session file to execute.
    pub script: Option<PathBuf>,
    /// Tape media to load before script execution.
    pub tape: Option<PathBuf>,
    /// Start tape transport on `tape-1` immediately.
    pub play_tape: bool,
    /// Run the BASIC autoload sequence on `tape-1` once boot is detected.
    pub autoload_tape: bool,
}

/// Final report emitted on stdout when a script file is supplied.
#[derive(Debug, Serialize)]
pub struct RunnerReport {
    /// Structured observations emitted by the script's steps.
    pub observations: Vec<ScriptObservation>,
    /// Machine time reached after the script completed (master half-cycles).
    pub time: u64,
    /// Whether tape media was loaded at exit.
    pub tape_loaded: bool,
    /// Whether tape transport was playing at exit.
    pub tape_playing: bool,
}

/// Runs the script. Eager 48K boot, then iterates convenience-flag
/// steps (synthesised from the CLI) followed by JSON-file steps.
/// Returns the final report.
pub fn run_script(inputs: ScriptInputs) -> Result<RunnerReport, AppError> {
    let runtime = boot_eager_48k()?;
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    let synthetic = synthetic_steps_from_cli(&inputs);
    let json_script = match &inputs.script {
        Some(path) => Some(HeadlessScript::from_path(path).map_err(|err| {
            AppError::Io(std::io::Error::other(format!(
                "failed to load script {}: {err}",
                path.display()
            )))
        })?),
        None => None,
    };

    let mut observations = Vec::new();

    // Convenience-flag steps run first, in CLI-order. The tape itself
    // has to be loaded into the session before MediaTransport /
    // AutoloadTape can act on it, so we keep the tape bytes in scope
    // for the duration of the call.
    let tape_bytes = match &inputs.tape {
        Some(path) => Some(
            read_media_asset(path, MediaKind::Tape).map_err(AppError::from)?,
        ),
        None => None,
    };

    if let Some(loaded) = &tape_bytes {
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            DEFAULT_TAPE_SLOT,
            MediaKind::Tape,
            &loaded.bytes,
        ));
        session.load_media(&media)?;
    }

    for step in &synthetic {
        if let Some(observation) = execute_step(step, &mut session)? {
            observations.push(observation);
        }
    }

    if let Some(script) = &json_script {
        for step in &script.steps {
            if let Some(observation) = execute_step(step, &mut session)? {
                observations.push(observation);
            }
        }
    }

    Ok(RunnerReport {
        observations,
        time: session.time().get(),
        tape_loaded: session.machine().machine().tape_is_loaded(),
        tape_playing: session.machine().machine().tape_is_playing(),
    })
}

/// Executes one `ScriptStep`, intercepting the system-specific
/// variants before delegating to the shell executor.
///
/// Pub(crate) so the binary's MCP mode dispatches its tool calls
/// through the same path script mode uses; SetMachine / AutoloadTape /
/// LoadBasicProgram interception is shared across both modes.
pub(crate) fn execute_step(
    step: &ScriptStep,
    session: &mut HeadlessSession<Spectrum48kRuntime, SpectrumSessionQueryProvider>,
) -> Result<Option<ScriptObservation>, AppError> {
    match step {
        ScriptStep::SetMachine { machine } => {
            // SetMachine isn't yet wired in script mode. Mid-script
            // runtime swaps need an enum-of-sessions wrapper (each
            // variant is a distinct `HeadlessSession<M, Q>` type)
            // and a follow-up commit will add it. Until then, eager
            // 48K covers Code198x's existing pipeline.
            Err(AppError::ScriptUnsupported {
                step: "set_machine",
                reason: format!(
                    "set_machine to '{machine}' not yet supported in script mode; \
                     this binary boots 48K eagerly. Mid-script runtime swaps land \
                     in a follow-up commit."
                ),
            })
        }
        ScriptStep::AutoloadTape {
            slot,
            max_boot_frames,
        } => execute_autoload_tape(session, slot, *max_boot_frames).map(Some),
        ScriptStep::LoadBasicProgram { path, run } => {
            execute_load_basic_program(session, path, *run).map(Some)
        }
        other => other
            .execute_collect(session)
            .map_err(map_script_error),
    }
}

/// Executes an `autoload_tape` step against the current 48K session.
/// Wraps the existing `runtime-sinclair-zx-spectrum::autoload_basic_tape`
/// helper, which is currently 48K-specific.
fn execute_autoload_tape(
    session: &mut HeadlessSession<Spectrum48kRuntime, SpectrumSessionQueryProvider>,
    slot: &str,
    max_boot_frames: u32,
) -> Result<ScriptObservation, AppError> {
    let result = autoload_basic_tape(session, slot, max_boot_frames)?;
    Ok(ScriptObservation::AutoloadTape {
        slot: result.slot,
        boot_frames: result.boot.frames,
    })
}

/// Reads one BASIC source file from disk, tokenises it, and installs
/// the result as the live machine's program via the runtime helper.
fn execute_load_basic_program(
    session: &mut HeadlessSession<Spectrum48kRuntime, SpectrumSessionQueryProvider>,
    path: &PathBuf,
    run: bool,
) -> Result<ScriptObservation, AppError> {
    let source = std::fs::read_to_string(path).map_err(|err| {
        AppError::Io(std::io::Error::other(format!(
            "failed to read BASIC source {}: {err}",
            path.display()
        )))
    })?;
    let program = tokenise(&source).map_err(|reason| AppError::Io(std::io::Error::other(
        format!("failed to tokenise BASIC source {}: {reason}", path.display()),
    )))?;
    let result = load_basic_program(session, &program, run, DEFAULT_BASIC_LOADER_BOOT_FRAMES)
        .map_err(|err| AppError::Io(std::io::Error::other(format!(
            "BASIC loader failed for {}: {err}",
            path.display()
        ))))?;
    Ok(ScriptObservation::LoadBasicProgram {
        program_bytes: result.program_bytes,
        ran: result.ran,
    })
}

/// Eager-boot the default 48K runtime from the conventional ROM path
/// (`~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom`). Returns
/// `AppError::MissingRom` if the ROM isn't present.
///
/// Pub(crate) so the binary's MCP mode reuses the same boot path —
/// MCP's session lifecycle starts identically to script mode.
pub(crate) fn boot_eager_48k() -> Result<Spectrum48kRuntime, AppError> {
    let root = rom_root().ok_or_else(|| AppError::MissingRom {
        path: "$HOME unset; cannot locate ROM bundle".to_owned(),
    })?;
    let bundle = variant_rom_bundle(MachineKind::Spectrum48K, &root);
    let (id, path) = bundle.into_iter().next().ok_or_else(|| AppError::MissingRom {
        path: "48K bundle is empty (internal error)".to_owned(),
    })?;
    if !path.is_file() {
        return Err(AppError::MissingRom {
            path: path.display().to_string(),
        });
    }
    let rom = read_firmware_asset(&path)?.bytes;
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(id, &rom));
    Spectrum48kRuntime::from_firmware(&firmware).map_err(AppError::from)
}

/// Translates surviving CLI convenience flags into prepended
/// `ScriptStep`s. Order matters: tape media is loaded by the runner
/// itself before this returns (so MediaTransport / AutoloadTape can
/// act on it); the steps here cover transport + autoload only.
fn synthetic_steps_from_cli(inputs: &ScriptInputs) -> Vec<ScriptStep> {
    let mut steps = Vec::new();
    if inputs.play_tape {
        steps.push(ScriptStep::MediaTransport {
            slot: DEFAULT_TAPE_SLOT.to_owned(),
            transport: emu198x_shell::ScriptMediaTransportAction::Start,
        });
    }
    if inputs.autoload_tape {
        steps.push(ScriptStep::AutoloadTape {
            slot: DEFAULT_TAPE_SLOT.to_owned(),
            max_boot_frames: DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
        });
    }
    steps
}

/// Maps a `ScriptError` from the shell executor onto the binary's
/// `AppError`. The `SystemSpecificStep` arm becomes a clearer
/// `ScriptUnsupported` with a binary-side reason; everything else
/// flows through transparently.
fn map_script_error(err: ScriptError) -> AppError {
    match err {
        ScriptError::SystemSpecificStep { step } => AppError::ScriptUnsupported {
            step,
            reason: format!(
                "step `{step}` is system-specific and not yet supported in this binary"
            ),
        },
        ScriptError::Asset(e) => AppError::Asset(e),
        ScriptError::Io(e) => AppError::Io(e),
        ScriptError::Parse(e) => AppError::Io(std::io::Error::other(format!("script parse: {e}"))),
        ScriptError::Session(e) => AppError::Session(e),
        ScriptError::Query(e) => AppError::Query(e),
    }
}

// MediaTransport command alias kept for documentation symmetry —
// the actual translation happens inline in `synthetic_steps_from_cli`
// via `ScriptStep::MediaTransport`. Suppress dead-code lint locally:
#[allow(dead_code)]
fn _control_command_anchor() -> ControlCommand {
    ControlCommand::MediaTransport(MediaTransportCommand::new(
        DEFAULT_TAPE_SLOT,
        MediaTransportAction::Start,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_steps_default_to_empty() {
        let steps = synthetic_steps_from_cli(&ScriptInputs::default());
        assert!(steps.is_empty());
    }

    #[test]
    fn synthetic_steps_translate_play_tape_to_media_transport() {
        let steps = synthetic_steps_from_cli(&ScriptInputs {
            play_tape: true,
            ..Default::default()
        });
        assert_eq!(
            steps,
            vec![ScriptStep::MediaTransport {
                slot: DEFAULT_TAPE_SLOT.to_owned(),
                transport: emu198x_shell::ScriptMediaTransportAction::Start,
            }]
        );
    }

    #[test]
    fn synthetic_steps_translate_autoload_to_autoload_tape() {
        let steps = synthetic_steps_from_cli(&ScriptInputs {
            autoload_tape: true,
            ..Default::default()
        });
        assert_eq!(
            steps,
            vec![ScriptStep::AutoloadTape {
                slot: DEFAULT_TAPE_SLOT.to_owned(),
                max_boot_frames: DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
            }]
        );
    }
}
