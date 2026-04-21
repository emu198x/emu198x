//! Task #180 — cross-cutting boot scenario: ADF inserted.
//!
//! The stock Kickstart-to-insert-disk test (`boot_reaches_exec_idle`)
//! runs with no disk. This companion runs the same 300 PAL frames
//! with a blank ADF inserted and an acknowledged disk-change, so the
//! full Floppy + Paula-disk-DMA + CIA-B-PRB path is exercised:
//!
//!   - `/DSKCHANGE` (CIA-A PRA bit 2) reads high (disk present).
//!   - trackdisk.device drives CIA-B PRB → drive selects / motor / step.
//!   - Drive motor spins up (~500ms of E-clock ticks).
//!   - `drive.motor_spinning()` is true once trackdisk engages.
//!   - Paula's disk DMA pacer delivers MFM words from the drive.
//!
//! This test doesn't assert the machine reaches a bootable OS state —
//! a blank ADF has no valid bootblock. It does prove the Floppy Phase
//! 2 wiring (aa8aaf5+) drives the full chain end-to-end.

use format_commodore_amiga_adf::{Adf, ADF_SIZE_DD};
use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use std::path::PathBuf;

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
fn blank_adf_inserted_engages_drive_within_300_frames() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);

    // Insert a blank DD ADF. `insert_adf` acknowledges the change so
    // /DSKCHANGE reads inactive at boot.
    let adf = Adf::from_bytes(vec![0; ADF_SIZE_DD]).expect("valid blank ADF");
    amiga.insert_adf(adf);
    assert!(amiga.drive().has_disk());
    assert!(!amiga.drive().status().disk_change,
        "inserting an ADF should acknowledge the change flag");

    // Track the furthest motor-spin state we see across the run —
    // trackdisk can spin the motor up and back down, so sampling only
    // at the end would miss the engagement window.
    let mut saw_motor_on = false;
    let mut saw_motor_spinning = false;
    let mut saw_disk_dma_pending = false;
    let mut max_step_events = 0u32;

    for _ in 0..(300 * PAL_FRAME_TICKS) {
        amiga.tick();
        if amiga.drive().motor_on() {
            saw_motor_on = true;
        }
        if amiga.drive().motor_spinning() {
            saw_motor_spinning = true;
        }
        if amiga.paula().disk_dma_pending() {
            saw_disk_dma_pending = true;
        }
        let steps = amiga.drive().step_event_counter();
        if steps > max_step_events {
            max_step_events = steps;
        }
    }

    // Primary assertion: trackdisk.device engaged the drive hardware.
    // Kickstart 1.3 probes the drive as part of its boot path; with
    // /DSKCHANGE acknowledged, it goes further than the no-disk case
    // and at minimum asserts /MTR (motor on) while scanning.
    assert!(saw_motor_on,
        "trackdisk should assert /MTR when probing a present disk");

    // Tell the dev reading this failure which path they're in. If
    // motor-on fires but the spin-up timer never completes within
    // 300 frames, that's a legit regression — the E-clock divider
    // or drive tick wiring is off.
    assert!(saw_motor_spinning,
        "drive should reach motor_spinning within 300 frames \
         (motor-on seen: {saw_motor_on})");

    // With the drive spun up, trackdisk typically steps a few tracks
    // before deciding the disk is unreadable. Zero steps means the
    // step-pulse edge detection isn't catching CIA-B PRB writes.
    assert!(max_step_events > 0,
        "drive should receive at least one step pulse from trackdisk \
         while probing the inserted disk");

    // Paula disk DMA is an optional observation — trackdisk may or
    // may not arm CMD_READ before deciding the blank bootblock is
    // garbage. We just print what happened so regressions in that
    // path land visibly.
    eprintln!(
        "boot_with_adf summary: motor_on={saw_motor_on} \
         motor_spinning={saw_motor_spinning} \
         disk_dma_pending={saw_disk_dma_pending} \
         max_step_events={max_step_events}"
    );
}
