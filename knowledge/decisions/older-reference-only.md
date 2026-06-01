---
title: Emu198x-Older is fully harvested, reference-only
date: 2026-06-01
status: binding
scope: Amiga / 68k / Spectrum / NES / C64 chip layers
---

# Emu198x-Older is fully harvested, reference-only

## Decision

`Emu198x-Older` is the frozen earlier Rust codebase (commit
`a4f37b1`) that holds an earlier-generation Amiga + Spectrum + NES
+ C64 forward-port. Companion to `Emu198x-Oldest` (the multi-system
donor — see [[emu198x:aga-donor-reference-only]]).

After verification on 2026-06-01, **all substantive content is
already forward-ported** into current Emu198x and is at or beyond
parity. Older joins Oldest as a reference snapshot only.

## What was verified at parity

Spot-checked the candidates where Older had bigger crates than
current:

| Crate | Older LoC | Current LoC | Status |
|---|---|---|---|
| `motorola-68000` (monolith) | 14 155 | 6 954 (`motorola-68000`) + split per-variant `-68010/20/30/40` | Forward-ported and **split** by family member |
| `motorola-68000/src/fpu.rs` | 719 | 705 in `motorola-68040/src/fpu.rs` (same docstring) | At parity, moved to 68040 crate |
| `motorola-68000/src/mmu.rs` | 2 429 | 2 421 in `motorola-68030/src/mmu.rs` | At parity, moved to 68030 crate |
| `commodore-denise-ocs` | 2 319 | 1 714 | 19 interlace mentions in current; smaller because cleaner |
| `mos-cia-6526` | 913 | 640 | Smaller because current is more cycle-accurate (no test scaffolding) |
| `machine-commodore-amiga` (monolith) | 2 580 | split into `machine-commodore-amiga-{ocs,ecs,a1200}` | Forward-ported and **split** per chipset |

Apart from those: every other Older chip and machine crate is at or
larger than its Older counterpart, indicating current has progressed
materially past the snapshot. Workbench 3.1 boots; Tom Harte 68000
runs 100% (1 000 058 tests).

## What was pulled out

**`format-sinclair-zx-spectrum-rzx`** (564 LoC, 7/7 tests) — the
only Older crate without a current equivalent. RZX is the
Ramsoft input-recording format used across the Spectrum emulator
scene (RealSpectrum, ZXSpin, Spectaculator, ZEsarUX). Useful
specifically for replay-based regression testing — record a
baseline session against a known-good build, replay it in CI on
every PR, and verify the result hash. A subtle contention or AY
regression that doesn't break boot tests will trip the replay.

Carries the `Adapted from Emu198x-Older/…` provenance line in
the module docstring per project convention.

## What stays in Older as reference-only

Everything else. Specifically:

- **`motorola-68000` monolith with bundled FPU + MMU + microcode**
  — consult when the split crates need to grow new instructions or
  when investigating regression in the FPU/MMU paths
- **`machine-commodore-amiga` / `machine-commodore-amiga-archive`**
  (A500 OCS monolith) — consult for the original tick-loop and
  clock-tree structure before they were split per-chipset
- **`format-amstrad-dsk`** — Amstrad disk format; reference for when
  CPC support actually lands
- **Spectrum variant machines** (`machine-pentagon-128`,
  `machine-scorpion-zs256`, `machine-timex-tc2048`,
  `machine-timex-ts2068`) — reference for the regional/clone
  variants beyond the canonical 48/128/+/+2/+3
- **`format-sinclair-zx-spectrum-rzx`** — the *original* of what was
  just extracted; consult on format-spec questions, not for
  re-extraction

## Drift triggers

- About to "pull X out of Older" — STOP and grep current Emu198x
  first. The CLAUDE.md description ("Amiga + Spectrum at the point
  of the wiki→Emu198x/knowledge migration") understates the scope;
  Older has substantial C64 / NES / Spectrum-variant content too,
  and almost all of it is already in current under different crate
  names (split by chipset/variant).
- LoC says Older is bigger → STOP. Current is smaller in several
  places because it dropped test scaffolding and refactored, not
  because it's missing features. Read the actual files before
  asserting a gap.
- Tempted to delete `Emu198x-Older/` to free disk — STOP. It's the
  ground-truth snapshot of the pre-split codebase and the
  archaeology trail for "why is this file split the way it is".
  Keep it frozen.

## Status of both archive codebases

As of 2026-06-01:

- `Emu198x-Oldest/` (multi-system donor) — fully harvested for 22
  systems; AGA scaffold reference-only — see
  [[emu198x:aga-donor-reference-only]].
- `Emu198x-Older/` (frozen earlier Amiga + Spectrum + NES + C64) —
  RZX pulled; everything else is forward-ported and at parity.
  Reference-only.

Neither needs further extraction passes.
