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
///
/// This is the **shared** debug tier — the 24 donor extractions plus C64 and
/// Dragon implement it (via the `impl_*_debug_target!` macros below) and so get
/// `register_common_tools` for free. Spectrum and Amiga are deliberately on a
/// **bespoke** tier: they implement no `DebugTarget` and hand-build a richer MCP
/// surface instead. That asymmetry is intentional, not cruft — see
/// `knowledge/decisions/debug-surface-tiers.md` before "tidying" them onto these
/// macros. (Amiga can't: it's 68000, and the macro family is 6502/Z80/6809.)
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
    /// Returns `None` only for a target that has not overridden this method.
    /// Every wired CPU returns `Some`: Z80 via `zilog_z80::disassemble`; the 6502
    /// family and the 6809 via the Asm198x `isa_disasm` spec disassembler
    /// (`decode_one_6502` / `decode_one_6809`).
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

// Storage normalisers used by the debug-target macros. They let one macro body
// serve both a lazily-built `machine: Option<M>` and an eagerly-built
// `machine: M`, by reducing both to `Option<&M>` / `Option<&mut M>`. Passed to
// the macros as a function *path* (never a `self`-bearing token, which would not
// survive macro hygiene), applied to `self.machine` inside the generated method.
#[doc(hidden)]
#[must_use]
pub fn opt_ref<T>(m: &Option<T>) -> Option<&T> {
    m.as_ref()
}
#[doc(hidden)]
pub fn opt_mut<T>(m: &mut Option<T>) -> Option<&mut T> {
    m.as_mut()
}
#[doc(hidden)]
#[must_use]
pub fn opt_present<T>(m: &Option<T>) -> bool {
    m.is_some()
}
#[doc(hidden)]
#[must_use]
pub fn direct_ref<T>(m: &T) -> Option<&T> {
    Some(m)
}
#[doc(hidden)]
pub fn direct_mut<T>(m: &mut T) -> Option<&mut T> {
    Some(m)
}
#[doc(hidden)]
#[must_use]
pub fn direct_present<T>(_m: &T) -> bool {
    true
}

/// Emit the [`MachineCore`](crate::MachineCore) `debug_target` /
/// `debug_target_mut` overrides. Invoke **inside** a runtime's
/// `impl MachineCore` block. The runtime must implement [`DebugTarget`]. The
/// bare form assumes a lazily-built `machine: Option<M>`; the `direct` form an
/// eagerly-built `machine: M`.
///
/// ```ignore
/// impl MachineCore for FooRuntime {
///     // … other methods …
///     emu198x_shell::debug_target_hooks!();
/// }
/// ```
#[macro_export]
macro_rules! debug_target_hooks {
    // Lazy machine (`machine: Option<M>`): the target exists once the machine
    // is constructed.
    () => {
        $crate::debug_target_hooks!(@impl $crate::debug::opt_present);
    };
    // Eager machine (`machine: M`, built at construction): always present.
    (direct) => {
        $crate::debug_target_hooks!(@impl $crate::debug::direct_present);
    };
    (@impl $present:path) => {
        fn debug_target(&self) -> ::core::option::Option<&dyn $crate::DebugTarget> {
            if $present(&self.machine) {
                ::core::option::Option::Some(self as &dyn $crate::DebugTarget)
            } else {
                ::core::option::Option::None
            }
        }
        fn debug_target_mut(&mut self) -> ::core::option::Option<&mut dyn $crate::DebugTarget> {
            if $present(&self.machine) {
                ::core::option::Option::Some(self as &mut dyn $crate::DebugTarget)
            } else {
                ::core::option::Option::None
            }
        }
    };
}

/// Implement [`DebugTarget`] for a Z80-family runtime. The machine `M` must
/// expose `cpu() -> &Z80`, `peek(u16) -> u8`, `poke(u16, u8)`,
/// `step_instruction() -> u64`, `start_io_trace()`, and `take_io_trace() ->
/// Vec<_>` (with public `pc`/`port`/`value`/`write` fields). The runtime must
/// have `time: MachineTime` and `update_rgba_framebuffer(&mut self)`. Requires
/// `serde_json` and `zilog-z80` in the runtime's dependencies.
///
/// Storage-agnostic like the 6502/6809 macros: the bare form serves a
/// lazily-built `machine: Option<M>`, the `direct` form an eager `machine: M`.
/// (No eager Z80 consumer today — present for parity.)
#[macro_export]
macro_rules! impl_z80_debug_target {
    ($runtime:ty) => {
        $crate::impl_z80_debug_target!(@impl $runtime,
            $crate::debug::opt_ref, $crate::debug::opt_mut);
    };
    ($runtime:ty, direct) => {
        $crate::impl_z80_debug_target!(@impl $runtime,
            $crate::debug::direct_ref, $crate::debug::direct_mut);
    };
    (@impl $runtime:ty, $get:path, $get_mut:path) => {
        impl $crate::DebugTarget for $runtime {
            fn pc(&self) -> u16 {
                $get(&self.machine).map_or(0, |m| m.cpu().regs.pc)
            }
            fn peek(&self, addr: u16) -> u8 {
                $get(&self.machine).map_or(0xFF, |m| m.peek(addr))
            }
            fn poke(&mut self, addr: u16, value: u8) {
                if let ::core::option::Option::Some(m) = $get_mut(&mut self.machine) {
                    m.poke(addr, value);
                }
                self.update_rgba_framebuffer();
            }
            fn cpu_state(&self) -> ::serde_json::Value {
                let ::core::option::Option::Some(m) = $get(&self.machine) else {
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
                let m = $get(&self.machine)?;
                Some(::zilog_z80::disassemble(addr, |a| m.peek(a)))
            }
            fn step_instruction(&mut self) -> u64 {
                use ::zilog_z80::Z80Stepper as _;
                let ticks = match $get_mut(&mut self.machine) {
                    ::core::option::Option::Some(m) => m.step_instruction(),
                    ::core::option::Option::None => return 0,
                };
                self.time = self.time.saturating_add(ticks);
                self.update_rgba_framebuffer();
                ticks
            }
            fn supports_io_trace(&self) -> bool {
                true
            }
            fn start_io_trace(&mut self) {
                if let ::core::option::Option::Some(m) = $get_mut(&mut self.machine) {
                    m.start_io_trace();
                }
            }
            fn take_io_trace(&mut self) -> Vec<$crate::IoEvent> {
                $get_mut(&mut self.machine).map_or_else(Vec::new, |m| {
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

/// Implement [`DebugTarget`] for a 6502-family runtime. The machine `M` must
/// expose `cpu() -> &M6502` (with `.regs` `a`/`x`/`y`/`sp`/`pc`/`p`), `peek`,
/// `poke`, and `step_instruction`; the runtime must have `time: MachineTime` and
/// `update_rgba_framebuffer(&mut self)`. `disasm` decodes via the Asm198x
/// `isa_disasm` spec disassembler; I/O tracing is unsupported (memory-mapped).
///
/// Storage-agnostic: the bare form serves a lazily-built `machine: Option<M>`;
/// the `direct` form serves an eagerly-built `machine: M`.
///
/// ```ignore
/// emu198x_shell::impl_6502_debug_target!(PetRuntime);          // machine: Option<Pet>
/// emu198x_shell::impl_6502_debug_target!(C64Runtime, direct);  // machine: C64
/// ```
#[macro_export]
macro_rules! impl_6502_debug_target {
    ($runtime:ty) => {
        $crate::impl_6502_debug_target!(@impl $runtime,
            $crate::debug::opt_ref, $crate::debug::opt_mut);
    };
    ($runtime:ty, direct) => {
        $crate::impl_6502_debug_target!(@impl $runtime,
            $crate::debug::direct_ref, $crate::debug::direct_mut);
    };
    (@impl $runtime:ty, $get:path, $get_mut:path) => {
        impl $crate::DebugTarget for $runtime {
            fn pc(&self) -> u16 {
                $get(&self.machine).map_or(0, |m| m.cpu().regs.pc)
            }
            fn peek(&self, addr: u16) -> u8 {
                $get(&self.machine).map_or(0xFF, |m| m.peek(addr))
            }
            fn poke(&mut self, addr: u16, value: u8) {
                if let ::core::option::Option::Some(m) = $get_mut(&mut self.machine) {
                    m.poke(addr, value);
                }
                self.update_rgba_framebuffer();
            }
            fn cpu_state(&self) -> ::serde_json::Value {
                let ::core::option::Option::Some(m) = $get(&self.machine) else {
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
                let ticks = match $get_mut(&mut self.machine) {
                    ::core::option::Option::Some(m) => m.step_instruction(),
                    ::core::option::Option::None => return 0,
                };
                self.time = self.time.saturating_add(ticks);
                self.update_rgba_framebuffer();
                ticks
            }
            fn disassemble(&self, addr: u16) -> Option<(String, u8)> {
                let m = $get(&self.machine)?;
                $crate::isa_disasm::decode_one_6502(addr, |a| m.peek(a))
            }
        }
    };
}

/// Implement [`DebugTarget`] for a 6809 runtime (Dragon/CoCo). The machine `M`
/// must expose `cpu() -> &Mc6809` (with `.regs` `a`/`b`/`dp`/`cc`/`x`/`y`/`u`/
/// `s`/`pc`), `peek`, `poke`, and `step_instruction`; the runtime must have
/// `time: MachineTime` and `update_rgba_framebuffer(&mut self)`. `disasm`
/// decodes via the Asm198x `isa_disasm` spec disassembler; I/O tracing is
/// unsupported (memory-mapped). Storage-agnostic, like the 6502 macro.
#[macro_export]
macro_rules! impl_6809_debug_target {
    ($runtime:ty) => {
        $crate::impl_6809_debug_target!(@impl $runtime,
            $crate::debug::opt_ref, $crate::debug::opt_mut);
    };
    ($runtime:ty, direct) => {
        $crate::impl_6809_debug_target!(@impl $runtime,
            $crate::debug::direct_ref, $crate::debug::direct_mut);
    };
    (@impl $runtime:ty, $get:path, $get_mut:path) => {
        impl $crate::DebugTarget for $runtime {
            fn pc(&self) -> u16 {
                $get(&self.machine).map_or(0, |m| m.cpu().regs.pc)
            }
            fn peek(&self, addr: u16) -> u8 {
                $get(&self.machine).map_or(0xFF, |m| m.peek(addr))
            }
            fn poke(&mut self, addr: u16, value: u8) {
                if let ::core::option::Option::Some(m) = $get_mut(&mut self.machine) {
                    m.poke(addr, value);
                }
                self.update_rgba_framebuffer();
            }
            fn cpu_state(&self) -> ::serde_json::Value {
                let ::core::option::Option::Some(m) = $get(&self.machine) else {
                    return ::serde_json::json!({});
                };
                let r = &m.cpu().regs;
                ::serde_json::json!({
                    "a":  format!("${:02X}", r.a),
                    "b":  format!("${:02X}", r.b),
                    "dp": format!("${:02X}", r.dp),
                    "cc": format!("${:02X}", r.cc),
                    "x":  format!("${:04X}", r.x),
                    "y":  format!("${:04X}", r.y),
                    "u":  format!("${:04X}", r.u),
                    "s":  format!("${:04X}", r.s),
                    "pc": format!("${:04X}", r.pc),
                })
            }
            fn step_instruction(&mut self) -> u64 {
                let ticks = match $get_mut(&mut self.machine) {
                    ::core::option::Option::Some(m) => m.step_instruction(),
                    ::core::option::Option::None => return 0,
                };
                self.time = self.time.saturating_add(ticks);
                self.update_rgba_framebuffer();
                ticks
            }
            fn disassemble(&self, addr: u16) -> Option<(String, u8)> {
                let m = $get(&self.machine)?;
                $crate::isa_disasm::decode_one_6809(addr, |a| m.peek(a))
            }
        }
    };
}
