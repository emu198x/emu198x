---
title: "plan: C64 VIC-II VC/VCBASE/RC rewrite — incremental"
type: plan
date: 2026-06-30
system: docs/systems/commodore/c64.md
parent_plan: docs/plans/2026-06-08-c64-100-percent-plan.md
decision: knowledge/decisions/c64-architecture-review.md
status: in-progress — Increment 1 (oracle harness) underway
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

### Increment 1 — oracle harness  ← (current)

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

### Increment 2 — VC/VCBASE/RC/VMLI counter state (shadow)

Introduce the counter chain as engine fields, advanced per Bauer §3.7.2 with
the VICE update-flag cycles, running **in parallel** with the existing geometry
addressing. The oracle asserts the shadow counters produce the same matrix
addresses the geometry path uses on normal screens — but the counters do not
yet drive fetches. No `FRAME_ROUTING_VERSION` bump (output unchanged).

### Increment 3 — per-cycle c-access / g-access streaming

Replace `fetch_screen_row` (batched, geometry) with per-cycle c-access (Phi2,
badline only) into a 40-entry matrix line buffer indexed by VMLI, and g-access
(Phi1) addressed via VC/RC. Render off the streamed data. **Bumps
`FRAME_ROUTING_VERSION` → 2** and triggers a C64 catalogue re-capture (the
Seam 4 oracle makes that fail loud). The Increment 1 ignored tests flip green.

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
