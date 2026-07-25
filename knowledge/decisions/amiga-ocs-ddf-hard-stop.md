# Decision: Original Agnus DDF hard-stop terminal policy

**Date:** July 2026

## The question

What should the original Agnus bitplane sequencer do when the beam reaches
the fixed data-fetch stop at horizontal count `$D8`?

## Confirmed behaviour

The fixed stop is a beam event, not a numeric clamp applied to the live
DDFSTOP register.

The *Amiga A500/A2000 Technical Reference Manual* installs the event at
count `$D8` to prevent bitplane DMA from entering refresh or disk-DMA time.
For an active fetch sequence, the event therefore requests termination when
DDFSTOP is later than `$D8` or when its comparator has already passed.

An ordinary DDFSTOP that has already frozen an earlier terminal fetch wins.
The fixed event does not extend it.

This unconditional policy is limited to original Agnus. Enhanced Agnus and
Alice require their own policy because BEAMCON0 can disable hard limits.

## Conflicting terminal-fetch evidence

Two Commodore sources disagree about the high-resolution terminal fetch.

The third-edition *Amiga Hardware Reference Manual*, page 80, table 3-14,
states that `$18..$D8` fetches 25 low-resolution words but 49
high-resolution words. Its prose says only one high-resolution word is
fetched at the rightmost limit.

The *Amiga A500/A2000 Technical Reference Manual*, page 212, draws the
`$D8..$E0` terminal interval as:

- one complete low-resolution word period, numbered 21;
- two high-resolution word periods, numbered 40 and 41.

In the inspected revisions, WinUAE, vAmiga and Minimig-AGA each encode an
eight-CCK terminal sequence. For a `$18`-aligned high-resolution run that
agrees with the technical-reference diagram and produces 50 words, not 49.
The inspected implementation evidence is:

- WinUAE `c32694e338fa5f34977f522eb4898adb069d2e73`,
  principally `custom.cpp`;
- vAmiga `60fd1e6b69dcd77c9f44d1291bd37ec715362ab0`,
  principally `Core/Components/Agnus/Sequencer/SequencerBpl.cpp`;
- Minimig-AGA `3ab91cd9220d4d047886d215b515227cbe568bdd`,
  principally `rtl/agnus_bitplanedma.v`.

The repository has no hardware observation that resolves the conflict.

## The decision

Preserve the established full fetch-unit terminal policy while adding the
previously missing fixed event.

At `$D8`, an already-running original-Agnus fetch sequence receives a stop
request. Its terminal endpoint remains relative to the matched DDFSTRT
phase: a `$18`-aligned run ends inclusively at `$DF`, while a `$1C`-aligned
run has a calculated inclusive endpoint of `$E3`. Emu198x can represent that
logical endpoint before an NTSC long line wraps. On a short line, the
cross-wrap decision uses the endpoint only to preserve the proven next-line
start-admission result; its exact bus activity remains unresolved. A run
first started by a DDFSTRT match at `$D8` does not consume the same hard-stop
edge.

The current machine execution model evaluates Agnus beam events before
dispatching the Copper for that CCK. The frozen endpoint therefore precedes
a same-position Copper MOVE. If programmed DDFSTOP itself matches at `$D8`,
the ordinary comparator match is recorded first. Its endpoint is identical,
so no second hard-stop transition is required. Once an endpoint is frozen,
a later comparator or register write cannot replace it. These are explicit
implementation-ordering rules with regressions, not additional timing claims
taken from the manuals.

This is a deliberate evidence-weighted implementation choice, not a claim
that the Hardware Reference Manual's 49-word statement has been disproved.
The 49-versus-50 result remains open pending a real-hardware pointer or DMA
trace.

No extra serialized field is needed. `ddf_fetch_end` already preserves the
behavioural state that cannot be reconstructed after `$D8`. Runtime
snapshots nevertheless advance to schema version 7: a version-6 snapshot
captured after `$D8` could contain an active start with no terminal endpoint.
Restoring it under the new implementation could continue fetching past the
fixed boundary.

## Deferred behaviour

This decision does not define:

- the exact bus slot, pointer advance or modulo timing of a phase-shifted
  original-Agnus terminal unit across horizontal wrap;
- register-equal DDF boundaries with a pre-existing run, and
  stop-before-start sequences;
- the enhanced-chipset left-hand hard start and complete multi-region
  sequencer;
- variable-beam interactions;
- AGA wide-fetch final states;
- exact modulo timing;
- a resolution of the 49-versus-50 evidence conflict.

## Verification

Hermetic tests cover:

- DDFSTOP at `$D8`, later than `$D8`, and already behind the beam;
- a DDFSTRT match coincident with the `$D8` hard edge;
- phase-shifted endpoint calculation to `$E3` and surviving post-`$DF`
  grants;
- release of post-terminal bus slots;
- immunity to a later DDFSTOP rewrite;
- a same-CCK Copper rewrite after the hard event;
- machine-level bitplane pointer advances;
- the mixed Fat Agnus `HARDDIS` path retaining its available
  post-`$DF` grants;
- a machine snapshot round trip immediately before the hard event;
- postcard snapshot round trips both immediately before the hard event and
  while the terminal unit is pending;
- a `$1C`-phased `$E3` endpoint surviving postcard restore immediately
  before short-line wrap and producing the same next-line admission result.

## Related documents

- [One Agnus DMA-slot authority per CCK](amiga-single-slot-authority.md)
- [Original Agnus cross-line DDF hard-start gate](amiga-ocs-ddf-hard-start-gate.md)
- [Enhanced Agnus horizontal DDF hard limits](amiga-enhanced-ddf-hard-limits.md)
- [Idle register-equal DDF boundaries](amiga-idle-equal-ddf-boundaries.md)
- [Amiga sprite DMA lifecycle](amiga-sprite-dma-lifecycle.md)
- [Save State Format](save-state-format.md)
