//! Machine-agnostic debug access for the shared MCP debug tools.
//!
//! Each runtime implements [`DebugTarget`] (delegating to its concrete
//! machine, and resyncing its own derived state — framebuffer, clock —
//! after advancing). [`crate::MachineCore`] surfaces it through
//! [`MachineCore::debug_target`](crate::MachineCore::debug_target) so the
//! generic tools in [`crate::mcp_tools`] (`memory_read`, `poke_byte`,
//! `disasm`, `run_until_pc`, `step`, `io_trace`, …) work on any machine
//! without per-binary duplication.
//!
//! The surface is sized for the 8-bit machines (16-bit address bus). CPU
//! register layout and disassembly are machine-specific, so they are
//! returned as JSON / an optional decoded line rather than a fixed shape.

use serde_json::Value;

/// One captured I/O port access, for the I/O trace on port-mapped
/// (Z80-family) machines.
#[derive(Debug, Clone, Copy)]
pub struct IoEvent {
    /// CPU program counter at the time of the access.
    pub pc: u16,
    /// I/O port (low 8 bits of the address bus).
    pub port: u8,
    /// Byte written, or byte returned on a read.
    pub value: u8,
    /// `true` for an output (`OUT`), `false` for an input (`IN`).
    pub write: bool,
}

/// Debug access to a running machine, behind a trait so the shared MCP
/// tools stay machine-agnostic.
///
/// Implementors are the per-system runtimes. Advancing methods
/// ([`step_instruction`](DebugTarget::step_instruction)) must keep the
/// runtime's derived state current so a screenshot taken afterwards is
/// accurate.
pub trait DebugTarget {
    /// Current CPU program counter.
    fn pc(&self) -> u16;

    /// Read one byte with no side effects (the debugger's view of the bus).
    fn peek(&self, addr: u16) -> u8;

    /// Write one byte to writable memory (ignored for ROM / I/O).
    fn poke(&mut self, addr: u16, value: u8);

    /// CPU register snapshot as JSON. The layout is CPU-specific (Z80 vs
    /// 6502 vs …), so each machine returns its own object.
    fn cpu_state(&self) -> Value;

    /// Disassemble one instruction at `addr`, returning `(text, length)`.
    ///
    /// Returns `None` when no in-tree disassembler exists for this CPU yet
    /// — currently the 6502 family, pending the Asm198x spec crate. Z80
    /// machines return `Some` via `zilog_z80::disassemble`.
    fn disassemble(&self, _addr: u16) -> Option<(String, u8)> {
        None
    }

    /// Run exactly one whole CPU instruction, returning the number of
    /// authoritative-clock ticks consumed. Implementors must resync the
    /// runtime's derived state (framebuffer, clock) before returning.
    fn step_instruction(&mut self) -> u64;

    /// Whether this machine supports I/O port tracing (port-mapped
    /// Z80-family machines). Memory-mapped machines (6502 family) return
    /// `false`; debug them with `memory_read` / `disasm` / `run_until_pc`.
    fn supports_io_trace(&self) -> bool {
        false
    }

    /// Begin capturing I/O port accesses.
    fn start_io_trace(&mut self) {}

    /// Stop capturing and return the events recorded since
    /// [`start_io_trace`](DebugTarget::start_io_trace).
    fn take_io_trace(&mut self) -> Vec<IoEvent> {
        Vec::new()
    }
}
