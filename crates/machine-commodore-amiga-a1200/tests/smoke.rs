//! Stage-A / Stage-B smoke tests for the A1200 machine wiring.
//!
//! Stage A established the bare structural shape (machine constructs,
//! ticks, snapshot round-trips). Stage B swapped the CPU from
//! `Cpu68000` to `Cpu68020`; the swap is observable via the
//! variant_decode_hook the wrapper installs on the inner core.
//!
//! Booting a real Kickstart 3.1 ROM is Stage C, tracked separately
//! in `knowledge/decisions/amiga-machine-rollout-plan.md`.

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

#[test]
fn machine_runs_on_cpu68020_with_variant_hooks_installed() {
    // Stage B: Cpu68020 wraps Cpu68010 wraps Cpu68000. The 68020 /
    // 68010 hook installation should be visible at the Cpu68000
    // layer through the deref chain.
    let m = a1200_chip_only();
    let cpu = m.cpu();
    assert!(
        cpu.variant_decode_hook.is_some(),
        "Cpu68020 should have installed decode_68020_opcode on inner Cpu68000"
    );
    assert!(
        cpu.variant_continue_hook.is_some(),
        "Cpu68010 layer should have installed continue_68010_opcode"
    );
    // 68020-specific flags
    assert!(cpu.variant_scaled_index, "68020 enables scaled index");
    assert!(cpu.variant_extended_sr_writes, "68020 widens SR write mask");
    assert!(
        cpu.variant_format2_vectors,
        "68020 uses Format-$2 exception frames"
    );
    // 68010-specific flags inherited through Deref
    assert!(
        cpu.variant_six_word_frame,
        "68010+ uses 8-byte exception frame"
    );
    assert!(
        cpu.variant_musashi_bcd_v,
        "68010+ uses Musashi BCD V semantics"
    );
    assert!(
        cpu.variant_musashi_div_overflow,
        "68010+ uses Musashi DIV overflow semantics"
    );
}

#[test]
fn snapshot_round_trip_preserves_cpu68020_variant_hooks() {
    // The Cpu68000 fields backing the hooks are `#[serde(skip)]`,
    // so a naive deserialize would zero them out. Cpu68020's custom
    // Deserialize re-installs the hooks; this test confirms the
    // round-trip path lands a fully-configured 68020 on the other
    // side.
    let mut m = a1200_chip_only();
    for _ in 0..50 {
        m.tick();
    }
    let snap = m.snapshot_state();
    let bytes = postcard::to_allocvec(&snap).expect("serialize");
    let restored: machine_commodore_amiga_a1200::AmigaA1200Snapshot =
        postcard::from_bytes(&bytes).expect("deserialize");

    let mut m2 = a1200_chip_only();
    m2.restore_snapshot_state(restored);

    let cpu = m2.cpu();
    assert!(cpu.variant_decode_hook.is_some());
    assert!(cpu.variant_continue_hook.is_some());
    assert!(cpu.variant_scaled_index);
    assert!(cpu.variant_extended_sr_writes);
    assert!(cpu.variant_format2_vectors);
    assert!(cpu.variant_six_word_frame);
}
