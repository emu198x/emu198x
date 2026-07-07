//! Boot smoke test for the real 1571 DOS ROM (310654-05).
//!
//! `#[ignore]`d: needs the ROM staged at
//! `~/.emu198x/roms/commodore-c64/1571.rom`. Run with:
//!
//!     cargo test -p machine-commodore-1571 --test boot_smoke -- --ignored --nocapture
//!
//! Boots the drive standalone (no IEC bus) and reports where the CPU settles.
//! A healthy drive reaches a small idle-loop PC range; a hang on peripheral
//! init (the 1581 lesson) shows as a stuck PC or a tight non-progressing loop.

use std::collections::BTreeMap;
use std::path::PathBuf;

use machine_commodore_1571::{Drive1571, Drive1571Config};

fn rom_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join(".emu198x/roms/commodore-c64/1571.rom")
}

#[test]
#[ignore = "needs the real 1571 DOS ROM at ~/.emu198x/roms/commodore-c64/1571.rom"]
fn boots_to_an_idle_loop() {
    let rom = std::fs::read(rom_path()).expect("stage the 1571 DOS ROM first");
    assert_eq!(rom.len(), 0x8000, "1571 DOS ROM is 32 KB");

    let mut drive = Drive1571::new(Drive1571Config { dos_rom: &rom }).expect("valid ROM");

    // Run ~6M cycles (~3s at 2 MHz) — the 1581 reached idle well inside this.
    // Track page transitions to see the boot phases, and histogram the settled
    // tail to locate the idle loop.
    let mut prev_page = 0xFFFFu16;
    let mut phases: Vec<(u64, u16)> = Vec::new();
    let mut tail: BTreeMap<u16, u32> = BTreeMap::new();
    let total = 6_000_000u64;
    let tail_start = total - 200_000;

    for c in 0..total {
        drive.tick();
        let pc = drive.cpu().regs.pc;
        let page = pc >> 8;
        if page != prev_page {
            if phases.len() < 400 {
                phases.push((c, pc));
            }
            prev_page = page;
        }
        if c >= tail_start {
            *tail.entry(pc).or_default() += 1;
        }
    }

    println!("\n=== 1571 boot smoke ===");
    println!("  page-transition trail (cycle: PC):");
    for (c, pc) in phases.iter().take(120) {
        print!("  {c}:${pc:04X}");
    }
    println!();

    let mut top: Vec<(u16, u32)> = tail.into_iter().collect();
    top.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
    println!("  settled-tail top PCs (last 200k cycles):");
    for (pc, n) in top.iter().take(12) {
        println!("    ${pc:04X}: {n}");
    }
    let final_pc = drive.cpu().regs.pc;
    let i_flag = (drive.cpu().regs.p >> 2) & 1;
    println!("  final PC=${final_pc:04X}  I-flag={i_flag}");
    // Distinct PCs in the tail: a tight idle loop is a handful; scattered means
    // it never settled.
    println!("  distinct tail PCs: {}", top.len());

    // A healthy drive reaches an interruptible idle (I-flag clear) and spends
    // the tail concentrated in a small loop rather than a stuck/scattered hang.
    assert_eq!(i_flag, 0, "drive should reach an interruptible idle (I=0)");
    assert!(
        top[0].1 > 5_000,
        "idle should dwell in a tight loop; top PC ${:04X} only seen {} times",
        top[0].0,
        top[0].1
    );
}
