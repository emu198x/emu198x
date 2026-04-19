//! Trace the ExecBase pointer at $00000004 frame-by-frame for both
//! chip-only and chip+slow boots.
//!
//! The Kickstart boot sequence has TWO ExecBase placements:
//!  1. Bootstrap ExecBase: raw-allocated from a pool starting at $400
//!     during pre-Exec init (V37 trace $F801FE-$F8022A).
//!  2. Proper ExecBase: AllocMem-allocated $57C bytes once Exec is up
//!     enough to call its own allocator (V37 trace $F80438-$F80498).
//!
//! In slow-RAM boots, the proper ExecBase lands at $C00276 (slow RAM)
//! — no conflict with the bootstrap. In chip-only boots ExecBase reads
//! as $676 at frame 250, which is OUTSIDE the AllocMem-managed region
//! ($8E8-$7E800). That's the bootstrap value, suggesting the swap
//! never completed.
//!
//! This test prints every change to ExecBase to confirm or refute
//! that hypothesis directly.

use std::path::PathBuf;
use machine_commodore_amiga::Amiga;

fn rom() -> Vec<u8> {
    let home = std::env::var("HOME").unwrap();
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    std::fs::read(&path).expect("read kick13.rom")
}

fn read_long(amiga: &Amiga, addr: u32) -> u32 {
    (u32::from(amiga.memory.read_word(addr)) << 16)
        | u32::from(amiga.memory.read_word(addr.wrapping_add(2)))
}

fn classify(addr: u32) -> &'static str {
    match addr {
        0..=0xFF => "(low memory / vector table)",
        0x100..=0x3FF => "(reserved / KickMemPtr area)",
        0x400..=0x7FF => "(bootstrap ExecBase pool)",
        0x800..=0x8E7 => "(below MemHeader range)",
        0x8E8..=0x7E7FF => "(within chip-RAM AllocMem pool)",
        0x7E800..=0x7FFFF => "(top of chip RAM, reserved for stacks)",
        0x80000..=0xBFFFFF => "(unmapped chip / fast)",
        0xC00000..=0xC7FFFF => "(slow RAM / trapdoor)",
        0xC80000..=0xDFFFFF => "(unmapped slow / chipset)",
        0xE00000..=0xF7FFFF => "(reserved / Gayle / RTC)",
        0xF80000..=0xFFFFFF => "(Kickstart ROM)",
        _ => "(out of range)",
    }
}

fn run_and_trace(label: &str, slow_ram: usize, frames: u64) {
    let mut amiga = if slow_ram == 0 {
        Amiga::new(rom())
    } else {
        Amiga::new_with_slow_ram(rom(), slow_ram)
    };
    eprintln!("===== {label} =====");
    let mut last_exec_base = 0u32;
    for frame in 0..frames {
        amiga.run_frame();
        let eb = read_long(&amiga, 0x000004);
        if eb != last_exec_base {
            eprintln!("  f{:3}: ExecBase = ${:08X}  {}", frame, eb, classify(eb));
            last_exec_base = eb;
        }
    }
    // Final layout dump.
    let eb = read_long(&amiga, 0x000004);
    eprintln!("  final: ExecBase = ${:08X}  {}", eb, classify(eb));

    // What's actually at $400-$800 in chip RAM (where bootstrap lives)?
    eprintln!("  $400..$700 dump (32-bit words):");
    for addr in (0x400..0x700).step_by(16) {
        let mut row = format!("    ${:04X}:", addr);
        for off in (0..16).step_by(4) {
            let v = read_long(&amiga, addr + off);
            row.push_str(&format!(" {:08X}", v));
        }
        eprintln!("{row}");
    }
    eprintln!();

    // Where does COP1LC point? Dump the first 64 bytes of "copper list".
    // Real copper lists are a stream of MOVE/WAIT/SKIP instructions:
    //   MOVE  reg, val   = $00xx 0000-FFFF   (xx = register offset / 2)
    //   WAIT  vp, hp     = $xxxx FFFE        (high bit of low word = 0)
    //   SKIP  vp, hp     = $xxxx FFFF        (high bit of low word = 1)
    let cop1lc = amiga.copper.cop1lc;
    eprintln!("  COP1LC = ${:08X} — content (16 longs = 8 copper insns):", cop1lc);
    if cop1lc != 0 && cop1lc < 0x80000 {
        for i in 0..16 {
            let addr = cop1lc.wrapping_add(i * 4);
            let v = read_long(&amiga, addr);
            let hi = (v >> 16) as u16;
            let lo = v as u16;
            let kind = if hi & 1 == 0 {
                let reg = hi & 0x1FE;
                format!("MOVE  ${:03X} = ${:04X}", reg, lo)
            } else if lo & 1 == 0 {
                format!("WAIT  v=${:02X} h=${:02X}", (hi >> 8) & 0xFF, hi & 0xFE)
            } else {
                format!("SKIP  v=${:02X} h=${:02X}", (hi >> 8) & 0xFF, hi & 0xFE)
            };
            eprintln!("    ${:08X}: {:08X}  {}", addr, v, kind);
        }
    }
    eprintln!();
}

#[test]
#[ignore]
fn trace_execbase_swap() {
    run_and_trace("chip-only", 0, 250);
    run_and_trace("chip + 512K slow RAM", 512 * 1024, 250);
}
