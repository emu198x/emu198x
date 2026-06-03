//! MTX boot I/O trace — regression marker for the paging + I/O-map fix.
//!
//! Before the fix the boot derailed the instant the power-on RAM-sizing loop
//! wrote a page number to port `$00`: the donor paging model swapped the
//! executing OS ROM out for RAM and the CPU ran off into zeroed memory
//! (PC ≈ `$1A65`), touching nothing else. With MEMU's paging and I/O port map
//! ([`machine_memotech_mtx`] docs), the boot now completes all power-on
//! hardware init — RAM sizing, the country-code read on port `$06`, and ROM
//! subpage enumeration — staying in the ROM the whole time.
//!
//! It does **not** yet reach BASIC `Ready`: a `RST $28` ROM-routine system
//! call then paths into an absent ROM subpage and restarts. This test asserts
//! the progress that *is* solid; the reset loop is tracked in
//! `knowledge/systems/memotech-mtx.md`.
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
//!   2. `~/.emu198x/roms/memotech-mtx/mtx.rom`

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
#[ignore = "needs MTX ROM — run with --ignored --nocapture"]
fn boot_completes_power_on_init() {
    let Some(path) = rom_path() else {
        panic!(
            "MTX ROM not found — set EMU198X_MTX_ROM or place mtx.rom \
             at ~/.emu198x/roms/memotech-mtx/"
        );
    };
    let rom = fs::read(&path).expect("read ROM");
    assert_eq!(rom.len(), 0x4000, "ROM must be 16 KB");

    let mut sys = Mtx::new(rom, MtxModel::Mtx512).expect("init");
    sys.start_io_trace();
    for _ in 0..120 {
        sys.run_frame();
    }
    let events = sys.take_io_trace();

    // The boot must never derail into RAM: the OS ROM stays mapped, so every
    // instruction-fetch site (and thus every I/O site) sits inside the ROM.
    assert!(
        events.iter().all(|e| e.pc < 0x4000),
        "boot left the ROM — paging derailed it (max PC {:04X})",
        events.iter().map(|e| e.pc).max().unwrap_or(0)
    );

    // It reaches ROM-subpage enumeration: it writes the ROM-page nibble
    // ($10..$70) to port $00 from the $01F2 loop.
    let enumerates_rom = events
        .iter()
        .any(|e| e.write && e.port == 0x00 && (0x10..=0x70).contains(&e.value));
    assert!(enumerates_rom, "boot never reached ROM enumeration");

    // It reads the country code on port $06 and (English machine, no key)
    // gets 0x03 — proving the keyboard sense-high port is correct.
    let country_read = events.iter().find(|e| !e.write && e.port == 0x06);
    let country = country_read.expect("boot never read the country port $06");
    assert_eq!(
        country.value, 0x03,
        "country/sense-high read should be 0x03"
    );
}
