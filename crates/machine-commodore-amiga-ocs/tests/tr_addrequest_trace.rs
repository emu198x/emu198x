//! Hit-count every byte of TR_ADDREQUEST's code range to see
//! which paths execute during trackdisk's 500ms MICROHZ request.
//!
//! TR_ADDREQUEST starts at $FE91D0 and ends before $FE9330-ish.
//! Within that range, the key branches are:
//!   $FE91D4  BCLR  #0, io_Flags (clears IOF_QUICK)
//!   $FE91EC  CMPA.L 8(A3), A3  (empty-queue check)
//!   $FE91F0  BNE.S $FE920A     (non-empty branch)
//!   $FE91F2  (empty-queue path start)
//!   $FE9206  BT.W $FE92BE      (empty path → tail)
//!   $FE920A  (non-empty path: scan sorted queue)
//!   $FE92BE  (common tail: restore IDNestCnt, return)
//!
//! We'd also like to know whether $FE94FA is reached — that's
//! the routine called with both a "current" and "new" time ptr,
//! likely for insertion-point resolution.

use std::collections::BTreeMap;
use std::path::PathBuf;
use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};

const INTERESTING: &[(u32, &str)] = &[
    (0x00FE_94A2, "LOAD TB latch helper entry"),
    (0x00FE_94AC, "TBHI write"),
    (0x00FE_94B2, "RTS after TB latch"),
    (0x00FE_94B4, "RTS at $94B4 (padding?)"),
    (0x00FE_94B6, "BCHG CRB helper: LEA"),
    (0x00FE_94B8, "LEA mid (word 2)"),
    (0x00FE_94BA, "LEA mid (word 3)"),
    (0x00FE_94BC, "BCHG #0, CRB"),
    (0x00FE_94C0, "BCHG mid"),
    (0x00FE_94C2, "BEQ.S post-BCHG"),
    (0x00FE_94C4, "BCHG was 0 (was stopped)"),
    (0x00FE_94F8, "BCHG was 1 (was running)"),
    (0x00FE_94FA, "$94FA helper entry"),
];

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        eprintln!("skipping: Kickstart 1.3 ROM missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

#[test]
#[ignore]
fn trace_tr_addrequest_paths() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);

    let mut hits: BTreeMap<u32, u64> = INTERESTING
        .iter()
        .map(|(pc, _)| (*pc, 0u64))
        .collect();
    let mut prev_pc = amiga.cpu().regs.pc;

    for _ in 0..(400 * PAL_FRAME_TICKS) {
        amiga.tick();
        let pc = amiga.cpu().regs.pc;
        if pc == prev_pc {
            continue;
        }
        if let Some(c) = hits.get_mut(&pc) {
            *c += 1;
        }
        prev_pc = pc;
    }

    eprintln!("\n=== TR_ADDREQUEST path hit counts (400 frames) ===");
    for (pc, label) in INTERESTING {
        let c = hits[pc];
        eprintln!("  ${pc:08X}  {c:>5}  {label}");
    }
}
