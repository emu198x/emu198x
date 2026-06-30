---
title: "plan: C64 VIC-II VC/VCBASE/RC rewrite — incremental"
type: plan
date: 2026-06-30
system: docs/systems/commodore/c64.md
parent_plan: docs/plans/2026-06-08-c64-100-percent-plan.md
decision: knowledge/decisions/c64-architecture-review.md
status: in-progress — Increments 1 (oracle, 40e3eafd) + 2 (shadow counters, c694a4db) + 3 (c-access streaming) landed; Increment 3b (g-access via VC/RC) next
---

# C64 VIC-II VC/VCBASE/RC rewrite — incremental plan

This is the working breakdown for **Bucket 1 / Tier B** of the
[C64 100% plan](2026-06-08-c64-100-percent-plan.md): replacing the VIC-II's
geometry-derived addressing with the real video-counter chain (VC, VCBASE,
RC, VMLI) and the per-cycle c-access / g-access split. It exists so the
increment structure survives context clears — the prior planning session's
breakdown was never persisted, which is why this file now does.

## Why

Today `mos-vic-ii` derives the matrix address from screen geometry:

```rust
// crates/mos-vic-ii/src/lib.rs:349
let text_row = (self.raster_line - DISPLAY_START_LINE) / 8;   // geometry
let screen_addr = screen_base + text_row * 40 + col;          // batched, cycle 15
```

The 40 screen-code + 40 colour reads happen in one batched loop at cycle 15
(`fetch_screen_row`), and `char_row` is advanced by line-wrap arithmetic. The
**timing scaffold around this is already correct** (badline detection, the
3-cycle BA lead-in, sprite DMA windows — all audited in Seam 1 of the
[architecture review](../../knowledge/decisions/c64-architecture-review.md)).
What is missing is the *addressing layer underneath*: the real VIC-II latches
VC from VCBASE, advances VC per c-access, advances RC per row, and streams one
c-access (Phi2) + one g-access (Phi1) per cycle. That chain is what makes
**VSP/AGSP, FLI, DMA-delay, linecrunch, and the sprite-crunch edge cases**
exact. Geometry addressing gets every normal screen right and every cycle-exact
raster trick wrong.

## Ground truth (cited sources)

- **VICE `cycle_tab_pal[]`** — `emulators/c64/vice-3.10/src/viciisc/vicii-chip-model.c:111-238`.
  The canonical per-phase (Phi1/Phi2) PAL schedule. All other emulators
  (VirtualC64, Frodo, Hoxs64) validate against this. NTSC table follows at
  `:272-403`.
- **Repo reference distillation** — `reference/by-topic/vic-ii/vic-ii-reference.md`:
  badline cycle effect (`:436-455`), cycle-by-cycle PAL table (`:479-540`).
  Agrees with VICE.
- **VC/VCBASE/RC/VMLI pseudo-code** — Christian Bauer's *vic-ii.txt* §3.7.2.
  **Not reproduced verbatim in-repo** (the distillation references it but does
  not encode the algorithm); Increment 2 transcribes it from Bauer with the
  per-cycle update flags cross-checked against VICE's `UpdateVc` (Phi2 cyc 14),
  `UpdateRc` (Phi2 cyc 58), `UpdateMcBase` (Phi2 cyc 16) markers.
- **Synthesis** — `syntheses/commodore-c64/vic-ii-rendering-and-badlines.md`.

## Cycle-numbering note (load-bearing)

VICE and the reference number PAL cycles **1–63**. Our engine's `raster_cycle`
is **0-based, 0–62**, yet reuses VICE's literal numbers (`badline_ba_low` =
`12..=54`, sprite eval at `55`, p-access at `58/60/62`). Whether that is a
systematic off-by-one or a deliberate origin choice is itself an oracle finding
(see Increment 1). The oracle encodes the canonical schedule in **1-based**
numbering and carries an explicit `engine→canonical` cycle map so the
convention is visible, not assumed.

## Increments

Each increment is one commit's worth of work, builds + tests green before the
next, and is independently revertable.

### Increment 1 — oracle harness  ✅ landed (`40e3eafd`)

Build the per-cycle comparator **before** touching the addressing, per the 100%
plan's recommended sequence ("build alongside, not after"). **No engine
behaviour change.**

- `src/oracle.rs` (`pub mod oracle`): encode the VICE PAL schedule as data —
  per cycle 1–63, the Phi1 access, the Phi2 access, the BA-low sprite/fetch
  mask, and the update flags (`UpdateVc`/`UpdateRc`/`UpdateMcBase`/`ChkSprDma`).
  `AccessKind` enum (Idle, Refresh, FetchC, FetchG, SprPtr(n), SprData(n,k)).
  A `compare(observed, &CANONICAL_PAL, map)` returning structured divergences.
- `tests/vic_oracle.rs`: a `RecordingMemory: VicMemory` that logs every
  `read_vram` / `read_colour` with the cycle it occurred on; drive a real `Vic`
  across a badline line, a non-badline line, and a sprite-active line; capture
  per-cycle `ba_low` + the fetch set; compare to canonical.
- **Live-assert** the facts Seam 1 already proved correct: BA-low window on a
  badline, BA released at the right cycle, sprite BA lead-in.
- **Document (not fail)** the known divergences the rewrite must close: the
  batched 40-c-access-at-cycle-15 fetch, batched sprite data fetch, and any
  cycle-origin off-by-one. These land as `#[ignore]`d tests carrying the
  acceptance criterion for Increment 3, so they flip green when the rewrite is
  done.

**Done when:** the harness runs, live assertions pass against the current
engine, and the rewrite's target behaviour is encoded as ignored tests with
explicit expected values.

### Increment 2 — VC/VCBASE/RC/VMLI counter state (shadow)  ✅ landed (`c694a4db`)

Introduced the counter chain as engine fields, advanced per the VICE rules
(start-of-frame reset, UpdateVc cyc 14, UpdateRc cyc 58, badline-clears-idle,
per-g-access VC/VMLI advance), running **in parallel** with the existing
geometry addressing. The oracle proves the shadow counters produce the same
matrix addresses the geometry path uses (VC == base + column over the 40
c-accesses; VCBASE == text_row × 40 per row; RC 0-7 per block). The counters
do not yet drive fetches; `FRAME_ROUTING_VERSION` stays 1 (output unchanged).

### Increment 3 — per-cycle c-access streaming  ✅ landed

Replaced the batched, geometry-addressed `fetch_screen_row` with a per-cycle
c-access: on a badline, each Phi2 cycle (15-54) reads one video-matrix code +
colour into the matrix line buffer at VMLI, addressed by `screen_base + VC`.
The two Increment-1 acceptance tests (c-access + colour stream one-per-cycle)
flip green; the sprite one stays for Increment 4.

**Output is bit-identical** for normal screens — `screen_base + VC` equals the
old `screen_base + text_row*40 + col` (proven by Increment 2), so every C64
render/golden/boot test passes unchanged. Because no captured frame hash
changes, **`FRAME_ROUTING_VERSION` stays 1 and no re-capture is triggered** —
the version bump moves to Increment 3b, which is where pixels can actually
diverge. (One behavioural refinement: `last_bus_data` now tracks each c-access
rather than the batch, marginally more correct for `$2F-$3F` open-bus reads;
frame hashes capture pixels, not bus-read order, so unaffected.)

### Increment 3b — g-access / render addressed via VC/RC  ← (next)

Switch the g-access (character/bitmap fetch) and the renderer to address via
VC/RC and read from the VMLI-indexed matrix buffer, replacing the remaining
geometry (`char_row`/`text_row`) addressing. This is the half that **diverges
from geometry under mid-line register writes** ($D011 YSCROLL, $D018, $D016) —
i.e. it is the actual enabler for VSP/AGSP/FLI. It needs new oracle cases for
those divergent scenarios *before* the switch (so the change is provable, not
vibes), and it **bumps `FRAME_ROUTING_VERSION` → 2** with a forced C64
catalogue re-capture (the Seam 4 oracle makes that fail loud). Split out from
Increment 3 because, unlike the c-access streaming, it changes output and so
deserves its own verification and the deliberate re-capture gate.

### Increment 4 — sprite DMA / pointer per-cycle

Move sprite p-access / s-access onto the documented cycle slots end-to-end
(partly present in `fetch_sprite_if_scheduled`), verified against the oracle.
Sprite-crunch, Y-expansion, and `$D015`-change-mid-line edge cases.

### Increment 5 — demoscene-trick validation

VSP/AGSP, FLI, DMA-delay, linecrunch — validated against the oracle and, where
sourced, against VIC-II test programs (groepaz/Lorenz; not currently in-repo —
sourcing tracked here).

### Increment 6 — NTSC 6567 (optional, post-PAL)

Encode the 65-cycle 6567R8 (and 64-cycle R56A) tables; extend the oracle and
counter chain. PAL is canonical for the engineering bar; NTSC follows.

## Non-goals

Same as the architecture review: no new chip crates, no chip-boundary refactor,
no cartridge/REU/8580 work (those are other buckets of the 100% plan).
