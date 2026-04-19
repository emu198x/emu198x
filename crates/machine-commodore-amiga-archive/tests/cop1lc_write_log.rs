//! Trace every CPU write to COP1LCH ($DFF080) and COP1LCL ($DFF082)
//! during chip-only and chip+slow KS 1.3 boots.
//!
//! Both halves of the copper-list pointer are 16-bit writes. Pattern of
//! writes will show:
//!  - PC of the writer
//!  - VALUE written (the address half)
//!  - CPU registers at write time (a0/a1/d0) — gives context for what
//!    triggered the write
//!
//! Expected:
//!  - Slow-RAM ends with COP1LC = $00000420 (real copper list there)
//!  - Chip-only ends with COP1LC = $00000000 or garbage $08000000
//!
//! What we want to learn:
//!  - Does chip-only ever write a non-zero high half (COP1LCH)?
//!  - Does chip-only's COP1LCL get the right address (somewhere in
//!    chip RAM with a real copper list at it)?
//!  - When does the teardown happen — same PC sequence as slow-RAM
//!    or different?

use std::path::PathBuf;
use machine_commodore_amiga::Amiga;

fn rom() -> Vec<u8> {
    let home = std::env::var("HOME").unwrap();
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    std::fs::read(&path).expect("read kick13.rom")
}

fn run_and_dump(label: &str, slow_ram: usize) {
    let mut amiga = if slow_ram == 0 {
        Amiga::new(rom())
    } else {
        Amiga::new_with_slow_ram(rom(), slow_ram)
    };

    eprintln!("===== {label} =====");

    let mut prev_cop1lc = 0u32;
    let mut last_drain_len = 0;
    let mut all_cop_writes: Vec<(u64, String)> = Vec::new();

    for frame in 0..250u64 {
        amiga.run_frame();

        let cur_log_len = amiga.debug_custom_write_log.len();
        if cur_log_len > last_drain_len {
            for entry in amiga.debug_custom_write_log.iter().skip(last_drain_len) {
                if entry.contains("offset=$080") || entry.contains("offset=$082")
                    || entry.contains("offset=$084") || entry.contains("offset=$086")
                {
                    all_cop_writes.push((frame, entry.clone()));
                }
            }
            last_drain_len = cur_log_len;
        }

        let cur = amiga.copper.cop1lc;
        if cur != prev_cop1lc {
            eprintln!("  f{frame:3}: COP1LC changed ${prev_cop1lc:08X} → ${cur:08X}");
            prev_cop1lc = cur;
        }
    }

    if all_cop_writes.is_empty() {
        eprintln!("  (no COP*LC writes captured)");
    } else {
        eprintln!("  {} COP*LC writes captured (COP1LC=$080/$082, COP2LC=$084/$086):",
            all_cop_writes.len());
        for (frame, entry) in all_cop_writes.iter() {
            eprintln!("    f{:3}: {}", frame, entry);
        }
    }

    eprintln!("  Final: COP1LC = ${:08X}  COP2LC = ${:08X}",
        amiga.copper.cop1lc, amiga.copper.cop2lc);
    eprintln!("  Copper PC: ${:08X}", amiga.copper.pc);

    // Dump 32 bytes (8 copper instructions) at the LAST captured COP1LC
    // address — find which buffer the boot SAID is the copper list.
    let target = if let Some((_, last)) = all_cop_writes.last() {
        // Parse "val=$xxxx" from the last entry — that's COP1LCL.
        if let Some(idx) = last.find("val=$") {
            let rest = &last[idx + 5..idx + 9];
            u32::from_str_radix(rest, 16).ok()
        } else { None }
    } else { None };

    // ALSO scan a wider range for any MOVE-COP1LC patterns the copper
    // might encounter. Comment in lib.rs:2434 mentions these pattern
    // locations from a previous boot trace.
    eprintln!("  Scanning chip RAM $0420-$3500 for MOVE-COP1LC ($080/$082) instructions:");
    for addr in (0x0420..0x3500).step_by(2) {
        let w1 = amiga.memory.read_word(addr);
        let w2 = amiga.memory.read_word(addr + 2);
        // Copper MOVE: word1 = reg<<1, word2 = value. Only flag $080/$082.
        if w1 == 0x0080 || w1 == 0x0082 {
            eprintln!(
                "    ${:08X}: {:04X} {:04X}  copper MOVE ${:03X} = ${:04X}",
                addr, w1, w2, w1, w2
            );
        }
    }
    eprintln!();

    if let Some(addr) = target {
        eprintln!("  Content at COP1LC=${addr:08X} (full list, until end-of-copper $FFFFFFFE):");
        for i in 0..64u32 {
            let a = addr + i * 4;
            let hi = (u32::from(amiga.memory.read_word(a)) << 16)
                | u32::from(amiga.memory.read_word(a + 2));
            let h_word = (hi >> 16) as u16;
            let l_word = hi as u16;
            let kind = if h_word & 1 == 0 {
                let reg = h_word & 0x1FE;
                let mark = if reg == 0x080 || reg == 0x082 || reg == 0x088 {
                    " ← COP1LC/JMP write!"
                } else { "" };
                format!("MOVE  reg=${:03X} val=${:04X}{}", reg, l_word, mark)
            } else if l_word & 1 == 0 {
                let mark = if hi == 0xFFFF_FFFE { " ← END-OF-LIST" } else { "" };
                format!("WAIT  v=${:02X} h=${:02X}{}", (h_word >> 8) & 0xFF, h_word & 0xFE, mark)
            } else {
                format!("SKIP  v=${:02X} h=${:02X}", (h_word >> 8) & 0xFF, h_word & 0xFE)
            };
            eprintln!("    ${a:08X}: {hi:08X}  {kind}");
            if hi == 0xFFFF_FFFE {
                break;
            }
        }
    }
    eprintln!();
}

#[test]
#[ignore]
fn trace_cop1lc_writes() {
    run_and_dump("chip-only", 0);
    run_and_dump("chip + 512K slow RAM", 512 * 1024);
}
