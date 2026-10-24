//! Shared-tier debug surface for the Super NES's 65C816.
//!
//! The `impl_*_debug_primitives!` macros cover the 8-bit CPUs only, so this
//! implements [`DebugPrimitives`] by hand, as the Mega Drive does.
//!
//! Addresses are the 65C816's 24-bit bus: bank in bits 16-23. Reads see
//! work RAM and the cartridge without side effects; the I/O registers read
//! as the open bus. A step runs master clocks until the CPU starts its next
//! instruction. Disassembly uses Isa198x's 65816 decoder at the widths the
//! CPU has now, which is what the bytes at the PC will execute with.

use emu198x_shell::DebugPrimitives;
use serde_json::{Value, json};

use crate::runtime::SnesRuntime;

/// Safety bound on one step, in master clocks: far longer than any
/// instruction plus a long DMA, so it only stops a CPU in `WAI` or `STP`.
const STEP_CLOCK_LIMIT: u64 = 4_000_000;

impl DebugPrimitives for SnesRuntime {
    fn dbg_pc(&self) -> u32 {
        self.machine.as_ref().map_or(0, |m| {
            u32::from(m.cpu.regs.pbr) << 16 | u32::from(m.cpu.regs.pc)
        })
    }

    fn dbg_peek(&self, addr: u32) -> u8 {
        self.machine
            .as_ref()
            .map_or(0, |m| m.peek(addr & 0xFF_FFFF))
    }

    fn dbg_poke(&mut self, addr: u32, value: u8) {
        if let Some(m) = self.machine.as_mut() {
            m.poke(addr & 0xFF_FFFF, value);
        }
        self.update_rgba_framebuffer();
    }

    fn dbg_cpu_state(&self) -> Value {
        let Some(m) = self.machine.as_ref() else {
            return json!({});
        };
        let r = &m.cpu.regs;
        json!({
            "pc": format!("${:02X}:{:04X}", r.pbr, r.pc),
            "a": format!("${:04X}", r.a),
            "x": format!("${:04X}", r.x),
            "y": format!("${:04X}", r.y),
            "s": format!("${:04X}", r.s),
            "d": format!("${:04X}", r.d),
            "dbr": format!("${:02X}", r.dbr),
            "pbr": format!("${:02X}", r.pbr),
            "p": format!("${:02X}", r.p),
            "emulation": r.e,
            "m8": m.cpu.m(),
            "x8": m.cpu.x(),
            "waiting": m.cpu.waiting(),
            "stopped": m.cpu.stopped(),
            "cycles": m.cpu.total_cycles,
        })
    }

    fn dbg_disassemble(&self, addr: u32) -> Option<(String, u8)> {
        let m = self.machine.as_ref()?;
        // Prefix a REP so the decoder knows the immediate widths.
        let mask = u8::from(!m.cpu.m()) << 5 | u8::from(!m.cpu.x()) << 4;
        let mut code = Vec::with_capacity(6);
        if mask != 0 {
            code.extend_from_slice(&[0xC2, mask]);
        }
        let start = code.len();
        let bank = addr & 0xFF_0000;
        for i in 0..4u32 {
            code.push(m.peek(bank | (addr.wrapping_add(i) & 0xFFFF)));
        }
        let origin = (addr as u16).wrapping_sub(start as u16);
        isa198x_disasm::disassemble_65816(&code, origin)
            .into_iter()
            .find(|line| line.addr == u32::from(addr as u16) && !line.bytes.is_empty())
            .filter(|line| !line.text.starts_with('.') || line.text.starts_with(".byte"))
            .map(|line| (line.text, line.bytes.len() as u8))
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
        let clocks = m.step_instruction(STEP_CLOCK_LIMIT);
        self.time = self.time.saturating_add(clocks);
        clocks
    }

    fn dbg_resync(&mut self) {
        self.update_rgba_framebuffer();
    }
}

#[cfg(test)]
mod tests {
    use crate::{Model, with_cartridge};
    use emu198x_shell::DebugTarget;

    /// NOP, LDA #$12, BRA * at $8000, as a 32 KB headerless LoROM.
    fn runtime() -> crate::SnesRuntime {
        let mut rom = vec![0u8; 0x8000];
        rom[..5].copy_from_slice(&[0xEA, 0xA9, 0x12, 0x80, 0xFE]);
        rom[0x7FFC..0x7FFE].copy_from_slice(&0x8000u16.to_le_bytes());
        with_cartridge(Model::SnesNtsc, rom)
    }

    #[test]
    fn stepping_walks_the_program_an_instruction_at_a_time() {
        let mut rt = runtime();
        let mut pcs = Vec::new();
        for _ in 0..4 {
            assert!(rt.step_instruction() > 0);
            pcs.push(rt.pc());
        }
        assert_eq!(pcs, [0x8000, 0x8001, 0x8003, 0x8003]);
        assert_eq!(rt.disassemble(0x8001).expect("decoded").1, 2);
        assert!(rt.cpu_state()["a"].is_string());
    }

    #[test]
    fn peek_and_poke_go_through_the_machine() {
        let mut rt = runtime();
        assert_eq!(rt.peek(0x8001), 0xA9);
        rt.poke(0x7E_0010, 0x99);
        assert_eq!(rt.peek(0x7E_0010), 0x99);
        assert_eq!(rt.peek(0x0010), 0x99, "low RAM mirrors bank $7E");
    }
}
