//! Agnus DMA slot arbitration — hardware-correct positions (#30).
//!
//! The 227-CCK line (hpos 0x00..0xE2) allocates every fixed chipset
//! channel to an ODD hpos, matching vAmiga's `SequencerDas.cpp`
//! `dasDMA` table (the non-circular reference anchor). The CPU takes the
//! even cells (and any unclaimed odd cell); the copper takes the even
//! FREE cells when COPEN is set.
//!
//! ```text
//!   hpos 0x00          free (CPU / copper-even)
//!   hpos 0x01/03/05    memory refresh
//!   hpos 0x07/09/0B    disk DMA D0/D1/D2 (DMACON.DSKEN)
//!   hpos 0x0D/0F/11/13 audio DMA ch0..3 (per-channel gate)
//!   hpos 0x15..0x33    sprite DMA: sprite (hpos-0x15)/4, 2 odd slots each
//!   hpos 0x1C..        bitplane (DDF-gated) / copper (even) / CPU
//!   hpos 0xE2 / 0xE3   end-of-line refresh (short / long line)
//! ```
//!
//! Priority on a contended cell:
//!   disk > refresh > audio > bitplane > sprite > copper > cpu
//! (blitter contention is layered onto the Cpu slots in `cck_bus_plan`).
//!
//! Chip-level gates:
//!   - Every DMA channel needs DMACON.DMAEN (bit 9) set.
//!   - Per-channel DMACON bit gates each group.
//!   - Bitplane DMA also needs BPLCON0.BPU > 0 and hpos within the
//!     DDF fetch window.

use commodore_agnus_ocs::{Agnus, PaulaReturnProgressPolicy, SlotOwner};

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

fn observe_ddf_start(agnus: &mut Agnus) {
    if agnus.agnus_id < 0x2000 && !agnus.vertical_diw_active() {
        let (diwstrt, diwstop) = if agnus.diwstrt == 0 && agnus.diwstop == 0 {
            agnus.vpos = 0x0030;
            (0x2C81, 0x2CC1)
        } else {
            (agnus.diwstrt, agnus.diwstop)
        };
        assert!(agnus.vpos <= 0x00FF);
        agnus.write_diwstop(diwstop);
        // Establish the hidden vertical latch through a real current-line
        // VSTART event, then restore the caller's register value while
        // preserving its sprite-comparator setup.
        agnus.write_diwstrt((agnus.vpos << 8) | (diwstrt & 0x00FF));
        agnus.write_diwstrt(diwstrt);
        for _ in 0..8 {
            agnus.tick_cck();
        }
    }
    let mask = if agnus.agnus_id >= 0x2000 {
        0x00FE
    } else {
        0x00FC
    };
    let start = agnus.ddfstrt & mask;
    assert!(start > 0, "test helper requires a non-zero DDFSTRT");
    agnus.hpos = start - 1;
    agnus.tick_cck();
    assert_eq!(agnus.ddf_start_match(), Some(start));
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

// Hardware-correct positions (vAmiga `SequencerDas.cpp`): every fixed
// chipset slot is ODD; the even cells are CPU (or copper when COPEN).
// These values are the non-circular reference anchor for #30.

#[test]
fn refresh_slots_are_01_03_05_and_end_of_line() {
    let mut a = agnus_with_dmacon(DMAEN | DSKEN | AUD0EN | SPREN);
    for hpos in [0x01u16, 0x03, 0x05, 0xE2] {
        at_hpos(&mut a, hpos);
        assert_eq!(
            a.current_slot(),
            SlotOwner::Refresh,
            "hpos {hpos:#x} is refresh regardless of DMACON"
        );
    }
}

#[test]
fn even_cells_in_the_fixed_region_are_cpu_without_copen() {
    let mut a = agnus_with_dmacon(DMAEN | DSKEN);
    for hpos in [0x00u16, 0x02, 0x04, 0x06, 0x0C, 0x14] {
        at_hpos(&mut a, hpos);
        assert_eq!(
            a.current_slot(),
            SlotOwner::Cpu,
            "even hpos {hpos:#x} -> CPU"
        );
    }
}

// ────────────────────────────────────────────────────────────────
// Disk slots (0x07/0x09/0x0B) — gated by DMACON.DSKEN
// ────────────────────────────────────────────────────────────────

#[test]
fn disk_slots_07_09_0b_granted_when_dsken_and_master_enabled() {
    let mut a = agnus_with_dmacon(DMAEN | DSKEN);
    for hpos in [0x07u16, 0x09, 0x0B] {
        at_hpos(&mut a, hpos);
        assert_eq!(a.current_slot(), SlotOwner::Disk, "hpos {hpos:#x} -> disk");
    }
}

#[test]
fn disk_slots_fall_back_to_cpu_without_dsken() {
    let mut a = agnus_with_dmacon(DMAEN);
    for hpos in [0x07u16, 0x09, 0x0B] {
        at_hpos(&mut a, hpos);
        assert_eq!(
            a.current_slot(),
            SlotOwner::Cpu,
            "hpos {hpos:#x}: no DSKEN -> CPU"
        );
    }
}

#[test]
fn disk_slots_fall_back_to_cpu_without_master_enable() {
    let mut a = agnus_with_dmacon(DSKEN);
    for hpos in [0x07u16, 0x09, 0x0B] {
        at_hpos(&mut a, hpos);
        assert_eq!(a.current_slot(), SlotOwner::Cpu);
    }
}

// ────────────────────────────────────────────────────────────────
// Audio slots (0x0D/0x0F/0x11/0x13) — per-channel gates
// ────────────────────────────────────────────────────────────────

#[test]
fn audio_channels_each_own_exactly_one_slot() {
    let mut a = agnus_with_dmacon(DMAEN | AUD0EN | AUD1EN | AUD2EN | AUD3EN);
    for (hpos, expected_ch) in [(0x0Du16, 0u8), (0x0F, 1), (0x11, 2), (0x13, 3)] {
        at_hpos(&mut a, hpos);
        assert_eq!(
            a.current_slot(),
            SlotOwner::Audio(expected_ch),
            "hpos {hpos:#x} -> audio {expected_ch}"
        );
    }
}

#[test]
fn audio_channel_disabled_individually_yields_cpu() {
    let mut a = agnus_with_dmacon(DMAEN | AUD0EN | AUD2EN); // 1 + 3 disabled
    at_hpos(&mut a, 0x0F);
    assert_eq!(a.current_slot(), SlotOwner::Cpu);
    at_hpos(&mut a, 0x13);
    assert_eq!(a.current_slot(), SlotOwner::Cpu);
    at_hpos(&mut a, 0x0D);
    assert_eq!(a.current_slot(), SlotOwner::Audio(0));
    at_hpos(&mut a, 0x11);
    assert_eq!(a.current_slot(), SlotOwner::Audio(2));
}

// ────────────────────────────────────────────────────────────────
// Sprite slots (0x15..0x33) — 8 sprites × 2 odd slots each
// ────────────────────────────────────────────────────────────────

#[test]
fn sprite_slots_map_channel_to_hpos_pairs() {
    // vAmiga: sprite n words at 0x15+4n (first) and 0x17+4n (second).
    let mut a = agnus_with_dmacon(DMAEN | SPREN);
    a.vpos = 30;
    for ch in 0u8..8 {
        a.poke_sprite_ctl(ch as usize, 30 << 8);
        let base = 0x15 + u16::from(ch) * 4;
        at_hpos(&mut a, base);
        assert_eq!(
            a.current_slot(),
            SlotOwner::Sprite(ch),
            "sprite {ch} first slot at hpos {base:#x}"
        );
        at_hpos(&mut a, base + 2);
        assert_eq!(
            a.current_slot(),
            SlotOwner::Sprite(ch),
            "sprite {ch} second slot at hpos {:#x}",
            base + 2
        );
    }
}

#[test]
fn sprite_slots_fall_back_to_cpu_without_spren() {
    let mut a = agnus_with_dmacon(DMAEN);
    at_hpos(&mut a, 0x15);
    assert_eq!(a.current_slot(), SlotOwner::Cpu);
}

#[test]
fn idle_sprite_opportunity_falls_back_to_cpu_even_with_copper_enabled() {
    for dmacon in [DMAEN | SPREN, DMAEN | SPREN | COPEN] {
        let mut a = agnus_with_dmacon(dmacon);
        a.vpos = 30;
        a.poke_sprite_ctl(0, 50 << 8);
        at_hpos(&mut a, 0x15);

        let plan = a.cck_bus_plan();
        assert_eq!(plan.slot_owner, SlotOwner::Cpu);
        assert_eq!(plan.sprite_dma_service_channel, None);
        assert!(!plan.copper_dma_slot_granted);
        assert!(plan.cpu_chip_bus_granted);
        assert_eq!(
            plan.paula_return_progress_policy,
            PaulaReturnProgressPolicy::Advance
        );
    }
}

#[test]
fn sprite_opportunity_during_vertical_blank_falls_back_to_cpu() {
    let mut a = agnus_with_dmacon(DMAEN | SPREN);
    a.vpos = 0;
    at_hpos(&mut a, 0x15);

    assert_eq!(a.current_slot(), SlotOwner::Cpu);
}

// ────────────────────────────────────────────────────────────────
// Bitplane fetch window (1C..=E2)
// ────────────────────────────────────────────────────────────────

fn program_standard_lowres(a: &mut Agnus) {
    a.bplcon0 = 0x3000; // BPU = 3 (3 planes)
    a.ddfstrt = 0x0038; // canonical LORES DDFSTRT
    a.ddfstop = 0x00D0; // canonical LORES DDFSTOP
    observe_ddf_start(a);
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
    observe_ddf_start(&mut a);
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
    a.agnus_id = 0x2300;
    a.max_bitplanes = 8; // AGA Alice
    a.bplcon0 = 0x0010; // BPU = 8 (BPU3 = BPLCON0 bit 4), lores
    a.ddfstrt = 0x0038;
    a.ddfstop = 0x00D0;
    assert_eq!(a.num_bitplanes(), 8);
    observe_ddf_start(&mut a);

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
    a.agnus_id = 0x2300;
    a.max_bitplanes = 8; // AGA, but only 6 planes active
    a.bplcon0 = 0x6000; // BPU = 6, lores
    a.ddfstrt = 0x0038;
    a.ddfstop = 0x00D0;
    assert_eq!(a.num_bitplanes(), 6);
    observe_ddf_start(&mut a);

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
    a.agnus_id = 0x2000;
    a.bplcon0 = 0x2040; // BPU=2 + SHRES (bit 6), not hires
    a.ddfstrt = 0x0038;
    a.ddfstop = 0x00D0;
    assert_eq!(a.num_bitplanes(), 2);
    observe_ddf_start(&mut a);

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
    a.agnus_id = 0x2300;
    a.max_bitplanes = 8; // AGA
    a.fmode = 0x0001; // 32-bit fetch
    a.bplcon0 = 0x4040; // BPU=4 + SHRES
    a.ddfstrt = 0x0038;
    a.ddfstop = 0x00D0;
    assert_eq!(a.num_bitplanes(), 4);
    observe_ddf_start(&mut a);

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
    a.agnus_id = 0x2300;
    a.max_bitplanes = 8; // AGA
    a.fmode = 0x0003; // 64-bit fetch
    a.bplcon0 = 0x0050; // BPU=8 (bit 4) + SHRES (bit 6)
    a.ddfstrt = 0x0038;
    a.ddfstop = 0x00D0;
    assert_eq!(a.num_bitplanes(), 8);
    observe_ddf_start(&mut a);

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
    a.vpos = 30;
    a.diwstrt = 0x1E81;
    a.diwstop = 0xA0C1;
    a.poke_sprite_ctl(0, 30 << 8);
    observe_ddf_start(&mut a);

    at_hpos(&mut a, 0x07);
    assert!(a.cck_bus_plan().disk_dma_slot_granted);

    at_hpos(&mut a, 0x0D);
    assert_eq!(a.cck_bus_plan().audio_dma_service_channel, Some(0));

    at_hpos(&mut a, 0x15);
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
