---
title: "plan: Amiga single-bus-per-CCK rewrite (#30) — one hardware-correct DMA slot authority"
type: plan
date: 2026-06-12
issue: 30
basis: cross-validated reference study of vAmiga + Minimig-AGA + WinUAE (2026-06-12), captured below
supersedes_behaviour: the dual cck_bus_plan / dma_claim arbitration; the compressed fixed-slot map; the DDFSTRT-offset bitplane grid
---

# Amiga single-bus-per-CCK rewrite (#30)

Collapse the two disagreeing bus-arbitration authorities into **one**
hardware-correct DMA slot allocation, matching the real Agnus time-slot
layout. This is the cycle-exact foundation the rest of the chipset work
(blitter contention #31, copper↔blitter sync #33, future audio/sprite
timing) builds on.

Scope was set with Steve on 2026-06-12: **full hardware slot-allocation
rewrite**, including re-deriving the **bitplane fetch grid** (not just the
ownership/contention layer), validated against vAmiga's per-hpos table +
boots + reference spot-checks.

## The problem (today)

Two authorities, disagreeing, each gating different consumers:

| Authority | Models | Gates (run loop) |
|---|---|---|
| `Agnus::current_slot` / `cck_bus_plan` | refresh/disk/audio/sprite/bitplane/copper/CPU | disk/audio/sprite/blitter DMA |
| `denise::dma_claim` + copper's own parity | bitplane only (Free/Bitplane) | copper + CPU chip-RAM stall |

`cck_bus_plan`'s `copper_dma_slot_granted` / `cpu_chip_bus_granted` are
**consumed only by tests** — the running machine gates copper + CPU off
the simpler, separate `dma_claim`. Three concrete defects:

1. **Fixed-slot map is compressed/wrong.** `current_slot` packs channels
   onto *consecutive* hpos (disk `0x04–06`, audio `0x07–0A`, sprites
   `0x0B–1A`); real hardware spaces them on **odd** hpos with the CPU
   taking the even gaps.
2. **Copper parity is split.** `current_slot` grants copper *even* hpos;
   `copper.tick_cck` gates on *odd* — and only the latter runs the
   machine.
3. **CPU only stalls during bitplane DMA**, never for refresh/disk/audio/
   sprite/copper — so chip-RAM-bound CPU code is unrealistically fast.

## The target model (vAmiga 0-based hpos; our hpos is identical: 0x00–0xE2, 227 CCK PAL)

Reference: vAmiga `SequencerDas.cpp:26-67`, `AgnusEvents.cpp:589-607`,
`SequencerBpl.cpp:516-558`, `AgnusDma.cpp:21-53`, `Agnus.cpp:339-371`;
Minimig `agnus.v:137-224` (priority chain), `agnus_copper.v`. Full spec in
the research appendix at the bottom of this file.

**Fixed chipset slots — all ODD hpos, gated by DMACON:**

| hpos | Owner |
|---|---|
| `0x01,0x03,0x05`, + EOL `0xE2`(short)/`0xE3`(long) | Refresh (unconditional) |
| `0x07,0x09,0x0B` | Disk D0/D1/D2 (DSKEN) |
| `0x0D,0x0F,0x11,0x13` | Audio A0–A3 (slot always present; fetch gated by per-channel data-request) |
| `0x15..0x33` (odd) | Sprite `n=(hpos-0x15)/4`, word `((hpos-0x15)/2)&1` (SPREN) |

**Bitplane (DDF-gated):** DDFSTRT/DDFSTOP masked `0xFC` on OCS (`0xFE`
ECS). **VERIFIED 2026-06-12: our `LOWRES_DDF_TO_PLANE` (idx 1,2,3,5,6,7 →
BPL4,6,2,3,5,1; idx 0,4 free) and `HIRES_DDF_TO_PLANE` (H4,H2,H3,H1) are
byte-identical to vAmiga's `computeLoresFetchUnit` / `computeHiresFetchUnit`
(`SequencerBpl.cpp:517-556`).** The bitplane fetch grid is therefore
**already hardware-correct and does NOT change** — what Denise fetches
each cell is unchanged, removing the rendering-regression risk. The
research agent's "odd-only, 4-plane" summary was an extraction error (it
dropped the `channels>=5/>=6` entries). The free in-unit cells (offsets
0,4 — *even* hpos when DDFSTRT is even) are exactly where the copper runs.

**Copper:** takes a slot iff `COPEN AND busOwner==NONE AND IS_EVEN(hpos)
AND hpos ∉ {0xE0,0xE1}`. (Even free cells only; the `E0/E1` block defers
the access to `E2`.)

**CPU:** takes any cell `busOwner==NONE` after BPL/sprite/copper/blitter
resolve — i.e. **always the even cells** copper/blitter didn't take, plus
any odd cell no channel claimed (disabled channels). `bls` (CPU starved
≥2 cycles) blocks a non-nasty blitter so the CPU isn't locked out.

**Priority on a contended cell:**
`disk > refresh > audio > bitplane > sprite > copper > blitter > cpu`
(Minimig `agnus.v:137-224`; bitplane > sprite on a DDF∩sprite overlap).

## Phased implementation (each phase leaves the tree green or behind a gate)

### Phase 0 — Reference table as an executable spec (test-only)
- Add `tests/slot_allocation.rs` to `commodore-agnus-ocs` asserting
  `current_slot(hpos, …)` for a canonical OCS PAL line (DMACON all on,
  6-bitplane lores, DDFSTRT=0x38) against the vAmiga `dasDMA` values —
  the **non-circular** ground truth. Initially red.
- Gate: compiles; documents the target.

### Phase 1 — Rewrite `current_slot()` (the single authority)
- Replace the fixed-slot match arms with the odd-parity vAmiga map.
- **Bitplane grid unchanged** (verified vAmiga-identical) — keep the
  existing DDF→plane tables; only the *gating against the new fixed
  slots* and the priority order change.
- Copper = even free cells; CPU = leftover; priority chain as above.
- Keep `cck_bus_plan` as the thin derivation it already is.
- Gate: Phase-0 table test green; agnus unit tests updated to the new
  positions; `cargo clippy -D warnings`.

### Phase 2 — Rewire consumers; retire `dma_claim`
- **Copper** (`copper.tick_cck`): drop the independent `odd && claim.is_free()`
  gate; the driver passes "this CCK is the Copper slot" from
  `cck_bus_plan`. Copper now runs on **even** cells. Preserve the WAIT /
  2-cycle / `E1→E2` / BFD=0 machinery.
- **CPU** (`service_cpu_bus`): stall when `current_slot != Cpu` (all
  owners), replacing `!dma_claim().is_free()`.
- **Sprite fetch** (driver): move the `0x0B` base + second_word logic to
  the `0x15..0x33` mapping.
- Delete `denise::dma_claim` + `DmaClaim` and their unit tests; update the
  driver `tick`/`service_cpu_bus` defaults.
- Gate: boots (WB1.3) + copper/sprite/audio/disk/blitter tests pass
  functionally; clippy.

### Phase 3 — Rendering re-validation (now low-risk)
- The bitplane fetch grid is unchanged, so Denise's per-cell plane
  fetches don't move. The residual rendering effect is **sprite timing**
  (sprite slots moved 0x0B→0x15) and any second-order copper-timing
  shift. Confirm sprite rendering + a few golden scenes are correct vs
  reference before re-baselining.
- Gate: rendering confirmed correct vs reference, not merely re-baselined.

### Phase 4 — Per-variant (ECS/AGA) + re-baseline
- Apply to `AgnusEcs`/`AgnusAga` overrides (DDF mask `0xFE`; AGA wide
  fetch / SHRES grids; the ECS `cck_bus_plan` copy at `agnus-ecs/lib.rs`).
- Regenerate the moved goldens (folds into the planned golden
  re-baseline); WB3.1 boot; full Amiga suite.

## Validation strategy (goldens are being re-baselined, so ground elsewhere)
- **Non-circular anchor:** the Phase-0 per-hpos table test against
  vAmiga's `dasDMA` values.
- **Functional gates:** WB1.3 + WB3.1 boots reach the desktop; copper
  (`m10_copper`, WAIT machinery), sprite, audio, disk, blitter tests pass.
- **Reference spot-checks:** a few frames compared to vAmiga before
  regenerating any golden.
- **Then** re-baseline the goldens.

## Risks / rollback
- **Rendering regression hidden by re-baselining** — mitigated by Phase 3
  reference comparison + the table test.
- **Timing shift breaks boot** — each phase boots WB before proceeding;
  roll back the phase, not the whole rewrite.
- **Blast radius**: disk/audio/sprite DMA positions, the driver sprite
  constant, every timing-sensitive test, all goldens. Expected and
  accepted per the scope decision.
- This touches the binding clock/bus model; on landing, add a
  `knowledge/decisions/` record (single-authority slot allocation) and
  update any master-clock/contention notes.

## Research appendix
The full citation-grounded spec (per-hpos table, sprite hpos→(n,word)
map, DDF mask, fetch-unit plane order, copper/CPU predicates, priority
chain, vAmiga/Minimig divergences) is preserved in the session that
produced this plan and summarised in "The target model" above. Primary
citations: vAmiga `SequencerDas.cpp:26-67`, `SequencerBpl.cpp:516-558`,
`AgnusDma.cpp:21-53`, `Agnus.cpp:339-371`, `Copper/CopperEvents.cpp:36-83`,
`Agnus.h:357`; Minimig `agnus.v:137-224`, `agnus_copper.v:131-362`.
