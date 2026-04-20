//! Trap the init-function entry points of the critical ROM
//! residents to see which ones actually run during Exec's
//! InitCode phase.
//!
//! ROM layout (from scanning rt_MatchWord=$4AFC records in the
//! KS 1.3 image):
//!
//! | Resident          | Pri  | Init func    | Notes               |
//! |-------------------|-----:|--------------|---------------------|
//! | exec.library      |  120 | $00FC00D2    | (runs first)        |
//! | expansion.library |  110 | $00FC4BA0    | AUTOINIT            |
//! | graphics.library  |   65 | $00FCABA2    |                     |
//! | layers.library    |   31 | $00FE0A2C    |                     |
//! | trackdisk.device  |   20 | $00FE97BE    |                     |
//! | intuition.library |   10 | $00FD3DB6    | AUTOINIT            |
//! | romboot.library   |  -40 | $00FEB0A8    |                     |
//! | strap             |  -60 | $00FE8444    | runs last — disk/   |
//! |                   |      |              | insert-disk screen  |
//!
//! The missing display setup is almost certainly driven by strap
//! (or romboot) — they're the low-priority "I run last after
//! everyone else is up" residents that handle disk boot and its
//! no-disk fallback.
//!
//! Counts + first-hit tick for each init entry, from frame 0.

use std::path::PathBuf;
use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};

// Resident init function addresses from ROM scan.
const INIT_FNS: &[(u32, &str)] = &[
    (0x00FC_4BA0, "expansion  "),
    (0x00FC_ABA2, "graphics   "),
    (0x00FE_0A2C, "layers     "),
    (0x00FE_97BE, "trackdisk  "),
    (0x00FD_3DB6, "intuition  "),
    (0x00FE_B0A8, "romboot    "),
    (0x00FE_8444, "strap      "),
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

fn run(amiga: &mut AmigaOcs, label: &str) {
    eprintln!("\n########## {label} ##########");

    let mut hits: Vec<(u32, &'static str, u64, u64)> = INIT_FNS
        .iter()
        .map(|(a, n)| (*a, *n, 0, u64::MAX))
        .collect();
    let mut prev_pc = amiga.cpu().regs.pc;
    let mut tick = 0u64;

    for _ in 0..(400 * PAL_FRAME_TICKS) {
        amiga.tick();
        tick += 1;
        let pc = amiga.cpu().regs.pc;
        if pc == prev_pc {
            continue;
        }
        for entry in hits.iter_mut() {
            if pc == entry.0 {
                entry.2 += 1;
                if entry.3 == u64::MAX {
                    entry.3 = tick;
                }
            }
        }
        prev_pc = pc;
    }

    eprintln!(
        "=== Resident init function hits (400 frames, {} ticks) ===",
        tick
    );
    eprintln!("  {:<14}  {:>8}  {:>14}", "Resident", "hits", "first-tick");
    for (addr, name, count, first) in &hits {
        let first_str = if *first == u64::MAX {
            "(never)".to_string()
        } else {
            let cck = first / 2;
            let frame = cck / 70824;
            format!("{first} (frame~{frame})")
        };
        eprintln!("  {name}  ${addr:08X}  {count:>6}  {first_str}");
    }

    let not_run: Vec<_> = hits.iter().filter(|(_, _, _, f)| *f == u64::MAX).collect();
    if !not_run.is_empty() {
        eprintln!("\n→ Not run in 400 frames:");
        for (addr, name, _, _) in &not_run {
            eprintln!("  {name}  ${addr:08X}");
        }
    }
}

#[test]
#[ignore]
fn trap_resident_init_functions() {
    let Some(rom) = load_kickstart() else { return };
    let mut slow = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    run(&mut slow, "slow-RAM");

    let mut chip_only = AmigaOcs::new(rom);
    run(&mut chip_only, "chip-only");
}
