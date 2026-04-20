//! Diagnostic: walk Exec MemList in chip-only vs slow-RAM configs.
//!
//! Per ChatGPT's analysis (recorded 2026-04-20): the chip-only and
//! slow-RAM boot paths diverge because Exec builds different MemList
//! layouts. With slow RAM, generic allocations drift to non-chip
//! memory, leaving chip RAM freer; without it, the same allocations
//! consume chip RAM, shifting addresses.
//!
//! This test runs both configurations and dumps the MemList headers
//! at multiple checkpoints. Compare the side-by-side output to see:
//!   - When does Exec build the MemList?
//!   - How many MemHeaders exist, and where?
//!   - Where do the layouts first diverge?
//!
//! Run with:
//!   cargo test -p machine-commodore-amiga-ocs --test diag_memlist_diff \
//!     -- --ignored --nocapture
//!
//! ExecBase struct (KS 1.x) MemList offset:
//!   offset 322 (decimal) = $0142
//!   layout: lh_Head (4), lh_Tail (4), lh_TailPred (4), lh_Type (1), pad (1)
//!
//! MemHeader struct:
//!   Node (14) + mh_Attributes (UWORD) + mh_First (APTR)
//!     + mh_Lower (APTR) + mh_Upper (APTR) + mh_Free (ULONG)
//!   Total 32 bytes.
//!
//! Walk the list from lh_Head following ln_Succ until ln_Succ == NULL.

use std::path::PathBuf;
use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_LINES, PAL_LINE_CCKS};

const EXEC_MEMLIST_OFFSET: u32 = 322;
const MH_ATTRS_OFFSET: u32 = 14;
const MH_FIRST_OFFSET: u32 = 16;
const MH_LOWER_OFFSET: u32 = 20;
const MH_UPPER_OFFSET: u32 = 24;
const MH_FREE_OFFSET: u32 = 28;
const NODE_SUCC_OFFSET: u32 = 0;
const NODE_NAME_OFFSET: u32 = 10;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        eprintln!("skipping: Kickstart 1.3 ROM missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

/// Read a long from chip RAM directly (bypasses OVL).
fn chip_long(amiga: &AmigaOcs, addr: u32) -> u32 {
    let b0 = u32::from(amiga.read_chip_ram_byte(addr));
    let b1 = u32::from(amiga.read_chip_ram_byte(addr.wrapping_add(1)));
    let b2 = u32::from(amiga.read_chip_ram_byte(addr.wrapping_add(2)));
    let b3 = u32::from(amiga.read_chip_ram_byte(addr.wrapping_add(3)));
    (b0 << 24) | (b1 << 16) | (b2 << 8) | b3
}

/// Read a long from anywhere reachable by the bus (uses OVL-aware path
/// for low memory; works for chip RAM after OVL has been cleared, and
/// for slow RAM at $C00000 if installed).
fn any_long(amiga: &AmigaOcs, addr: u32) -> u32 {
    amiga.read_long(addr)
}

fn any_word(amiga: &AmigaOcs, addr: u32) -> u16 {
    amiga.read_word(addr)
}

/// Read a NUL-terminated ASCII name from any-bus memory, max 32 chars.
fn read_name(amiga: &AmigaOcs, addr: u32) -> String {
    if addr == 0 {
        return "<null>".into();
    }
    let mut s = String::new();
    for i in 0..32 {
        let b = amiga.read_word(addr.wrapping_add(i)) >> 8;
        let b = b as u8;
        if b == 0 {
            break;
        }
        if b.is_ascii() && !b.is_ascii_control() {
            s.push(b as char);
        } else {
            s.push('?');
        }
    }
    s
}

#[derive(Default)]
struct MemHeaderDump {
    addr: u32,
    name: String,
    attrs: u16,
    lower: u32,
    upper: u32,
    free: u32,
}

fn walk_memlist(amiga: &AmigaOcs, label: &str) {
    // ExecBase pointer lives at $4 in chip RAM. Bypass OVL for safety
    // — once Exec has run, $4 is in chip RAM but OVL might still be on
    // depending on timing.
    let exec_base = chip_long(amiga, 0x0000_0004);
    if exec_base == 0 {
        eprintln!("  [{label}] ExecBase=$00000000 — Exec hasn't initialised yet");
        return;
    }
    eprintln!("  [{label}] ExecBase=${exec_base:08X}");

    // MemList head at ExecBase + 322. The head pointer is the first
    // node (or it points to lh_Tail which is NULL if empty).
    let memlist_addr = exec_base.wrapping_add(EXEC_MEMLIST_OFFSET);
    let head = any_long(amiga, memlist_addr);
    let tail = any_long(amiga, memlist_addr.wrapping_add(4));
    let tailpred = any_long(amiga, memlist_addr.wrapping_add(8));
    eprintln!(
        "  [{label}] MemList @ ${:08X}: head=${head:08X} tail=${tail:08X} tailpred=${tailpred:08X}",
        memlist_addr
    );

    // Walk the list. The last real node's ln_Succ points to lh_Tail
    // (a sentinel address whose stored value is NULL). To detect the
    // sentinel we read ln_Succ FIRST and skip the node when it's NULL.
    let mut node = head;
    let mut idx = 0u32;
    while node != 0 && idx < 8 {
        let succ = any_long(amiga, node.wrapping_add(NODE_SUCC_OFFSET));
        if succ == 0 {
            // This is the lh_Tail sentinel — not a real header.
            break;
        }
        let name_ptr = any_long(amiga, node.wrapping_add(NODE_NAME_OFFSET));
        let name = read_name(amiga, name_ptr);
        let attrs = any_word(amiga, node.wrapping_add(MH_ATTRS_OFFSET));
        let first = any_long(amiga, node.wrapping_add(MH_FIRST_OFFSET));
        let lower = any_long(amiga, node.wrapping_add(MH_LOWER_OFFSET));
        let upper = any_long(amiga, node.wrapping_add(MH_UPPER_OFFSET));
        let free = any_long(amiga, node.wrapping_add(MH_FREE_OFFSET));
        let dump = MemHeaderDump {
            addr: node,
            name,
            attrs,
            lower,
            upper,
            free,
        };
        eprintln!(
            "  [{label}]   #{idx} @${addr:08X} \"{name}\" attrs=${attrs:04X} \
             lower=${lower:08X} upper=${upper:08X} free=${free:08X} first=${first:08X}",
            addr = dump.addr,
            name = dump.name,
            attrs = dump.attrs,
            lower = dump.lower,
            upper = dump.upper,
            free = dump.free,
        );
        node = succ;
        idx += 1;
    }
    if idx == 8 {
        eprintln!("  [{label}]   (truncated after 8 headers)");
    }
}

fn checkpoint(amiga: &AmigaOcs, label: &str) {
    let pc = amiga.cpu().regs.pc;
    let sr = amiga.cpu().regs.sr;
    let cck = amiga.cck_count();
    eprintln!(
        "[{label}] cck={cck:10} pc=${pc:08X} sr=${sr:04X} \
         dmacon=${dmacon:04X} intena=${intena:04X} intreq=${intreq:04X} \
         bplcon0=${bplcon0:04X}",
        dmacon = amiga.dmacon(),
        intena = amiga.intena(),
        intreq = amiga.intreq(),
        bplcon0 = amiga.bplcon0(),
    );
    walk_memlist(amiga, label);
}

fn run_to_frame(amiga: &mut AmigaOcs, frame: u64) {
    let frame_ccks = u64::from(PAL_LINE_CCKS) * u64::from(PAL_FRAME_LINES);
    let target = frame * frame_ccks;
    while amiga.cck_count() < target {
        amiga.tick_cck();
    }
}

#[test]
#[ignore]
fn diff_memlist_chip_only_vs_slow_ram() {
    let Some(rom) = load_kickstart() else { return };

    let mut chip_only = AmigaOcs::new(rom.clone());
    let mut slow_ram = AmigaOcs::with_slow_ram(rom, 512 * 1024);

    let checkpoints = [10u64, 25, 50, 75, 100, 150, 200, 300];

    for frame in checkpoints {
        eprintln!("\n=== Frame {frame} ===");
        run_to_frame(&mut chip_only, frame);
        run_to_frame(&mut slow_ram, frame);
        checkpoint(&chip_only, "chip-only");
        checkpoint(&slow_ram, "slow-RAM ");
    }
}
