# Decision: Agnus blitter startup before the first channel operation

**Date:** July 2026

## The question

What happens between a blit-starting `BLTSIZE` write and the first
blitter channel operation?

## Evidence

The third-edition *Amiga Hardware Reference Manual*, printed page 186,
distinguishes the original A1000 Agnus from Fat Agnus when software
starts a blit while memory access is locked out. The A1000 can still
report the blitter as idle immediately after the start, while Fat Agnus
asserts busy at the `BLTSIZE` write. Appendix A, printed page 274,
defines Copper BFD for both `WAIT` and `SKIP`: when it is set, blitter
finished has no effect; when it is clear, the blitter-finished condition
must accompany the beam comparison.

The inspected WinUAE revision
`c32694e338fa5f34977f522eb4898adb069d2e73` starts its cycle-exact
blitter counter at `-CYCLECOUNT_START`, where `CYCLECOUNT_START` is two.
Only a CCK on which the blitter may advance increments that counter.
The first accepted CCK sets `got_cycle` and reloads `blitzero`; neither
startup CCK performs the pending channel operation.

WinUAE exposes the internal running state immediately on later Agnus
revisions but suppresses A1000 busy until `got_cycle` is set. Its
`DMACONR` path and Copper comparator both consume this visible busy
result. The same comparator serves `WAIT` and `SKIP`, so BFD clear adds
the blitter-idle condition to either beam comparison. WinUAE's changelog
independently records BLTZERO being set on the next free cycle after
`BLTSIZE`, with the same timing as A1000 BLTBUSY.

The inspected vAmiga revision
`60fd1e6b69dcd77c9f44d1291bd37ec715362ab0` has separate `BLT_STRT1`
and `BLT_STRT2` states. Each waits for a free bus before channel
execution begins. It therefore corroborates a shared two-stage,
bus-dependent startup. That revision sets BBUSY and BZERO when preparing
the blit and does not model the A1000 visible-busy distinction, so it is
not the authority for those two signals.

The repository has no primary hardware trace that samples the internal
startup phase across A1000, later original Agnus, ECS and AGA revisions.
The revision-specific visible-busy rule and the precise BZERO reload
point therefore rely on the pinned WinUAE implementation, supported by
the manufacturer appendix's externally observable A1000/Fat Agnus
distinction.

## The decision

A `BLTSIZE` write starts internal blitter activity immediately on every
supported Agnus and Alice revision. Internal busy is the source for
arbitration, blitter-nasty ownership, progress admission and completion
draining. It is not delayed for the A1000.

Every revision then consumes two accepted startup CCKs before the first
A, B, C, D or internal channel operation:

1. The first accepted startup CCK reloads BZERO and leaves one startup
   CCK pending.
2. The second accepted startup CCK drains the startup phase without
   servicing a channel operation.
3. The next accepted CCK may perform the first pending channel
   operation.

An accepted startup CCK is one for which the existing Agnus bus plan
asserts `blitter_dma_progress_granted`. Disabling blitter DMA or losing
the current CPU/free cell to a higher-priority DMA client therefore
holds the startup phase. The implementation does not advance startup
from elapsed wall-clock CCKs alone.

BZERO retains the preceding blit's result between `BLTSIZE` and the
first accepted startup CCK. The first accepted CCK reloads it to the
all-zero assumption; subsequent D results may clear it through the
existing running-NOR rule.

Internal and visible busy are separate observations:

- A1000 Agnus keeps visible busy clear in the just-started state and
  asserts it on the first accepted startup CCK.
- Later original Agnus, Fat Agnus, ECS Agnus and Alice expose busy
  immediately at `BLTSIZE`.

`DMACONR.BBUSY` and Copper BFD synchronization consume visible busy.
Arbitration and blitter-nasty logic consume internal busy. Copper BFD
has the same meaning for both `WAIT` and `SKIP`: with BFD clear, the
beam condition is insufficient while visible busy is asserted; with
BFD set, the instruction ignores blitter status. Both instructions
sample that status after instruction-pair fetch in the serialized
comparison phase defined by
[Copper WAIT and SKIP comparison phase](amiga-copper-wait-skip-comparison.md).

Runtime diagnostics retain `blitter.busy` and
`agnus.blitter_busy` as internal activity. They add visible-busy and
startup-count observations rather than changing the meaning of existing
paths:

- `blitter.busy_visible`;
- `blitter.startup_ccks_remaining`;
- `agnus.blitter_busy_visible`; and
- `agnus.blitter_startup_ccks_remaining`.

This keeps scripts that use internal busy stable while making the
revision-specific external signal and hidden startup phase inspectable.

## Save-state compatibility

The remaining startup count changes every nested Agnus postcard. The
pending Copper comparison also gains a serialized `WAIT`/`SKIP`
discriminator. The Amiga runtime envelope advances to schema version 16
and rejects version 15 before payload decoding.

A version-15 snapshot records that a blit is active but cannot
distinguish the just-started state, the point after the first accepted
startup CCK or the point after the second. Reconstructing the phase from
beam position, DMA registers or pending channel work would change A1000
BBUSY visibility, BZERO reload timing and admission of the first channel
operation.

Raw postcards of `Agnus`, `AgnusEcs`, `AgnusAga`,
`AmigaOcsSnapshot`, `AmigaEcsSnapshot` and `AmigaA1200Snapshot` remain
unversioned and change positional layout. Durable save states must use
the versioned runtime envelope.

## Model boundary

The current bus plan treats a CPU/free cell admitted through
`blitter_dma_progress_granted` as the model's accepted CCK. This is the
existing scheduler granularity. It does not claim the exact physical
bus cell on which a Copper that yields after a fetch opportunity lets
the blitter advance.

The synchronous test and diagnostic drain paths consume the same two
startup outcomes before channel operations so their memory results
remain comparable with the scheduled path. They are not evidence for
physical behaviour while blitter DMA is disabled.

## Deferred behaviour

This decision does not define:

- revision-specific completion phases, including the exact relative
  deassertion times observed by `DMACONR` and the Copper;
- the exact yielded-Copper slot on which a startup state advances;
- CPU and non-nasty blitter coexistence within a nominal CPU/free cell;
- physical synchronous-drain behaviour while blitter DMA is disabled;
- channel-pipeline effects from mid-blit register writes; or
- the final-D and completion-interrupt pipeline.

Those behaviours require their own evidence and tests. In particular,
the established startup status input does not close the separate
Copper-resume-at-completion accuracy edge.

## Verification

Hermetic tests cover:

- the two accepted startup CCKs on A1000, later original Agnus and an
  enhanced wrapper;
- no startup progress while blitter DMA is disabled or the selected
  CCK is contended;
- internal busy and blitter-nasty arbitration beginning before A1000
  visible busy;
- A1000 visible busy asserting on the first accepted startup CCK while
  later revisions expose it immediately;
- BZERO retaining its preceding result until the first accepted CCK;
- no channel operation or completion interrupt during startup;
- restart re-arming both startup CCKs;
- Copper `WAIT` and `SKIP` applying the same BFD condition;
- BFD sampling after decode, including BBUSY transitions before the
  pending comparison;
- runtime diagnostics distinguishing internal, visible and startup
  state; and
- runtime postcard round-trip before and during startup and during a
  pending Copper `SKIP`, plus rejection of version-15 envelopes.

## Drift triggers

Reject these patterns:

- delaying internal blitter activity until A1000 BBUSY becomes visible;
- performing a channel operation on either startup CCK;
- reloading BZERO directly on `BLTSIZE`;
- advancing startup while blitter DMA is disabled or no progress grant
  exists;
- feeding internal busy directly to `DMACONR` or Copper BFD;
- applying BFD to `WAIT` but not `SKIP`;
- replacing the existing runtime `busy` diagnostic with visible busy;
- reconstructing startup phase during snapshot restore.

## Related Documents

- [One Agnus DMA-slot authority per CCK](amiga-single-slot-authority.md)
- [Copper WAIT and SKIP comparison phase](amiga-copper-wait-skip-comparison.md)
- [Original Agnus hard vertical-blank close](amiga-original-agnus-hard-vertical-blank.md)
- [Amiga full-family architecture review](amiga-full-family-architecture-review.md)
- [Live-machine save-state serialization](savestate-live-machine-serde.md)
