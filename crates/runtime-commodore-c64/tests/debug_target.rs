//! Exercises the shared `DebugTarget` surface on the C64 runtime — a 6502
//! machine whose `machine: C64` field is non-optional, so the target is
//! hand-rolled rather than macro-generated. Disassembly goes through the Asm198x
//! `isa_disasm` spec crate; the C64 is memory-mapped, so no `io_trace`.
//!
//! Runs without ROMs: a zero-filled `C64Runtime::blank` is enough to exercise
//! peek/poke/pc/disassemble/step on RAM.

use emu198x_shell::MachineCore;
use runtime_commodore_c64::{C64Runtime, Model};

#[test]
fn debug_surface_works_on_6502() {
    let mut runtime = C64Runtime::blank(Model::C64PalBreadbin);

    // Poke a known instruction into RAM and disassemble it through the spec
    // crate: LDA #$42 = A9 42.
    {
        let dbg = runtime.debug_target_mut().expect("debug target");
        dbg.poke(0x1000, 0xA9);
        dbg.poke(0x1001, 0x42);
        assert_eq!(dbg.peek(0x1000), 0xA9, "poke/peek round-trips through RAM");
    }

    {
        let dbg = runtime.debug_target().expect("debug target");

        assert_eq!(
            dbg.disassemble(0x1000),
            Some(("LDA #$42".to_string(), 2)),
            "6502 disassembles via isa_disasm::decode_one_6502"
        );

        let cpu = dbg.cpu_state();
        assert!(cpu.get("pc").is_some(), "cpu_state exposes pc");
        assert!(cpu.get("a").is_some(), "6502 cpu_state exposes A");
        assert!(
            !dbg.supports_io_trace(),
            "the C64's 6502 is memory-mapped, not port-mapped"
        );
    }

    // Stepping advances the CPU and consumes phi2 cycles.
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
