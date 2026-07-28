//! Shared-tier debug surface for the Amiga (68000 / 68020 family).
//!
//! The Amiga MCP runs on the family enum [`AmigaRuntimeKind`], not the
//! `machine: Option<M>` struct the `impl_*_debug_target!` macros assume, so it
//! implements [`DebugPrimitives`] by hand — delegating to the [`AmigaLiveAccess`]
//! adapter the enum already exposes — and gets [`emu198x_shell::DebugTarget`]
//! from the shell's blanket impl. That is what puts the Amiga on the **same**
//! debug surface as every other machine rather than a bespoke tier.
//!
//! Addresses are the full 24-bit bus (`DebugTarget` is `u32`-wide). The 68000 is
//! big-endian and the live-access surface exposes word reads, so a debugger byte
//! read folds onto the aligned word.

use emu198x_shell::DebugPrimitives;
use motorola_68000::disasm::disassemble as m68k_disassemble;
use serde_json::{Value, json};

use crate::live_access::AmigaLiveAccess;
use crate::variants::AmigaRuntimeKind;

/// Safety bound on a single-instruction step: one 680x0 instruction retires in
/// far fewer master/4 ticks than this, so it only guards against a stopped or
/// wedged CPU rather than capping normal stepping. If the bound is reached,
/// `dbg_step` returns the complete ticks consumed without claiming that an
/// instruction completed. The explicit debug-boundary counter lets shared
/// callers detect that outcome without interpreting CPU-specific JSON.
const STEP_TICK_LIMIT: u64 = 1_000_000;

impl DebugPrimitives for AmigaRuntimeKind {
    fn dbg_pc(&self) -> u32 {
        self.cpu_pc()
    }

    fn dbg_peek(&self, addr: u32) -> u8 {
        // 68000 is big-endian: the high byte of a word sits at the even (lower)
        // address. The live-access surface exposes word reads only, so fold a
        // byte read onto the aligned word.
        let word = self.read_word(addr & !1);
        if addr & 1 == 0 {
            (word >> 8) as u8
        } else {
            (word & 0xFF) as u8
        }
    }

    fn dbg_poke(&mut self, addr: u32, value: u8) {
        self.poke_byte(addr, value);
    }

    fn dbg_cpu_state(&self) -> Value {
        // Full 68k register file + exception state pushed down from the
        // old bespoke `query_cpu` MCP tool (#456): individual D0-D7 /
        // A0-A7 (A7 = the active stack pointer), the supervisor + IRQ
        // mask decode, VBR, and the in-flight exception / followup
        // bookkeeping. Hex strings keep this consistent with the rest of
        // the fleet's `DebugTarget` surface.
        let cpu = self.cpu_snapshot();
        let r = &cpu.regs;
        json!({
            "pc": format!("${:08X}", r.pc),
            "instr_start_pc": format!("${:08X}", cpu.instr_start_pc),
            "sr": format!("${:04X}", r.sr),
            "supervisor": r.is_supervisor(),
            "interrupt_mask": r.interrupt_mask(),
            "ssp": format!("${:08X}", r.ssp),
            "usp": format!("${:08X}", r.usp),
            "vbr": format!("${:08X}", r.vbr),
            "d0": format!("${:08X}", r.d[0]),
            "d1": format!("${:08X}", r.d[1]),
            "d2": format!("${:08X}", r.d[2]),
            "d3": format!("${:08X}", r.d[3]),
            "d4": format!("${:08X}", r.d[4]),
            "d5": format!("${:08X}", r.d[5]),
            "d6": format!("${:08X}", r.d[6]),
            "d7": format!("${:08X}", r.d[7]),
            "a0": format!("${:08X}", r.a(0)),
            "a1": format!("${:08X}", r.a(1)),
            "a2": format!("${:08X}", r.a(2)),
            "a3": format!("${:08X}", r.a(3)),
            "a4": format!("${:08X}", r.a(4)),
            "a5": format!("${:08X}", r.a(5)),
            "a6": format!("${:08X}", r.a(6)),
            "a7": format!("${:08X}", r.a(7)),
            "ipl_pin": cpu.ipl,
            "interrupts_taken": cpu.interrupts_taken,
            "exc_vector": cpu.exc_vector,
            "in_followup": cpu.in_followup,
            "followup_tag": cpu.followup_tag,
            "instruction_starts": cpu.instruction_starts,
        })
    }

    fn dbg_instruction_boundary_count(&self) -> Option<u64> {
        Some(self.cpu_instruction_starts())
    }

    fn dbg_disassemble(&self, addr: u32) -> Option<(String, u8)> {
        Some(m68k_disassemble(addr, |a| self.dbg_peek(a)))
    }

    fn dbg_step(&mut self) -> u64 {
        self.step_cpu_instruction(STEP_TICK_LIMIT)
    }

    // io_trace defaults to unsupported: the 68000 is memory-mapped, so the
    // debugger uses memory_read / disasm / run_until_pc instead.
}
