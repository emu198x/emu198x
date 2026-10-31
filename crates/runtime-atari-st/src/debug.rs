//! Shared-tier debug surface for the Atari ST's 68000.
//!
//! The `impl_*_debug_primitives!` macros cover the 8-bit CPUs only, so this
//! implements [`DebugPrimitives`] by hand, as the Amiga and the Mega Drive
//! do, and gets [`emu198x_shell::DebugTarget`] from the shell's blanket impl.
//!
//! Addresses are the 68000's 24-bit bus. Reads see RAM and TOS without side
//! effects; I/O reads `$FF`. Stepping runs CPU cycles until the 68000 starts
//! its next instruction, so a step's ticks are CPU cycles, the runtime's
//! unit of time.

use emu198x_shell::DebugPrimitives;
use motorola_68000::disasm::disassemble as m68k_disassemble;
use serde_json::{Value, json};

use crate::runtime::AtariStRuntime;

/// Safety bound on a single-instruction step, in CPU cycles. The longest
/// 68000 instruction, a DIVS, takes under 200; this bound otherwise only
/// guards against a stopped CPU. If it is reached, `dbg_step` returns the
/// cycles consumed without an instruction having completed, which the
/// boundary counter shows.
const STEP_TICK_LIMIT: u64 = 200_000;

impl DebugPrimitives for AtariStRuntime {
    fn dbg_pc(&self) -> u32 {
        // The PC runs ahead of the instruction by the prefetch; the
        // debugger means the instruction itself.
        self.machine.as_ref().map_or(0, |m| m.cpu().instr_start_pc)
    }

    fn dbg_peek(&self, addr: u32) -> u8 {
        self.machine.as_ref().map_or(0xFF, |m| m.peek(addr))
    }

    fn dbg_poke(&mut self, addr: u32, value: u8) {
        if let Some(m) = self.machine.as_mut() {
            m.poke(addr, value);
        }
        self.update_rgba_framebuffer();
    }

    fn dbg_cpu_state(&self) -> Value {
        let Some(m) = self.machine.as_ref() else {
            return json!({});
        };
        let cpu = m.cpu();
        let r = &cpu.regs;
        json!({
            "pc": format!("${:08X}", r.pc),
            "instr_start_pc": format!("${:08X}", cpu.instr_start_pc),
            "sr": format!("${:04X}", r.sr),
            "supervisor": r.is_supervisor(),
            "interrupt_mask": r.interrupt_mask(),
            "ssp": format!("${:08X}", r.ssp),
            "usp": format!("${:08X}", r.usp),
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
            "instruction_starts": cpu.instruction_starts,
        })
    }

    fn dbg_instruction_boundary_count(&self) -> Option<u64> {
        Some(
            self.machine
                .as_ref()
                .map_or(0, |m| m.cpu().instruction_starts),
        )
    }

    fn dbg_disassemble(&self, addr: u32) -> Option<(String, u8)> {
        let m = self.machine.as_ref()?;
        Some(m68k_disassemble(addr, |a| m.peek(a)))
    }

    fn dbg_step(&mut self) -> u64 {
        let ticks = self.dbg_step_no_resync();
        self.update_rgba_framebuffer();
        ticks
    }

    fn dbg_step_no_resync(&mut self) -> u64 {
        let Some(m) = self.machine.as_mut() else {
            return 0;
        };
        let start = m.cpu().instruction_starts;
        let mut ticks = 0;
        while m.cpu().instruction_starts == start && ticks < STEP_TICK_LIMIT {
            m.tick();
            ticks += 1;
        }
        self.time = self.time.saturating_add(ticks);
        ticks
    }

    fn dbg_resync(&mut self) {
        self.update_rgba_framebuffer();
    }

    // io_trace defaults to unsupported: the 68000 is memory-mapped, so the
    // debugger uses memory_read / disasm / run_until_pc instead.
}

#[cfg(test)]
mod tests {
    use crate::{AtariStRuntime, Model};
    use emu198x_shell::DebugTarget;

    /// A 192 KB TOS whose program at `$FC0008` is NOP, NOP, BRA.S *.
    fn runtime() -> AtariStRuntime {
        let mut tos = vec![0u8; 0x3_0000];
        tos[0..4].copy_from_slice(&0x0000_8000u32.to_be_bytes());
        tos[4..8].copy_from_slice(&0x00FC_0008u32.to_be_bytes());
        tos[8..14].copy_from_slice(&[0x4E, 0x71, 0x4E, 0x71, 0x60, 0xFE]);
        AtariStRuntime::new(Model::St520Pal, tos).expect("192 KB")
    }

    #[test]
    fn stepping_walks_the_program_an_instruction_at_a_time() {
        let mut rt = runtime();
        // Power-on leaves the 68000 fetching: the first step reaches the
        // instruction at the reset vector.
        let first = rt.instruction_boundary_count().expect("counted");
        let mut pcs = Vec::new();
        for _ in 0..4 {
            let ticks = rt.step_instruction();
            assert!(ticks > 0);
            pcs.push(rt.pc());
        }
        assert_eq!(rt.instruction_boundary_count(), Some(first + 4));
        assert_eq!(pcs, [0xFC_0008, 0xFC_000A, 0xFC_000C, 0xFC_000C]);
        assert_eq!(rt.disassemble(0xFC_0008).expect("decoded").1, 2);
        assert!(rt.cpu_state()["d0"].is_string());
    }

    #[test]
    fn peek_and_poke_go_through_the_machine() {
        let mut rt = runtime();
        assert_eq!(rt.peek(0xFC_0008), 0x4E);
        assert_eq!(rt.peek(5), 0xFC, "the reset vector reads from TOS");
        rt.poke(0x1_0010, 0x99);
        assert_eq!(rt.peek(0x1_0010), 0x99);
        assert_eq!(rt.peek(0xFF01_0010), 0x99, "the bus is 24 bits");
    }
}
