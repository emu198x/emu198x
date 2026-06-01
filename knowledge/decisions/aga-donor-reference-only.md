---
title: Donor AGA scaffold is reference-only, not extraction
date: 2026-06-01
status: binding
scope: amiga / chip layering
---

# Donor AGA scaffold is reference-only, not extraction

## Decision

The Amiga **AGA chipset scaffold** in `Emu198x-Oldest/crates/` —
`commodore-agnus-aga` and `commodore-denise-aga` — is **not
extracted** like every other donor system in the 2026-06-01
harvest. It stays in the donor as a frozen reference snapshot.

Our current `commodore-agnus-aga` (191 LoC) and `commodore-denise-aga`
(442 LoC) are the deliberate forward-port; their docstrings carry
`Adapted from Emu198x-Oldest/...` provenance lines. We've layered
recent AGA-specific work on top:

- `d31e46a`: AGA 64-bit bitplane wide fetch (FMODE) + fix display
  corruption
- `369d50b`: AGA Workbench palette (68020 full-format EA decode)
- `bc1e43a`: DENISEID `$FFF8` → `$00F8` for AGA Lisa

Workbench 3.1 boots and runs on this stack — the architectural slice
is correct.

## Why not extract

The donor's larger Agnus AGA (278 LoC vs our 191) and slightly
smaller Denise AGA (372 LoC vs our 442) reflect a different
architectural choice: the donor implements AGA 24-bit palette
resolve, HAM8 chaining, BPLCON4 XOR, and wide-sprite emit
end-to-end. Our forward-port deliberately defers those paths — the
ECS 12-bit path remains live until a catalogue entry actually
requires AGA-specific rendering — and adds them incrementally as
they're needed.

Re-extracting the donor's AGA would either:

- Replace our deliberate deferral with bulk code that isn't pressure-
  tested by any catalogue entry, or
- Force a back-and-forth integration with our incremental approach.

Neither is the right move.

## What the donor's AGA stays useful for

When we *do* land 24-bit palette / HAM8 / wide-sprite emit, the
donor's working implementations are the **first place to read for
implementation precedent** — same role as the third-party emulators
in `198x/emulators/` (vAmiga, WinUAE). Specifically:

- `Emu198x-Oldest/crates/commodore-denise-aga/src/lib.rs`
  - `resolve_color_rgb24()` — 24-bit palette resolve with HAM8
    chaining + BPLCON4 XOR
  - `set_palette_aga()` — bank + LOCT write decomposition
  - `write_sprite_data_wide()` + `write_sprite_datb_wide()` — wide
    sprite DMA emission paths
- `Emu198x-Oldest/crates/commodore-agnus-aga/src/lib.rs`
  - `cck_bus_plan()` — 8-bitplane lowres bus-slot plan

## Drift triggers

If you find yourself:

- Comparing donor AGA LoC counts to ours and concluding "we should
  pull more in" — STOP. That comparison ignores the architectural
  choice. Re-read this record.
- About to copy `resolve_color_rgb24` wholesale into our crate —
  STOP. Read the donor's implementation as reference, write our own
  against current ECS rendering, and verify with a catalogue entry
  that exercises HAM8 / 24-bit / wide sprites.
- Tempted to delete `Emu198x-Oldest/crates/commodore-{agnus,denise}-aga/`
  — STOP. The donor codebase is fully harvested for every other
  system as of 2026-06-01, and these scaffolds are the last useful
  reference left there. Keep them frozen.

## Status of the rest of the donor

22 systems were extracted from `Emu198x-Oldest` between 2026-05 and
2026-06-01 (see `docs/status/outstanding-work.md` § rollup). Nothing
substantive remains apart from the AGA scaffold covered here.
