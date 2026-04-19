//! Kickstart 1.3 boot invariants — locks in the behaviour that the
//! 2026-04-19 chip-bus-arbitration fix unblocked (CPU was being stalled
//! by Agnus DMA arbitration even when accessing ROM).
//!
//! These tests boot the real Kickstart 1.3 ROM and assert specific
//! observable state by frame ~210 — at which point the iconic
//! "insert Workbench" screen with the floating disk-and-hand graphic
//! should be fully composed.
//!
//! Each test is `#[ignore]` because it depends on a real Kickstart 1.3
//! ROM at `~/.emu198x/roms/commodore-amiga/kick13.rom` and takes 1-3
//! seconds. Skipped silently with a stderr note when the ROM is absent.
//!
//! Run with:
//!   cargo test -p machine-commodore-amiga --test kickstart_boot_invariants -- --ignored

use std::path::PathBuf;

use machine_commodore_amiga::Amiga;

/// Frames to run before sampling state. The script runner reaches the
/// insert-disk screen by ~210 frames; 250 gives a small margin.
const KICKSTART_BOOT_FRAMES: u64 = 250;

fn kickstart13_path() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME is set");
    PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom")
}

/// Returns `Some(amiga)` after booting Kickstart 1.3 to the insert-disk
/// screen, or `None` (with a stderr skip note) when the ROM is absent.
fn boot_kickstart13() -> Option<Amiga> {
    let path = kickstart13_path();
    if !path.exists() {
        eprintln!(
            "skipping: Kickstart 1.3 ROM missing at {}",
            path.display()
        );
        return None;
    }
    let kickstart = std::fs::read(&path).expect("read Kickstart 1.3 ROM");
    // Match runtime config: 512 KiB trapdoor slow RAM at $C00000.
    // Kickstart 1.2/1.3 need somewhere to put ExecBase that isn't
    // chip RAM, otherwise boot diverges in subtle ways.
    let mut amiga = Amiga::new_with_slow_ram(kickstart, 512 * 1024);
    for _ in 0..KICKSTART_BOOT_FRAMES {
        amiga.run_frame();
    }
    Some(amiga)
}

/// Read ExecBase from low memory ($00000004), validating that the value
/// looks like a sane chip- or slow-RAM address (even-aligned, in range).
fn read_exec_base(amiga: &Amiga) -> Option<u32> {
    let value = (u32::from(amiga.memory.read_word(0x000004)) << 16)
        | u32::from(amiga.memory.read_word(0x000006));
    let in_chip = (0x00_0400..0x10_0000).contains(&value);
    let in_slow = (0xC0_0000..0xC8_0000).contains(&value);
    if (in_chip || in_slow) && value & 1 == 0 {
        Some(value)
    } else {
        None
    }
}

// ─── Display invariants ────────────────────────────────────────────

#[test]
#[ignore]
fn kick13_renders_insert_disk_screen() {
    let Some(amiga) = boot_kickstart13() else { return };

    // The insert-disk screen is mostly white background — most pixels
    // should be non-black.
    let fb = amiga.framebuffer();
    let non_black = fb.iter().filter(|&&p| (p & 0x00FF_FFFF) != 0).count();
    assert!(
        non_black > 100_000,
        "expected >100k non-black pixels on insert-disk screen, got {non_black}"
    );
}

#[test]
#[ignore]
fn kick13_palette_matches_insert_disk_screen() {
    let Some(amiga) = boot_kickstart13() else { return };

    // Kickstart 1.3 insert-disk screen sets:
    //   color00 = $0FFF (white background)
    //   color01 = $0000 (black foreground)
    assert_eq!(
        amiga.denise.palette[0] & 0x0FFF,
        0x0FFF,
        "color00 should be white ($0FFF)"
    );
    assert_eq!(
        amiga.denise.palette[1] & 0x0FFF,
        0x000,
        "color01 should be black ($0000)"
    );
}

// ─── Chipset-state invariants ──────────────────────────────────────

#[test]
#[ignore]
fn kick13_dmacon_has_display_channels_enabled() {
    let Some(amiga) = boot_kickstart13() else { return };

    let dmacon = amiga.agnus.dmacon;
    // At the insert-disk screen: master DMAEN, plus the channels
    // Kickstart needs to compose the screen and poll for a disk.
    assert_ne!(dmacon & 0x0200, 0, "DMAEN should be set, dmacon=${dmacon:04X}");
    assert_ne!(dmacon & 0x0100, 0, "BPLEN should be set, dmacon=${dmacon:04X}");
    assert_ne!(dmacon & 0x0080, 0, "COPEN should be set, dmacon=${dmacon:04X}");
    assert_ne!(dmacon & 0x0040, 0, "BLTEN should be set, dmacon=${dmacon:04X}");
    assert_ne!(dmacon & 0x0010, 0, "DSKEN should be set, dmacon=${dmacon:04X}");
    // SPREN intentionally not asserted: the disk-and-hand graphic is
    // a 3-bitplane image composed by the blitter, not sprites.
}

// NOTE: Kickstart 1.3 leaves BPLCON0 with BPU=0 at the insert-disk
// screen even though bitplane DMA is fetching data (BPLEN=1) and the
// graphic is visible. Our Denise emulation accepts this and renders
// whatever's in the BPLnDAT shift registers. Whether that matches
// real Denise silicon is a separate question — track in a follow-up
// rather than asserting here.

#[test]
#[ignore]
fn kick13_copper_is_executing_its_display_list() {
    let Some(mut amiga) = boot_kickstart13() else { return };

    let cop1lc = amiga.copper.cop1lc;
    let pc_a = amiga.copper.pc;
    assert!(cop1lc > 0, "cop1lc should point to a real copper list");
    assert!(pc_a > 0, "copper.pc should be non-zero (copper running)");

    // Sanity: copper PC should sit within or near the list it's
    // executing (lists are at most a few KB).
    let pc_near_list = (cop1lc..cop1lc.wrapping_add(0x1000)).contains(&pc_a);
    assert!(
        pc_near_list,
        "copper.pc (${pc_a:08X}) should be within $1000 bytes of cop1lc (${cop1lc:08X})"
    );

    // After another frame, copper should have ticked again — restart
    // happens at vpos==0 so PC will reset to cop1lc area.
    amiga.run_frame();
    let pc_b = amiga.copper.pc;
    assert!(pc_b > 0, "copper.pc should still be running after another frame");
}

// ─── OS-state invariants ───────────────────────────────────────────

#[test]
#[ignore]
fn kick13_execbase_is_set_in_low_memory() {
    let Some(amiga) = boot_kickstart13() else { return };

    let Some(base) = read_exec_base(&amiga) else {
        panic!(
            "ExecBase pointer at $00000004 not in expected range — \
             chip RAM ($000400-$0FFFFF) or slow RAM ($C00000-$C7FFFF)"
        );
    };
    assert_eq!(base & 1, 0, "ExecBase must be even-aligned, got ${base:08X}");
}

#[test]
#[ignore]
fn kick13_no_alert_raised_during_boot() {
    let Some(amiga) = boot_kickstart13() else { return };

    let Some(base) = read_exec_base(&amiga) else { return };
    // ExecBase + $0202..$0205 holds the most recent alert (if any).
    // $FFFFFFFF means no alert has been raised.
    let last_alert = (u32::from(amiga.memory.read_word(base.wrapping_add(0x202))) << 16)
        | u32::from(amiga.memory.read_word(base.wrapping_add(0x204)));
    assert_eq!(
        last_alert, 0xFFFF_FFFF,
        "Exec raised an Alert during boot: ${last_alert:08X}"
    );
}
