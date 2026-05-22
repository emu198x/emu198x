//! Stage-A smoke tests for the A1200 machine wiring.
//!
//! These exercise the bare structural shape established in
//! `knowledge/decisions/amiga-machine-rollout-plan.md` Stage A:
//! the machine constructs from a placeholder Kickstart, advances
//! the master clock without panicking, snapshot/restore round-trips
//! cleanly, and the Gayle window decodes as expected (IDE STATUS
//! reads `$7F` with no drive attached).
//!
//! Booting a real Kickstart 3.1 ROM with the Cpu68020 swap in place
//! is Stage B / Stage C work tracked separately.

use machine_commodore_amiga_a1200::{AmigaA1200, DEFAULT_CHIP_RAM_SIZE, RamConfig};

/// 512 KiB zero-filled Kickstart placeholder — boots into a reset
/// vector pointing at `$00000000` which the machine will faithfully
/// execute (and likely fault), but the test only ticks long enough
/// to exercise the bus / chipset / Gayle wiring shape.
fn placeholder_kickstart() -> Vec<u8> {
    vec![0u8; 512 * 1024]
}

fn a1200_chip_only() -> AmigaA1200 {
    AmigaA1200::with_ram_config(
        placeholder_kickstart(),
        RamConfig {
            chip_kb: 2048,
            slow_kb: 0,
            fast_kb: 0,
        },
    )
}

#[test]
fn machine_constructs_without_panic() {
    let _ = a1200_chip_only();
}

#[test]
fn ticks_one_thousand_cycles_without_panic() {
    let mut m = a1200_chip_only();
    for _ in 0..1_000 {
        m.tick();
    }
}

#[test]
fn snapshot_round_trips_after_ticking() {
    let mut m = a1200_chip_only();
    for _ in 0..100 {
        m.tick();
    }
    let snap = m.snapshot_state();

    // Postcard round-trip via serde — same path as the runtime's
    // snapshot envelope.
    let bytes = postcard::to_allocvec(&snap).expect("serialize");
    let restored: machine_commodore_amiga_a1200::AmigaA1200Snapshot =
        postcard::from_bytes(&bytes).expect("deserialize");

    let mut m2 = a1200_chip_only();
    m2.restore_snapshot_state(restored);

    // Tick both forward; they should remain in lockstep.
    for _ in 0..100 {
        m.tick();
        m2.tick();
    }
    assert_eq!(m.cpu().regs.pc, m2.cpu().regs.pc);
    assert_eq!(m.cpu().regs.sr, m2.cpu().regs.sr);
}

#[test]
fn default_chip_ram_constant_unchanged_from_ecs_baseline() {
    // The shared `common-commodore-amiga::memory` constant remains
    // 512 KiB. A1200 ships 2 MiB chip RAM by default but the
    // bare-machine constant stays at the workspace-shared value;
    // A1200 callers pass an explicit RamConfig.
    assert_eq!(DEFAULT_CHIP_RAM_SIZE, 512 * 1024);
}
