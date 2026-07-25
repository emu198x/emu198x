# Decision: Original Agnus DDF run termination on DMA disable

**Date:** July 2026

## The question

What should original Agnus do when effective bitplane DMA is disabled during
an active DDF fetch run before any terminal unit has been requested?

## Evidence

The Amiga hardware manuals define the `DMAEN` and `BPLEN` controls. They do
not specify whether an interrupted bitplane sequencer can resume its old
DDF phase when both controls become enabled again.

The inspected WinUAE revision
`c32694e338fa5f34977f522eb4898adb069d2e73` samples a delayed original-
chipset bitplane-DMA enable. When DMA or the vertical display window becomes
inactive, an active `bprun` leaves its running state and then clears.
Restoring DMA does not reactivate it. A new DDFSTRT match is required.

The inspected vAmiga revision
`60fd1e6b69dcd77c9f44d1291bd37ec715362ab0` independently clears `bprun`
and its fetch counter on `SIG_BMAPEN_CLR`. `SIG_BMAPEN_SET` restores only
the DMA enable. Its original-chipset sequencer also requires a later
`SIG_BPHSTART` event to begin another run.

The two implementations agree on the eventual no-resume result. They use
different internal event positions and do not establish one shared
write-to-stop latency or final pipelined bitplane slot. The repository has
no real-hardware trace for that boundary.

## The decision

Represent termination of an unstopped original-Agnus run with a serialized
current-line abort latch.

The latch is set when effective bitplane DMA changes from enabled to
disabled while:

- original Agnus is installed;
- a DDFSTRT fetch origin has been recorded; and
- no ordinary or fixed terminal endpoint has been requested.

Clearing either `BPLEN` or the master `DMAEN` bit can produce this transition.

Once Emu198x has recorded the abort transition, the run:

- retains its observed DDFSTRT value as the frozen display-phase origin;
- owns no new bitplane bus slots;
- does not consume a later DDFSTOP or fixed `$D8` event;
- cannot resume when bitplane DMA is enabled again in the same line; and
- does not close the original-Agnus hard-start permission because no
  terminal fetch unit completed.

Preserving DDFSTRT history is intentional. Denise uses that origin for its
display-pipeline coordinate. Erasing it would conflate sequencer termination
with a new display phase.

The abort latch resets at horizontal wrap with the other current-line run
state. A normal next-line DDFSTRT comparator can therefore establish a fresh
run when DMA, the vertical display window and the carried hard-start
permission allow it.

Enhanced Agnus and Alice serialize the shared inner field but do not consume
it. Their DMA soft-enable, hard-window and multi-region behaviour remains a
separate sequencer problem.

The machine loop fixes the Copper's grant before dispatching that Copper
MOVE, so a DMACON write cannot retroactively take that cell from the Copper.
Other ownership is recomputed afterwards. Verification judges only the
settled result, eight CCKs after disable and eight after re-enable, beyond
the delayed transitions in both inspected implementations. It therefore
does not claim the exact hardware latency.

The new field changes every nested Amiga machine postcard. Runtime postcards
therefore advance to schema version 11 and reject version 10 before payload
decoding. A version-10 OCS snapshot captured after DMA was re-enabled can
contain DMA enabled, a historical DDFSTRT origin and no terminal endpoint,
with no record that the old run was already aborted. That state is
indistinguishable from a legitimate active run during restore.

The runtime uses one global Amiga envelope version, so version-10 ECS and AGA
runtime snapshots are also rejected even though those chipsets do not
consume this latch. Raw postcards of `AmigaOcsSnapshot`, `AmigaEcsSnapshot`
and `AmigaA1200Snapshot` are unversioned and their positional layout has
changed. They have no migration path. Durable save states must use the
runtime envelope.

## Deferred behaviour

This decision does not define:

- disabling DMA after DDFSTOP or the fixed right edge has already requested
  a terminal unit;
- exact DMACON write latency or the final in-flight bitplane slot;
- already-fetched Denise shifter output after the sequencer stops;
- a vertical display-window transition during an active run;
- a rewritten future DDFSTRT after an abort in the same line;
- multiple DDF regions;
- enhanced-chipset DMA-enable transitions; or
- exact modulo timing.

## Verification

Hermetic tests cover:

- both `BPLEN` and master `DMAEN` terminating an active unstopped OCS run;
- eight-CCK disable and re-enable observation intervals followed by no stale
  bitplane ownership;
- the observed DDFSTRT phase remaining available while no DDFSTOP or `$D8`
  terminal endpoint is created;
- the hard-start permission remaining open;
- disabling DMA before DDFSTRT not creating an abort;
- enhanced Agnus ignoring the shared OCS latch;
- machine-level bitplane pointers remaining fixed after re-enable;
- postcard round-trip after re-enable, deterministic no-resume through
  `$D8`, and normal next-line re-arming; and
- rejection of version-10 runtime postcards.

## Related documents

- [Original Agnus cross-line DDF hard-start gate](amiga-ocs-ddf-hard-start-gate.md)
- [Original Agnus DDF hard-stop terminal policy](amiga-ocs-ddf-hard-stop.md)
- [Idle register-equal DDF boundaries](amiga-idle-equal-ddf-boundaries.md)
- [One Agnus DMA-slot authority per CCK](amiga-single-slot-authority.md)
- [Save State Format](save-state-format.md)
