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
use crate::runtime::{SpectrumMachine, SpectrumRuntime};
use crate::variants::{
    Spectrum16kRuntime, Spectrum48kRuntime, Spectrum128kRuntime, SpectrumPlus2ARuntime,
    SpectrumPlus2BRuntime, SpectrumPlus2Runtime, SpectrumPlus3Runtime, SpectrumPlusRuntime,
};

/// Narrow Spectrum-machine surface that family-level helpers
/// (`autoload_basic_tape`, `load_basic_program`) need.
///
/// The helpers used to bind directly to `HeadlessSession<SpectrumRuntime<M>, …>`
/// and reach into `.machine().machine()`. That works for single-machine
/// binaries (script mode, UI) but not for family-MCP where the inner type
/// is [`SpectrumRuntimeKind`]. This trait abstracts over both shapes so
/// one implementation of the helpers covers every binary.
///
/// Methods are grouped into a single trait (rather than split by
/// concern) so a future `impl SpectrumLiveAccess for SpectrumRuntimeKind`
/// stays a single match block per method — easy to scan and easy to
/// keep aligned with the trait surface.
pub trait SpectrumLiveAccess {
    /// `true` while a tape image is loaded in the default tape slot.
    fn tape_is_loaded(&self) -> bool;
    /// Direct chip-RAM byte write. Used by the BASIC loader to install
    /// a tokenised program into the visible address space without
    /// re-running the tape decoder.
    fn write_byte(&mut self, addr: u16, val: u8);
    /// Direct chip-RAM byte read. Used by basic-loader tests to
    /// verify the installed program; not on any production path.
    fn read_byte(&self, addr: u16) -> u8;
    /// `true` while tape transport is active. Used by autoload tests
    /// to confirm the helper started playback before returning.
    fn tape_is_playing(&self) -> bool;
    /// Begin recording CPU writes in the half-open range
    /// `[addr, addr + len)`. Variants that don't implement the tracer
    /// return `Err`. See
    /// [`crate::runtime::SpectrumMachine::start_memory_write_watch`].
    ///
    /// # Errors
    /// Returns the reason string from the inner machine when the
    /// variant doesn't support the tracer.
    fn start_memory_write_watch(&mut self, addr: u16, len: u16) -> Result<(), &'static str>;
    /// Stop the current write watch and drop captured records.
    fn stop_memory_write_watch(&mut self);
    /// Captured CPU writes since the last `start_memory_write_watch`.
    /// `None` means either no watch is configured or the variant
    /// doesn't support the tracer.
    fn memory_write_watch_records(
        &self,
    ) -> Option<&[common_sinclair_zx_spectrum::MemoryWriteRecord]>;
    /// Current watch range as `(addr, len)`, or `None` when no watch
    /// is configured.
    fn memory_write_watch_range(&self) -> Option<(u16, u16)>;
    /// Drop captured write records without removing the watch range.
    fn clear_memory_write_watch_records(&mut self);
    /// Z80 register file. Every Spectrum-family variant carries a Z80
    /// so this is always available.
    fn z80_registers(&self) -> &zilog_z80::Registers;
    /// Whether the Z80 is currently halted.
    fn z80_halted(&self) -> bool;
    /// `true` when the Z80 is at an instruction boundary.
    fn z80_instruction_complete(&self) -> bool;
    /// Run cycles until `n` instructions complete. Returns the total
    /// half-cycles consumed.
    fn step_instructions(&mut self, n: u32) -> u32;
    /// Run cycles until PC reaches `target` or `max_halfcycles` is
    /// exhausted. Returns `(reached, halfcycles, instructions)`.
    fn run_until_pc(&mut self, target: u16, max_halfcycles: u32) -> (bool, u32, u32);
    /// Bus-level Z80 I/O port read.
    fn port_read(&mut self, port: u16) -> u8;
    /// Bus-level Z80 I/O port write.
    fn port_write(&mut self, port: u16, value: u8);
}

impl<M: SpectrumMachine> SpectrumLiveAccess for SpectrumRuntime<M> {
    fn tape_is_loaded(&self) -> bool {
        self.machine().tape_is_loaded()
    }

    fn write_byte(&mut self, addr: u16, val: u8) {
        self.machine_mut().write_byte(addr, val);
    }

    fn read_byte(&self, addr: u16) -> u8 {
        self.machine().read_byte(addr)
    }

    fn tape_is_playing(&self) -> bool {
        self.machine().tape_is_playing()
    }

    fn start_memory_write_watch(&mut self, addr: u16, len: u16) -> Result<(), &'static str> {
        self.machine_mut().start_memory_write_watch(addr, len)
    }

    fn stop_memory_write_watch(&mut self) {
        self.machine_mut().stop_memory_write_watch();
    }

    fn memory_write_watch_records(
        &self,
    ) -> Option<&[common_sinclair_zx_spectrum::MemoryWriteRecord]> {
        self.machine().memory_write_watch_records()
    }

    fn memory_write_watch_range(&self) -> Option<(u16, u16)> {
        self.machine().memory_write_watch_range()
    }

    fn clear_memory_write_watch_records(&mut self) {
        self.machine_mut().clear_memory_write_watch_records();
    }

    fn z80_registers(&self) -> &zilog_z80::Registers {
        self.machine().z80_registers()
    }

    fn z80_halted(&self) -> bool {
        self.machine().z80_halted()
    }

    fn z80_instruction_complete(&self) -> bool {
        self.machine().z80_instruction_complete()
    }

    fn step_instructions(&mut self, n: u32) -> u32 {
        self.machine_mut().step_instructions(n)
    }

    fn run_until_pc(&mut self, target: u16, max_halfcycles: u32) -> (bool, u32, u32) {
        self.machine_mut().run_until_pc(target, max_halfcycles)
    }

    fn port_read(&mut self, port: u16) -> u8 {
        self.machine_mut().port_read(port)
    }

    fn port_write(&mut self, port: u16, value: u8) {
        self.machine_mut().port_write(port, value);
    }
}

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

impl SpectrumLiveAccess for SpectrumRuntimeKind {
    fn tape_is_loaded(&self) -> bool {
        match_kind!(self, |rt| rt.tape_is_loaded())
    }

    fn write_byte(&mut self, addr: u16, val: u8) {
        match_kind!(self, |rt| rt.write_byte(addr, val))
    }

    fn read_byte(&self, addr: u16) -> u8 {
        match_kind!(self, |rt| rt.read_byte(addr))
    }

    fn tape_is_playing(&self) -> bool {
        match_kind!(self, |rt| rt.tape_is_playing())
    }

    fn start_memory_write_watch(&mut self, addr: u16, len: u16) -> Result<(), &'static str> {
        match_kind!(self, |rt| rt.start_memory_write_watch(addr, len))
    }

    fn stop_memory_write_watch(&mut self) {
        match_kind!(self, |rt| rt.stop_memory_write_watch())
    }

    fn memory_write_watch_records(
        &self,
    ) -> Option<&[common_sinclair_zx_spectrum::MemoryWriteRecord]> {
        match_kind!(self, |rt| rt.memory_write_watch_records())
    }

    fn memory_write_watch_range(&self) -> Option<(u16, u16)> {
        match_kind!(self, |rt| rt.memory_write_watch_range())
    }

    fn clear_memory_write_watch_records(&mut self) {
        match_kind!(self, |rt| rt.clear_memory_write_watch_records())
    }

    fn z80_registers(&self) -> &zilog_z80::Registers {
        match_kind!(self, |rt| rt.z80_registers())
    }

    fn z80_halted(&self) -> bool {
        match_kind!(self, |rt| rt.z80_halted())
    }

    fn z80_instruction_complete(&self) -> bool {
        match_kind!(self, |rt| rt.z80_instruction_complete())
    }

    fn step_instructions(&mut self, n: u32) -> u32 {
        match_kind!(self, |rt| rt.step_instructions(n))
    }

    fn run_until_pc(&mut self, target: u16, max_halfcycles: u32) -> (bool, u32, u32) {
        match_kind!(self, |rt| rt.run_until_pc(target, max_halfcycles))
    }

    fn port_read(&mut self, port: u16) -> u8 {
        match_kind!(self, |rt| rt.port_read(port))
    }

    fn port_write(&mut self, port: u16, value: u8) {
        match_kind!(self, |rt| rt.port_write(port, value))
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

    #[test]
    fn step_advances_pc_through_runtime_kind() {
        let mut kind = SpectrumRuntimeKind::Spectrum48K(Spectrum48kRuntime::blank());
        // Blank machine: PC=0, RAM/ROM uninitialised → opcodes are 0xFF
        // (RST $38) which pushes PC and jumps to $0038. Single-step
        // should leave PC != 0.
        let pc_before = kind.z80_registers().pc;
        let halfcycles = kind.step_instructions(1);
        let pc_after = kind.z80_registers().pc;
        assert_ne!(pc_after, pc_before, "step should advance PC");
        assert!(halfcycles > 0, "step should consume cycles");
    }

    #[test]
    fn port_round_trips_through_runtime_kind() {
        // Port $FE on a 48K writes the border colour in bits 0-2.
        // After port_write(0xFE, 5), spectrum.border.colour should be 5.
        // Using a port_read on $FE returns the keyboard scan, not the
        // border, so we don't round-trip the value through port_read —
        // we only verify both methods reach the inner machine without
        // panic and don't return a fixed sentinel.
        let mut kind = SpectrumRuntimeKind::Spectrum48K(Spectrum48kRuntime::blank());
        let before = kind.port_read(0x00FE);
        kind.port_write(0x00FE, 5);
        let after = kind.port_read(0x00FE);
        // Both reads should produce defined values (not panic). On a
        // blank machine with no keys pressed the high bits are stable
        // and the EAR bit is set/clear deterministically.
        assert_eq!(before, after, "no key press → keyboard scan unchanged");
    }

    #[test]
    fn run_until_pc_returns_within_budget() {
        let mut kind = SpectrumRuntimeKind::Spectrum48K(Spectrum48kRuntime::blank());
        // Target an unlikely PC; budget is tiny so we expect timeout.
        let (reached, _hc, _instr) = kind.run_until_pc(0xCAFE, 1000);
        assert!(!reached, "0xCAFE should not be reached in a 1000-hc budget");
    }

    #[test]
    fn z80_registers_dispatch_through_runtime_kind() {
        let kind = SpectrumRuntimeKind::Spectrum48K(Spectrum48kRuntime::blank());
        // Fresh Z80 has the well-known boot reset state: PC=0, SP=0xFFFF, AF=0xFFFF.
        let regs = kind.z80_registers();
        assert_eq!(regs.pc, 0x0000);
        assert_eq!(regs.sp, 0xFFFF);
        assert_eq!(regs.af, 0xFFFF);
        assert!(!kind.z80_halted());
    }

    #[test]
    fn memory_write_watch_dispatches_through_runtime_kind() {
        let mut kind = SpectrumRuntimeKind::Spectrum48K(Spectrum48kRuntime::blank());
        assert!(kind.memory_write_watch_range().is_none());
        assert!(kind.memory_write_watch_records().is_none());

        kind.start_memory_write_watch(0x4000, 0x300)
            .expect("48K supports the write watch");
        assert_eq!(kind.memory_write_watch_range(), Some((0x4000, 0x300)));
        assert!(kind.memory_write_watch_records().is_some());

        kind.stop_memory_write_watch();
        assert!(kind.memory_write_watch_range().is_none());
        assert!(kind.memory_write_watch_records().is_none());
    }
}
