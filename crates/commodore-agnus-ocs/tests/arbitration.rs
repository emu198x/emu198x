//! Phase 1 characterization tests — Agnus DMA slot arbitration.
//!
//! Per HRM Chapter 6 Table 6-1 (DMA time slot allocation).
//!
//! The 227-CCK line partitions into fixed and variable regions:
//!
//! ```text
//!   hpos 0x00          free (CPU)
//!   hpos 0x01-0x03     memory refresh (3 slots)
//!   hpos 0x04-0x06     disk DMA (3 slots, gated by DMACON.DSKEN)
//!   hpos 0x07-0x0A     audio DMA (4 slots, one per channel)
//!   hpos 0x0B-0x1A     sprite DMA (8 sprites × 2 slots each)
//!   hpos 0x1B          refresh
//!   hpos 0x1C..=0xE2   bitplane / copper / CPU (display fetch window)
//! ```
//!
//! Chip-level gates:
//!   - Every DMA channel needs DMACON.DMAEN (bit 9) set.
//!   - Per-channel DMACON bit gates each group.
//!   - Bitplane DMA also needs BPLCON0.BPU > 0 and hpos within the
//!     DDF fetch window.

use commodore_agnus_ocs::{Agnus, SlotOwner};

/// DMACON bits used by this test file.
const DMAEN: u16 = 0x0200;
const DSKEN: u16 = 0x0010;
const AUD0EN: u16 = 0x0001;
const AUD1EN: u16 = 0x0002;
const AUD2EN: u16 = 0x0004;
const AUD3EN: u16 = 0x0008;
const SPREN: u16 = 0x0020;
const BPLEN: u16 = 0x0100;
const COPEN: u16 = 0x0080;
const BLTEN: u16 = 0x0040;
const BLTPRI: u16 = 0x0400;

fn agnus_with_dmacon(dmacon: u16) -> Agnus {
    let mut a = Agnus::new();
    a.dmacon = dmacon;
    a
}

fn at_hpos(agnus: &mut Agnus, hpos: u16) {
    agnus.hpos = hpos;
}

// ────────────────────────────────────────────────────────────────
// Fixed-slot table
// ────────────────────────────────────────────────────────────────

#[test]
fn hpos_0_is_cpu_slot() {
    let mut a = agnus_with_dmacon(DMAEN);
    at_hpos(&mut a, 0);
    assert_eq!(a.current_slot(), SlotOwner::Cpu);
}

#[test]
fn hpos_1_through_3_are_refresh_slots() {
    let mut a = agnus_with_dmacon(DMAEN | DSKEN | AUD0EN | SPREN);
    for hpos in 1..=3 {
        at_hpos(&mut a, hpos);
        assert_eq!(
            a.current_slot(),
            SlotOwner::Refresh,
            "hpos {hpos:#x} is always refresh regardless of DMACON"
        );
    }
}

#[test]
fn hpos_1b_is_the_final_refresh_slot() {
    let mut a = agnus_with_dmacon(DMAEN | BPLEN);
    at_hpos(&mut a, 0x1B);
    assert_eq!(a.current_slot(), SlotOwner::Refresh);
}

// ────────────────────────────────────────────────────────────────
// Disk slots (4-6) — gated by DMACON.DSKEN
// ────────────────────────────────────────────────────────────────

#[test]
fn disk_slots_4_5_6_granted_when_dsken_and_master_enabled() {
    let mut a = agnus_with_dmacon(DMAEN | DSKEN);
    for hpos in 4..=6 {
        at_hpos(&mut a, hpos);
        assert_eq!(a.current_slot(), SlotOwner::Disk);
    }
}

#[test]
fn disk_slots_fall_back_to_cpu_without_dsken() {
    let mut a = agnus_with_dmacon(DMAEN); // master on, DSKEN off
    for hpos in 4..=6 {
        at_hpos(&mut a, hpos);
        assert_eq!(
            a.current_slot(),
            SlotOwner::Cpu,
            "hpos {hpos:#x}: no DSKEN → CPU"
        );
    }
}

#[test]
fn disk_slots_fall_back_to_cpu_without_master_enable() {
    // DSKEN set but DMAEN clear → no DMA happens at all.
    let mut a = agnus_with_dmacon(DSKEN);
    for hpos in 4..=6 {
        at_hpos(&mut a, hpos);
        assert_eq!(a.current_slot(), SlotOwner::Cpu);
    }
}

// ────────────────────────────────────────────────────────────────
// Audio slots (7-A) — per-channel gates
// ────────────────────────────────────────────────────────────────

#[test]
fn audio_channels_each_own_exactly_one_slot() {
    let mut a = agnus_with_dmacon(DMAEN | AUD0EN | AUD1EN | AUD2EN | AUD3EN);
    for (hpos, expected_ch) in [(0x07u16, 0u8), (0x08, 1), (0x09, 2), (0x0A, 3)] {
        at_hpos(&mut a, hpos);
        assert_eq!(
            a.current_slot(),
            SlotOwner::Audio(expected_ch),
            "hpos {hpos:#x} → audio {expected_ch}"
        );
    }
}

#[test]
fn audio_channel_disabled_individually_yields_cpu() {
    let mut a = agnus_with_dmacon(DMAEN | AUD0EN | AUD2EN); // 1 + 3 disabled
    at_hpos(&mut a, 0x08);
    assert_eq!(a.current_slot(), SlotOwner::Cpu);
    at_hpos(&mut a, 0x0A);
    assert_eq!(a.current_slot(), SlotOwner::Cpu);
    at_hpos(&mut a, 0x07);
    assert_eq!(a.current_slot(), SlotOwner::Audio(0));
    at_hpos(&mut a, 0x09);
    assert_eq!(a.current_slot(), SlotOwner::Audio(2));
}

// ────────────────────────────────────────────────────────────────
// Sprite slots (B-1A) — 8 sprites × 2 slots each
// ────────────────────────────────────────────────────────────────

#[test]
fn sprite_slots_map_channel_to_hpos_pairs() {
    let mut a = agnus_with_dmacon(DMAEN | SPREN);
    for ch in 0u8..8 {
        let base = 0x0B + u16::from(ch) * 2;
        at_hpos(&mut a, base);
        assert_eq!(
            a.current_slot(),
            SlotOwner::Sprite(ch),
            "sprite {ch} first slot at hpos {base:#x}"
        );
        at_hpos(&mut a, base + 1);
        assert_eq!(
            a.current_slot(),
            SlotOwner::Sprite(ch),
            "sprite {ch} second slot at hpos {:#x}",
            base + 1
        );
    }
}

#[test]
fn sprite_slots_fall_back_to_cpu_without_spren() {
    let mut a = agnus_with_dmacon(DMAEN);
    at_hpos(&mut a, 0x0B);
    assert_eq!(a.current_slot(), SlotOwner::Cpu);
}

// ────────────────────────────────────────────────────────────────
// Bitplane fetch window (1C..=E2)
// ────────────────────────────────────────────────────────────────

fn program_standard_lowres(a: &mut Agnus) {
    a.bplcon0 = 0x3000; // BPU = 3 (3 planes)
    a.ddfstrt = 0x0038; // canonical LORES DDFSTRT
    a.ddfstop = 0x00D0; // canonical LORES DDFSTOP
}

#[test]
fn outside_fetch_window_even_slots_go_to_copper_when_enabled() {
    let mut a = agnus_with_dmacon(DMAEN | COPEN | BPLEN);
    program_standard_lowres(&mut a);
    // Between 0x1C (post-sprite refresh) and DDFSTRT = 0x38 is free —
    // copper gets even slots.
    at_hpos(&mut a, 0x1C);
    assert_eq!(a.current_slot(), SlotOwner::Copper);
    at_hpos(&mut a, 0x1D);
    assert_eq!(a.current_slot(), SlotOwner::Cpu, "odd slot yields to CPU");
}

#[test]
fn no_bitplane_fetch_when_bplcon0_bpu_is_zero() {
    let mut a = agnus_with_dmacon(DMAEN | BPLEN);
    a.bplcon0 = 0x0000; // BPU = 0
    a.ddfstrt = 0x0038;
    a.ddfstop = 0x00D0;
    at_hpos(&mut a, 0x40);
    // Even slot inside DDF window — but with no planes, slot is CPU
    // (or copper if COPEN).
    assert_eq!(a.current_slot(), SlotOwner::Cpu);
}

#[test]
fn bitplane_dma_claims_slots_in_the_fetch_window() {
    let mut a = agnus_with_dmacon(DMAEN | BPLEN);
    program_standard_lowres(&mut a);
    // LORES DDFSTRT = 0x38: the 8-CCK group begins at 0x38; per the
    // LOWRES_DDF_TO_PLANE table, positions 1,2,3,5,6,7 are plane fetches
    // (plane indices 3,5,1,2,4,0 respectively). Position 0 and 4 are free.
    at_hpos(&mut a, 0x38); // pos 0 — free
    assert_ne!(a.current_slot(), SlotOwner::Bitplane(0));
    at_hpos(&mut a, 0x39); // pos 1 — plane 3, but 3-plane config has max plane 2
    // With BPU=3, planes 0..2 exist. Slot for plane 3 at pos 1 is free.
    assert_ne!(a.current_slot(), SlotOwner::Bitplane(3));
    at_hpos(&mut a, 0x3B); // pos 3 — plane 1
    assert_eq!(a.current_slot(), SlotOwner::Bitplane(1));
    at_hpos(&mut a, 0x3F); // pos 7 — plane 0 (BPL1)
    assert_eq!(a.current_slot(), SlotOwner::Bitplane(0));
}

#[test]
fn aga_lowres_eight_planes_fill_the_two_idle_slots() {
    // #99: in AGA lowres with 8 planes, the two slots OCS/ECS leave idle
    // (positions 0 and 4 of the 8-CCK group) carry BPL7 and BPL8.
    let mut a = agnus_with_dmacon(DMAEN | BPLEN);
    a.max_bitplanes = 8; // AGA Alice
    a.bplcon0 = 0x0010; // BPU = 8 (BPU3 = BPLCON0 bit 4), lores
    a.ddfstrt = 0x0038;
    a.ddfstop = 0x00D0;
    assert_eq!(a.num_bitplanes(), 8);

    at_hpos(&mut a, 0x38); // pos 0 — BPL7 (idle on OCS/ECS)
    assert_eq!(a.current_slot(), SlotOwner::Bitplane(6));
    at_hpos(&mut a, 0x3C); // pos 4 — BPL8 (idle on OCS/ECS)
    assert_eq!(a.current_slot(), SlotOwner::Bitplane(7));
    at_hpos(&mut a, 0x3F); // pos 7 — BPL1 still loads last
    assert_eq!(a.current_slot(), SlotOwner::Bitplane(0));
}

#[test]
fn aga_lowres_six_planes_leave_slots_0_and_4_idle() {
    // The AGA table is a strict superset: with ≤6 planes the `< num_bpl`
    // filter drops BPL7/BPL8, so positions 0 and 4 stay free exactly as on
    // OCS/ECS — AGA at 6 planes fetches identically to the OCS table.
    let mut a = agnus_with_dmacon(DMAEN | BPLEN);
    a.max_bitplanes = 8; // AGA, but only 6 planes active
    a.bplcon0 = 0x6000; // BPU = 6, lores
    a.ddfstrt = 0x0038;
    a.ddfstop = 0x00D0;
    assert_eq!(a.num_bitplanes(), 6);

    at_hpos(&mut a, 0x38); // pos 0 — BPL7 filtered out → not a bitplane slot
    assert_ne!(a.current_slot(), SlotOwner::Bitplane(6));
    at_hpos(&mut a, 0x3C); // pos 4 — BPL8 filtered out
    assert_ne!(a.current_slot(), SlotOwner::Bitplane(7));
    at_hpos(&mut a, 0x39); // pos 1 — BPL4 (plane 3) present at 6 planes
    assert_eq!(a.current_slot(), SlotOwner::Bitplane(3));
}

#[test]
fn shres_fmode0_fetches_bpl1_bpl2_every_two_ccks() {
    // #469: ECS/AGA SuperHires at 16-bit fetch (FMODE=0) is 2 planes /
    // 4 colours, fetched in 2-CCK groups — twice the hires rate. Before
    // the fix this fell through to the lores 8-CCK group and starved the
    // (4 source-pixels/output) SHRES shifter.
    let mut a = agnus_with_dmacon(DMAEN | BPLEN);
    a.bplcon0 = 0x2040; // BPU=2 + SHRES (bit 6), not hires
    a.ddfstrt = 0x0038;
    a.ddfstop = 0x00D0;
    assert_eq!(a.num_bitplanes(), 2);

    at_hpos(&mut a, 0x38); // group pos 0 — BPL2
    assert_eq!(a.current_slot(), SlotOwner::Bitplane(1));
    at_hpos(&mut a, 0x39); // group pos 1 — BPL1 (loads last)
    assert_eq!(a.current_slot(), SlotOwner::Bitplane(0));
    at_hpos(&mut a, 0x3A); // next group — BPL2 again (2-CCK cadence)
    assert_eq!(a.current_slot(), SlotOwner::Bitplane(1));
    at_hpos(&mut a, 0x3B);
    assert_eq!(a.current_slot(), SlotOwner::Bitplane(0));
}

#[test]
fn shres_fmode1_four_planes_cover_bpl1_through_bpl4() {
    // #469: SuperHires at 32-bit fetch (FMODE=1) is 4 planes / 16 colours,
    // fetchstart 4. The 8-entry wide order does not nest to 4 slots, so the
    // fetch reuses the 4-slot hires order — its plane set is exactly 0..3.
    let mut a = agnus_with_dmacon(DMAEN | BPLEN);
    a.max_bitplanes = 8; // AGA
    a.fmode = 0x0001; // 32-bit fetch
    a.bplcon0 = 0x4040; // BPU=4 + SHRES
    a.ddfstrt = 0x0038;
    a.ddfstop = 0x00D0;
    assert_eq!(a.num_bitplanes(), 4);

    let planes: Vec<_> = (0x38u16..0x3C)
        .map(|h| {
            at_hpos(&mut a, h);
            a.current_slot()
        })
        .collect();
    // All four planes fetched within the 4-CCK group, BPL1 last.
    assert_eq!(
        planes,
        vec![
            SlotOwner::Bitplane(3),
            SlotOwner::Bitplane(1),
            SlotOwner::Bitplane(2),
            SlotOwner::Bitplane(0),
        ]
    );
}

#[test]
fn shres_fmode2_eight_planes_cover_bpl1_through_bpl8() {
    // #469: SuperHires at 64-bit fetch (FMODE=2) is 8 planes / 256 colours,
    // fetchstart 8 — the existing wide order already covers planes 0..7.
    let mut a = agnus_with_dmacon(DMAEN | BPLEN);
    a.max_bitplanes = 8; // AGA
    a.fmode = 0x0003; // 64-bit fetch
    a.bplcon0 = 0x0050; // BPU=8 (bit 4) + SHRES (bit 6)
    a.ddfstrt = 0x0038;
    a.ddfstop = 0x00D0;
    assert_eq!(a.num_bitplanes(), 8);

    let mut seen = std::collections::BTreeSet::new();
    for h in 0x38u16..0x40 {
        at_hpos(&mut a, h);
        if let SlotOwner::Bitplane(p) = a.current_slot() {
            seen.insert(p);
        }
    }
    // Every plane BPL1..BPL8 is fetched once in the 8-CCK group.
    assert_eq!(seen, (0u8..8).collect());
}

#[test]
fn num_bitplanes_clamps_to_six_for_ocs() {
    let mut a = Agnus::new();
    a.bplcon0 = 0x7000; // BPU = 7, invalid on OCS
    assert_eq!(a.num_bitplanes(), 6);
    a.bplcon0 = 0x3000;
    assert_eq!(a.num_bitplanes(), 3);
}

// ────────────────────────────────────────────────────────────────
// Copper-slot parity rule
// ────────────────────────────────────────────────────────────────

#[test]
fn copper_claims_even_slots_only_when_no_bitplane_competes() {
    let mut a = agnus_with_dmacon(DMAEN | COPEN);
    a.bplcon0 = 0; // no bitplanes
    at_hpos(&mut a, 0x80); // well inside the variable window
    assert_eq!(a.current_slot(), SlotOwner::Copper);
    at_hpos(&mut a, 0x81); // odd
    assert_eq!(a.current_slot(), SlotOwner::Cpu);
}

// ────────────────────────────────────────────────────────────────
// Blitter nasty mode (BLTPRI + BLTEN + blitter_busy)
// ────────────────────────────────────────────────────────────────

#[test]
fn blitter_nasty_requires_all_three_conditions() {
    let mut a = Agnus::new();
    a.dmacon = DMAEN | BLTEN | BLTPRI;
    a.blitter_busy = true;
    assert!(a.blitter_nasty_active());

    a.blitter_busy = false;
    assert!(!a.blitter_nasty_active(), "idle blitter cannot be nasty");

    a.blitter_busy = true;
    a.dmacon = DMAEN | BLTEN; // no BLTPRI
    assert!(!a.blitter_nasty_active());

    a.dmacon = DMAEN | BLTPRI; // no BLTEN
    assert!(!a.blitter_nasty_active());
}

// ────────────────────────────────────────────────────────────────
// CckBusPlan — the machine-facing arbitration summary
// ────────────────────────────────────────────────────────────────

#[test]
fn bus_plan_echoes_slot_grants_by_category() {
    let mut a = agnus_with_dmacon(DMAEN | DSKEN | AUD0EN | SPREN | COPEN | BPLEN);
    a.bplcon0 = 0x1000;
    a.ddfstrt = 0x38;
    a.ddfstop = 0xD0;

    at_hpos(&mut a, 0x04);
    assert!(a.cck_bus_plan().disk_dma_slot_granted);

    at_hpos(&mut a, 0x07);
    assert_eq!(a.cck_bus_plan().audio_dma_service_channel, Some(0));

    at_hpos(&mut a, 0x0B);
    assert_eq!(a.cck_bus_plan().sprite_dma_service_channel, Some(0));

    at_hpos(&mut a, 0x3F);
    assert_eq!(a.cck_bus_plan().bitplane_dma_fetch_plane, Some(0));
}

#[test]
fn bus_plan_cpu_grant_matches_slot_owner() {
    let mut a = agnus_with_dmacon(DMAEN);
    at_hpos(&mut a, 0);
    assert!(a.cck_bus_plan().cpu_chip_bus_granted);
    at_hpos(&mut a, 0x01);
    assert!(
        !a.cck_bus_plan().cpu_chip_bus_granted,
        "refresh steals from CPU"
    );
}
