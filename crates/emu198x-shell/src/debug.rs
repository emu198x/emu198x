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
    /// Returns `None` when no disassembler is wired for this CPU yet — currently
    /// the 6809 family (its debug target has no disassemble hook). Z80 machines
    /// return `Some` via `zilog_z80::disassemble`; the 6502 family via the
    /// Asm198x `isa_disasm::decode_one_6502` spec disassembler.
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

/// Emit the [`MachineCore`](crate::MachineCore) `debug_target` /
/// `debug_target_mut` overrides. Invoke **inside** a runtime's
/// `impl MachineCore` block. Assumes the runtime has a `machine:
/// Option<_>` field and implements [`DebugTarget`].
///
/// ```ignore
/// impl MachineCore for FooRuntime {
///     // … other methods …
///     emu198x_shell::debug_target_hooks!();
/// }
/// ```
#[macro_export]
macro_rules! debug_target_hooks {
    () => {
        fn debug_target(&self) -> ::core::option::Option<&dyn $crate::DebugTarget> {
            self.machine
                .as_ref()
                .map(|_| self as &dyn $crate::DebugTarget)
        }
        fn debug_target_mut(&mut self) -> ::core::option::Option<&mut dyn $crate::DebugTarget> {
            if self.machine.is_some() {
                ::core::option::Option::Some(self as &mut dyn $crate::DebugTarget)
            } else {
                ::core::option::Option::None
            }
        }
    };
}

/// Implement [`DebugTarget`] for a Z80-family runtime by delegating to its
/// `machine: Option<M>` field. The machine `M` must expose `cpu() -> &Z80`,
/// `peek(u16) -> u8`, `poke(u16, u8)`, `step_instruction() -> u64`,
/// `start_io_trace()`, and `take_io_trace() -> Vec<_>` (with public
/// `pc`/`port`/`value`/`write` fields). The runtime must have `time:
/// MachineTime` and `update_rgba_framebuffer(&mut self)`. Requires
/// `serde_json` and `zilog-z80` in the runtime's dependencies.
#[macro_export]
macro_rules! impl_z80_debug_target {
    ($runtime:ty) => {
        impl $crate::DebugTarget for $runtime {
            fn pc(&self) -> u16 {
                self.machine.as_ref().map_or(0, |m| m.cpu().regs.pc)
            }
            fn peek(&self, addr: u16) -> u8 {
                self.machine.as_ref().map_or(0xFF, |m| m.peek(addr))
            }
            fn poke(&mut self, addr: u16, value: u8) {
                if let Some(m) = self.machine.as_mut() {
                    m.poke(addr, value);
                }
                self.update_rgba_framebuffer();
            }
            fn cpu_state(&self) -> ::serde_json::Value {
                let Some(m) = self.machine.as_ref() else {
                    return ::serde_json::json!({});
                };
                let c = m.cpu();
                let r = &c.regs;
                ::serde_json::json!({
                    "af": format!("${:04X}", r.af),
                    "bc": format!("${:04X}", r.bc),
                    "de": format!("${:04X}", r.de),
                    "hl": format!("${:04X}", r.hl),
                    "ix": format!("${:04X}", r.ix),
                    "iy": format!("${:04X}", r.iy),
                    "sp": format!("${:04X}", r.sp),
                    "pc": format!("${:04X}", r.pc),
                    "i":  format!("${:02X}", r.i),
                    "r":  format!("${:02X}", r.r),
                    "iff1": r.iff1,
                    "iff2": r.iff2,
                    "im":   r.im,
                    "halt": c.halt,
                })
            }
            fn disassemble(&self, addr: u16) -> Option<(String, u8)> {
                let m = self.machine.as_ref()?;
                Some(::zilog_z80::disassemble(addr, |a| m.peek(a)))
            }
            fn step_instruction(&mut self) -> u64 {
                use ::zilog_z80::Z80Stepper as _;
                let ticks = match self.machine.as_mut() {
                    Some(m) => m.step_instruction(),
                    None => return 0,
                };
                self.time = self.time.saturating_add(ticks);
                self.update_rgba_framebuffer();
                ticks
            }
            fn supports_io_trace(&self) -> bool {
                true
            }
            fn start_io_trace(&mut self) {
                if let Some(m) = self.machine.as_mut() {
                    m.start_io_trace();
                }
            }
            fn take_io_trace(&mut self) -> Vec<$crate::IoEvent> {
                self.machine.as_mut().map_or_else(Vec::new, |m| {
                    m.take_io_trace()
                        .into_iter()
                        .map(|e| $crate::IoEvent {
                            pc: e.pc,
                            port: e.port,
                            value: e.value,
                            write: e.write,
                        })
                        .collect()
                })
            }
        }
    };
}

/// Implement [`DebugTarget`] for a 6502-family runtime by delegating to its
/// `machine: Option<M>` field. The machine `M` must expose `cpu() -> &M6502`
/// (with `.regs` `a`/`x`/`y`/`sp`/`pc`/`p`), `peek`, `poke`, and
/// `step_instruction`. `disasm` decodes via the Asm198x `isa_disasm` spec
/// disassembler (`$crate::isa_disasm::decode_one_6502`); I/O tracing is
/// unsupported (memory-mapped CPU).
#[macro_export]
macro_rules! impl_6502_debug_target {
    ($runtime:ty) => {
        impl $crate::DebugTarget for $runtime {
            fn pc(&self) -> u16 {
                self.machine.as_ref().map_or(0, |m| m.cpu().regs.pc)
            }
            fn peek(&self, addr: u16) -> u8 {
                self.machine.as_ref().map_or(0xFF, |m| m.peek(addr))
            }
            fn poke(&mut self, addr: u16, value: u8) {
                if let Some(m) = self.machine.as_mut() {
                    m.poke(addr, value);
                }
                self.update_rgba_framebuffer();
            }
            fn cpu_state(&self) -> ::serde_json::Value {
                let Some(m) = self.machine.as_ref() else {
                    return ::serde_json::json!({});
                };
                let r = &m.cpu().regs;
                ::serde_json::json!({
                    "a":  format!("${:02X}", r.a),
                    "x":  format!("${:02X}", r.x),
                    "y":  format!("${:02X}", r.y),
                    "sp": format!("${:02X}", r.sp),
                    "pc": format!("${:04X}", r.pc),
                    "p":  format!("${:02X}", r.p),
                })
            }
            fn step_instruction(&mut self) -> u64 {
                let ticks = match self.machine.as_mut() {
                    Some(m) => m.step_instruction(),
                    None => return 0,
                };
                self.time = self.time.saturating_add(ticks);
                self.update_rgba_framebuffer();
                ticks
            }
            fn disassemble(&self, addr: u16) -> Option<(String, u8)> {
                let m = self.machine.as_ref()?;
                $crate::isa_disasm::decode_one_6502(addr, |a| m.peek(a))
            }
        }
    };
}
