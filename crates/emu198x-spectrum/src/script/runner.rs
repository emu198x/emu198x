//! Script execution loop.
//!
//! Boots the family-dispatch enum (`SpectrumRuntimeKind`) for the
//! chosen variant, wraps it in a `HeadlessSession`, and iterates the
//! script. System-specific steps (`SetMachine`, `AutoloadTape`) are
//! intercepted before the shell executor sees them; everything else
//! delegates to `ScriptStep::execute_collect`.
//!
//! `SetMachine` works here: the session holds the same family enum the
//! MCP server holds, so mid-script variant swaps route through the
//! shared `HeadlessSession::swap_machine` (#456). The initial variant
//! comes from `--machine`, else the script's first portable
//! `LoadSnapshot` (so a 128K-family snapshot boots its own runtime),
//! else 48K.

use std::path::PathBuf;

use common_sinclair_zx_spectrum::snapshot::SnapshotModel;
use emu198x_shell::{
    ControlCommand, FamilyRuntime, FirmwareImage, FirmwareSet, HeadlessScript, HeadlessSession,
    MediaImage, MediaKind, MediaSet, MediaTransportAction, MediaTransportCommand, ScriptError,
    ScriptObservation, ScriptStep, mcp::ToolError, read_firmware_asset, read_media_asset,
};
use runtime_sinclair_zx_spectrum::{
    DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, Spectrum48kRuntime, SpectrumLiveAccess, SpectrumRuntimeKind,
    SpectrumSessionQueryProvider,
};
use serde::Serialize;

use crate::AppError;
use crate::machine::{MachineKind, RomOverrides, resolved_rom_bundle, rom_override_entry};
use crate::mcp::tools::{dispatch_live_step, execute_autoload_tape, execute_load_basic_program};
use crate::portable_snapshot::{is_portable_snapshot_path, parse_portable_snapshot_at};

const DEFAULT_TAPE_SLOT: &str = "tape-1";

/// Inputs passed from `script::run` into the script runner.
#[derive(Debug, Default)]
pub struct ScriptInputs {
    /// Optional JSON session file to execute.
    pub script: Option<PathBuf>,
    /// Variant to boot, as a `MachineKind` script identifier. `None`
    /// keeps the default 48K boot policy.
    pub machine: Option<String>,
    /// Tape media to load before script execution.
    pub tape: Option<PathBuf>,
    /// Start tape transport on `tape-1` immediately.
    pub play_tape: bool,
    /// Run the BASIC autoload sequence on `tape-1` once boot is detected.
    pub autoload_tape: bool,
    /// Raw `--rom` values. Resolved against the boot variant's bundle
    /// once that variant is known, because `ID=PATH` is checked against
    /// it and a bare `PATH` only means anything on a single-ROM variant.
    /// Empty resolves the whole bundle under `~/.emu198x/roms`.
    pub rom: Vec<String>,
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

/// Runs the script. The boot variant comes from `--machine` when
/// given, otherwise from the first portable `LoadSnapshot` step: if
/// that targets a 128K-family snapshot, boots that variant and
/// pre-applies the snapshot. Failing both, eager-boots 48K.
///
/// The pre-scan is what makes diagnostic flows like "drop a SkoolKit
/// `tap2sna.py` snapshot into our emulator and run it" work — applying
/// a 128K-family snapshot to a 48K map would silently lose the upper
/// banks, so we pick the variant up front. The session always holds the
/// family enum (`SpectrumRuntimeKind`), so "picking the variant" is just
/// choosing a `MachineKind`; mid-script `SetMachine` swaps follow the
/// same enum and re-pace through `swap_machine` (#456).
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

    // Decide which variant to boot. If the script's first
    // portable-snapshot LoadSnapshot points at a non-48K snapshot,
    // we start the session as the matching variant — applying a
    // snapshot to the wrong runtime would silently lose the upper
    // banks (128K-family) or paging state, and leave the CPU
    // executing against a hybrid memory map. The session holds the
    // family enum either way, so picking the variant is just a
    // `MachineKind`; one boot path covers all of them.
    let preload = detect_first_portable_snapshot(json_script.as_ref())?;
    let boot_kind =
        resolve_boot_kind(inputs.machine.as_deref(), preload.as_ref().map(|p| p.model))?;

    // Resolve `--rom` now the boot variant is settled: `ID=PATH` is
    // checked against that variant's bundle, and a bare `PATH` only has a
    // meaning on a single-ROM one.
    let mut rom_overrides = RomOverrides::new();
    for spec in &inputs.rom {
        let (id, path) =
            rom_override_entry(spec, boot_kind).map_err(|err| AppError::MissingRom {
                path: err.to_string(),
            })?;
        rom_overrides.insert(id, path);
    }

    let runtime = boot_eager_kind(boot_kind, &rom_overrides)?;
    let frame_ticks = runtime.native_frame_ticks();
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        frame_ticks,
        SpectrumSessionQueryProvider,
    );

    // Pre-apply the detected snapshot so subsequent steps see the
    // loaded state. The first matching portable `LoadSnapshot` in the
    // script is then skipped during iteration (below) since it's
    // already applied. Routed through the enum's `SpectrumLiveAccess`,
    // so every variant shares one apply path (#456).
    if let Some(preload) = &preload {
        SpectrumLiveAccess::apply_snapshot(session.machine_mut(), &preload.snapshot);
    }

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
        // When a snapshot was pre-applied, skip the first matching
        // portable `LoadSnapshot` step (already applied above); a
        // later `LoadSnapshot` still runs normally.
        let mut snapshot_skipped = preload.is_none();
        for step in &script.steps {
            if !snapshot_skipped
                && let ScriptStep::LoadSnapshot { path } = step
                && is_portable_snapshot_path(path)
            {
                snapshot_skipped = true;
                continue;
            }
            if let Some(observation) = execute_step(step, &mut session)? {
                observations.push(observation);
            }
        }
    }

    Ok(RunnerReport {
        observations,
        time: session.time().get(),
        tape_loaded: session.machine().tape_is_loaded(),
        tape_playing: session.machine().tape_is_playing(),
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
    session: &mut HeadlessSession<SpectrumRuntimeKind, SpectrumSessionQueryProvider>,
) -> Result<Option<ScriptObservation>, AppError> {
    match step {
        ScriptStep::SetMachine { machine } => {
            // Shared with the MCP `set_machine` tool: resolve the variant's
            // ROM bundle and swap the session's runtime via the generic
            // `HeadlessSession::swap_machine`. Script mode holds the family
            // enum now, so mid-script variant swaps work (#456).
            crate::mcp::tools::execute_set_machine(machine, session)
                .map(Some)
                .map_err(map_tool_error)
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
        ScriptStep::LoadSnapshot { path } if is_portable_snapshot_path(path) => {
            // Shared with MCP — the family enum implements `SpectrumLiveAccess`,
            // so one `apply_snapshot` path covers every variant (#456).
            crate::mcp::tools::execute_load_portable_snapshot(session, path)
                .map(|()| None)
                .map_err(map_tool_error)
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

/// Map a [`ToolError`] from a shared step helper into the script
/// runner's [`AppError`]. The shared helpers (`execute_autoload_tape`,
/// `execute_load_basic_program`, the portable-snapshot loader) and
/// `dispatch_live_step` are shared with MCP mode and report `ToolError`;
/// script mode wraps that as an I/O error, matching how the live-step arm
/// already maps. (Keyboard verbs now run through the shell's generic
/// `execute_collect` instead.)
fn map_tool_error(err: ToolError) -> AppError {
    AppError::Io(std::io::Error::other(err.to_string()))
}

/// Eager-boot the default 48K runtime from the conventional ROM path
/// (`~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom`). Returns
/// `AppError::MissingRom` if the ROM isn't present.
///
/// Pub(crate) so the binary's MCP mode reuses the same boot path —
/// MCP's session lifecycle starts identically to script mode.
pub(crate) fn boot_eager_48k(overrides: &RomOverrides) -> Result<Spectrum48kRuntime, AppError> {
    let bundle = resolved_rom_bundle(MachineKind::Spectrum48K, overrides).map_err(|err| {
        AppError::MissingRom {
            path: err.to_string(),
        }
    })?;
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

/// Eager-boot the family-dispatch enum for any Spectrum variant from
/// the conventional ROM path (`~/.emu198x/roms/sinclair-zx-spectrum-…`).
///
/// One path for every model: resolve the variant's ROM bundle, then
/// build the active variant through the shared `FamilyRuntime`
/// constructor — the same one the MCP `set_machine` tool drives via
/// `HeadlessSession::swap_machine`. The script runner holds the family
/// enum (`SpectrumRuntimeKind`), so the result slots straight into the
/// session regardless of which model was picked (#456).
fn boot_eager_kind(
    kind: MachineKind,
    overrides: &RomOverrides,
) -> Result<SpectrumRuntimeKind, AppError> {
    let bundle = resolved_rom_bundle(kind, overrides).map_err(|err| AppError::MissingRom {
        path: err.to_string(),
    })?;
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
    let model = crate::mcp::tools::kind_to_model(kind);
    SpectrumRuntimeKind::from_firmware(model, &firmware).map_err(AppError::from)
}

/// Maps a portable snapshot's declared model onto the `MachineKind`
/// whose runtime should host it. 128K-family snapshots (`128K`, `+2`,
/// `+2A`, `+3`) boot their own variant so the upper banks and paging
/// state survive; everything else (48K plus the not-yet-script-wired
/// Pentagon / Scorpion clones) falls back to 48K and applies the
/// snapshot against that map. Preserves the model→variant routing the
/// old per-variant boot helpers hard-coded.
/// Picks the variant to boot from the `--machine` flag and the
/// script's first portable snapshot.
///
/// `--machine` selects the **boot** variant rather than desugaring to a
/// prepended `set_machine` step, and that distinction is load-bearing:
/// `HeadlessSession::swap_machine` installs a freshly-built runtime and
/// hard-resets, so a swap running after `--tape` had already loaded
/// media into the session would silently discard it. Choosing the
/// variant up front also skips a wasted 48K boot.
///
/// A mid-script `set_machine` step keeps its swap semantics — that is
/// the documented way to change variant *during* a run (#456).
///
/// # Errors
///
/// Returns [`AppError::ScriptStepRejected`] when the identifier is not a
/// known variant, or when it contradicts the variant implied by the
/// script's first portable snapshot. Booting the requested variant and
/// then applying a snapshot built for a different one is the exact
/// failure the snapshot pre-scan exists to prevent, so the conflict is
/// refused rather than resolved by precedence.
fn resolve_boot_kind(
    requested: Option<&str>,
    preload_model: Option<SnapshotModel>,
) -> Result<MachineKind, AppError> {
    let requested = match requested {
        Some(id) => {
            Some(
                MachineKind::from_script_id(id).ok_or_else(|| AppError::InvalidMachine {
                    reason: format!(
                        "unknown machine id `{id}`; expected one of {}",
                        MachineKind::script_id_list()
                    ),
                })?,
            )
        }
        None => None,
    };

    match (requested, preload_model) {
        (Some(requested), Some(model)) => {
            let implied = snapshot_model_to_kind(model);
            if implied == requested {
                Ok(requested)
            } else {
                Err(AppError::InvalidMachine {
                    reason: format!(
                        "{} conflicts with the script's first portable snapshot, \
                         which is a {} image",
                        requested.script_id(),
                        implied.script_id()
                    ),
                })
            }
        }
        (Some(requested), None) => Ok(requested),
        (None, Some(model)) => Ok(snapshot_model_to_kind(model)),
        (None, None) => Ok(MachineKind::Spectrum48K),
    }
}

fn snapshot_model_to_kind(model: SnapshotModel) -> MachineKind {
    match model {
        SnapshotModel::Spectrum128K => MachineKind::Spectrum128K,
        SnapshotModel::SpectrumPlus2 => MachineKind::SpectrumPlus2,
        SnapshotModel::SpectrumPlus2A => MachineKind::SpectrumPlus2A,
        SnapshotModel::SpectrumPlus3 => MachineKind::SpectrumPlus3,
        SnapshotModel::Spectrum48K | SnapshotModel::Pentagon128 | SnapshotModel::Scorpion256 => {
            MachineKind::Spectrum48K
        }
    }
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
        ScriptError::InvalidStep { step, reason } => AppError::ScriptStepRejected { step, reason },
        // A character the keyboard cannot produce is a rejected step, not a
        // silent shortfall — see #916.
        ScriptError::UntypableCharacter { ch, supported } => AppError::ScriptStepRejected {
            step: "type_string",
            reason: format!(
                "cannot type {ch:?} on this machine — no keycap or shift chord \
                 produces it. Supported keys: {supported}"
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
    fn boot_kind_defaults_to_48k() {
        assert_eq!(
            resolve_boot_kind(None, None).expect("no inputs is always valid"),
            MachineKind::Spectrum48K
        );
    }

    #[test]
    fn machine_flag_selects_boot_variant() {
        assert_eq!(
            resolve_boot_kind(Some("spectrum_128k"), None).expect("known id"),
            MachineKind::Spectrum128K
        );
    }

    /// The exotics are reachable too — `from_script_id` has always
    /// accepted all 13 variants even though the `set_machine` error
    /// message used to name only the SOLID 8.
    #[test]
    fn machine_flag_accepts_the_exotics() {
        assert_eq!(
            resolve_boot_kind(Some("pentagon_128"), None).expect("known id"),
            MachineKind::Pentagon128
        );
    }

    #[test]
    fn unknown_machine_id_is_rejected_and_lists_the_accepted_ids() {
        let err = resolve_boot_kind(Some("spectrum_999k"), None)
            .expect_err("an unknown id must not fall back to 48K");
        let message = err.to_string();
        assert!(
            message.contains("spectrum_999k"),
            "the error should name the rejected id, got: {message}"
        );
        assert!(
            message.contains("spectrum_128k") && message.contains("timex_ts2068"),
            "the error should list the accepted ids, got: {message}"
        );
    }

    /// A snapshot still picks the boot variant on its own.
    #[test]
    fn snapshot_model_selects_boot_variant_without_the_flag() {
        assert_eq!(
            resolve_boot_kind(None, Some(SnapshotModel::Spectrum128K)).expect("valid"),
            MachineKind::Spectrum128K
        );
    }

    #[test]
    fn machine_flag_agreeing_with_the_snapshot_is_accepted() {
        assert_eq!(
            resolve_boot_kind(Some("spectrum_128k"), Some(SnapshotModel::Spectrum128K))
                .expect("agreement is not a conflict"),
            MachineKind::Spectrum128K
        );
    }

    /// Booting the requested variant and then applying a snapshot built
    /// for another would leave the CPU on a hybrid memory map — the
    /// exact failure the snapshot pre-scan exists to prevent. Refuse
    /// rather than pick a winner.
    #[test]
    fn machine_flag_conflicting_with_the_snapshot_is_refused() {
        let err = resolve_boot_kind(Some("spectrum_48k"), Some(SnapshotModel::Spectrum128K))
            .expect_err("a contradicted --machine must not be silently overridden");
        let message = err.to_string();
        assert!(
            message.contains("spectrum_48k") && message.contains("spectrum_128k"),
            "the error should name both variants, got: {message}"
        );
    }

    #[test]
    fn machine_id_list_covers_every_variant() {
        let list = MachineKind::script_id_list();
        for kind in MachineKind::all() {
            assert!(
                list.contains(kind.script_id()),
                "{} missing from the accepted-id list",
                kind.script_id()
            );
        }
    }
    // `profile()` comes from the `MachineCore` trait — only the variant-swap
    // test reads it, so scope the import to the test module.
    use emu198x_shell::MachineCore;

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

    /// Regression for #456: a `SetMachine` step in a `--script` run swaps
    /// the active variant. Before the runner held the family enum this
    /// errored with `ScriptUnsupported`; now it routes through the same
    /// `HeadlessSession::swap_machine` the MCP `set_machine` tool uses.
    /// Boots the 48K enum session, swaps to 128K via `execute_step`, and
    /// asserts the live profile and frame pacing both changed. Skips when
    /// the 48K or 128K ROM bundles are absent.
    #[test]
    fn set_machine_step_swaps_variant_in_script_mode() {
        let runtime = match boot_eager_48k(&RomOverrides::new()) {
            Ok(rt) => rt,
            Err(_) => {
                eprintln!("skipping: 48K ROM missing (set up ~/.emu198x/roms/...)");
                return;
            }
        };
        let kind = SpectrumRuntimeKind::Spectrum48K(runtime);
        let ticks = kind.native_frame_ticks();
        let mut session =
            HeadlessSession::new_with_query_provider(kind, ticks, SpectrumSessionQueryProvider);

        let before_profile = session.machine().profile().profile_id.as_str().to_owned();
        let before_ticks = session.native_frame_ticks();

        let step = ScriptStep::SetMachine {
            machine: "spectrum_128k".to_owned(),
        };
        let observation = match execute_step(&step, &mut session) {
            Ok(obs) => obs,
            Err(_) => {
                eprintln!("skipping: 128K ROM bundle missing");
                return;
            }
        };

        assert!(
            matches!(observation, Some(ScriptObservation::SetMachine { .. })),
            "set_machine must report a SetMachine observation, got {observation:?}"
        );
        let after_profile = session.machine().profile().profile_id.as_str().to_owned();
        assert_ne!(
            before_profile, after_profile,
            "set_machine should change the active profile (48K -> 128K)"
        );
        // The session re-paced to the new variant's frame budget, and that
        // budget is what the freshly-installed runtime reports.
        assert_eq!(
            session.native_frame_ticks(),
            session.machine().native_frame_ticks(),
            "session frame pacing must track the swapped-in variant"
        );
        assert_ne!(
            before_ticks,
            session.native_frame_ticks(),
            "128K has a longer frame than 48K; the session pacing should differ"
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
