//! `DebugPrimitives` for the CPC runtime.
//!
//! Hand-written rather than `impl_z80_debug_primitives!` for one reason: that
//! macro advertises I/O tracing unconditionally, and
//! [`emu198x_shell::IoEvent`] carries an 8-bit port. The CPC decodes I/O on
//! A15-A10 rather than the low byte — `$7F00` is the Gate Array and `$BC00`
//! the CRTC, and both have a low byte of zero — so a trace narrowed to `u8`
//! would report every device as port 0. Leaving the trace unsupported (the
//! trait's default) says what is true; the CPC's device state is reachable
//! through the `query` paths instead. The Spectrum's `debug.rs` takes the same
//! route for the same kind of reason.
//!
//! This is a deferral, not the intended end state: #926 widens the shared
//! `IoEvent` port to the `u16` the bus actually carries, after which this
//! module gains the trace and the ZX81 stops truncating its own. Until then
//! `io_trace` refuses rather than reporting every CPC device as port 0.

use emu198x_shell::DebugPrimitives;
use machine_amstrad_cpc::AmstradCpc;
use serde_json::{Value, json};
use zilog_z80::Z80Stepper as _;

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
        Some(zilog_z80::disassemble(addr as u16, |a| m.peek(a)))
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
}
