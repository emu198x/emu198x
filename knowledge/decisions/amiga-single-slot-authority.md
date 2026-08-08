# Decision: One Agnus DMA-slot authority per CCK

**Date**: June 2026 (#30)

**Implementation status:** Implemented. Disk memory transfers consume the
authoritative grant as of commit `24b20ce4`; Paula now supplies the per-cell
D0/D1/D2 request mask and Agnus retains actual disk use for the rest of the
CCK. The independent rotational stream crosses Paula's bounded FIFO as defined
by [Amiga disk rotation and DMA arbitration](amiga-disk-dma-fifo-arbitration.md).

## The decision

Agnus owns **one** function that decides who holds the chip bus on each
CCK: `Agnus::current_slot() -> SlotOwner`. `cck_bus_plan()` is a thin
derivation of it (per-consumer grant booleans). Every consumer — copper,
CPU, sprite DMA, audio DMA, disk DMA, bitplane fetch, Paula return-latency
— reads that one authority. There is no second, disagreeing slot model.

The hardware-correct OCS PAL layout (vAmiga `SequencerDas`/`SequencerBpl`,
Minimig `agnus.v` priority chain):

- Fixed chipset slots on **odd** hpos, gated by DMACON and the channel's actual
  request state: refresh `0x01/03/05` + EOL; disk D0/D1/D2
  `0x07/09/0B`; audio `0x0D/0F/11/13`;
  sprite opportunities `0x15..0x33` (`n=(hpos-0x15)/4`, word
  `((hpos-0x15)/2)&1`). A sprite claims its pair only when it requests a
  control or data fetch; an idle opportunity falls through the priority
  chain.
- Bitplane: DDF-gated, the vAmiga-identical fetch-unit→plane grids;
  DDFSTRT/DDFSTOP masked `$FC` on OCS, `$FE` on ECS/AGA (`agnus_id >=
  $2000`).
- Copper: the **even** free cells (`COPEN && busOwner==NONE && even &&
  hpos != 0xE0`).
- CPU: every cell no one else actually takes.
- Positional priority on a contended cell is
  `disk > refresh > audio > bitplane > sprite > copper > cpu`. Blitter
  arbitration then occupies an actually unused CPU/free or yielded-Copper
  cell. A mature CPU chip-RAM request outranks a non-nasty blitter; `BLTPRI`
  allows the blitter to pre-empt it.

A Copper-eligible cell is offered first to the Copper. A waiting, stopped or
internally throttled Copper yields that cell to an active blitter when the CPU
does not need the chip bus. A mature CPU chip-RAM request wins against a
non-nasty blitter, while a nasty blitter takes the cell before the CPU.
`current_slot` is positional: it cannot see a parked WAIT. The scheduler
therefore keys the second-stage choice off `Copper::bus_used_this_cck`,
mirroring vAmiga's `busOwner`, which a waiting Copper never sets.

An active Copper instruction owns both modeled fetch cells. The common Copper
abstraction reads the instruction pair when its second accepted fetch cell
completes, but the first phase is still a real bus allocation. Marking only the
second phase allowed a nasty blitter or CPU to reuse the first cell and changed
real-software output.

A state-sensitive grant remains authoritative for the whole CCK. Servicing
disk, sprite, Copper or blitter DMA can change the live state from which a new
plan would be computed, but that change cannot retroactively give an
already-consumed cell to another master. The scheduler therefore retains
actual-use latches across both CPU phases of the CCK and clears them only when
the next CCK starts.

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

The Copper runs on *even* cells (not odd) because real Agnus allocates the
even in-unit free cells to it; the old odd-cell rule was an artifact of the
compressed map. The CPU must receive a Copper-eligible cell when neither the
Copper nor blitter uses it; otherwise chip-RAM-bound code runs about twice too
slow. Conversely, the CPU must remain stalled when a parked Copper yields the
cell to a nasty blitter.

## Implications

- `denise::dma_claim` / `DmaClaim` are deleted. Bitplane *rendering*
  (Denise) reads `cck_bus_plan().bitplane_dma_fetch_plane` — same grid as
  arbitration, no parallel fetch logic.
- The driver passes `copper_slot_granted` (from `cck_bus_plan`) into
  `Copper::tick_cck`; the copper no longer computes its own parity.
- The machine-facing plan combines Agnus's D0/D1/D2 position decode with
  Paula's current stage mask. One read word requests D2, two request D1 and
  D2, and a full FIFO requests all three. An idle disk opportunity falls
  through instead of reserving the bus.
- `service_cpu_bus` consumes the plan's explicit CPU grant, including
  actual disk, sprite, Copper and blitter ownership. A parked-Copper
  fallthrough reaches an idle-CPU or nasty blitter; a competing non-nasty
  blitter yields to the CPU.
- A just-started blit consumes two accepted startup CCKs before its first
  channel operation. An accepted startup CCK is a CPU/free cell for which
  the same plan asserts `blitter_dma_progress_granted`; disabled blitter DMA
  or a higher-priority owner holds the phase. Internal busy participates in
  arbitration immediately, while A1000-visible BBUSY is a separate signal.
  The startup and signal rules are defined in
  [Agnus blitter startup before the first channel operation](amiga-agnus-blitter-startup.md).
- OCS bitplane vertical eligibility is part of the Agnus plan, not a
  Denise-side fetch gate. Original Agnus preserves it as a serialized
  VSTART/VSTOP latch; it is not reconstructed as a circular range from live
  registers and beam position. This prevents inactive display DMA from
  ghost-owning the bus outside the display window. The comparator, rewrite
  and run-termination rules are defined in
  [Original Agnus vertical display-window latch](amiga-ocs-vertical-diw-latch.md).
- DDFSTRT is an edge, not a continuously reconstructed range boundary. On
  each line Agnus records the masked comparator that opens the current run
  and uses it as the frozen fetch-phase origin for both arbitration and
  Denise while that run remains active.
  The match resets at horizontal position zero and is evaluated when the beam
  enters a CCK, before a Copper MOVE can alter the register in that CCK.
  A write at or behind the beam therefore cannot start the line
  retroactively; a write to an unreached position can still match; a write
  after a match cannot rephase an active fetch sequence. Early OCS records
  the match only while bitplane DMA and its vertical display window are
  active. Fat Agnus, ECS Agnus and Alice retain the match independently and
  apply those gates when arbitration consumes it. The match is serialized
  because it cannot be reconstructed from the current registers and beam
  position.
  DDFSTOP is also a comparator event for an ordinary start-before-stop
  region. Agnus observes the masked stop when the beam enters its CCK,
  before a Copper MOVE at that position, and freezes the inclusive terminal
  fetch endpoint from the active fetch cadence. The old stop therefore wins
  if Copper overwrites it on the matching CCK; a newly written current or
  past stop cannot match retroactively; an unreached future replacement can
  match; and a later write cannot cancel a stop already in progress.
  The observed stop and terminal endpoint are serialized because live
  DDFSTOP and beam position cannot reconstruct either one. The ordinary
  endpoint retains the Hardware Reference Manual's documented fetch counts,
  including the complete terminal fetch unit.
  Original Agnus's fixed right-hand stop is another event owned by this
  sequencer and freezes the phase-dependent terminal endpoint before the
  machine loop dispatches a same-position Copper MOVE. Its evidence and
  selected terminal policy are recorded in
  [Original Agnus DDF hard-stop terminal policy](amiga-ocs-ddf-hard-stop.md).
  If effective original-Agnus bitplane eligibility falls before a terminal
  request, a serialized abort latch removes the old run from future slot
  ownership and stop scheduling without erasing its display-phase origin.
  Re-enabling DMA or reopening the vertical latch cannot resume that run. A
  rewritten future DDFSTRT may replace the old origin only when its comparator
  is reached with the normal OCS admission gates active; current, behind-beam
  and ineligible comparators remain missed. The two terminating gates are
  defined in
  [Original Agnus DDF run termination on DMA disable](amiga-ocs-ddf-dma-disable.md)
  and
  [Original Agnus vertical display-window latch](amiga-ocs-vertical-diw-latch.md).
  Register-equal boundaries with a pre-existing run, stop-before-start,
  raw register-write latency, DMA disable or vertical close after a pending
  terminal request, multiple enhanced-chipset regions, exact cross-wrap
  terminal bus and pointer timing, exact modulo timing and Alice's explicit
  final state remain separate accuracy work. Revision-specific original-Agnus
  force-off timing is defined in
  [Original Agnus hard vertical-blank close](amiga-original-agnus-hard-vertical-blank.md).
  The implemented
  clean-idle equality and original-Agnus hard-start admission transitions,
  including the compressed short-line wrap result, are recorded in
  [Idle register-equal DDF boundaries](amiga-idle-equal-ddf-boundaries.md)
  and [Original Agnus cross-line DDF hard-start gate](amiga-ocs-ddf-hard-start-gate.md).
- Sprite control/data request state is likewise part of the Agnus plan.
  `SPREN` exposes the scheduled opportunities but does not make an idle
  channel own them; unused cells remain available to the blitter or CPU.
  Once a sprite performs a fetch, its transient bus-use record keeps both
  master/4 phases of that CCK unavailable to the CPU even if the fetch
  changes the state from which a fresh plan would be derived.
  The comparator and fixed PAL/NTSC reset rules are defined in
  [Amiga sprite DMA lifecycle](amiga-sprite-dma-lifecycle.md).
- ECS/AGA pass their DIWHIGH-aware vertical eligibility into the same complete
  priority calculation. A suppressed bitplane request therefore falls through
  to sprite, copper, blitter or CPU as appropriate; AGA wide-fetch + SHRES grids
  remain in the shared `bitplane_slot_at`. The board also passes that concrete
  vertical state into the common render path; Denise does not reconstruct a
  second OCS-only vertical range after Agnus has granted the fetch. ECS/AGA
  vertical eligibility is a serialized comparator-driven latch: VSTART opens
  it, VSTOP or the fixed hard vertical-blank start event closes it, and stop
  takes precedence. Register writes change future comparator events rather than
  reconstructing live state from the current beam position. An unreachable
  VSTART therefore cannot open DMA merely because its numeric value is greater
  than VSTOP. Minimig's `agnus_bitplanedma`, vAmiga's sequencer and WinUAE's
  `vdiwstate` independently implement this as a set/clear latch rather than a
  circular range. Base DIWSTRT or DIWSTOP writes return decoding to the legacy
  implicit VSTOP high bit. Any subsequent DIWHIGH write, including zero,
  selects explicit high bits. Alice exposes V10..V8; ECS Agnus additionally
  exposes the undocumented V11 modelled by WinUAE.
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
- Starting a bitplane sequence from `hpos >= live DDFSTRT`, or phasing
  Denise from the live DDFSTRT value after Agnus has already observed the
  line's comparator.
- Deriving the end of an active fetch region from live DDFSTOP after its
  comparator has matched, or manufacturing a missed stop because the beam
  has already passed a newly written value.
- A fixed-slot map that packs channels onto consecutive hpos.
- Treating `SPREN + sprite hpos` as sufficient ownership without checking
  whether that sprite requests control or data DMA on the current line.
- Recomputing `cck_bus_plan()` after a state-mutating DMA service and using
  the new result to retroactively grant the same CCK to the CPU.
- Treating an enabled but unrequested D0/D1/D2 opportunity as disk ownership.
- Letting a parked Copper hand its eligible cell directly to the CPU without
  first applying blitter priority.
- Recording only the Copper phase that completes the paired instruction read;
  both accepted fetch cells own the bus.
- Reconstructing any installed Agnus's vertical bitplane eligibility as a
  range comparison
  over VSTART, VSTOP and the current beam position instead of preserving the
  comparator-driven latch.
- Re-decoding a separate OCS vertical range in the render path instead of using
  the installed Agnus revision's vertical display-window state.

**Phrases that signal drift:**

- "The copper requests the bus on odd cycles" (true of the HRM prose, but
  our copper runs on the even *free* cells Agnus grants).
- "Just check `dma_claim` for the CPU stall."
- "Bitplane DMA only claims even cells, so the CPU is fine on odd."
- "Let Denise compute its own DDF fetch window."
- "The beam is past DDFSTRT, so DMA must already be active."
- "The beam is past DDFSTOP, so recompute the fetch end from its current
  value."
- "current_slot already says Copper, so stall the CPU."
- "The Copper is waiting, so this cell always belongs to the CPU."
- "The first Copper fetch phase does not read the pair yet, so it is free."
- "SPREN reserves every sprite slot whether or not the channel fetches."

## Related documents

- [Amiga disk rotation and DMA arbitration](amiga-disk-dma-fifo-arbitration.md)
- [Amiga accuracy closure campaign](amiga-accuracy-closure-campaign.md)
- [Original Agnus vertical display-window latch](amiga-ocs-vertical-diw-latch.md)
- [Original Agnus hard vertical-blank close](amiga-original-agnus-hard-vertical-blank.md)
- [Original Agnus DDF run termination on DMA disable](amiga-ocs-ddf-dma-disable.md)
- [Original Agnus DDF hard-stop terminal policy](amiga-ocs-ddf-hard-stop.md)
- [Original Agnus cross-line DDF hard-start gate](amiga-ocs-ddf-hard-start-gate.md)
- [Agnus blitter startup before the first channel operation](amiga-agnus-blitter-startup.md)
- [Copper WAIT and SKIP comparison phase](amiga-copper-wait-skip-comparison.md)
- [Amiga sprite DMA lifecycle](amiga-sprite-dma-lifecycle.md)
