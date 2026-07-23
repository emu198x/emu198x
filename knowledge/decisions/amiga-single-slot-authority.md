# Decision: One Agnus DMA-slot authority per CCK

**Date**: June 2026 (#30)

## The decision

Agnus owns **one** function that decides who holds the chip bus on each
CCK: `Agnus::current_slot() -> SlotOwner`. `cck_bus_plan()` is a thin
derivation of it (per-consumer grant booleans). Every consumer — copper,
CPU, sprite DMA, audio DMA, disk DMA, bitplane fetch, Paula return-latency
— reads that one authority. There is no second, disagreeing slot model.

The hardware-correct OCS PAL layout (vAmiga `SequencerDas`/`SequencerBpl`,
Minimig `agnus.v` priority chain):

- Fixed chipset slots on **odd** hpos, gated by DMACON:
  refresh `0x01/03/05` + EOL; disk `0x07/09/0B`; audio `0x0D/0F/11/13`;
  sprites `0x15..0x33` (`n=(hpos-0x15)/4`, word `((hpos-0x15)/2)&1`).
- Bitplane: DDF-gated, the vAmiga-identical fetch-unit→plane grids;
  DDFSTRT/DDFSTOP masked `$FC` on OCS, `$FE` on ECS/AGA (`agnus_id >=
  $2000`).
- Copper: the **even** free cells (`COPEN && busOwner==NONE && even &&
  hpos != 0xE0`).
- CPU: every cell no one else actually takes.
- Priority on a contended cell:
  `disk > refresh > audio > bitplane > sprite > copper > cpu`.

A granted copper cell is consumed by the CPU unless the copper *actually
fetched* it that CCK. `current_slot` is positional — it grants the copper
every even free cell when COPEN is set and cannot see a parked WAIT — so
the CPU gate keys off `Copper::bus_used_this_cck`, set only at the
copper's real chip-RAM fetch. This mirrors vAmiga's `busOwner`, which a
waiting copper never sets.

## Why

Before #30 two authorities disagreed: `current_slot`/`cck_bus_plan`
(refresh/disk/audio/sprite/bitplane/copper/CPU) and a separate
`denise::dma_claim` (bitplane-only, odd-cell). The running machine gated
copper + CPU off `dma_claim` while the table grants were consumed only by
tests. Three defects followed: a compressed/wrong fixed-slot map, a split
copper parity (granted even / ran odd), and a CPU that stalled only for
bitplane DMA. Collapsing to one authority is the cycle-exact foundation
the blitter-contention, copper↔blitter-sync, and audio/sprite-timing work
builds on.

The copper runs on *even* cells (not odd) because real Agnus allocates the
even in-unit free cells to it; the old odd-cell rule was an artifact of the
compressed map. The CPU must yield idle-copper cells or chip-RAM-bound code
runs ~2× too slow (it regressed the WB1.3 boot until the
`bus_used_this_cck` gate landed).

## Implications

- `denise::dma_claim` / `DmaClaim` are deleted. Bitplane *rendering*
  (Denise) reads `cck_bus_plan().bitplane_dma_fetch_plane` — same grid as
  arbitration, no parallel fetch logic.
- The driver passes `copper_slot_granted` (from `cck_bus_plan`) into
  `Copper::tick_cck`; the copper no longer computes its own parity.
- `service_cpu_bus` consumes the plan's explicit CPU grant, including
  blitter-nasty ownership, while preserving the parked-Copper fallthrough.
- OCS bitplane vertical eligibility is part of the Agnus plan rather than a
  Denise-side fetch gate. This prevents inactive display DMA from ghost-owning
  the bus outside the display window.
- ECS/AGA pass their DIWHIGH-aware vertical eligibility into the same complete
  priority calculation. A suppressed bitplane request therefore falls through
  to sprite, copper, blitter or CPU as appropriate; AGA wide-fetch + SHRES grids
  remain in the shared `bitplane_slot_at`.
- Golden timing shifts (e.g. the WB1.3 free-memory counter) are boot-state
  variation, not render bugs; the desktop/boot screens stay pixel-correct.

## Drift triggers

If I catch myself proposing any of these, stop and re-read the "Why".

**Code patterns to reject:**

- Re-introducing `dma_claim` / `DmaClaim`, or any per-consumer slot rule
  that doesn't go through `current_slot` / `cck_bus_plan`.
- Gating the copper on its own `hpos & 1` parity instead of the driver's
  grant.
- Stalling the CPU on `current_slot() != Cpu` directly (over-stalls — a
  parked copper would lock the CPU out of every even cell).
- Computing a bitplane fetch grid in Denise separate from
  `bitplane_slot_at`.
- Using raw DDFSTRT/DDFSTOP for the fetch grid without the per-variant
  `$FC`/`$FE` mask.
- A fixed-slot map that packs channels onto consecutive hpos.

**Phrases that signal drift:**

- "The copper requests the bus on odd cycles" (true of the HRM prose, but
  our copper runs on the even *free* cells Agnus grants).
- "Just check `dma_claim` for the CPU stall."
- "Bitplane DMA only claims even cells, so the CPU is fine on odd."
- "Let Denise compute its own DDF fetch window."
- "current_slot already says Copper, so stall the CPU."
