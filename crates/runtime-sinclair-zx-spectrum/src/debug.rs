//! `DebugPrimitives` for the Spectrum family enum.
//!
//! Mirrors the Amiga: the `impl_z80_debug_primitives!` macro targets a
//! runtime *struct* with `machine` / `time` / `update_rgba_framebuffer`
//! fields, but [`SpectrumRuntimeKind`] is the family *enum*, so we
//! hand-write the impl delegating to the existing [`SpectrumLiveAccess`]
//! surface. The shell's blanket `impl<T: DebugPrimitives> DebugTarget for
//! T` then provides `DebugTarget`, which `MachineCore::debug_target`
//! exposes — putting the Spectrum onto the same debug tier as the rest of
//! the fleet.
//!
//! `dbg_cpu_state` here carries the full Z80 register file + decoded
//! flags, so the shared `register_debug_tools` `query_cpu` is the rich
//! curriculum surface — the bespoke `query_cpu` override was removed
//! (#456).
//!
//! `io_trace` was left unsupported here for a long time, on the grounds
//! that `port_read` / `port_write` and the AY write-watch covered the
//! ground. They do not cover the same ground: those sample a port when
//! *you* ask, and a trace records what the *program* did. On a machine
//! that decodes I/O on single address lines — bit 0 clear is the ULA, so
//! keyboard, border, speaker and tape all arrive at `$FE` and its even
//! mirrors — watching the traffic is often the only way to see which
//! device a write was aimed at. Wired 2026-08-25 (#1183).

use emu198x_shell::{DebugPrimitives, IoEvent};
use serde_json::{Value, json};

use crate::family_runtime::{SpectrumLiveAccess, SpectrumRuntimeKind};

// Spectrum addresses are 16-bit; the shared debug surface is u32 to span
// the wider 68k/6502 buses. Truncation to u16 is the intended narrowing.
#[allow(clippy::cast_possible_truncation)]
impl DebugPrimitives for SpectrumRuntimeKind {
    fn dbg_pc(&self) -> u32 {
        u32::from(self.z80_registers().pc)
    }

    fn dbg_peek(&self, addr: u32) -> u8 {
        self.read_byte(addr as u16)
    }

    fn dbg_poke(&mut self, addr: u32, value: u8) {
        self.write_byte(addr as u16, value);
    }

    fn dbg_cpu_state(&self) -> Value {
        // The full Z80 register file pushed down from the old bespoke
        // `query_cpu` MCP tool (#456): the main bank with its 8-bit
        // halves, the alternate bank, index + interrupt state, and the
        // decoded F flags. Hex strings keep this consistent with the
        // rest of the fleet's `DebugTarget` surface.
        let r = self.z80_registers();
        let f = r.f();
        json!({
            "pc": format!("${:04X}", r.pc),
            "sp": format!("${:04X}", r.sp),
            "i":  format!("${:02X}", r.i),
            "r":  format!("${:02X}", r.r),
            "af": format!("${:04X}", r.af),
            "a":  format!("${:02X}", r.a()),
            "f":  format!("${:02X}", f),
            "bc": format!("${:04X}", r.bc),
            "b":  format!("${:02X}", r.b()),
            "c":  format!("${:02X}", r.c()),
            "de": format!("${:04X}", r.de),
            "d":  format!("${:02X}", r.d()),
            "e":  format!("${:02X}", r.e()),
            "hl": format!("${:04X}", r.hl),
            "h":  format!("${:02X}", r.h()),
            "l":  format!("${:02X}", r.l()),
            "af_alt": format!("${:04X}", r.af_alt),
            "bc_alt": format!("${:04X}", r.bc_alt),
            "de_alt": format!("${:04X}", r.de_alt),
            "hl_alt": format!("${:04X}", r.hl_alt),
            "ix": format!("${:04X}", r.ix),
            "iy": format!("${:04X}", r.iy),
            "im":   r.im,
            "iff1": r.iff1,
            "iff2": r.iff2,
            "flags": {
                "s":  f & 0x80 != 0,
                "z":  f & 0x40 != 0,
                "f5": f & 0x20 != 0,
                "h":  f & 0x10 != 0,
                "f3": f & 0x08 != 0,
                "pv": f & 0x04 != 0,
                "n":  f & 0x02 != 0,
                "c":  f & 0x01 != 0,
            },
            "halt": self.z80_halted(),
        })
    }

    fn dbg_disassemble(&self, addr: u32) -> Option<(String, u8)> {
        Some(emu198x_zilog_z80::disassemble(addr as u16, |a| {
            self.read_byte(a)
        }))
    }

    fn dbg_step(&mut self) -> u64 {
        u64::from(self.step_instructions(1))
    }

    fn dbg_supports_io_trace(&self) -> bool {
        // Asked per variant rather than answered for the family, so a
        // variant added later cannot inherit a claim it does not honour.
        // Every current variant traces.
        SpectrumLiveAccess::supports_io_trace(self)
    }

    fn dbg_start_io_trace(&mut self) {
        SpectrumLiveAccess::start_io_trace(self);
    }

    fn dbg_take_io_trace(&mut self) -> Vec<IoEvent> {
        SpectrumLiveAccess::take_io_trace(self)
            .into_iter()
            .map(|e| IoEvent {
                pc: u32::from(e.pc),
                // The shared event keeps the full sixteen-bit bus, which
                // is what distinguishes the AY's select port from its
                // data port — both have a low byte of $FD.
                port: e.port,
                value: e.value,
                write: e.write,
            })
            .collect()
    }
}
