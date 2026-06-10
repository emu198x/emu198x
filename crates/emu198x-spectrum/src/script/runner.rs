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

use std::path::{Path, PathBuf};

use common_sinclair_zx_spectrum::snapshot::SnapshotModel;
use common_sinclair_zx_spectrum::timing::{TIMING_48K, TIMING_128K, TIMING_PLUS2A};
use emu198x_shell::{
    ControlCommand, FirmwareImage, FirmwareSet, HeadlessScript, HeadlessSession, MachineCore,
    MachineError, MediaImage, MediaKind, MediaSet, MediaTransportAction, MediaTransportCommand,
    ScriptError, ScriptObservation, ScriptStep, SessionQueryProvider, mcp::ToolError,
    read_firmware_asset, read_media_asset,
};
use runtime_sinclair_zx_spectrum::{
    DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, Spectrum48kRuntime, Spectrum128kRuntime, SpectrumMachine,
    SpectrumPlus2ARuntime, SpectrumPlus2Runtime, SpectrumPlus3Runtime, SpectrumRuntime,
    SpectrumSessionQueryProvider,
};
use serde::Serialize;

use crate::AppError;
use crate::machine::{MachineKind, rom_root, variant_rom_bundle};
use crate::mcp::tools::{
    dispatch_live_step, execute_autoload_tape, execute_load_basic_program, execute_press_key,
    execute_type_string,
};
use crate::portable_snapshot::{is_portable_snapshot_path, parse_portable_snapshot_at};

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

/// Runs the script. Pre-scans the script for the first portable
/// `LoadSnapshot` step; if it targets a 128K-family snapshot, boots
/// a 128K runtime instead of the default 48K and pre-applies the
/// snapshot. Otherwise eager-boots 48K and iterates as usual.
///
/// The pre-scan is what makes diagnostic flows like "drop a SkoolKit
/// `tap2sna.py` snapshot into our emulator and run it" work — the
/// `HeadlessSession` type is monomorphised per runtime, so we have to
/// pick the right concrete type before constructing the session.
pub fn run_script(inputs: ScriptInputs) -> Result<RunnerReport, AppError> {
    let json_script = match &inputs.script {
        Some(path) => Some(HeadlessScript::from_path(path).map_err(|err| {
            AppError::Io(std::io::Error::other(format!(
                "failed to load script {}: {err}",
                path.display()
            )))
        })?),
        None => None,
    };

    // Decide which runtime to boot. If the script's first
    // portable-snapshot LoadSnapshot points at a non-48K snapshot,
    // we have to start the session as the matching variant —
    // applying a snapshot to the wrong runtime would silently lose
    // the upper banks (128K-family) or paging state, and leave the
    // CPU executing against a hybrid memory map.
    let preload = detect_first_portable_snapshot(json_script.as_ref())?;
    if let Some(preload) = preload {
        match preload.model {
            SnapshotModel::Spectrum128K => {
                return run_script_for_variant(
                    inputs,
                    json_script,
                    preload,
                    boot_eager_variant::<Spectrum128kRuntime>(MachineKind::Spectrum128K)?,
                    TIMING_128K.halfcycles_per_frame,
                    "128K",
                );
            }
            SnapshotModel::SpectrumPlus2 => {
                return run_script_for_variant(
                    inputs,
                    json_script,
                    preload,
                    boot_eager_variant::<SpectrumPlus2Runtime>(MachineKind::SpectrumPlus2)?,
                    TIMING_128K.halfcycles_per_frame,
                    "+2",
                );
            }
            SnapshotModel::SpectrumPlus2A => {
                return run_script_for_variant(
                    inputs,
                    json_script,
                    preload,
                    boot_eager_variant::<SpectrumPlus2ARuntime>(MachineKind::SpectrumPlus2A)?,
                    TIMING_PLUS2A.halfcycles_per_frame,
                    "+2A",
                );
            }
            SnapshotModel::SpectrumPlus3 => {
                return run_script_for_variant(
                    inputs,
                    json_script,
                    preload,
                    boot_eager_variant::<SpectrumPlus3Runtime>(MachineKind::SpectrumPlus3)?,
                    TIMING_PLUS2A.halfcycles_per_frame,
                    "+3",
                );
            }
            SnapshotModel::Spectrum48K
            | SnapshotModel::Pentagon128
            | SnapshotModel::Scorpion256 => {
                // 48K snapshots flow through the existing eager-48K
                // path below. Pentagon / Scorpion: snapshot exists
                // but no script-side runtime wired yet — fall through
                // to 48K and let the (degraded) apply_snapshot run;
                // a later commit can add their boot helpers.
            }
        }
    }

    let runtime = boot_eager_48k()?;
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    let synthetic = synthetic_steps_from_cli(&inputs);

    let mut observations = Vec::new();

    // Convenience-flag steps run first, in CLI-order. The tape itself
    // has to be loaded into the session before MediaTransport /
    // AutoloadTape can act on it, so we keep the tape bytes in scope
    // for the duration of the call.
    let tape_bytes = match &inputs.tape {
        Some(path) => Some(read_media_asset(path, MediaKind::Tape).map_err(AppError::from)?),
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
        } => execute_autoload_tape(session, slot, *max_boot_frames)
            .map(Some)
            .map_err(map_tool_error),
        ScriptStep::LoadBasicProgram { path, run } => {
            execute_load_basic_program(session, path, *run)
                .map(Some)
                .map_err(map_tool_error)
        }
        ScriptStep::PressKey { key, hold_frames } => execute_press_key(session, key, *hold_frames)
            .map(Some)
            .map_err(map_tool_error),
        ScriptStep::TypeString {
            text,
            hold_frames,
            settle_frames,
        } => execute_type_string(session, text, *hold_frames, *settle_frames)
            .map(Some)
            .map_err(map_tool_error),
        ScriptStep::LoadSnapshot { path } if is_portable_snapshot_path(path) => {
            execute_load_portable_snapshot(session, path).map(|_| None)
        }
        // Inspection / debug / live-memory steps (memory, poke, ports, CPU/AY
        // queries, single-step, disassembly, watches) share one implementation
        // with MCP mode via `dispatch_live_step`, so the two can't drift. Only
        // steps it doesn't own fall through to the shell's generic executor.
        other => match dispatch_live_step(other, session) {
            Some(result) => result.map_err(map_tool_error),
            None => other.execute_collect(session).map_err(map_script_error),
        },
    }
}

/// Parses a portable `.sna` / `.z80` snapshot (or extracts one from a
/// `.zip` archive carrying a single matching file) and applies it to
/// the live machine. The UI-side equivalent lives in
/// `crates/emu198x-spectrum/src/ui/runner.rs::import_portable_snapshot_from_path`;
/// MCP shares the classifier + parser through
/// [`crate::portable_snapshot`].
fn execute_load_portable_snapshot(
    session: &mut HeadlessSession<Spectrum48kRuntime, SpectrumSessionQueryProvider>,
    path: &Path,
) -> Result<(), AppError> {
    if session.is_recording() {
        return Err(AppError::ScriptUnsupported {
            step: "load_snapshot",
            reason: format!(
                "cannot load portable snapshot {} while a video recording is in flight; \
                 stop the recording first",
                path.display()
            ),
        });
    }
    let snapshot = parse_portable_snapshot_at(path)?;
    SpectrumMachine::apply_snapshot(session.machine_mut().machine_mut(), &snapshot);
    Ok(())
}

/// Map a [`ToolError`] from a shared step helper into the script
/// runner's [`AppError`]. The keyboard-injection helpers
/// (`execute_press_key` / `execute_type_string`) and `dispatch_live_step`
/// are shared with MCP mode and report `ToolError`; script mode wraps
/// that as an I/O error, matching how the live-step arm already maps.
fn map_tool_error(err: ToolError) -> AppError {
    AppError::Io(std::io::Error::other(err.to_string()))
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
    let (id, path) = bundle
        .into_iter()
        .next()
        .ok_or_else(|| AppError::MissingRom {
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

/// Local boot trait that abstracts over the inherent `from_firmware`
/// constructor each Spectrum runtime exposes. The runtime crate doesn't
/// publish a public trait for this — the constructors are inherent
/// methods — so we adapt them here with a thin local trait so
/// `boot_eager_variant<R>` can stay generic over the chosen variant.
trait BootFromFirmware: Sized {
    fn from_firmware(firmware: &FirmwareSet<'_>) -> Result<Self, MachineError>;
}

impl BootFromFirmware for Spectrum128kRuntime {
    fn from_firmware(firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        Spectrum128kRuntime::from_firmware(firmware)
    }
}

impl BootFromFirmware for SpectrumPlus2Runtime {
    fn from_firmware(firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        SpectrumPlus2Runtime::from_firmware(firmware)
    }
}

impl BootFromFirmware for SpectrumPlus2ARuntime {
    fn from_firmware(firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        SpectrumPlus2ARuntime::from_firmware(firmware)
    }
}

impl BootFromFirmware for SpectrumPlus3Runtime {
    fn from_firmware(firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        SpectrumPlus3Runtime::from_firmware(firmware)
    }
}

/// Eager-boot a 128K-family runtime variant from the conventional ROM
/// path (`~/.emu198x/roms/sinclair-zx-spectrum-…`). Used when the
/// script's first portable `LoadSnapshot` step targets a 128K-family
/// snapshot — see `run_script_for_variant`.
fn boot_eager_variant<R: BootFromFirmware>(kind: MachineKind) -> Result<R, AppError> {
    let root = rom_root().ok_or_else(|| AppError::MissingRom {
        path: "$HOME unset; cannot locate ROM bundle".to_owned(),
    })?;
    let bundle = variant_rom_bundle(kind, &root);
    // Two-pass: read all ROM bytes first into a stable Vec<Vec<u8>>,
    // then push borrows into the FirmwareSet. Avoids the borrow-checker
    // conflict between mutating the holder and borrowing its entries.
    let mut rom_bytes: Vec<(String, Vec<u8>)> = Vec::new();
    for (id, path) in bundle {
        if !path.is_file() {
            return Err(AppError::MissingRom {
                path: path.display().to_string(),
            });
        }
        rom_bytes.push((id.to_string(), read_firmware_asset(&path)?.bytes.to_vec()));
    }
    let mut firmware = FirmwareSet::new();
    for (id, bytes) in &rom_bytes {
        firmware.push(FirmwareImage::new(id.clone(), bytes));
    }
    R::from_firmware(&firmware).map_err(AppError::from)
}

/// One pre-loaded portable snapshot — used by `run_script` to decide
/// which runtime to boot before constructing the `HeadlessSession`.
struct PreloadedSnapshot {
    snapshot: common_sinclair_zx_spectrum::snapshot::Snapshot,
    model: SnapshotModel,
}

/// Scans the script for the first portable `LoadSnapshot` step and
/// parses the referenced file. Returns `None` if the script has no
/// portable LoadSnapshot (or no script at all). Errors only on I/O
/// or parse failures.
fn detect_first_portable_snapshot(
    script: Option<&HeadlessScript>,
) -> Result<Option<PreloadedSnapshot>, AppError> {
    let Some(script) = script else {
        return Ok(None);
    };
    for step in &script.steps {
        if let ScriptStep::LoadSnapshot { path } = step
            && is_portable_snapshot_path(path)
        {
            let snapshot = parse_portable_snapshot_at(path)?;
            let model = snapshot.model;
            return Ok(Some(PreloadedSnapshot { snapshot, model }));
        }
    }
    Ok(None)
}

/// Runs the script against the given pre-booted variant runtime,
/// pre-applying the detected portable snapshot before iterating
/// remaining steps.
///
/// Generic over `M: SpectrumMachine` so any Spectrum variant whose
/// runtime is a `SpectrumRuntime<M>` plugs in unchanged. The
/// pre-applied `LoadSnapshot` step is skipped when iterating. CLI
/// convenience flags (tape / play-tape / autoload-tape) are
/// 48K-specific and rejected here with a clear error. `variant_label`
/// is woven into error messages so the user knows which runtime was
/// auto-selected.
fn run_script_for_variant<M>(
    inputs: ScriptInputs,
    script: Option<HeadlessScript>,
    preload: PreloadedSnapshot,
    runtime: SpectrumRuntime<M>,
    frame_halfcycles: u32,
    variant_label: &'static str,
) -> Result<RunnerReport, AppError>
where
    M: SpectrumMachine,
    SpectrumRuntime<M>: MachineCore,
    SpectrumSessionQueryProvider: SessionQueryProvider<SpectrumRuntime<M>>,
{
    if inputs.tape.is_some() || inputs.play_tape || inputs.autoload_tape {
        return Err(AppError::ScriptUnsupported {
            step: "tape convenience flag",
            reason: format!(
                "--tape / --play-tape / --autoload-tape are 48K-only convenience flags; \
                 they can't combine with a {variant_label} LoadSnapshot in the same script. \
                 Drop the flags and add explicit MediaTransport / AutoloadTape steps if you \
                 need tape interaction post-snapshot."
            ),
        });
    }

    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(frame_halfcycles),
        SpectrumSessionQueryProvider,
    );

    // Pre-apply the snapshot so subsequent steps see the loaded state.
    SpectrumMachine::apply_snapshot(session.machine_mut().machine_mut(), &preload.snapshot);

    let mut observations = Vec::new();
    if let Some(script) = &script {
        let mut snapshot_applied = false;
        for step in &script.steps {
            // Skip the LoadSnapshot we already applied. Match by
            // step kind + path so a second LoadSnapshot later in the
            // script still runs (a `restore_snapshot`-style postcard
            // path) — though the typical case is a single LoadSnapshot.
            if !snapshot_applied
                && let ScriptStep::LoadSnapshot { path } = step
                && is_portable_snapshot_path(path)
            {
                snapshot_applied = true;
                continue;
            }
            // The non-48K path doesn't honour the 48K-specific
            // interceptions (AutoloadTape / LoadBasicProgram) — those
            // helpers are tape-loader-specific and bound to
            // `Spectrum48kRuntime`. Everything else delegates through
            // the shell crate's generic dispatch.
            match step {
                ScriptStep::AutoloadTape { .. } | ScriptStep::LoadBasicProgram { .. } => {
                    return Err(AppError::ScriptUnsupported {
                        step: "48K-only step in non-48K snapshot mode",
                        reason: format!(
                            "{} is currently only implemented for the 48K runtime; \
                             the binary picked a {variant_label} runtime because the \
                             script's first LoadSnapshot targets a {variant_label} snapshot. \
                             Drop the {} step or move it to a separate 48K script.",
                            step_name(step),
                            step_name(step),
                        ),
                    });
                }
                ScriptStep::SetMachine { machine } => {
                    return Err(AppError::ScriptUnsupported {
                        step: "set_machine",
                        reason: format!(
                            "set_machine to '{machine}' not yet supported in script mode."
                        ),
                    });
                }
                other => {
                    if let Some(obs) = other
                        .execute_collect(&mut session)
                        .map_err(map_script_error)?
                    {
                        observations.push(obs);
                    }
                }
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

fn step_name(step: &ScriptStep) -> &'static str {
    match step {
        ScriptStep::AutoloadTape { .. } => "autoload_tape",
        ScriptStep::LoadBasicProgram { .. } => "load_basic_program",
        ScriptStep::SetMachine { .. } => "set_machine",
        _ => "unknown",
    }
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
    fn portable_snapshot_extensions_are_detected() {
        for ext in ["sna", "z80", "zip", "SNA", "Z80", "ZIP"] {
            let path = PathBuf::from(format!("/tmp/snap.{ext}"));
            assert!(
                is_portable_snapshot_path(&path),
                "expected portable-snapshot dispatch for .{ext} (got fallthrough)"
            );
        }
    }

    #[test]
    fn non_portable_extensions_fall_through_to_postcard() {
        for path in [
            PathBuf::from("/tmp/state.snap"),
            PathBuf::from("/tmp/state.bin"),
            PathBuf::from("/tmp/state"),
            PathBuf::from("/tmp/state.postcard"),
        ] {
            assert!(
                !is_portable_snapshot_path(&path),
                "expected postcard fallthrough for {path:?}"
            );
        }
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
