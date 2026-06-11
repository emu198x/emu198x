//! Exercises the shared `DebugTarget` surface on the NES runtime — a lazy
//! `Option<Nes>` 6502 machine now wired via `impl_6502_debug_primitives!`
//! (it was a stub: `poke` no-op'd, `cpu_state` was pc-only, `disassemble`
//! returned `None`, `step` returned 0). Disassembly goes through the Asm198x
//! `isa_disasm` spec crate; the NES is memory-mapped, so no `io_trace`.
//!
//! A minimal NROM cartridge makes the machine live; the debug surface is
//! exercised against RAM, so no real ROM is needed.

use emu198x_shell::{MachineCore, MediaImage, MediaKind, MediaSet};
use runtime_nintendo_nes::{Model, NesRuntime};

fn minimal_ines() -> Vec<u8> {
    // 16 KiB PRG of NOPs with the reset vector at $8000.
    let mut prg = vec![0xeau8; 16 * 1024];
    prg[0x3ffc] = 0x00;
    prg[0x3ffd] = 0x80;
    let chr = vec![0u8; 8 * 1024];
    let mut data = vec![0u8; 16 + prg.len() + chr.len()];
    data[0..4].copy_from_slice(b"NES\x1a");
    data[4] = 1; // 1 × 16 KiB PRG
    data[5] = 1; // 1 × 8 KiB CHR
    data[16..16 + prg.len()].copy_from_slice(&prg);
    data[16 + prg.len()..].copy_from_slice(&chr);
    data
}

#[test]
fn debug_surface_works_on_nes_6502() {
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let rom = minimal_ines();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("load minimal NROM");

    // poke/peek round-trips through RAM, and a planted instruction
    // disassembles. Both were broken: `dbg_poke` was a silent no-op and
    // `dbg_disassemble` returned `None`. LDA #$42 = A9 42 at RAM $0200.
    {
        let dbg = runtime.debug_target_mut().expect("debug target present");
        dbg.poke(0x0200, 0xA9);
        dbg.poke(0x0201, 0x42);
        assert_eq!(
            dbg.peek(0x0200),
            0xA9,
            "poke/peek round-trips through RAM (poke was a silent no-op before)"
        );
    }
    {
        let dbg = runtime.debug_target().expect("debug target present");
        assert_eq!(
            dbg.disassemble(0x0200),
            Some(("LDA #$42".to_string(), 2)),
            "6502 disassembles via isa_disasm::decode_one_6502 (was None before)"
        );

        // cpu_state now exposes the full register file, not just pc.
        let cpu = dbg.cpu_state();
        for key in ["a", "x", "y", "sp", "pc", "p"] {
            assert!(
                cpu.get(key).is_some(),
                "cpu_state must expose `{key}` (was pc-only before): {cpu}"
            );
        }
        assert!(
            !dbg.supports_io_trace(),
            "the NES 6502 is memory-mapped, not port-mapped"
        );
    }

    // Stepping advances the CPU and consumes master ticks (was a no-op
    // returning 0). The minimal ROM NOPs forever from $8000.
    let ticks: u64 = (0..8)
        .map(|_| {
            runtime
                .debug_target_mut()
                .expect("debug target present")
                .step_instruction()
        })
        .sum();
    assert!(
        ticks > 0,
        "stepping consumed master ticks (dbg_step returned 0 before)"
    );
}
