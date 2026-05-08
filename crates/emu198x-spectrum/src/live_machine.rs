//! Type-erased Spectrum runtime handle for the binary.
//!
//! The binary needs one variable that holds *whichever* Spectrum
//! variant the user is currently running, but each variant produces a
//! distinct concrete `SpectrumRuntime<M>` type. Rust's monomorphic match
//! arms can't merge the eight variants into one variable directly.
//!
//! [`LiveSpectrumRuntime`] solves this with a trait + a blanket impl
//! over `SpectrumRuntime<M: SpectrumMachine>`. Every method the binary
//! calls on the runtime appears once in the trait; the blanket impl
//! delegates uniformly. `Box<dyn LiveSpectrumRuntime>` becomes the
//! storage type. Adding a new variant doesn't touch this file — the
//! blanket picks it up automatically — except for the closed-set
//! [`build_runtime`] factory below, which is the *one* place the
//! `MachineKind → SpectrumRuntime<M>` mapping has to enumerate.
//!
//! Per-frame call sites (`time`, `run_until`, etc.) tolerate dyn-dispatch
//! cost happily — they're called once per frame, not per cycle.

// UI mode is the only consumer in this commit. Script mode will start
// using `LiveSpectrumRuntime` + `build_runtime` when SetMachine support
// lands; until then, headless builds see them as dead code.
#![cfg_attr(not(feature = "ui"), allow(dead_code))]

use std::time::Duration;

use common_sinclair_zx_spectrum::snapshot::Snapshot;
use emu198x_shell::{
    ControlCommand, FirmwareSet, HostIo, MachineCore, MachineError, MachineProfile, MachineTime,
    MediaSet, QueryError, QueryResult, ResetKind, RunResult,
};
use runtime_sinclair_zx_spectrum::{
    AudioControls, SpeakerChannel, Spectrum128kRuntime, Spectrum16kRuntime, Spectrum48kRuntime,
    SpectrumMachine, SpectrumPlus2ARuntime, SpectrumPlus2BRuntime, SpectrumPlus2Runtime,
    SpectrumPlus3Runtime, SpectrumPlusRuntime, SpectrumRuntime, SpectrumSessionQueryProvider,
};

use crate::machine::MachineKind;

/// Object-safe surface every Spectrum runtime exposes to the binary.
///
/// One method per real call site. Add new methods only when the binary
/// actually needs them — the trait is the binary's contract with the
/// runtime, not a mirror of the runtime's full surface.
pub trait LiveSpectrumRuntime {
    /// Runtime time in master half-cycles.
    fn time(&self) -> MachineTime;

    /// Advances the runtime up to `target` half-cycles, draining input
    /// events and emitting one frame + audio packet through `host`.
    fn run_until(
        &mut self,
        target: MachineTime,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError>;

    /// Issues one runtime control command (media transport, etc.).
    fn command(&mut self, command: &ControlCommand) -> Result<(), MachineError>;

    /// Hard- or soft-resets the runtime.
    fn reset(&mut self, kind: ResetKind);

    /// Loads media images into the runtime's slots.
    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError>;

    /// Returns whether the current variant has a disk slot named `slot`.
    /// Drives the File > Open Disk... menu's enabled state — only +3
    /// returns `true` today.
    fn supports_disk_slot(&self, slot: &str) -> bool;

    /// Serializes the current machine state for `State > Save Snapshot...`.
    fn snapshot_bytes(&self) -> Result<Vec<u8>, MachineError>;

    /// Restores a previously serialized machine state for
    /// `State > Load Snapshot...`.
    fn restore_snapshot(&mut self, bytes: &[u8]) -> Result<(), MachineError>;

    /// Applies a parsed `.sna` / `.z80` snapshot. The portable
    /// snapshot path — distinct from `restore_snapshot`, which decodes
    /// the runtime's own postcard save state.
    fn apply_snapshot(&mut self, snap: &Snapshot);

    /// Variant profile (display name, clock, capabilities).
    fn profile(&self) -> &MachineProfile;

    /// Resolves a query path through the runtime's session provider.
    fn query(&self, path: &str) -> Result<Option<QueryResult>, QueryError>;

    /// Authoritative frame length in master half-cycles. Differs across
    /// variants (48K-class 14 MHz vs 128K-class 17.7 MHz vs Timex
    /// 14.112 MHz / NTSC).
    fn frame_halfcycles(&self) -> u32;

    /// Wall-clock duration for one emulator frame, derived from the
    /// variant's master clock and frame length. This is what the
    /// binary's pacing loop budgets per frame.
    fn frame_duration(&self) -> Duration;

    /// Current host-side audio controls.
    fn audio_controls(&self) -> AudioControls;

    /// Replaces the host-side audio controls wholesale.
    fn set_audio_controls(&mut self, controls: AudioControls);

    /// Enables or disables one host-side audio channel.
    fn set_audio_channel_enabled(&mut self, channel: SpeakerChannel, enabled: bool);

    /// Sets the host-side gain for one audio channel.
    fn set_audio_channel_gain(&mut self, channel: SpeakerChannel, gain: f32);
}

impl<M: SpectrumMachine> LiveSpectrumRuntime for SpectrumRuntime<M> {
    fn time(&self) -> MachineTime {
        MachineCore::time(self)
    }

    fn run_until(
        &mut self,
        target: MachineTime,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        MachineCore::run_until(self, target, host)
    }

    fn command(&mut self, command: &ControlCommand) -> Result<(), MachineError> {
        MachineCore::command(self, command)
    }

    fn reset(&mut self, kind: ResetKind) {
        MachineCore::reset(self, kind);
    }

    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        MachineCore::load_media(self, media)
    }

    fn supports_disk_slot(&self, slot: &str) -> bool {
        SpectrumMachine::supports_disk_slot(SpectrumRuntime::machine(self), slot)
    }

    fn snapshot_bytes(&self) -> Result<Vec<u8>, MachineError> {
        MachineCore::snapshot(self)
    }

    fn restore_snapshot(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        MachineCore::restore(self, bytes)
    }

    fn apply_snapshot(&mut self, snap: &Snapshot) {
        SpectrumMachine::apply_snapshot(SpectrumRuntime::machine_mut(self), snap);
    }

    fn profile(&self) -> &MachineProfile {
        MachineCore::profile(self)
    }

    fn query(&self, path: &str) -> Result<Option<QueryResult>, QueryError> {
        use emu198x_shell::SessionQueryProvider;
        SpectrumSessionQueryProvider.query(self, path)
    }

    fn frame_halfcycles(&self) -> u32 {
        SpectrumRuntime::machine(self).frame_halfcycles()
    }

    fn frame_duration(&self) -> Duration {
        let halfcycles = u64::from(self.frame_halfcycles());
        let rate = &MachineCore::profile(self).clock.rate;
        let master_hz = rate.numerator_hz as f64 / rate.denominator_hz as f64;
        Duration::from_secs_f64(halfcycles as f64 / master_hz)
    }

    fn audio_controls(&self) -> AudioControls {
        SpectrumRuntime::audio_controls(self)
    }

    fn set_audio_controls(&mut self, controls: AudioControls) {
        SpectrumRuntime::set_audio_controls(self, controls);
    }

    fn set_audio_channel_enabled(&mut self, channel: SpeakerChannel, enabled: bool) {
        SpectrumRuntime::set_audio_channel_enabled(self, channel, enabled);
    }

    fn set_audio_channel_gain(&mut self, channel: SpeakerChannel, gain: f32) {
        SpectrumRuntime::set_audio_channel_gain(self, channel, gain);
    }
}

/// Constructs a fresh boxed runtime for the requested variant from the
/// supplied firmware set.
///
/// The match below is the *only* place in the binary that enumerates
/// the closed `MachineKind` set. Every other call site works through
/// the trait object — adding a new variant only requires extending
/// [`MachineKind`], adding a `from_firmware` constructor on the runtime
/// crate (already there for the SOLID 8), and adding one match arm
/// here.
///
/// # Errors
///
/// Returns `MachineError` if the firmware set is missing any required
/// ROM, has unknown firmware, or contains an invalid ROM image.
pub fn build_runtime(
    kind: MachineKind,
    firmware: &FirmwareSet<'_>,
) -> Result<Box<dyn LiveSpectrumRuntime>, MachineError> {
    Ok(match kind {
        MachineKind::Spectrum16K => Box::new(Spectrum16kRuntime::from_firmware(firmware)?),
        MachineKind::Spectrum48K => Box::new(Spectrum48kRuntime::from_firmware(firmware)?),
        MachineKind::SpectrumPlus => Box::new(SpectrumPlusRuntime::from_firmware(firmware)?),
        MachineKind::Spectrum128K => Box::new(Spectrum128kRuntime::from_firmware(firmware)?),
        MachineKind::SpectrumPlus2 => Box::new(SpectrumPlus2Runtime::from_firmware(firmware)?),
        MachineKind::SpectrumPlus2A => Box::new(SpectrumPlus2ARuntime::from_firmware(firmware)?),
        MachineKind::SpectrumPlus2B => Box::new(SpectrumPlus2BRuntime::from_firmware(firmware)?),
        MachineKind::SpectrumPlus3 => Box::new(SpectrumPlus3Runtime::from_firmware(firmware)?),
    })
}
