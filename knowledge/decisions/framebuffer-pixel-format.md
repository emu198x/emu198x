# Decision: framebuffer pixel format is a per-chip choice, not a fleet rule

**Date:** 2026-07-07
**Status:** Landed — records existing behaviour; narrows [RULES.md](../../RULES.md) Rule 11. Closes #776 (VIC-II ARGB32 audit flag) and folds in #774 (VIC-II distillation refresh).

## The decision

A video chip may emit its framebuffer as either **palette-indexed `u8`** or
**ARGB32 `u32`**, whichever is natural for that chip. There is no requirement
that a core render to indices with a separate RGBA stage. The `CapturedFrame`
pipeline already carries both (`PixelFormat::Indexed8 + palette` and
`PixelFormat::Rgba8888`) and converts indexed frames to RGBA in
`emu198x-native-video`, so the choice is invisible downstream.

The **VIC-II renders directly to ARGB32 and that is correct.** No change to
`mos-vic-ii`.

## Why this came up

The C64-family audit filed #776 reading RULES.md Rule 11 — "The ULA renders to a
palette-indexed `u8` framebuffer; RGBA conversion is a separate stage" — as a
project-wide pipeline convention, and flagged the VIC-II (`Vec<u32>`, palette
baked in at render time via `palette::PALETTE`) as breaking it.

A fleet survey inverts the premise. Rule 11 is followed by **8 crates** — the
Spectrum ULA family (`ferranti-ula-6c001e`, `amstrad-ula-40077`,
`pentagon-ula`, `sinclair-ula-7k010e`, `scorpion-ula`, `timex-scld`) plus the
Game Boy PPU (`nintendo-game-boy-ppu` + machine) — and **ignored by 32**,
including the VIC-II's own architectural siblings: `mos-vic-i`,
`ricoh-ppu-2c02` (NES), `ti-tms9918`, `sega-vdp`, and every Atari chip
(`atari-tia`/`gtia`/`maria`). The VIC-II is not the odd one out; it matches the
overwhelming majority. If anything Rule 11 was mis-worded, not the VIC-II
mis-built.

## Why per-chip choice is right (and indexing the VIC-II is not worth it)

This is **not a correctness issue.** A C64 frame is pixel-identical whether the
VIC-II bakes the palette at render time or emits indices for a later lookup.

Indexing the VIC-II would only unlock forward-looking capability that **nothing
consumes today**:

- **Palette swapping** — Pepto / Colodore / VICE-variant C64 palettes, PAL/NTSC
  tint, CRT colour-bleed — as a cheap late-stage lookup instead of a re-render.
  Genuinely appealing *for the C64*, but there is no palette-picker UI or shader
  asking for it.
- **Index-based GPU shaders** — the SDL_GPU direction *could* push the palette
  lookup onto the GPU. Not built.
- **Palette-independent catalogue frame hashes** — hashing indices would let a
  palette tweak leave golden frames valid. A maintenance nicety, not a need.

And none of it pays off from converting the VIC-II *alone*: you cannot
palette-swap or index-shade "the fleet" until all 32 ARGB cores are indexed. A
single indexed core among its ARGB siblings buys inconsistency and no
user-visible feature. **If** we ever want index-space work, the right unit of
decision is a deliberate fleet-wide migration (VIC-II could be the pilot then),
not a lone rework of a freshly-rewritten chip. Recorded here so the audit flag
does not resurface: this was considered and deferred, not missed.

Consistent with the project values — "solve the problem in front of you, no
abstractions for hypothetical futures" and "boring technology wins."

## What changed

- **RULES.md Rule 11** reworded: still true for the ULA family, now explicitly
  *not* a fleet-wide rule, with the ARGB32 majority named and a pointer here.
- **`knowledge/chips/mos-vic-ii.md`** refreshed (the #774 fold-in) — the
  distillation was stale from the V2 sprite-sequencer rewrite
  (`FRAME_ROUTING_VERSION = 2`); re-derived from the current pipeline, including
  a note that ARGB32 output is deliberate per this decision. Local-only doc, so
  not part of the PR diff.
- **No code change.**

## Related

- [RULES.md](../../RULES.md) Rule 11 — the rule this narrows.
- [C64 architecture review](c64-architecture-review.md) — the review that set up
  `FRAME_ROUTING_VERSION`; the sprite-sequencer rewrite bumped it to 2.
- [Runtime internal shape](runtime-internal-shape.md) — where `CapturedFrame`
  and the per-runtime frame construction live.
- [`../chips/mos-vic-ii.md`](../chips/mos-vic-ii.md) — the VIC-II distillation
  (local-only) refreshed alongside this decision.
