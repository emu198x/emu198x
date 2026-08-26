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
//! Addresses are `u32` so the surface spans the 8/16-bit machines (which use the
//! low 16 bits) and the 68000 family (24-bit bus, 32-bit PC) uniformly. CPU
//! register layout and disassembly are machine-specific, so they are returned as
//! JSON / an optional decoded line rather than a fixed shape.

use serde_json::Value;

/// One captured I/O port access, for the I/O trace on port-mapped
/// (Z80-family) machines.
#[derive(Debug, Clone, Copy)]
pub struct IoEvent {
    /// CPU program counter at the time of the access.
    pub pc: u32,
    /// I/O port — the **full 16-bit address bus**, not the low byte.
    ///
    /// A Z80 puts all sixteen bits on the bus during `IN`/`OUT`, and the
    /// machines that decode on the high byte are exactly the ones a
    /// truncated port cannot describe. The CPC selects its Gate Array at
    /// `$7F00` and its CRTC at `$BC00`; both have a low byte of zero, so
    /// an eight-bit trace reports every device as port 0. See #926.
    ///
    /// Machines that genuinely decode on eight bits may keep a `u8` in
    /// their own event type — the conversion widens it.
    pub port: u16,
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
/// Runtimes do not implement this directly — they implement [`DebugPrimitives`]
/// (the 8-bit machines via the `impl_*_debug_primitives!` macros below; the
/// Amiga family enum by hand) and a blanket impl turns that into `DebugTarget`.
/// Every such machine gets `register_common_tools`' debug verbs for free and
/// identically. Spectrum is the one remaining holdout on a hand-built MCP
/// surface — see `knowledge/decisions/debug-surface-tiers.md` (whose Amiga
/// deferral was overridden 2026-06-05 once the blanket-impl path proved out).
pub trait DebugTarget {
    /// Current CPU program counter.
    fn pc(&self) -> u32;

    /// Read one byte with no side effects (the debugger's view of the bus).
    fn peek(&self, addr: u32) -> u8;

    /// Write one byte to writable memory (ignored for ROM / I/O).
    fn poke(&mut self, addr: u32, value: u8);

    /// CPU register snapshot as JSON. The layout is CPU-specific (Z80 vs
    /// 6502 vs …), so each machine returns its own object.
    fn cpu_state(&self) -> Value;

    /// Monotonic count of CPU instruction boundaries, when stepping can
    /// legitimately stop before reaching one.
    ///
    /// Returning `None` is an exact-step guarantee: every call to
    /// [`step_instruction`](Self::step_instruction) completes one instruction.
    /// A bounded implementation that may return early must expose `Some` so
    /// generic script and MCP callers can distinguish consumed time from a
    /// completed instruction.
    fn instruction_boundary_count(&self) -> Option<u64> {
        None
    }

    /// Disassemble one instruction at `addr`, returning `(text, length)`.
    ///
    /// Returns `None` only for a target that has not overridden this method.
    /// Every wired CPU returns `Some`: Z80 via `emu198x_zilog_z80::disassemble`; the 6502
    /// family and the 6809 via the Asm198x `isa_disasm` spec disassembler
    /// (`decode_one_6502` / `decode_one_6809`).
    fn disassemble(&self, _addr: u32) -> Option<(String, u8)> {
        None
    }

    /// Attempt to run exactly one whole CPU instruction, returning the number
    /// of *complete* authoritative-clock ticks consumed. A processor clocked
    /// faster than the authoritative machine clock can stop part-way through
    /// one tick and legitimately return zero; that partial progress remains
    /// in machine state and is completed by a later step or run operation.
    ///
    /// An implementation may impose a safety bound when a stopped or wedged
    /// processor cannot reach another instruction boundary. In that case it
    /// returns the ticks consumed up to the bound and must expose a monotonic
    /// [`instruction_boundary_count`](Self::instruction_boundary_count).
    ///
    /// Implementors must resync the runtime's derived state (framebuffer,
    /// clock, and audio phase) before returning.
    fn step_instruction(&mut self) -> u64;

    /// Step one instruction *without* resyncing derived state.
    ///
    /// [`step_instruction`](Self::step_instruction) must leave the
    /// framebuffer, clock and audio phase consistent before returning, because
    /// its caller may read them. A loop that steps thousands of times before
    /// looking at anything pays that resync every iteration for no benefit: on
    /// the C64 it is a full RGBA framebuffer conversion per instruction, and
    /// measured at **34x the cost of the stepping itself** (#915).
    ///
    /// A caller using this **must** call [`resync`](Self::resync) before
    /// reading derived state or returning to anything that might. The default
    /// implementation is [`step_instruction`](Self::step_instruction), so a
    /// machine that has not opted in stays correct and merely stays slow.
    fn step_instruction_no_resync(&mut self) -> u64 {
        self.step_instruction()
    }

    /// Bring derived state (framebuffer, clock, audio phase) back in step
    /// after one or more [`step_instruction_no_resync`](Self::step_instruction_no_resync)
    /// calls. A no-op for machines that never deferred anything.
    fn resync(&mut self) {}

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

/// Machine-sourced debug primitives behind the shared [`DebugTarget`].
///
/// A runtime implements this — however it sources the values — and the blanket
/// impl below gives it [`DebugTarget`] for free. The 8-bit machines implement it
/// through the `impl_{6502,z80,6809}_debug_primitives!` macros (which key off a
/// `machine` field); runtimes that don't fit that shape — the Amiga family *enum*
/// behind `AmigaLiveAccess`, and eventually the generic `SpectrumRuntime<M>` —
/// implement it by hand. Either way they all land on the **same** `DebugTarget`
/// surface, which is the point: one debug tier, not a macro tier plus a bespoke
/// tier.
///
/// One blanket impl is the only `DebugTarget` impl in the workspace; every
/// machine reaches it through `DebugPrimitives`. (The orphan rule is what makes a
/// single blanket impl legal here: only a type's owning crate can implement
/// `DebugPrimitives` for it, so the compiler can rule out overlap with any
/// hand-written `DebugTarget` impl a flagship might still carry during a
/// migration.)
pub trait DebugPrimitives {
    /// See [`DebugTarget::pc`].
    fn dbg_pc(&self) -> u32;
    /// See [`DebugTarget::peek`].
    fn dbg_peek(&self, addr: u32) -> u8;
    /// See [`DebugTarget::poke`].
    fn dbg_poke(&mut self, addr: u32, value: u8);
    /// See [`DebugTarget::cpu_state`].
    fn dbg_cpu_state(&self) -> Value;
    /// See [`DebugTarget::instruction_boundary_count`].
    fn dbg_instruction_boundary_count(&self) -> Option<u64> {
        None
    }
    /// See [`DebugTarget::disassemble`].
    fn dbg_disassemble(&self, addr: u32) -> Option<(String, u8)>;
    /// See [`DebugTarget::step_instruction`].
    fn dbg_step(&mut self) -> u64;
    /// See [`DebugTarget::step_instruction_no_resync`].
    fn dbg_step_no_resync(&mut self) -> u64 {
        self.dbg_step()
    }
    /// See [`DebugTarget::resync`].
    fn dbg_resync(&mut self) {}
    /// See [`DebugTarget::supports_io_trace`].
    fn dbg_supports_io_trace(&self) -> bool {
        false
    }
    /// See [`DebugTarget::start_io_trace`].
    fn dbg_start_io_trace(&mut self) {}
    /// See [`DebugTarget::take_io_trace`].
    fn dbg_take_io_trace(&mut self) -> Vec<IoEvent> {
        Vec::new()
    }
}

impl<T: DebugPrimitives> DebugTarget for T {
    fn pc(&self) -> u32 {
        self.dbg_pc()
    }
    fn peek(&self, addr: u32) -> u8 {
        self.dbg_peek(addr)
    }
    fn poke(&mut self, addr: u32, value: u8) {
        self.dbg_poke(addr, value);
    }
    fn cpu_state(&self) -> Value {
        self.dbg_cpu_state()
    }
    fn instruction_boundary_count(&self) -> Option<u64> {
        self.dbg_instruction_boundary_count()
    }
    fn disassemble(&self, addr: u32) -> Option<(String, u8)> {
        self.dbg_disassemble(addr)
    }
    fn step_instruction_no_resync(&mut self) -> u64 {
        self.dbg_step_no_resync()
    }
    fn resync(&mut self) {
        self.dbg_resync();
    }
    fn step_instruction(&mut self) -> u64 {
        self.dbg_step()
    }
    fn supports_io_trace(&self) -> bool {
        self.dbg_supports_io_trace()
    }
    fn start_io_trace(&mut self) {
        self.dbg_start_io_trace();
    }
    fn take_io_trace(&mut self) -> Vec<IoEvent> {
        self.dbg_take_io_trace()
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

/// Implement [`DebugPrimitives`] for a Z80-family runtime; the shell's blanket
/// impl then provides [`DebugTarget`]. The machine `M` must expose
/// `cpu() -> &Z80`, `peek(u16) -> u8`, `poke(u16, u8)`,
/// `step_instruction() -> u64`, `start_io_trace()`, and `take_io_trace() ->
/// Vec<_>` (with public `pc`/`port`/`value`/`write` fields). The runtime must
/// have `time: MachineTime` and `update_rgba_framebuffer(&mut self)`. Requires
/// `serde_json` and `zilog-z80` in the runtime's dependencies.
///
/// Storage-agnostic like the 6502/6809 macros: the bare form serves a
/// lazily-built `machine: Option<M>`, the `direct` form an eager `machine: M`.
/// (No eager Z80 consumer today — present for parity.)
#[macro_export]
macro_rules! impl_z80_debug_primitives {
    ($runtime:ty) => {
        $crate::impl_z80_debug_primitives!(@impl $runtime,
            $crate::debug::opt_ref, $crate::debug::opt_mut);
    };
    ($runtime:ty, direct) => {
        $crate::impl_z80_debug_primitives!(@impl $runtime,
            $crate::debug::direct_ref, $crate::debug::direct_mut);
    };
    (@impl $runtime:ty, $get:path, $get_mut:path) => {
        impl $crate::DebugPrimitives for $runtime {
            fn dbg_pc(&self) -> u32 {
                $get(&self.machine).map_or(0, |m| u32::from(m.cpu().regs.pc))
            }
            fn dbg_peek(&self, addr: u32) -> u8 {
                $get(&self.machine).map_or(0xFF, |m| m.peek(addr as u16))
            }
            fn dbg_poke(&mut self, addr: u32, value: u8) {
                if let ::core::option::Option::Some(m) = $get_mut(&mut self.machine) {
                    m.poke(addr as u16, value);
                }
                self.update_rgba_framebuffer();
            }
            fn dbg_cpu_state(&self) -> ::serde_json::Value {
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
            fn dbg_disassemble(&self, addr: u32) -> Option<(String, u8)> {
                let m = $get(&self.machine)?;
                Some(::emu198x_zilog_z80::disassemble(addr as u16, |a| m.peek(a)))
            }
            fn dbg_step(&mut self) -> u64 {
                let ticks = self.dbg_step_no_resync();
                self.update_rgba_framebuffer();
                ticks
            }
            fn dbg_step_no_resync(&mut self) -> u64 {
                use ::emu198x_zilog_z80::Z80Stepper as _;
                let ticks = match $get_mut(&mut self.machine) {
                    ::core::option::Option::Some(m) => m.step_instruction(),
                    ::core::option::Option::None => return 0,
                };
                self.time = self.time.saturating_add(ticks);
                ticks
            }
            fn dbg_resync(&mut self) {
                self.update_rgba_framebuffer();
            }
            fn dbg_supports_io_trace(&self) -> bool {
                true
            }
            fn dbg_start_io_trace(&mut self) {
                if let ::core::option::Option::Some(m) = $get_mut(&mut self.machine) {
                    m.start_io_trace();
                }
            }
            fn dbg_take_io_trace(&mut self) -> Vec<$crate::IoEvent> {
                $get_mut(&mut self.machine).map_or_else(Vec::new, |m| {
                    m.take_io_trace()
                        .into_iter()
                        .map(|e| $crate::IoEvent {
                            pc: u32::from(e.pc),
                            port: e.port.into(),
                            value: e.value,
                            write: e.write,
                        })
                        .collect()
                })
            }
        }
    };
}

/// Implement [`DebugPrimitives`] for a 6502-family runtime; the shell's blanket
/// impl then provides [`DebugTarget`]. The machine `M` must expose
/// `cpu() -> &M6502` (with `.regs` `a`/`x`/`y`/`sp`/`pc`/`p`), `peek`,
/// `poke`, and `step_instruction`; the runtime must have `time: MachineTime` and
/// `update_rgba_framebuffer(&mut self)`. `disasm` decodes via the Asm198x
/// `isa_disasm` spec disassembler; I/O tracing is unsupported (memory-mapped).
///
/// Storage-agnostic: the bare form serves a lazily-built `machine: Option<M>`;
/// the `direct` form serves an eagerly-built `machine: M`.
///
/// ```ignore
/// emu198x_shell::impl_6502_debug_primitives!(PetRuntime);          // machine: Option<Pet>
/// emu198x_shell::impl_6502_debug_primitives!(C64Runtime, direct);  // machine: C64
/// ```
#[macro_export]
macro_rules! impl_6502_debug_primitives {
    ($runtime:ty) => {
        $crate::impl_6502_debug_primitives!(@impl $runtime,
            $crate::debug::opt_ref, $crate::debug::opt_mut);
    };
    ($runtime:ty, direct) => {
        $crate::impl_6502_debug_primitives!(@impl $runtime,
            $crate::debug::direct_ref, $crate::debug::direct_mut);
    };
    (@impl $runtime:ty, $get:path, $get_mut:path) => {
        impl $crate::DebugPrimitives for $runtime {
            fn dbg_pc(&self) -> u32 {
                $get(&self.machine).map_or(0, |m| u32::from(m.cpu().regs.pc))
            }
            fn dbg_peek(&self, addr: u32) -> u8 {
                $get(&self.machine).map_or(0xFF, |m| m.peek(addr as u16))
            }
            fn dbg_poke(&mut self, addr: u32, value: u8) {
                if let ::core::option::Option::Some(m) = $get_mut(&mut self.machine) {
                    m.poke(addr as u16, value);
                }
                self.update_rgba_framebuffer();
            }
            fn dbg_cpu_state(&self) -> ::serde_json::Value {
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
            fn dbg_step(&mut self) -> u64 {
                let ticks = self.dbg_step_no_resync();
                self.update_rgba_framebuffer();
                ticks
            }
            fn dbg_step_no_resync(&mut self) -> u64 {
                let ticks = match $get_mut(&mut self.machine) {
                    ::core::option::Option::Some(m) => m.step_instruction(),
                    ::core::option::Option::None => return 0,
                };
                self.time = self.time.saturating_add(ticks);
                ticks
            }
            fn dbg_resync(&mut self) {
                self.update_rgba_framebuffer();
            }
            fn dbg_disassemble(&self, addr: u32) -> Option<(String, u8)> {
                let m = $get(&self.machine)?;
                $crate::isa_disasm::decode_one_6502(addr as u16, |a| m.peek(a))
            }
        }
    };
}

/// Implement [`DebugPrimitives`] for a 6809 runtime (Dragon/CoCo); the shell's
/// blanket impl then provides [`DebugTarget`]. The machine `M` must expose
/// `cpu() -> &Mc6809` (with `.regs` `a`/`b`/`dp`/`cc`/`x`/`y`/`u`/
/// `s`/`pc`), `peek`, `poke`, and `step_instruction`; the runtime must have
/// `time: MachineTime` and `update_rgba_framebuffer(&mut self)`. `disasm`
/// decodes via the Asm198x `isa_disasm` spec disassembler; I/O tracing is
/// unsupported (memory-mapped). Storage-agnostic, like the 6502 macro.
#[macro_export]
macro_rules! impl_6809_debug_primitives {
    ($runtime:ty) => {
        $crate::impl_6809_debug_primitives!(@impl $runtime,
            $crate::debug::opt_ref, $crate::debug::opt_mut);
    };
    ($runtime:ty, direct) => {
        $crate::impl_6809_debug_primitives!(@impl $runtime,
            $crate::debug::direct_ref, $crate::debug::direct_mut);
    };
    (@impl $runtime:ty, $get:path, $get_mut:path) => {
        impl $crate::DebugPrimitives for $runtime {
            fn dbg_pc(&self) -> u32 {
                $get(&self.machine).map_or(0, |m| u32::from(m.cpu().regs.pc))
            }
            fn dbg_peek(&self, addr: u32) -> u8 {
                $get(&self.machine).map_or(0xFF, |m| m.peek(addr as u16))
            }
            fn dbg_poke(&mut self, addr: u32, value: u8) {
                if let ::core::option::Option::Some(m) = $get_mut(&mut self.machine) {
                    m.poke(addr as u16, value);
                }
                self.update_rgba_framebuffer();
            }
            fn dbg_cpu_state(&self) -> ::serde_json::Value {
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
            fn dbg_step(&mut self) -> u64 {
                let ticks = self.dbg_step_no_resync();
                self.update_rgba_framebuffer();
                ticks
            }
            fn dbg_step_no_resync(&mut self) -> u64 {
                let ticks = match $get_mut(&mut self.machine) {
                    ::core::option::Option::Some(m) => m.step_instruction(),
                    ::core::option::Option::None => return 0,
                };
                self.time = self.time.saturating_add(ticks);
                ticks
            }
            fn dbg_resync(&mut self) {
                self.update_rgba_framebuffer();
            }
            fn dbg_disassemble(&self, addr: u32) -> Option<(String, u8)> {
                let m = $get(&self.machine)?;
                $crate::isa_disasm::decode_one_6809(addr as u16, |a| m.peek(a))
            }
        }
    };
}

/// Implement [`DebugPrimitives`] for a Sharp LR35902 (Game Boy "SM83") runtime;
/// the shell's blanket impl then provides [`DebugTarget`]. The machine `M` must
/// expose `cpu() -> &Sm83` (with public `a`/`f`/`b`/`c`/`d`/`e`/`h`/`l`/`sp`/`pc`
/// and `ime`/`halt_mode`), `peek`, `poke`, and `step_instruction`; the runtime
/// must have `time: MachineTime` and `update_rgba_framebuffer(&mut self)`.
/// `disasm` decodes via `sharp_lr35902::disassemble`; I/O tracing is unsupported
/// (the Game Boy is memory-mapped). Requires `serde_json` and `sharp-lr35902` in
/// the runtime's dependencies.
///
/// Storage-agnostic: the bare form serves a lazily-built `machine: Option<M>`;
/// the `direct` form an eagerly-built `machine: M`.
///
/// ```ignore
/// emu198x_shell::impl_sm83_debug_primitives!(GameBoyRuntime); // machine: Option<GameBoy>
/// ```
#[macro_export]
macro_rules! impl_sm83_debug_primitives {
    ($runtime:ty) => {
        $crate::impl_sm83_debug_primitives!(@impl $runtime,
            $crate::debug::opt_ref, $crate::debug::opt_mut);
    };
    ($runtime:ty, direct) => {
        $crate::impl_sm83_debug_primitives!(@impl $runtime,
            $crate::debug::direct_ref, $crate::debug::direct_mut);
    };
    (@impl $runtime:ty, $get:path, $get_mut:path) => {
        impl $crate::DebugPrimitives for $runtime {
            fn dbg_pc(&self) -> u32 {
                $get(&self.machine).map_or(0, |m| u32::from(m.cpu().pc))
            }
            fn dbg_peek(&self, addr: u32) -> u8 {
                $get(&self.machine).map_or(0xFF, |m| m.peek(addr as u16))
            }
            fn dbg_poke(&mut self, addr: u32, value: u8) {
                if let ::core::option::Option::Some(m) = $get_mut(&mut self.machine) {
                    m.poke(addr as u16, value);
                }
                self.update_rgba_framebuffer();
            }
            fn dbg_cpu_state(&self) -> ::serde_json::Value {
                let ::core::option::Option::Some(m) = $get(&self.machine) else {
                    return ::serde_json::json!({});
                };
                let c = m.cpu();
                ::serde_json::json!({
                    "af": format!("${:04X}", (u16::from(c.a) << 8) | u16::from(c.f)),
                    "bc": format!("${:04X}", (u16::from(c.b) << 8) | u16::from(c.c)),
                    "de": format!("${:04X}", (u16::from(c.d) << 8) | u16::from(c.e)),
                    "hl": format!("${:04X}", (u16::from(c.h) << 8) | u16::from(c.l)),
                    "sp": format!("${:04X}", c.sp),
                    "pc": format!("${:04X}", c.pc),
                    "flags": {
                        "z": c.f & 0x80 != 0,
                        "n": c.f & 0x40 != 0,
                        "h": c.f & 0x20 != 0,
                        "c": c.f & 0x10 != 0,
                    },
                    "ime":  c.ime,
                    "halt": c.halt_mode,
                })
            }
            fn dbg_disassemble(&self, addr: u32) -> Option<(String, u8)> {
                let m = $get(&self.machine)?;
                Some(::sharp_lr35902::disassemble(addr as u16, |a| m.peek(a)))
            }
            fn dbg_step(&mut self) -> u64 {
                let ticks = self.dbg_step_no_resync();
                self.update_rgba_framebuffer();
                ticks
            }
            fn dbg_step_no_resync(&mut self) -> u64 {
                let ticks = match $get_mut(&mut self.machine) {
                    ::core::option::Option::Some(m) => m.step_instruction(),
                    ::core::option::Option::None => return 0,
                };
                self.time = self.time.saturating_add(ticks);
                ticks
            }
            fn dbg_resync(&mut self) {
                self.update_rgba_framebuffer();
            }
            // I/O tracing is unsupported — the Game Boy is memory-mapped.
        }
    };
}
