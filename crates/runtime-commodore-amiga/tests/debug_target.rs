//! Shared-tier `DebugTarget` surface on the Amiga (68000 family) — the first
//! 68000 machine to join it. The Amiga implements `DebugPrimitives` (delegating
//! to the `AmigaLiveAccess` adapter) and gets `DebugTarget` from the shell's
//! blanket impl; disassembly goes through `motorola_68000::disasm`. The 68000 is
//! memory-mapped, so no `io_trace`.
//!
//! This proves the *wiring* — every `DebugTarget` method reaches the live
//! machine through the blanket impl and returns the right shape. A blank machine
//! boots with the ROM overlay shadowing low reads, so memory round-trips are not
//! asserted here; chip-RAM read/write semantics are covered by the Amiga's own
//! memory tests.

use emu198x_shell::MachineCore;
use runtime_commodore_amiga::{AmigaLiveAccess, AmigaRuntimeKind, Model};

#[test]
fn debug_surface_works_on_68000() {
    let mut runtime = AmigaRuntimeKind::blank(Model::A500OcsPal);

    // pc + the poke/peek paths are wired (no panic), and the byte fold reads
    // through the full 24-bit address (proves the u32 widening).
    {
        let dbg = runtime.debug_target_mut().expect("debug target");
        let _pc = dbg.pc();
        dbg.poke(0x0008_0000, 0x12);
        let _ = dbg.peek(0x0008_0000);
    }

    {
        let dbg = runtime.debug_target().expect("debug target");

        // Disassembler is wired through motorola_68000::disasm: any address
        // decodes to a 68k instruction of plausible length.
        let (text, len) = dbg.disassemble(0x00F8_0000).expect("m68k disassembles");
        assert!(
            (2..=10).contains(&len),
            "68k instruction length is one or more words, got {len}"
        );
        assert!(!text.is_empty(), "disasm produced a mnemonic");

        // cpu_state carries the full 68k register shape.
        let cpu = dbg.cpu_state();
        for key in ["pc", "sr", "ssp", "usp", "d0", "d7", "a0", "a6"] {
            assert!(cpu.get(key).is_some(), "68k cpu_state exposes {key}");
        }

        assert!(
            !dbg.supports_io_trace(),
            "the 68000 is memory-mapped, not port-mapped"
        );
    }

    // Stepping advances the CPU and consumes bus cycles.
    let ticks = runtime
        .debug_target_mut()
        .expect("debug target")
        .step_instruction();
    assert!(ticks > 0, "stepping consumed cycles");
}

#[test]
fn debug_step_on_multi_edge_cpu_crosses_exactly_one_boundary() {
    let mut runtime = AmigaRuntimeKind::blank(Model::A1200AgaPal);
    let starts_before = runtime.cpu_instruction_starts();
    let machine_ticks_before = runtime.tick_count();
    let runtime_time_before = runtime.time().get();

    let ticks = runtime
        .debug_target_mut()
        .expect("debug target")
        .step_instruction();

    let starts_after = runtime.cpu_instruction_starts();
    let completed_ticks = runtime.tick_count().wrapping_sub(machine_ticks_before);
    assert!(ticks > 0, "stepping consumed system ticks");
    assert_eq!(ticks, completed_ticks);
    assert_eq!(
        runtime.time().get().wrapping_sub(runtime_time_before),
        completed_ticks,
        "debug stepping must keep runtime time aligned with completed machine ticks"
    );
    assert_eq!(
        starts_after.wrapping_sub(starts_before),
        1,
        "one debugger step must cross exactly one instruction boundary"
    );
}

#[test]
fn repeated_debug_steps_on_a530_each_cross_one_boundary() {
    let mut runtime = AmigaRuntimeKind::blank(Model::A500OcsPalGvpA530);

    for _ in 0..64 {
        let starts_before = runtime.cpu_instruction_starts();
        let machine_ticks_before = runtime.tick_count();
        let runtime_time_before = runtime.time().get();
        let ticks = runtime
            .debug_target_mut()
            .expect("debug target")
            .step_instruction();
        let completed_ticks = runtime.tick_count().wrapping_sub(machine_ticks_before);
        assert_eq!(
            runtime.cpu_instruction_starts().wrapping_sub(starts_before),
            1,
            "40 MHz A530 stepping must not run through a second instruction"
        );
        assert_eq!(ticks, completed_ticks);
        assert_eq!(
            runtime.time().get().wrapping_sub(runtime_time_before),
            completed_ticks,
            "partial and complete A530 steps must keep runtime time additive"
        );
    }
}
