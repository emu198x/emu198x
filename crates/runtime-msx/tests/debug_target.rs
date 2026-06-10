//! Exercises the shared `DebugTarget` surface on the MSX runtime.
//!
//! [`z80_disasm_is_wired`] runs in CI with a synthesized BIOS, so the
//! `impl_z80_debug_primitives!` macro's disassembly path is proven without
//! an external ROM — mirroring the C64 (6502) and Dragon (6809) guards.
//! [`debug_surface_works_on_z80`] is the fuller `#[ignore]` test that needs
//! the real BIOS at `~/.emu198x/roms/microsoft-msx/msx.rom` to exercise
//! I/O tracing against live boot code.
//!
//! ```text
//! cargo test -p runtime-msx --test debug_target -- --ignored --nocapture
//! ```

use std::path::PathBuf;

use emu198x_shell::MachineCore;
use runtime_msx::{Model, MsxRuntime};

fn bios() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/microsoft-msx/msx.rom");
    p.exists().then(|| std::fs::read(&p).expect("read BIOS"))
}

/// CI guard for the z80 macro's disassembly wiring. A 32 KiB synthetic BIOS
/// (all the runtime needs to become debuggable) carries a known instruction
/// at `$0000`; the macro-generated `disassemble` must decode it. Proves the
/// `disasm` base tool really works on a Z80 machine — not just that it
/// returns `Some`, but that it decodes the right instruction length.
#[test]
fn z80_disasm_is_wired() {
    // $3E $42 = `LD A,$42`, a two-byte Z80 instruction. A stubbed decoder
    // could not report length 2.
    let mut rom = vec![0u8; 32 * 1024];
    rom[0] = 0x3E;
    rom[1] = 0x42;
    let runtime = MsxRuntime::new(Model::Msx1Ntsc, rom).expect("32 KiB BIOS builds the machine");

    let dbg = runtime
        .debug_target()
        .expect("machine is debuggable once the BIOS is set");
    assert_eq!(dbg.peek(0x0000), 0x3E, "peek reads the planted BIOS byte");
    let (text, len) = dbg
        .disassemble(0x0000)
        .expect("the z80 macro wires a real disassembler");
    assert_eq!(len, 2, "LD A,n decodes as two bytes, not a stub");
    assert!(!text.is_empty(), "disasm yields a mnemonic: {text:?}");
    assert!(
        dbg.cpu_state().get("pc").is_some(),
        "cpu_state exposes the Z80 pc"
    );
}

#[test]
#[ignore = "needs MSX BIOS — run with --ignored"]
fn debug_surface_works_on_z80() {
    let Some(bios) = bios() else {
        panic!("MSX BIOS not found at ~/.emu198x/roms/microsoft-msx/msx.rom");
    };
    let first = bios[0];
    let mut runtime = MsxRuntime::new(Model::Msx1Ntsc, bios).expect("build runtime");

    // Inspection (immutable).
    {
        let dbg = runtime.debug_target().expect("debug target");
        let cpu = dbg.cpu_state();
        assert!(cpu.get("pc").is_some(), "cpu_state should expose pc");
        assert_eq!(dbg.peek(0x0000), first, "peek $0000 = BIOS byte 0");
        let (text, len) = dbg.disassemble(0x0000).expect("Z80 disassembler");
        assert!(
            !text.is_empty() && len >= 1,
            "disasm yields a line: {text:?}"
        );
        assert!(dbg.supports_io_trace(), "MSX is a port-mapped Z80 machine");
    }

    // Stepping advances the CPU.
    let pc_before = runtime.debug_target().expect("debug target").pc();
    let mut ticks = 0u64;
    for _ in 0..2000 {
        ticks += runtime
            .debug_target_mut()
            .expect("debug target")
            .step_instruction();
    }
    assert!(ticks > 0, "stepping consumed T-states");
    let pc_after = runtime.debug_target().expect("debug target").pc();
    assert_ne!(pc_before, pc_after, "PC moved over 2000 instructions");

    // I/O trace captures BIOS port activity.
    runtime
        .debug_target_mut()
        .expect("debug target")
        .start_io_trace();
    for _ in 0..4000 {
        runtime
            .debug_target_mut()
            .expect("debug target")
            .step_instruction();
    }
    let events = runtime
        .debug_target_mut()
        .expect("debug target")
        .take_io_trace();
    assert!(
        !events.is_empty(),
        "BIOS performs I/O the trace should capture"
    );
    println!("captured {} I/O events", events.len());

    // Poke + read-back into RAM (slot 3, page 3 region $E000).
    runtime
        .debug_target_mut()
        .expect("debug target")
        .poke(0xE000, 0x5A);
    assert_eq!(
        runtime.debug_target().expect("debug target").peek(0xE000),
        0x5A
    );
}
