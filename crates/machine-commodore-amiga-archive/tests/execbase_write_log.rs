//! Hook every CPU write to two ranges of interest during chip-only
//! KS 1.3 boot:
//!
//!  1. $00000000-$0000000A — vector table including ExecBase pointer at $4
//!     and BUSERR/ADDRERR vectors.
//!  2. $000008C0-$000008E2 — the chip-RAM MemHeader struct that overlaps
//!     the bootstrap ExecBase positive part by 24 bytes.
//!
//! What we want to learn:
//!  - Does ANY code attempt to write to $00000004 with a value other
//!    than the bootstrap $676? If yes, the swap-to-proper-ExecBase code
//!    runs but our value is wrong; if no, the swap path isn't reached.
//!  - Do writes to $8C2-$8DA (the overlap) corrupt ExecBase positive
//!    data, and at what PCs?

use std::path::PathBuf;
use machine_commodore_amiga::Amiga;

fn rom() -> Vec<u8> {
    let home = std::env::var("HOME").unwrap();
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    std::fs::read(&path).expect("read kick13.rom")
}

fn run_with_watch(label: &str, slow_ram: usize, watch: (u32, u32), frames: u64) {
    let mut amiga = if slow_ram == 0 {
        Amiga::new(rom())
    } else {
        Amiga::new_with_slow_ram(rom(), slow_ram)
    };
    amiga.debug_cpu_write_watch = Some(watch);

    eprintln!("===== {label}: watching ${:08X}..${:08X} =====", watch.0, watch.1);

    for _ in 0..frames {
        amiga.run_frame();
    }

    let log = &amiga.debug_cpu_write_log;
    if log.is_empty() {
        eprintln!("  (no writes to watched range)");
    } else {
        eprintln!("  {} writes captured (last 256):", log.len());
        for (pc, addr, size, val) in log.iter() {
            eprintln!("    pc=${pc:08X} → write {size} ${addr:06X} = ${val:08X}");
        }
    }
    eprintln!();
}

#[test]
#[ignore]
fn watch_writes_to_execbase_pointer_chip_only() {
    // Watch $0-$10 (vector table including ExecBase at $4).
    run_with_watch("chip-only / execbase-ptr range", 0, (0x000000, 0x000010), 250);
}

#[test]
#[ignore]
fn watch_writes_to_execbase_pointer_with_slow_ram() {
    run_with_watch("slow-RAM / execbase-ptr range", 512 * 1024, (0x000000, 0x000010), 250);
}

#[test]
#[ignore]
fn watch_writes_to_memheader_overlap_chip_only() {
    // Watch $8C0-$8E2 (MemHeader struct + overlap with bootstrap ExecBase).
    run_with_watch("chip-only / MemHeader overlap range", 0, (0x0008C0, 0x0008E2), 250);
}

#[test]
#[ignore]
fn watch_writes_to_memheader_overlap_with_slow_ram() {
    run_with_watch("slow-RAM / MemHeader overlap range", 512 * 1024, (0x0008C0, 0x0008E2), 250);
}
