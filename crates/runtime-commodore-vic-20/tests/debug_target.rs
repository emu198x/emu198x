//! Exercises the shared `DebugTarget` surface on the VIC-20 runtime
//! (a 6502 machine: no `disasm` yet, no `io_trace`).
//!
//! Gated `#[ignore]`: needs the VIC-20 ROM set at
//! `~/.emu198x/roms/commodore-vic-20/{kernal,basic,char}.rom`.
//!
//! ```text
//! cargo test -p runtime-commodore-vic-20 --test debug_target -- --ignored
//! ```

use std::path::PathBuf;

use emu198x_shell::MachineCore;
use runtime_commodore_vic_20::{Model, Vic20Runtime};

fn rom(name: &str) -> Option<Vec<u8>> {
    let home = std::env::var("HOME").ok()?;
    let p = PathBuf::from(home)
        .join(".emu198x/roms/commodore-vic-20")
        .join(name);
    p.exists().then(|| std::fs::read(&p).expect("read rom"))
}

#[test]
#[ignore = "needs VIC-20 ROM set — run with --ignored"]
fn debug_surface_works_on_6502() {
    let (Some(kernal), Some(basic), Some(char_rom)) =
        (rom("kernal.rom"), rom("basic.rom"), rom("char.rom"))
    else {
        panic!("VIC-20 ROM set not found at ~/.emu198x/roms/commodore-vic-20/");
    };
    let mut runtime =
        Vic20Runtime::new(Model::Vic20Pal, kernal, basic, char_rom).expect("build runtime");

    {
        let dbg = runtime.debug_target().expect("debug target");
        let cpu = dbg.cpu_state();
        assert!(cpu.get("pc").is_some(), "cpu_state should expose pc");
        assert!(cpu.get("a").is_some(), "6502 cpu_state exposes A");
        // 6502 disassembly is pending the Asm198x crate.
        assert!(
            dbg.disassemble(0xE000).is_none(),
            "6502 has no in-tree disassembler yet"
        );
        assert!(
            !dbg.supports_io_trace(),
            "6502 is memory-mapped, not port-mapped"
        );
    }

    // Stepping advances the CPU through the KERNAL reset path.
    let mut ticks = 0u64;
    for _ in 0..5000 {
        ticks += runtime
            .debug_target_mut()
            .expect("debug target")
            .step_instruction();
    }
    assert!(ticks > 0, "stepping consumed cycles");

    // Poke + read-back into zero-page RAM.
    runtime
        .debug_target_mut()
        .expect("debug target")
        .poke(0x0002, 0xA5);
    assert_eq!(
        runtime.debug_target().expect("debug target").peek(0x0002),
        0xA5
    );
}
