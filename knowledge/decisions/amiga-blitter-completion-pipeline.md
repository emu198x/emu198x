# Decision: Amiga blitter completion pipeline

**Date:** July 2026

## The question

In what order do main blitter completion, BZERO, the final D write,
`DMACONR.BBUSY`, Copper BFD synchronization and the blitter interrupt
source change at the end of a blit?

## Evidence

The third-edition *Amiga Hardware Reference Manual*, printed pages
186–187, defines the software-visible contract. The busy flag indicates
that a blit is in progress; software must wait for completion before it
uses the results; the blitter interrupt flag is set whenever a blit
finishes; and BZERO is valid only after completion, including when the D
DMA channel is disabled.

Printed page 188 describes a two-stage internal pipeline and illustrates
typical bus sequences with the final D transfer trailing the source
cycles. Printed page 189 explicitly limits that table to an illustration
and does not guarantee its cycle order. It is therefore evidence for a
completion pipeline, but not primary evidence for exact CCK offsets.

Printed page 220 describes BLIT as meaning that the requested transfer
has completed and that the blitter is ready for another task. Appendix
A, printed page 275, says that BFD clear requires the blitter-finished
condition, in addition to the beam comparison, before the Copper can
leave `WAIT` or perform `SKIP`. These descriptions define external
semantics. They do not specify the relative latching times of
`DMACONR`, the Copper and the interrupt path.

The inspected WinUAE revision
`c32694e338fa5f34977f522eb4898adb069d2e73` provides the strongest
available implementation evidence for those relative times. In its
cycle-exact path, a pre-AGA, non-line blit with D enabled reaches main
finish before its last result and final D transfer have drained. Main
finish clears `blit_main` and emits the blitter interrupt source. The
last result reaches BZERO on the following CCK, and the final D transfer
occurs one CCK after that.

WinUAE gives the two busy consumers different completion holds. Its
`DMACONR` path reports busy through the main-finish CCK and can report
idle on the following CCK. Its Copper path reports busy for one
additional CCK and can first satisfy BFD-clear synchronization on the
final-D CCK.

For AGA, that revision delays main finish and the interrupt source until
the final D stage of a non-line D-channel blit. The ordinary
`DMACONR` and Copper holds are then applied after that delayed finish.
Its line path finishes with the final D write, or with the cycle on
which that write would have occurred when one-dot suppression removes
it. That suppression is defined separately by
[Amiga blitter line-mode ONEDOT](amiga-blitter-line-onedot.md). A
non-line blit without D does not take the AGA final-D delay.

The inspected Minimig revision
`3ab91cd9220d4d047886d215b515227cbe568bdd` corroborates the pre-AGA
three-stage area-blit order. Its final normal `BLT_D` state asserts
`done`, followed by an empty `BLT_E` propagation state and the final D
write in `BLT_F`. D-disabled blits bypass the two trailing states, and
line blits finish in their final write or would-be-write state. Minimig
feeds one busy signal to both `DMACONR` and the Copper, and its Copper
source describes its cycle-exact behaviour as incomplete. It is
therefore corroboration for the pipeline shape, not authority for the
two observer timings.

The inspected vAmiga revision
`60fd1e6b69dcd77c9f44d1291bd37ec715362ab0` separately tracks BBUSY and
the longer-lived running state. `DMACONR` consumes BBUSY, while Copper
BFD consumes the running state and receives a termination notification.
This corroborates the need for separate observers. Its instruction
recipes and delayed Paula interrupt scheduling do not match WinUAE at
every boundary, so they do not establish the exact offsets selected
here.

No primary hardware trace in the repository samples all of BZERO,
destination memory, `DMACONR`, Copper progress and the interrupt source
across original Agnus, ECS Agnus and Alice. The exact offsets below are
therefore a compatibility decision based principally on the pinned
WinUAE implementation. The manual remains authoritative for the
software-visible meaning of completion.

## The decision

Blitter completion is a serialized pipeline, not a single busy-to-idle
edge. The implementation preserves main finish, last-result processing,
the final D transfer, internal drain, the source interrupt and the two
external busy observations as distinct events.

In the table below, `F` is the CCK on which the main word or line count
finishes. `F+1` and later labels describe the ordered completion CCKs in
an uninterrupted sequence. “First idle” means the first CCK on which
that consumer may observe the blitter-finished condition; it does not
mean that a CPU read or Copper fetch completes on that CCK.

| Case | Source `INT_BLIT` | Final BZERO result | Final D | Internal drain | `DMACONR` first idle | Copper BFD first idle |
| --- | --- | --- | --- | --- | --- | --- |
| Pre-AGA area blit with D | `F` | `F+1` | `F+2` | `F+2` | `F+1` | `F+2` |
| AGA area blit with D | `F+2` | `F+1` | `F+2` | `F+2` | `F+3` | `F+4` |
| Area blit without D, any revision | `F` | `F` | none | `F` | `F+1` | `F+2` |
| Line blit with an emitted D write, any revision | `F` | by `F` | `F` | `F` | `F+1` | `F+2` |
| Line blit with final D suppressed by ONEDOT, any revision | `F` | by `F` | none | `F` | `F+1` | `F+2` |

Pre-AGA covers A1000 and later original Agnus revisions as well as ECS
Agnus. AGA denotes Alice.

Internal busy remains asserted until the completion pipeline drains. It
continues to govern completion-stage admission and register-write
serialization. It is not cleared merely because the pre-AGA source
interrupt has been emitted or because `DMACONR` can already report
idle.

Blitter-nasty ownership is stage-aware rather than a synonym for the
longer internal drain. The internal result stage does not own a
chip-bus cell. This releases pre-AGA nasty ownership at main finish and
also leaves Alice's internal `F`/`F+1` stages unallocated while its
source remains delayed. A later final D transfer records actual same-CCK
chip-bus use, so a CPU chip-RAM access cannot reuse that transfer's
cell.

The source interrupt is emitted once:

- at main finish for pre-AGA area blits with D;
- at the final-D finish stage for AGA area blits with D; and
- at `F` for line blits and area blits without D on every revision.

This is the Agnus/Alice source event. It is not a claim that
`INTREQR.INTF_BLIT`, the interrupt encoder or the CPU interrupt-priority
input changes on the same CCK.

BZERO follows result generation, not destination writeback. A
D-disabled area blit therefore produces its final BZERO value at `F`
despite having no final D stage. For an area blit with D, the last result
updates BZERO at `F+1`; the same result reaches destination memory at
`F+2`. The implemented line pipeline has generated the final result by
its final D CCK.

The final-result stage is internal and advances on the CCK after `F`
without requiring a bus grant. The final-D stage requires the existing
blitter progress grant and remains pending until it receives one. The
`F+2` label therefore names the next admitted final-D stage; contention
can place more elapsed CCKs between `F+1` and that write.

`DMACONR.BBUSY` and Copper BFD retain separate completion observations.
For a source-finish event `S`, `DMACONR` remains busy through `S` and
can first report idle at `S+1`. Copper BFD remains busy through `S+1`
and can first treat the blitter as finished at `S+2`. Applying that
common rule after Alice's delayed `S = F+2` produces the AGA offsets in
the table.

A line-mode ONEDOT operation still generates its D result and advances
the line pipeline, but a suppressed D does not drive the chip bus. If
the final operation is suppressed, `F` is its would-be-write CCK and
the ordinary source and observer rules begin there.

## Save-state compatibility

The completion phase, both observer holds, whether the source interrupt
has already been emitted and the same-CCK blitter bus-use state are
hidden execution state. They are serialized rather than
reconstructed from channel pointers, BZERO, memory or current busy
observations.

The completion state introduced Amiga runtime schema version 17, which
rejected version 16 before payload decoding. A version-16 snapshot can
identify an active blit but cannot distinguish main finish from result
drain or final D, nor can it recover which observer must remain busy
after the source event. Guessing would risk a duplicate interrupt, a
missing final write or a one-CCK Copper difference.

The later line-mode decision advanced the envelope to version 18 for
serialized ONEDOT, texture-phase and current-CCK arbitration state.
Accepted MC68000 interrupt-acknowledge identity advanced it to version
19. The current runtime envelope is version 20 because level-7 sampled
input history and a pending transition must also survive restore.

Raw postcards of the affected Agnus and machine types remain
unversioned and change positional layout. Durable save states must use
the versioned runtime envelope.

## Model boundary

The `F` offsets select the best-supported compatibility model; the
hardware manual does not prove them. Tests should describe them as
pinned emulator behaviour unless a primary trace replaces that
evidence.

The table describes the uninterrupted ordering of completion stages.
The scheduler keeps those stages serialized when a required bus action
cannot proceed, but this does not establish the exact physical Alice
delay when the final D slot is contended.

The machine's transaction-level custom-register path currently drains
an active blit synchronously before applying another blitter-register
write. That preserves write ordering and final memory, but no external
observer runs between the drained stages. The same dispatcher is used
for CPU and Copper writes. It is an implementation boundary, not a
claim about physical CPU stalls or mid-blit Copper writes.

A `DMACONR` first-idle CCK describes the chip-side status input. It does
not include CPU bus-read latching or processor instruction timing.
Likewise, the Copper first-idle CCK identifies the BFD input; it does
not define the bus slot on which the Copper wakes, fetches its next word
or performs a `MOVE`.

## Deferred behaviour

This decision does not define:

- propagation from the blitter source event through Paula's
  `INTREQ` latch, the interrupt encoder and CPU IPL;
- exact stretching of the AGA final-D delay under bus contention;
- origin-aware CPU and Copper writes to blitter registers during an
  active blit;
- the Copper's first request and fetch after a BFD-clear wait becomes
  eligible; or
- same-CCK cancellation or arbitration between a waking Copper and the
  remaining completion pipeline.

Those paths need direct integration tests and, for a silicon claim,
stronger primary evidence.

## Verification

Hermetic tests cover:

- pre-AGA area completion at `F`, `F+1` and `F+2`, including early
  source interrupt, BZERO and final D;
- AGA area completion through `F+4`, including the delayed source
  interrupt and both observer holds;
- D-disabled area and line-mode completion without the area-D tail;
- line-mode ONEDOT completion on a bus-free final would-be D;
- `DMACONR` and Copper BFD observing their separate first-idle CCKs;
- no duplicate source interrupt while later completion stages drain;
- deterministic continuation across unavailable progress grants; and
- runtime snapshot round-trip during pre-AGA and Alice final-D tails,
  plus rejection of version-16 envelopes.

## Drift triggers

Reject these patterns:

- clearing internal busy while a result or final D stage remains;
- using one completion flag for internal drain, `DMACONR` and Copper
  BFD;
- treating an internal completion stage as a nasty-owned chip-bus
  transfer;
- delaying the pre-AGA source interrupt until the final D write;
- emitting the AGA D-channel source interrupt at the earlier main
  finish;
- deriving BZERO from whether D writes memory;
- collapsing the line or D-disabled cases into the area-D pipeline;
- charging a suppressed ONEDOT D as a final bus transfer;
- treating the source event as proof of same-CCK `INTREQ` or IPL
  propagation; or
- reconstructing completion phase or observer holds during snapshot
  restore.

## Related Documents

- [Agnus blitter startup before the first channel operation](amiga-agnus-blitter-startup.md)
- [Amiga blitter line-mode ONEDOT](amiga-blitter-line-onedot.md)
- [Amiga blitter line texture phase](amiga-blitter-line-texture-phase.md)
- [Copper WAIT and SKIP comparison phase](amiga-copper-wait-skip-comparison.md)
- [One Agnus DMA-slot authority per CCK](amiga-single-slot-authority.md)
- [Amiga full-family architecture review](amiga-full-family-architecture-review.md)
- [Live-machine save-state serialization](savestate-live-machine-serde.md)
