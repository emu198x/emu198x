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
//! The MCP server registers the richer bespoke Spectrum debug tools
//! (`query_cpu` with the full Z80 file + decoded flags, etc.) AFTER
//! `register_debug_tools`, so they override the shared ones by name. This
//! impl therefore backstops the shared surface (and the `io_trace` tool)
//! rather than being the output the curriculum reads. `io_trace` is left
//! unsupported (the default) — the Spectrum exposes I/O through
//! `port_read` / `port_write` and the AY write-watch instead.

use emu198x_shell::DebugPrimitives;
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
        let r = self.z80_registers();
        json!({
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
            "halt": self.z80_halted(),
        })
    }

    fn dbg_disassemble(&self, addr: u32) -> Option<(String, u8)> {
        Some(zilog_z80::disassemble(addr as u16, |a| self.read_byte(a)))
    }

    fn dbg_step(&mut self) -> u64 {
        u64::from(self.step_instructions(1))
    }
}
