//! `SpectrumRuntimeKind` — runtime-time dispatch over every Spectrum
//! family variant.
//!
//! Mirrors the `AmigaRuntimeKind` pattern in `runtime-commodore-amiga`:
//! a one-of enum that wraps a concrete `SpectrumRuntime<M>` per variant
//! and implements [`emu198x_shell::MachineCore`] by forwarding to the
//! inner case. Used by family-level MCP sessions that need to swap the
//! active variant at runtime through the `set_machine` script step.
//!
//! ## When to use this vs the concrete runtimes
//!
//! - Use the concrete `Spectrum48kRuntime` / `Spectrum128kRuntime` /
//!   `…` type aliases when the binary is single-machine (script-mode
//!   verifier targeting one variant, the UI binary's eager boot path).
//! - Use [`SpectrumRuntimeKind`] when the binary needs to host any
//!   variant chosen at runtime — today the MCP server, eventually any
//!   harness that drives the `set_machine` step.
//!
//! ## Variant coverage
//!
//! Only the SOLID 8 (16K, 48K, +, 128K, +2, +2A, +2B, +3) — same closed
//! set the binary's `MachineKind` enum and `build_runtime` factory
//! already enumerate. Exotic variants (Pentagon, Scorpion, Timex) are
//! reachable through the concrete `…Runtime` aliases when needed; we
//! add them here once an MCP user needs runtime switching to one.

use emu198x_shell::{
    ControlCommand, FirmwareSet, HostIo, MachineCore, MachineError, MachineProfile, MachineTime,
    MediaSet, QueryError, QueryResult, ResetKind, RunResult, SessionQueryProvider,
};

use crate::queries::SpectrumSessionQueryProvider;
use crate::variants::{
    Spectrum16kRuntime, Spectrum48kRuntime, Spectrum128kRuntime, SpectrumPlus2ARuntime,
    SpectrumPlus2BRuntime, SpectrumPlus2Runtime, SpectrumPlus3Runtime, SpectrumPlusRuntime,
};

/// Runtime-time dispatch over the eight SOLID Spectrum variants.
///
/// Constructed by the host (typically the MCP server) — pass a fresh
/// concrete runtime in. Re-construct (don't mutate the variant in
/// place) to swap machines mid-session; the host clears session-side
/// state separately.
pub enum SpectrumRuntimeKind {
    /// ZX Spectrum 16K.
    Spectrum16K(Spectrum16kRuntime),
    /// ZX Spectrum 48K.
    Spectrum48K(Spectrum48kRuntime),
    /// ZX Spectrum+ (electrically identical to 48K; identity is in the
    /// profile).
    SpectrumPlus(SpectrumPlusRuntime),
    /// ZX Spectrum 128K.
    Spectrum128K(Spectrum128kRuntime),
    /// Sinclair-branded Amstrad-built grey +2.
    SpectrumPlus2(SpectrumPlus2Runtime),
    /// ZX Spectrum +2A.
    SpectrumPlus2A(SpectrumPlus2ARuntime),
    /// ZX Spectrum +2B.
    SpectrumPlus2B(SpectrumPlus2BRuntime),
    /// ZX Spectrum +3.
    SpectrumPlus3(SpectrumPlus3Runtime),
}

impl SpectrumRuntimeKind {
    /// Construct from a [`crate::Model`] and a firmware bundle. Each
    /// SOLID-8 variant gets its concrete `SpectrumRuntime<M>` built
    /// via `from_firmware`. Exotic variants (Pentagon, Scorpion,
    /// Timex) return [`MachineError::UnsupportedOperation`] — they're
    /// reachable through the concrete `Pentagon128Runtime` / `…`
    /// aliases when a single-machine binary needs them.
    ///
    /// # Errors
    /// Returns the underlying `MachineError` from the inner runtime
    /// constructor on firmware-resolution failure, or
    /// `UnsupportedOperation` for an exotic variant.
    pub fn from_firmware(
        model: crate::Model,
        firmware: &FirmwareSet<'_>,
    ) -> Result<Self, MachineError> {
        Ok(match model {
            crate::Model::Spectrum16KPal => {
                Self::Spectrum16K(Spectrum16kRuntime::from_firmware(firmware)?)
            }
            crate::Model::Spectrum48KPal => {
                Self::Spectrum48K(Spectrum48kRuntime::from_firmware(firmware)?)
            }
            crate::Model::SpectrumPlus => {
                Self::SpectrumPlus(SpectrumPlusRuntime::from_firmware(firmware)?)
            }
            crate::Model::Spectrum128KPal => {
                Self::Spectrum128K(Spectrum128kRuntime::from_firmware(firmware)?)
            }
            crate::Model::SpectrumPlus2 => {
                Self::SpectrumPlus2(SpectrumPlus2Runtime::from_firmware(firmware)?)
            }
            crate::Model::SpectrumPlus2A => {
                Self::SpectrumPlus2A(SpectrumPlus2ARuntime::from_firmware(firmware)?)
            }
            crate::Model::SpectrumPlus2B => {
                Self::SpectrumPlus2B(SpectrumPlus2BRuntime::from_firmware(firmware)?)
            }
            crate::Model::SpectrumPlus3 => {
                Self::SpectrumPlus3(SpectrumPlus3Runtime::from_firmware(firmware)?)
            }
            crate::Model::Pentagon128
            | crate::Model::ScorpionZS256
            | crate::Model::TimexTC2048
            | crate::Model::TimexTC2068
            | crate::Model::TimexTS2068 => {
                return Err(MachineError::UnsupportedOperation {
                    operation: "SpectrumRuntimeKind::from_firmware(<exotic variant>)",
                });
            }
        })
    }

    /// Master half-cycles per frame for the active variant. The 48K
    /// family runs at 14 MHz with 69888 cycles/frame; the 128K family
    /// runs at 14.16 MHz with 70908 cycles/frame; the +2A/+2B/+3
    /// family runs at 17.7 MHz with 70908 cycles/frame. Used by host
    /// schedulers (e.g. `HeadlessSession::new`) to pace one native
    /// frame at the right number of half-cycles.
    #[must_use]
    pub fn frame_halfcycles(&self) -> u32 {
        use common_sinclair_zx_spectrum::timing::{TIMING_48K, TIMING_128K, TIMING_PLUS2A};
        match self {
            Self::Spectrum16K(_) | Self::Spectrum48K(_) | Self::SpectrumPlus(_) => {
                TIMING_48K.halfcycles_per_frame
            }
            Self::Spectrum128K(_) | Self::SpectrumPlus2(_) => TIMING_128K.halfcycles_per_frame,
            Self::SpectrumPlus2A(_) | Self::SpectrumPlus2B(_) | Self::SpectrumPlus3(_) => {
                TIMING_PLUS2A.halfcycles_per_frame
            }
        }
    }

    /// Returns a mutable reference to the inner 48K runtime when this
    /// kind is `Spectrum48K`, otherwise `None`. Used by 48K-only
    /// helpers (`autoload_basic_tape`, `load_basic_program`) on the
    /// family-MCP path so they keep working when the active variant
    /// is 48K and gracefully error otherwise.
    pub fn as_48k_mut(&mut self) -> Option<&mut Spectrum48kRuntime> {
        if let Self::Spectrum48K(rt) = self {
            Some(rt)
        } else {
            None
        }
    }
}

/// Hand-rolled mini-dispatcher so each `MachineCore` /
/// `SessionQueryProvider` method body fits on one line. Keeps the trait
/// impls scannable instead of an 8-arm match per method.
macro_rules! match_kind {
    ($self:expr, |$rt:ident| $body:expr) => {
        match $self {
            SpectrumRuntimeKind::Spectrum16K($rt) => $body,
            SpectrumRuntimeKind::Spectrum48K($rt) => $body,
            SpectrumRuntimeKind::SpectrumPlus($rt) => $body,
            SpectrumRuntimeKind::Spectrum128K($rt) => $body,
            SpectrumRuntimeKind::SpectrumPlus2($rt) => $body,
            SpectrumRuntimeKind::SpectrumPlus2A($rt) => $body,
            SpectrumRuntimeKind::SpectrumPlus2B($rt) => $body,
            SpectrumRuntimeKind::SpectrumPlus3($rt) => $body,
        }
    };
}

impl MachineCore for SpectrumRuntimeKind {
    fn profile(&self) -> &MachineProfile {
        match_kind!(self, |rt| rt.profile())
    }

    fn time(&self) -> MachineTime {
        match_kind!(self, |rt| rt.time())
    }

    fn reset(&mut self, kind: ResetKind) {
        match_kind!(self, |rt| rt.reset(kind))
    }

    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        match_kind!(self, |rt| rt.load_media(media))
    }

    fn run_until(
        &mut self,
        target: MachineTime,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        match_kind!(self, |rt| rt.run_until(target, host))
    }

    fn snapshot(&self) -> Result<Vec<u8>, MachineError> {
        match_kind!(self, |rt| rt.snapshot())
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        match_kind!(self, |rt| rt.restore(bytes))
    }

    fn command(&mut self, command: &ControlCommand) -> Result<(), MachineError> {
        match_kind!(self, |rt| rt.command(command))
    }

    fn capabilities(&self) -> emu198x_shell::CapabilitySet {
        match_kind!(self, |rt| rt.capabilities())
    }
}

impl SessionQueryProvider<SpectrumRuntimeKind> for SpectrumSessionQueryProvider {
    fn query_paths(&self, runtime: &SpectrumRuntimeKind, prefix: Option<&str>) -> Vec<String> {
        match_kind!(runtime, |rt| self.query_paths(rt, prefix))
    }

    fn query(
        &self,
        runtime: &SpectrumRuntimeKind,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        match_kind!(runtime, |rt| self.query(rt, path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::{ResetKind, SessionQueryProvider};

    #[test]
    fn machine_core_dispatches_to_inner_variants() {
        let mut kind = SpectrumRuntimeKind::Spectrum48K(Spectrum48kRuntime::blank());
        // Sanity: profile id starts with the family slug.
        assert!(kind.profile().profile_id.as_str().contains("48k"));
        // Reset is a no-op-ish call but must not panic across the
        // dispatch boundary.
        kind.reset(ResetKind::Hard);
    }

    #[test]
    fn query_provider_dispatches_to_inner_variants() {
        let kind = SpectrumRuntimeKind::Spectrum48K(Spectrum48kRuntime::blank());
        let provider = SpectrumSessionQueryProvider;
        let paths = provider.query_paths(&kind, Some("spectrum.tape."));
        assert!(
            paths.iter().any(|p| p.starts_with("spectrum.tape.")),
            "expected at least one spectrum.tape.* path; got {paths:?}"
        );
    }
}
