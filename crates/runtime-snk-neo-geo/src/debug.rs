//! Shared-tier debug surface for the Neo Geo's 68000.
//!
//! As on the Mega Drive, [`DebugPrimitives`] is implemented by hand for the
//! 68000, and [`emu198x_shell::DebugTarget`] comes from the shell's blanket
//! impl. Addresses are the 68000's 24-bit bus; reads see ROM, RAM, palette
//! and BIOS without side effects. Stepping runs master clocks until the
//! 68000 starts its next instruction.

use emu198x_shell::DebugPrimitives;
use motorola_68000::disasm::disassemble as m68k_disassemble;
use serde_json::{Value, json};

use crate::runtime::NeoGeoRuntime;

/// Safety bound on a single-instruction step, in master clocks: far beyond
/// the longest 68000 instruction, so it only guards against a stopped CPU.
const STEP_TICK_LIMIT: u64 = 1_000_000;

impl DebugPrimitives for NeoGeoRuntime {
    fn dbg_pc(&self) -> u32 {
        self.machine.as_ref().map_or(0, |m| m.cpu.instr_start_pc)
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
        let cpu = &m.cpu;
        let r = &cpu.regs;
        let mut state = json!({
            "pc": format!("${:08X}", r.pc),
            "instr_start_pc": format!("${:08X}", cpu.instr_start_pc),
            "sr": format!("${:04X}", r.sr),
            "supervisor": r.is_supervisor(),
            "interrupt_mask": r.interrupt_mask(),
            "ssp": format!("${:08X}", r.ssp),
            "usp": format!("${:08X}", r.usp),
            "ipl_pin": cpu.ipl,
            "interrupts_taken": cpu.interrupts_taken,
            "instruction_starts": cpu.instruction_starts,
        });
        for n in 0..8 {
            state[format!("d{n}")] = json!(format!("${:08X}", r.d[n]));
            state[format!("a{n}")] = json!(format!("${:08X}", r.a(n)));
        }
        state
    }

    fn dbg_instruction_boundary_count(&self) -> Option<u64> {
        Some(
            self.machine
                .as_ref()
                .map_or(0, |m| m.cpu.instruction_starts),
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
        let start = m.cpu.instruction_starts;
        let mut ticks = 0;
        while m.cpu.instruction_starts == start && ticks < STEP_TICK_LIMIT {
            m.tick();
            ticks += 1;
        }
        self.time = self.time.saturating_add(ticks);
        ticks
    }

    fn dbg_resync(&mut self) {
        self.update_rgba_framebuffer();
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::runtime_with_program;
    use emu198x_shell::DebugTarget;

    #[test]
    fn stepping_walks_the_program_an_instruction_at_a_time() {
        // NOP, NOP, BRA.S * at the cartridge's entry.
        let mut rt = runtime_with_program(&[0x4E71, 0x4E71, 0x60FE]);
        // The stand-in BIOS runs first, then hands over to $400.
        let mut steps = 0;
        while rt.pc() != 0x400 {
            assert!(rt.step_instruction() > 0);
            steps += 1;
            assert!(steps < 10, "never reached the cartridge");
        }
        let first = rt.instruction_boundary_count().expect("counted");
        let mut pcs = Vec::new();
        for _ in 0..3 {
            assert!(rt.step_instruction() > 0);
            pcs.push(rt.pc());
        }
        assert_eq!(rt.instruction_boundary_count(), Some(first + 3));
        assert_eq!(pcs, [0x402, 0x404, 0x404]);
        assert_eq!(rt.disassemble(0x400).expect("decoded").1, 2);
        assert!(rt.cpu_state()["d0"].is_string());
    }

    #[test]
    fn peek_and_poke_go_through_the_machine() {
        let mut rt = runtime_with_program(&[0x60FE]);
        assert_eq!(rt.peek(0x400), 0x60);
        rt.poke(0x10_0010, 0x99);
        assert_eq!(rt.peek(0x10_0010), 0x99);
        assert_eq!(rt.peek(0xFF10_0010), 0x99, "the bus is 24 bits");
    }
}
