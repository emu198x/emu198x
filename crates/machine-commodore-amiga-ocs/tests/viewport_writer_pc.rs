//! Task #96 — does chip-only ever reach the PC that sets View.ViewPort?
//!
//! In slow-RAM, PC $FE887A writes the low word of View.ViewPort
//! (visible as "W $00005A12 = $59E8"). Also $FE8850 earlier stores
//! $5A9A at +$18 — probably one of the cprlist fields. These are
//! in `graphics.library` init / intuition's screen setup.
//!
//! If chip-only NEVER hits $FE887A, whatever task/routine leads there
//! isn't running at all. If it DOES hit but writes different values,
//! the divergence is in the arguments.

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use std::path::PathBuf;

const TARGETS: &[(u32, &str)] = &[
    (0x00FE_8444, "romboot.resident entry (MOVEM)"),
    (0x00FE_8476, "romboot: AllocMem JSR"),
    (0x00FE_847A, "romboot: AllocMem TST"),
    (0x00FE_847E, "romboot: AllocMem FAIL → Alert"),
    (0x00FE_8494, "romboot: failure BRA to exit"),
    (0x00FE_8498, "romboot: AllocMem OK"),
    (0x00FE_8524, "romboot: scan loop BLE target"),
    (0x00FE_8560, "romboot: post-scan checkpoint"),
    (0x00FE_85A0, "romboot: matchword TST"),
    (0x00FE_85A4, "romboot: matchword compare"),
    (0x00FE_85AA, "romboot: matchword mismatch BNE"),
    (0x00FE_85BE, "romboot: checksum NOT"),
    (0x00FE_85F0, "romboot: VALID resident found — call Init"),
    (0x00FE_8600, "romboot: skip candidate, advance"),
    (0x00FE_86DE, "romboot: exit label"),
    (0x00FE_8610, "caller: MOVEL a5@(4), d0 test"),
    (0x00FE_8614, "caller: BNE skip-setup"),
    (0x00FE_8616, "caller: BSR $FE8732 (setup)"),
    (0x00FE_861A, "caller: continuation after setup"),
    (0x00FE_8732, "insert-disk setup: routine entry"),
    (0x00FE_8738, "OpenLibrary graphics.library JSR"),
    (0x00FE_8740, "OpenLibrary result BNE test"),
    (0x00FE_8742, "OpenLibrary FAILED -> Alert path"),
    (0x00FE_875A, "OpenLibrary OK -> MOVEM save regs"),
    (0x00FE_876E, "AllocMem 24218 chip bytes JSR"),
    (0x00FE_8774, "AllocMem result BNE.W test"),
    (0x00FE_8778, "AllocMem FAILED -> Alert path"),
    (0x00FE_8794, "AllocMem OK -> continuation"),
    (0x00FE_8888, "LoadView JSR"),
    (0x00FC_6374, "writes DyOffset"),
    (0x00FC_637A, "writes DxOffset"),
    (0x00FE_8850, "writes unknown @ View+$18"),
    (0x00FE_887A, "writes View.ViewPort low word"),
    (0x00FC_A682, "cprlist struct clear"),
];

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

fn count(label: &str, use_slow_ram: bool) {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = if use_slow_ram {
        AmigaOcs::with_slow_ram(rom, 512 * 1024)
    } else {
        AmigaOcs::new(rom)
    };
    let mut counts = vec![0u64; TARGETS.len()];
    let mut prev_pc = amiga.cpu().regs.pc;
    for _ in 0..(250u64 * PAL_FRAME_TICKS as u64) {
        amiga.tick();
        let pc = amiga.cpu().regs.pc;
        if pc == prev_pc {
            continue;
        }
        for (i, (tpc, _)) in TARGETS.iter().enumerate() {
            if pc == *tpc {
                counts[i] += 1;
            }
        }
        prev_pc = pc;
    }
    eprintln!("\n########## {label} ##########");
    for ((pc, desc), c) in TARGETS.iter().zip(counts.iter()) {
        eprintln!("  ${pc:08X}  {c:>6}  {desc}");
    }
}

#[test]
#[ignore]
fn compare_view_setup_paths() {
    count("slow-RAM", true);
    count("chip-only", false);
}
