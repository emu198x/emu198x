# Decision: Amiga register writes during an active blit

**Date:** 2026-07-31
**Status:** ACTIVE

## The question

What happens when the CPU or Copper writes a blitter register while a blit is
active?

## Evidence

Amiga system software provides `WaitBlit` because software must not reprogram
the blitter until the preceding operation has finished. This establishes the
programming contract. It does not imply that Agnus completes an in-flight
operation atomically when software violates that contract.

The inspected WinUAE revision
`c32694e338fa5f34977f522eb4898adb069d2e73` treats “Wait for Blitter” as a
compatibility option for software that fails to wait correctly. The option is
unavailable in cycle-exact MC68000 and MC68010 configurations. Its ordinary
cycle-exact path calls `maybe_blit` on register access but does not force an
active blit to completion when that compatibility option is inactive.

The inspected vAmiga revision
`60fd1e6b69dcd77c9f44d1291bd37ec715362ab0` records blitter control and size
changes as future Agnus register events. Its setters diagnose writes while the
blitter is running. A new `BLTSIZE` services a due blitter event, if any, then
replaces the scheduled blitter event instead of completing the preceding
operation.

Neither implementation is primary hardware evidence for the exact internal
effect of changing every pointer, mask, modulo, control or data register
between channel operations. The repository has no hardware trace that closes
that boundary.

## The decision

A CPU or Copper write to a blitter register remains an ordinary custom-chip
write. The existing chip-bus arbitration determines when its register dispatch
occurs. Dispatch must not run the blitter synchronously, advance the beam or
invent an elapsed wait.

CPU and Copper writes use the same custom-register dispatcher after their
respective bus grants. The access origin does not change the register's
meaning.

A write to `BLTSIZE`, or to `BLTSIZH` on a supported enhanced Agnus or Alice,
starts a new scheduled operation from the then-visible register values. If a
blit is already active, the new start replaces its serialized runtime state.
The machine does not force the old operation's remaining memory transfers,
completion pipeline or source interrupt before applying the new start.

Other writes update the register surface without implicitly finishing the
operation. The current implementation latches several channel parameters into
the operation runtime at start. Exact propagation from a mid-blit write into
that already-running channel pipeline remains outside the present claim and
must be tightened only from stronger evidence.

Software that requires ordered blits must observe the hardware programming
contract and wait for the first blit. Software that omits the wait may expose a
race, particularly when a faster processor reaches the second write earlier.
That is an accuracy-visible failure rather than a reason to hide chipset work
inside register dispatch.

`Agnus::run_blit_to_completion` remains a bounded component-test helper.
Running machines advance the blitter only through scheduled CCK work.

## Compatibility modes

The accuracy model has no implicit “wait for blitter” option. If a future
compatibility mode is needed for known software, it must be:

- explicit and disabled by default;
- identified as a software-compatibility intervention rather than hardware
  behaviour; and
- implemented without changing the accuracy profile's scheduler semantics.

## Verification

Machine-level regressions establish that:

- a CPU write changes a blitter register without advancing machine time or
  completing the active operation;
- a Copper `MOVE` can reach the same dispatcher without completing the active
  operation;
- a second legacy `BLTSIZE` replaces the scheduled operation without emitting
  the old operation's interrupt; and
- ECS and AGA size-extension starts replace active scheduled state without an
  intervening completion.

These tests establish scheduling and externally visible ordering. They do not
claim exact per-channel effects for every possible mid-blit register change.

## Related Documents

- [Amiga blitter completion pipeline](amiga-blitter-completion-pipeline.md)
- [Agnus blitter startup before the first channel operation](amiga-agnus-blitter-startup.md)
- [One Agnus DMA-slot authority per CCK](amiga-single-slot-authority.md)
- [Amiga accuracy closure campaign](amiga-accuracy-closure-campaign.md)
