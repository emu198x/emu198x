//! MTX boot smoke — OS + BASIC + ASSEM to the BASIC `Ready` prompt.
//!
//! The full power-on path: RAM sizing, country-code read on `$06`, ROM-subpage
//! enumeration, then a `RST $28` ROM-routine system call into the **ASSEM**
//! ROM (paged subpage 1) that brings up the VDP display and the BASIC main
//! loop. It needs the complete firmware — 8 KB OS + BASIC + ASSEM (24 KB). An
//! OS+BASIC-only (16 KB) image stops at that first ASSEM call and never
//! renders.
//!
//! Gated `#[ignore]` because the ROM is copyrighted and not shipped in-tree.
//!
//! Run with:
//! ```text
//! cargo test --release -p machine-memotech-mtx \
//!     --test boot_trace -- --ignored --nocapture
//! ```
//!
//! ROM source (first match wins):
//!   1. `EMU198X_MTX_ROM` env var (full file path)
//!   2. `~/.emu198x/roms/memotech-mtx/mtx.rom` (OS + BASIC + ASSEM)

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_memotech_mtx::{Mtx, MtxModel};

fn rom_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_MTX_ROM") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/memotech-mtx/mtx.rom");
    p.exists().then_some(p)
}

#[test]
#[ignore = "FIXTURE: needs MTX OS+BASIC+ASSEM ROM — run with --ignored"]
fn boots_to_basic_ready() {
    let Some(path) = rom_path() else {
        panic!(
            "MTX ROM not found — set EMU198X_MTX_ROM or place mtx.rom \
             (OS + BASIC + ASSEM) at ~/.emu198x/roms/memotech-mtx/"
        );
    };
    let rom = fs::read(&path).expect("read ROM");
    assert!(
        rom.len() >= 0x6000,
        "boot to Ready needs OS + BASIC + ASSEM (24 KB); got {} bytes — an \
         OS+BASIC-only image stops at the first ASSEM call",
        rom.len()
    );

    let mut sys = Mtx::new(rom, MtxModel::Mtx512).expect("init");
    sys.start_io_trace();
    for _ in 0..120 {
        sys.run_frame();
    }
    let events = sys.take_io_trace();

    // The boot programs the VDP heavily (display + screen RAM) — proof it got
    // through the ASSEM cold-start, not just the early hardware probe.
    let vdp_writes = events
        .iter()
        .filter(|e| e.write && (e.port == 0x01 || e.port == 0x02))
        .count();
    assert!(
        vdp_writes > 1000,
        "VDP barely programmed ({vdp_writes} writes) — boot did not complete"
    );

    // It runs the BASIC main loop: the keyboard is scanned on $05 and the
    // country/sense-high read on $06 is 0x03 (English, no key).
    assert!(
        events.iter().any(|e| !e.write && e.port == 0x05),
        "keyboard never scanned — no BASIC main loop"
    );
    assert!(
        events
            .iter()
            .any(|e| !e.write && e.port == 0x06 && e.value == 0x03),
        "country/sense-high read should be 0x03"
    );

    // The interrupt path now runs through the Z80 CTC: the OS programs it at
    // $08-$0B, and channel 0 (fed by the VDP /INT) is the live timebase driving
    // the IRQ. Positive proof the CTC is in the loop, not inert — the boot
    // reaches Ready only because these interrupts arrive.
    assert!(
        events
            .iter()
            .any(|e| e.write && (0x08..=0x0B).contains(&e.port)),
        "CTC never programmed — the OS should write $08-$0B"
    );
    assert!(
        sys.ctc().running(0) && sys.ctc().int_enabled(0),
        "CTC channel 0 should be running with interrupts enabled after boot"
    );

    // The screen is painted: a real text frame, not the all-backdrop blank of
    // the pre-fix stall.
    let fb = sys.framebuffer();
    assert_eq!(
        fb.len() as u32,
        sys.framebuffer_width() * sys.framebuffer_height()
    );
    let non_zero = fb.iter().filter(|&&px| px & 0x00FF_FFFF != 0).count();
    let distinct: std::collections::HashSet<u32> = fb.iter().copied().collect();
    assert!(
        non_zero >= 1000 && distinct.len() >= 2,
        "screen not rendered: {non_zero} non-backdrop px, {} colours",
        distinct.len()
    );
}
