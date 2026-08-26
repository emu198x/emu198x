//! `DebugPrimitives` for the CPC runtime.
//!
//! Hand-written rather than `impl_z80_debug_primitives!`, which is a
//! historical shape rather than a constraint now — see below.
//!
//! This module used to leave I/O tracing unsupported. [`emu198x_shell::IoEvent`]
//! carried an 8-bit port, and the CPC decodes I/O on A15-A10 rather than the
//! low byte: `$7F00` is the Gate Array, `$BC00` the CRTC, `$F400`-`$F700` the
//! PPI, and every one of those has a low byte of zero. A trace narrowed to
//! `u8` would have reported every CPC device as port 0, so refusing said what
//! was true.
//!
//! #926 widened the shared port to the `u16` the bus actually carries, so the
//! trace is here and reports whole addresses.

use emu198x_shell::DebugPrimitives;
use emu198x_zilog_z80::Z80Stepper as _;
use machine_amstrad_cpc::AmstradCpc;
use serde_json::{Value, json};

use crate::runtime::AmstradCpcRuntime;

// CPC addresses are 16-bit; the shared debug surface is u32 to span the wider
// 68k/6502 buses. Truncation to u16 is the intended narrowing.
#[allow(clippy::cast_possible_truncation)]
impl DebugPrimitives for AmstradCpcRuntime {
    fn dbg_pc(&self) -> u32 {
        self.machine().map_or(0, |m| u32::from(m.cpu().regs.pc))
    }

    fn dbg_peek(&self, addr: u32) -> u8 {
        self.machine().map_or(0xFF, |m| m.peek(addr as u16))
    }

    fn dbg_poke(&mut self, addr: u32, value: u8) {
        if let Some(m) = self.machine.as_mut() {
            m.poke(addr as u16, value);
        }
        self.update_rgba_framebuffer();
    }

    fn dbg_cpu_state(&self) -> Value {
        let Some(m) = self.machine() else {
            return json!({});
        };
        let c = m.cpu();
        let r = &c.regs;
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
            "halt": c.halt,
        })
    }

    fn dbg_disassemble(&self, addr: u32) -> Option<(String, u8)> {
        let m = self.machine()?;
        Some(emu198x_zilog_z80::disassemble(addr as u16, |a| m.peek(a)))
    }

    fn dbg_step(&mut self) -> u64 {
        let ticks = self.dbg_step_no_resync();
        self.update_rgba_framebuffer();
        ticks
    }

    fn dbg_step_no_resync(&mut self) -> u64 {
        let ticks = match self.machine.as_mut() {
            Some(m) => AmstradCpc::step_instruction(m),
            None => return 0,
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
        if let Some(m) = self.machine.as_mut() {
            m.start_io_trace();
        }
    }

    fn dbg_take_io_trace(&mut self) -> Vec<emu198x_shell::IoEvent> {
        self.machine.as_mut().map_or_else(Vec::new, |m| {
            m.take_io_trace()
                .into_iter()
                .map(|e| emu198x_shell::IoEvent {
                    pc: u32::from(e.pc),
                    port: e.port,
                    value: e.value,
                    write: e.write,
                })
                .collect()
        })
    }
}
