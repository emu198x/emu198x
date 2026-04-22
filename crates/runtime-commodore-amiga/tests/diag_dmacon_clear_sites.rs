//! Diagnostic: dump raw opcode words around the two ROM addresses
//! that clear DMACON.SPREN ($FD689E) and DMACON.BPLEN ($FE8554)
//! during KS 1.3 boot. Run with:
//!   cargo test -p runtime-commodore-amiga --test diag_dmacon_clear_sites \
//!       -- --ignored --nocapture
//!
//! The point is to see what code surrounds those writes — are they
//! reached via a conditional we shouldn't take, or is the clear a
//! step of a larger routine that normally re-enables afterwards?

use std::path::PathBuf;

use runtime_commodore_amiga::{AmigaRuntime, Model};

fn load_ks13() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        eprintln!("skipping: KS 1.3 ROM missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(&path).expect("read KS 1.3"))
}

#[test]
#[ignore = "needs KS 1.3 ROM — run with --ignored"]
fn dump_rom_around_dmacon_clear_sites() {
    let Some(rom) = load_ks13() else { return };
    // Use the runtime to get the ROM mapped at the right address
    // (ROM anchored at $FC0000 for 512 KiB images) via `read_word`.
    let rt = AmigaRuntime::new(Model::A500OcsPalA501, rom).unwrap();

    // Dump a generous window around each clear site — enough to see
    // the enclosing routine's prologue / epilogue and any nearby
    // conditional branches.
    dump_window(&rt, "SPREN clear site", 0x00FD_689E, 0x20, 0x40);
    dump_window(&rt, "BPLEN clear site", 0x00FE_8554, 0x20, 0x40);
    dump_window(
        &rt,
        "WAITBLIT spin (where the CPU is pinned)",
        0x00FC_5A70,
        0x10,
        0x20,
    );
}

fn dump_window(rt: &AmigaRuntime, label: &str, center: u32, pre_bytes: u32, post_bytes: u32) {
    let lo = (center - pre_bytes) & !1; // align even
    let hi = (center + post_bytes) & !1;
    println!("=== {label} at ${center:08X} ===");
    println!("  (marker: `->` is the exact PC of the DMACON write)");
    let mut addr = lo;
    while addr < hi {
        let w = rt.machine().read_word(addr);
        let marker = if addr == center { "->" } else { "  " };
        // Quick heuristic decode for the most common 68000
        // idioms we expect to see near custom-register writes.
        let hint = classify(w, addr, rt);
        println!("  {marker} ${:08X}: ${:04X}  {hint}", addr, w);
        addr = addr.wrapping_add(2);
    }
    println!();
}

/// Tag a few common 68000 opcodes to aid eyeballing. Not a proper
/// disassembler — just enough to spot `move.w #imm, xxx`-style
/// writes to custom registers, branches, and returns.
fn classify(word: u16, addr: u32, rt: &AmigaRuntime) -> String {
    // Full encoding decode would take a crate. These heuristics
    // only cover the patterns that matter for reading DMACON setup
    // code; miss cases fall through to an empty string.
    match word & 0xFFC0 {
        // move.w #imm, (d16, An) / (xxx).W / (xxx).L  — dest = $DFF096 via abs.L
        _ if word == 0x33FC => {
            // movew #imm, (xxx).L
            let imm = rt.machine().read_word(addr.wrapping_add(2));
            let abs = rt.machine().read_long(addr.wrapping_add(4));
            format!("move.w #${imm:04X}, (${abs:08X}).L")
        }
        _ if word == 0x31FC => {
            // movew #imm, (xxx).W
            let imm = rt.machine().read_word(addr.wrapping_add(2));
            let abs = rt.machine().read_word(addr.wrapping_add(4));
            format!("move.w #${imm:04X}, (${abs:04X}).W")
        }
        _ => match word & 0xF000 {
            0x6000 => format!("branch? disp ${:02X}", (word & 0xFF) as i8),
            0x4E00 if word == 0x4E75 => "rts".into(),
            0x4E00 if word == 0x4E71 => "nop".into(),
            0x4E00 if word == 0x4E73 => "rte".into(),
            _ => String::new(),
        },
    }
}
