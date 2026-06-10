//! Shared machine and profile types.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::capability::CapabilitySet;
use crate::control::ControlCommand;
use crate::error::MachineError;
use crate::firmware::FirmwareSet;
use crate::host::HostIo;
use crate::media::{FirmwareRequirement, MediaSet, MediaSlot};
use crate::time::{ClockDesc, MachineTime};

/// Stable machine-family identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Family {
    /// Sinclair ZX Spectrum-family machines.
    Spectrum,
    /// Commodore 64-family machines.
    C64,
    /// Nintendo Entertainment System and Famicom-family machines.
    Nes,
    /// Commodore Amiga-family machines.
    Amiga,
    /// Nintendo Game Boy-family machines (DMG, CGB, …).
    GameBoy,
    /// Dragon Data Dragon-family machines.
    Dragon,
    /// ASCII / Microsoft MSX-family machines.
    Msx,
    /// Any other or single-instance machine family without a dedicated
    /// variant yet. Use sparingly — promote to a named variant once a
    /// second machine in the same family appears.
    Other,
}

/// Region or video-standard family for a machine profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Region {
    /// PAL timing and regional defaults.
    Pal,
    /// NTSC timing and regional defaults.
    Ntsc,
    /// Any other region or timing family.
    Other,
}

/// Declared support tier for a machine profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SupportTier {
    /// Research assembled, implementation not yet functional.
    Research,
    /// Baseline boot or monitor path works.
    Boots,
    /// Representative software path works with known gaps.
    Usable,
    /// Control and teaching-facing workflows are stable.
    Teaching,
    /// Verification ladder is complete for the current scope.
    Reference,
}

/// Stable machine-family identifier such as `sinclair-zx-spectrum`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MachineId(pub Cow<'static, str>);

impl MachineId {
    /// Creates a machine identifier.
    #[must_use]
    pub fn new(id: impl Into<Cow<'static, str>>) -> Self {
        Self(id.into())
    }

    /// Returns the string form.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl From<&'static str> for MachineId {
    fn from(value: &'static str) -> Self {
        Self::new(value)
    }
}

/// Stable profile identifier such as `sinclair-zx-spectrum-48k-pal`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProfileId(pub Cow<'static, str>);

impl ProfileId {
    /// Creates a profile identifier.
    #[must_use]
    pub fn new(id: impl Into<Cow<'static, str>>) -> Self {
        Self(id.into())
    }

    /// Returns the string form.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl From<&'static str> for ProfileId {
    fn from(value: &'static str) -> Self {
        Self::new(value)
    }
}

/// Metadata for one concrete machine profile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineProfile {
    /// Stable machine-family identifier.
    pub machine_id: MachineId,
    /// Stable concrete profile identifier.
    pub profile_id: ProfileId,
    /// User-facing display name.
    pub display_name: Cow<'static, str>,
    /// High-level system family.
    pub family: Family,
    /// Region or timing family.
    pub region: Region,
    /// Declared implementation and verification tier.
    pub support_tier: SupportTier,
    /// First release year for the concrete profile.
    pub release_year: u16,
    /// Short human-readable summary.
    pub summary: Cow<'static, str>,
    /// Authoritative timing description for this profile.
    pub clock: ClockDesc,
    /// Required firmware descriptors.
    pub firmware: Vec<FirmwareRequirement>,
    /// Physical or host-visible media slots.
    pub media_slots: Vec<MediaSlot>,
    /// Declared capabilities.
    pub capabilities: CapabilitySet,
}

impl MachineProfile {
    // Intentionally no wide convenience constructor here.
    //
    // Machine profiles have enough fields that a giant positional constructor
    // becomes harder to read than a struct literal and tends to fight clippy.
}

/// Reset variants exposed by the shared control surface.
///
/// Serialised in lower-case on the wire (`"hard"`, `"soft"`) so it
/// can sit beside the snake-case [`ScriptStep`](crate::ScriptStep)
/// tag without surprising the script author.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "lowercase")]
pub enum ResetKind {
    /// A power-cycle equivalent reset. The default when a `reset` omits
    /// its `kind` (matches the historical per-binary behaviour).
    #[default]
    Hard,
    /// A machine-local soft reset.
    Soft,
}

/// Why execution stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum StopReason {
    /// Requested target time was reached.
    ReachedTarget,
    /// Machine halted waiting for external input or media.
    WaitingForInput,
    /// Machine reached a debugger break condition.
    Breakpoint,
    /// Machine entered a halted state.
    Halted,
}

/// Result of one shared execution request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunResult {
    /// Machine time reached by the execution request.
    pub reached: MachineTime,
    /// Why execution stopped.
    pub stop_reason: StopReason,
}

impl RunResult {
    /// Creates a `RunResult`.
    #[must_use]
    pub fn new(reached: MachineTime, stop_reason: StopReason) -> Self {
        Self {
            reached,
            stop_reason,
        }
    }
}

/// Narrow shared contract implemented by machine runtimes.
pub trait MachineCore {
    /// Returns the current machine profile.
    fn profile(&self) -> &MachineProfile;

    /// Returns the current authoritative machine time.
    fn time(&self) -> MachineTime;

    /// Resets the machine.
    fn reset(&mut self, kind: ResetKind);

    /// Loads one or more media images into the machine.
    ///
    /// # Errors
    ///
    /// Returns an error if the machine rejects the media set.
    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError>;

    /// Runs the machine until the requested target time.
    ///
    /// # Errors
    ///
    /// Returns an error if the host sinks reject emitted data.
    fn run_until(
        &mut self,
        target: MachineTime,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError>;

    /// Runs the machine for an exact number of sub-frame ticks, where
    /// one tick is one unit of the machine's authoritative clock (for
    /// the NES, one PPU dot / master clock). This enables cycle-exact
    /// stepping for debugging — advancing a few ticks at a time and
    /// querying state in between, which `run_until`'s frame granularity
    /// cannot express.
    ///
    /// The default implementation reports the operation as unsupported;
    /// runtimes whose core exposes a single-tick step override it.
    ///
    /// # Errors
    ///
    /// Returns an error if the runtime does not support sub-frame
    /// stepping, or if a host-side sink rejects emitted data.
    fn run_ticks(
        &mut self,
        _ticks: u64,
        _host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        Err(MachineError::UnsupportedOperation {
            operation: "run_ticks",
        })
    }

    /// Serializes a machine snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if snapshot generation fails.
    fn snapshot(&self) -> Result<Vec<u8>, MachineError>;

    /// Restores a machine snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if snapshot validation or decoding fails.
    fn restore(&mut self, bytes: &[u8]) -> Result<(), MachineError>;

    /// Applies one host-side control command.
    ///
    /// # Errors
    ///
    /// Returns an error if the machine does not support the command or if the
    /// command is invalid for the current media/configuration state.
    fn command(&mut self, command: &ControlCommand) -> Result<(), MachineError> {
        Err(MachineError::UnsupportedOperation {
            operation: command.operation_name(),
        })
    }

    /// Returns the currently available capability set.
    fn capabilities(&self) -> CapabilitySet;

    /// Returns a debug view of the running machine, if one is available.
    ///
    /// The default returns `None` (no debug surface). Runtimes that wrap a
    /// live machine override this to expose [`crate::debug::DebugTarget`],
    /// which powers the shared MCP debug tools (`memory_read`, `disasm`,
    /// `run_until_pc`, `step`, …) registered by
    /// [`crate::mcp_tools::register_debug_tools`].
    fn debug_target(&self) -> Option<&dyn crate::debug::DebugTarget> {
        None
    }

    /// Mutable counterpart of [`debug_target`](MachineCore::debug_target),
    /// for the tools that advance or modify the machine.
    fn debug_target_mut(&mut self) -> Option<&mut dyn crate::debug::DebugTarget> {
        None
    }
}

/// A runtime that is one of a system family's machine *variants* —
/// constructible by model and able to report its own native frame pacing.
///
/// This is the variant-dispatch *shape*, lifted to the shell so every
/// multi-variant family (the Spectrum's 13 models, the Amiga's OCS / ECS /
/// AGA, a future C128 alongside the C64, NTSC/PAL splits) implements it once
/// instead of re-hand-rolling the build-and-swap plumbing. A family's
/// runtime-dispatch enum (`SpectrumRuntimeKind`, `AmigaRuntimeKind`)
/// implements it; [`HeadlessSession::swap_machine`] then drives the swap
/// generically for both the MCP server and the `--script` runner.
///
/// It sits strictly *above* [`MachineCore`] (a supertrait bound) and never
/// touches the run loop — the active variant's `run_until` is reached by
/// ordinary dispatch — so the per-system run-loop boundary is unaffected.
pub trait FamilyRuntime: MachineCore + Sized {
    /// The family's model selector (its `Model` enum).
    type Model: Copy;

    /// Build the variant identified by `model` from already-loaded ROMs.
    ///
    /// # Errors
    ///
    /// Returns [`MachineError`] when the firmware is missing or invalid for
    /// the requested model.
    fn from_firmware(model: Self::Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError>;

    /// Native master-clock ticks per video frame for the active variant —
    /// the value to feed [`crate::HeadlessSession::set_native_frame_ticks`]
    /// so the session paces one native frame per `run_frames` call.
    fn native_frame_ticks(&self) -> u64;
}
