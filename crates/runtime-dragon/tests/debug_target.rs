//! Exercises the shared `DebugTarget` surface on the Dragon runtime — the first
//! 6809 machine to get one. Disassembly goes through the Asm198x `isa_disasm`
//! spec crate (`decode_one_6809`); the 6809 is memory-mapped, so no `io_trace`.
//! The target is hand-rolled: `machine: Dragon32` is non-optional, and there is
//! no 6809 debug macro.
//!
//! Runs without ROMs: a zero-filled `DragonRuntime::blank` is enough to exercise
//! peek/poke/pc/disassemble/step on RAM.

use emu198x_shell::MachineCore;
use runtime_dragon::{DragonRuntime, Model};

#[test]
fn debug_surface_works_on_6809() {
    let mut runtime = DragonRuntime::blank(Model::Dragon32Pal);

    // Poke a known instruction into RAM and disassemble it through the spec
    // crate: LDA #$42 = 86 42.
    {
        let dbg = runtime.debug_target_mut().expect("debug target");
        dbg.poke(0x1000, 0x86);
        dbg.poke(0x1001, 0x42);
        assert_eq!(dbg.peek(0x1000), 0x86, "poke/peek round-trips through RAM");
    }

    {
        let dbg = runtime.debug_target().expect("debug target");

        assert_eq!(
            dbg.disassemble(0x1000),
            Some(("lda #$42".to_string(), 2)),
            "6809 disassembles via isa_disasm::decode_one_6809"
        );

        let cpu = dbg.cpu_state();
        assert!(cpu.get("pc").is_some(), "cpu_state exposes pc");
        assert!(cpu.get("a").is_some(), "6809 cpu_state exposes A");
        assert!(cpu.get("u").is_some(), "6809 cpu_state exposes the U stack");
        assert!(
            cpu.get("dp").is_some(),
            "6809 cpu_state exposes the direct page"
        );
        assert!(
            !dbg.supports_io_trace(),
            "the 6809 is memory-mapped, not port-mapped"
        );
    }

    // Stepping advances the CPU and consumes bus cycles.
    let ticks: u64 = (0..16)
        .map(|_| {
            runtime
                .debug_target_mut()
                .expect("debug target")
                .step_instruction()
        })
        .sum();
    assert!(ticks > 0, "stepping consumed cycles");
}
