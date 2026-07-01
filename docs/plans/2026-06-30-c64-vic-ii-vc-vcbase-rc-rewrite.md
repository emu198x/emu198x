---
title: "plan: C64 VIC-II VC/VCBASE/RC rewrite — incremental"
type: plan
date: 2026-06-30
system: docs/systems/commodore/c64.md
parent_plan: docs/plans/2026-06-08-c64-100-percent-plan.md
decision: knowledge/decisions/c64-architecture-review.md
status: in-progress — Increments 1-4 landed on branch c64-vic-ii-rewrite-oracle-harness (PR #711; addressing fully counter-driven, oracle passes 0 ignored, output-identical); Increment 5 pixel-oracle harness landed on c64-vicii-testbench-validation-inc5 (gfxfetch 99.33% vs VICE). Sprite vertical chain attempted a 2nd time (2026-07-01, cleaner BA/render decomposition, unit-green) and rolled back again — net-negative on the survey. Blockers are intrinsic to the chain, NOT a timing bug: a cycle-origin probe showed the write-phase is sound (dmadelay 100%), so "fix cycle-origin first" was wrong. Real blockers: frame-wrap phantom copy regresses sequencer-bug; mid-line $D017 expand toggles produce no rendered height change. Next: iterative testbench-driven chain development vs a per-sprite-height oracle. See Increment 5 § Second attempt.
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

### Increment 3b — g-access / render addressed via VC/RC  ✅ landed

Switched the g-access (character/bitmap fetch) and the renderer to address via
the counter chain — RC for the character sub-row, VC for the bitmap matrix
offset — and removed the geometry fields (`char_row`, `text_row`) entirely. The
text g-access is now `char_base + char_code*8 + RC`; the bitmap g-access is
`bitmap_base + VC*8 + RC` (VICE `g_fetch_addr`, vicii-fetch.c:169). The renderer
is now fully driven by the video-counter chain — no geometry left.

**Output is bit-identical** for normal content: at render time `RC == char_row`
and `VC == text_row*40 + col`, so every C64 unit / render / golden
(`diag_aztec_vic_state`) / boot / snapshot test passes unchanged, clippy clean.

**The `FRAME_ROUTING_VERSION` bump did *not* materialise** — and that is the
honest, correct outcome. The rewrite, done carefully, is a faithful refactor:
it reproduces every pixel of existing (geometry-correct) content while making
the addressing hardware-exact. The behaviour only *diverges* from geometry
under mid-line register writes ($D011 YSCROLL, $D018, $D016) — the VSP/AGSP/FLI
cases — which **no current test or catalogue title exercises**. So no captured
frame hash changes, and forcing a re-capture would be churn. The version bump
(and re-capture) now belongs to **Increment 5**, where a sourced trick test
first demonstrates a real divergence and captures its golden under v2.

### Increment 4 — sprite p/s-access split across two cycles  ✅ landed

Split the batched sprite DMA fetch (pointer + 3 data bytes all on the p-access
cycle) into the hardware two-cycle shape: **p-access + data byte 0** on the
sprite's pointer cycle, **data bytes 1-2** on the next cycle. The data base is
latched in `sprite_fetch_base` between the two cycles, which matters for sprite
2 — its pair straddles the line boundary (engine cycle 62 then 0). The last
ignored oracle test (`sprite0_data_access_spans_two_cycles`) flips green, so the
whole harness now passes with **zero ignored** acceptance tests.

**Output is bit-identical:** the same three bytes land in `sprite_data[i]`
before the sprite is rendered on the next line — only the *cycle* the reads
happen on changes. Full C64 suite + clippy green. Like the addressing work,
the behavioural payoff (sprite-crunch, `$D015`-mid-line, sprite-pointer-fetch
timing) is latent until trick content exercises it (Increment 5).

Remaining sprite edge cases (sprite-crunch, Y-expansion DMA quirks) ride along
with Increment 5's validation rather than being chased blind here.

### Increment 5 — demoscene-trick validation  🚧 harness landed; validation in progress

Pixel-oracle validation against the **VICE VICII testbench** (46 categories,
per-chip-revision reference PNGs), staged external + env-gated at
`~/.emu198x/test-suites/c64-vicii/` (from `vice-emu-code-r46155-testprogs.zip`).
Harness: `crates/runtime-commodore-c64/tests/vicii_testbench.rs` — boots real
ROMs, loads a test `.prg`, RUNs it, captures the framebuffer, and compares to
VICE's PAL 6569 reference **by C64 colour index** (VICE's PNGs use a different
palette, so raw RGB won't match). Crop alignment derived by calibration:
our 416×312 → VICE's 384×272 at offset **(16, 16)**.

`gfxfetch` is locked as a 99.33% regression floor. A breadth survey
(`survey_testbench_categories`) then measured 13 rewrite-relevant categories to
reveal the *shape* of divergence rather than fixate on one test:

| match | category | reading |
|------:|----------|---------|
| 100.00% | dmadelay | DMA-delay trick the rewrite enables — **exact** |
| 99.99% | greydot | grey-dot bug — exact |
| 99.71% | spritedma | sprite DMA on/off — near-exact |
| 99.33% | gfxfetch | in-line fetch — small residual (locked floor) |
| 95.18% | spritecrunch | sprite-crunch edge |
| 94.29% | spritefetchbug | sprite fetch edge |
| 92.29% | sequencer-bug | sequencer edge |
| 92.09% | border | border timing |
| 88.98% | videomode | mode switches |
| 87.80% | screenpos | screen position |
| 83.88% | vicii_timing | register-write timing |
| 76.28% | sb_sprite_fetch | single/blank sprite fetch |
| 18.10% | colorfetchbug | **unmodeled colour-fetch bug** (distinct quirk) |

Findings: (1) the rewrite's **core is validated** — `dmadelay` exact, the
DMA/fetch/grey-dot cluster all ≥99%. (2) The residuals are **spread, not
uniform** → not one global sub-cycle offset; each is a specific behaviour at
varying fidelity. (3) `colorfetchbug` (18%) is an outlier confirmed by
framebuffer dump to be a *fundamentally different image* — the VIC-II
colour-fetch bug is not modelled at all. That is a **distinct accuracy item**,
not part of the VC/VCBASE/RC addressing rewrite; it should be tracked
separately, not conflated with "the rewrite is incomplete".

Remaining Increment 5 work is therefore a **prioritised set of targeted
fidelity fixes** (worst-first: colour-fetch bug, `sb_sprite_fetch`,
`vicii_timing` register-write delay, then the 88-95% edge cluster), each
turned into a gated regression test as it is closed. The version bump +
catalogue re-capture land with whichever fix first changes a catalogue title's
pixels.

#### Sprite vertical chain (MC/MCBASE/exp-flop) — attempted, rolled back

The sprite cluster (`spritecrunch` half-height, `spritefetchbug`,
`sb_sprite_fetch`) traces to **one root cause**: our sprite model uses a
simplified fixed height (`line_in_sprite < 21/42`) instead of the real
per-sprite **MC / MCBASE / expansion-flip-flop** vertical chain. Sprite crunch
*is* the expansion-flop timing, so it can't be reproduced without it.

The VICE algorithm (extracted, `vicii-cycle.c` / `vicii-fetch.c`) is:
- **check_sprite_dma** (cyc 55, 56): `enable & Y==raster&0xFF & DMA-off` → DMA
  on, `MCBASE=0`, `exp_flop=1`.
- **check_exp** (cyc 56): DMA-on & Y-expanded → `exp_flop ^= 1`.
- **check_sprite_display** (cyc 58): `MC = MCBASE`; display bit set when
  `DMA & enable & Y==raster` (persists until DMA off), cleared when DMA off.
- **s-access:** read `(pointer<<6) + MC`, then `MC = (MC+1) & 63`.
- **sprite_mcbase_update** (cyc 16): `if exp_flop { MCBASE = MC; if MCBASE==63
  → DMA off }`.

**Implemented in full and rolled back** (uncommitted) because it introduced a
**1-line sprite-appearance regression**: `sprite_renders_at_correct_position`
went black. Root cause of the regression: VICE turns DMA/display on when
`Y == raster` and relies on a **separate delayed sprite sequencer** for the
final pixel phase; our `overlay_sprites` draws immediately on the display-bit
line, so the VICE-literal `Y==raster` compare renders sprites one line late.
**Hypothesised fix for next session:** compare `Y == (raster_line + 1) & 0xFF`
in `check_sprite_dma` + `check_sprite_display` to align the chain with our
fetch-ahead + immediate-draw pipeline — then validate against
`sprite_renders_at_correct_position`, the survey (`spritecrunch` should climb),
and the C64 goldens. Also needs rework of ~4 old-model unit tests
(`sprite_dma_y_expand_extends_height`, `sprite_fetch_happens_at_p_access_cycles`,
`sprite_y_expand_fetch_uses_halved_data_line`) which assert on the removed
`sprite_active`/`sprite_dma_active`/`data_line` internals. This is a clean,
well-scoped first task for a fresh session — deferred from the current one
under "roll back rather than thrash" rather than landed subtly-broken.

##### Second attempt (2026-07-01) — re-ported with the `+1` fix, rolled back again

Re-implemented the full chain with a cleaner **decomposition that avoided the
prior 1-line regression entirely**: keep the BA/CPU-stall path
(`evaluate_sprite_dma` → `sprite_dma_active`, cyc 55, current-line phase)
**completely untouched**, and add the MC/MCBASE/exp-flop chain to drive **only**
the render/data path (`sprite_paccess`/`sprite_saccess` read `(ptr<<6)+MC`;
height/crunch from the chain). The two paths were *already* independently phased
in the geometry model — that is why `each_sprite_steals_canonical_cycles` (BA,
current-line) and `sprite_renders_at_correct_position` (render, fetch-ahead)
coexisted. Applying the `+1` (`Y == next_display_line & 0xFF`) only to the
render chain, **all 75 unit + 11 oracle tests stayed green with zero rework** —
including the ~4 tests the first attempt feared, because `sprite_active` /
`sprite_dma_active` were preserved. Clippy clean.

**But the testbench survey showed the change is net-negative**, so it was rolled
back (uncommitted). Two concrete, sourced blockers — the real state of play:

1. **8-bit frame-wrap phantom activation regresses `sequencer-bug` 92.29 %→81.10 %.**
   `sequencer-bug/bug.prg` runs 8 sprites at **Y=50, Y-expanded**. The chain's
   `Y == raster & 0xFF` compare (faithful to VICE) matches Y=50 *twice* per
   frame: once at the real position (renders 50-91) and once at raster ≈305,
   whose `&0xFF` low byte is also 50 — re-activating the sprite to render
   306-311 then **wrapping through lines 0-35 (top border)**. Those wrapped-copy
   pixels fall inside the survey crop (our lines 16-35) and mismatch VICE's
   reference, which shows the sprites only once. The **old geometry model used
   absolute (non-wrapped) line ranges so it never drew the phantom copy** — i.e.
   the geometry approximation was *closer to VICE's reference* here. Open
   question for next time: does VICE actually suppress/clip this wrapped copy
   (its reference PNG shows clean top border), or is the reference captured on a
   frame before the wrap? Resolve against VICE before re-landing — a naive
   faithful port makes this category *worse*, not better.

2. **The target sprite categories did not move at all** — `spritecrunch`
   (95.18 %), `spritefetchbug` (94.29 %), `sb_sprite_fetch` (76.28 %) came back
   **byte-identical** to the geometry model. My first read was "the crunch
   bit-math (`vicii-mem.c` d017_store, gated on cyc 15) never engaged because
   the CPU write lands on the wrong `raster_cycle`" — pointing at a CPU/VIC
   cycle-origin bug. **A follow-up probe (2026-07-01) disproved that.**

**Cycle-origin probe — the write-phase is basically aligned, so it is NOT the
lever.** Instrumenting `Vic::write` to log the `raster_cycle` each cycle-precise
write is seen on:

- The **tick order does pre-increment**: `machine.rs` tick runs `vic.tick()`
  (which increments `raster_cycle` at the *end* of its work) *before* the CPU
  write, so a write is attributed to the cycle the VIC is *about* to process,
  one past the cycle it just rendered. Real, but small.
- `dmadelay/test1-2a-03.prg` (**100 % match**) writes `$D011` YSCROLL across a
  cycle sweep at line 47 (cyc 55/56/61/62/10) and reproduces VICE's badline
  delay **exactly**. If the write-phase were grossly off, this could not be
  100 %. So the CPU/VIC write alignment is *sound* for the badline mechanism.
- `spritecrunch-3b-00`'s crunch write (`$D017 ← $00`, clearing expand) lands at
  **cyc 17 on line 87, every line** — *not* the cyc-15 bit-math gate. So this
  variant's half-height comes from **normal exp-flop timing**, not the special
  cyc-15 corruption. The chain must reproduce it through the ordinary
  MCBASE/exp-flop update, and it didn't (byte-identical) — meaning the chain's
  mid-line-expand handling had no visible effect, a chain-internal issue.

**Corrected net finding:** the chain is easy to implement correctly (unit-green,
clean decomposition), but its blockers are **intrinsic to the chain**, not a
global timing bug: (1) the frame-wrap phantom copy (regresses `sequencer-bug`),
and (2) mid-line `$D017` expand toggles must actually change rendered sprite
height through MCBASE/exp-flop (currently no visible effect). The right next
step is **iterative, testbench-driven development of the chain against a
per-sprite-height oracle** — start from the simplest sprite-height testbench
case, get MC/MCBASE/exp-flop to move pixels there, resolve the frame-wrap copy
against VICE's actual output, and only then widen. A blind faithful re-port is
not enough; each sprite behaviour needs to be pinned to a specific reference
image as it is closed. (The `+1`-phase decomposition and the exact VICE cycle
map above are correct groundwork to build on.)

#### Rebuild groundwork (2026-07-01) — per-scanline oracle + scope finding

Built the per-scanline oracle (`diff_by_row` in `vicii_testbench.rs`, committed
`4af1111c`): for a category it reports the match % of each reference row below a
threshold, tagged with the engine raster line. This is the "build the comparator
first" discipline that carried Increments 1-4, applied to sprites so each chain
step pins to specific rows. Ground truth against the geometry baseline:

| category | match | where it diverges | reading |
|----------|------:|-------------------|---------|
| `spritedma` | 99.78 % | lines 55-90, 96-98 % edge rows | **parity anchor** — basic sprites already correct |
| `spritecrunch` | 95 % | lines 86-125, 81-95 % | the MC/MCBASE crunch-height case (~5 % available) |
| `sequencer-bug` | 92 % | lines **92+**, 50 % | a **graphics-sequencer** issue *below* the Y=50 sprites — not sprites |
| `spritefetchbug` | 94 % | periodic 30-50 % dips every ~10 lines | precise per-sprite fetch-bug behaviour |
| `sb_sprite_fetch` | 76 % | 102 rows, 16-50 %, whole top region | fundamentally different image |

**Scope finding — the fetch chain is necessary but nowhere near sufficient.**
VICE draws sprites through a **separate draw-stage shift-register sequencer**
(`vicii-draw-cycle.c:342` `draw_sprites`, `sbuf_reg` / `sprite_active_bits` /
`sbuf_mc_flops` / `sbuf_expx_flops`) — the "delayed sprite sequencer" this doc
referenced. That sequencer is what suppresses the frame-wrap phantom copy (VICE
renders no sprite at lines 16-35 for a Y=50 sprite, matching our geometry model),
and it is what the `spritefetchbug` / `sb_sprite_fetch` divergences turn on. Our
engine has no equivalent — `overlay_sprites` draws directly from `sprite_data`
at the sprite's X position. So:

- The MC/MCBASE/exp-flop **fetch** chain alone buys **at most the ~5 %
  `spritecrunch` gain**, and even that is entangled with the wrap.
- The large wins (`sb_sprite_fetch` 76 %, `spritefetchbug` 94 %→) require
  **porting VICE's draw-stage sprite sequencer** — a substantial, multi-increment
  subsystem replacing `overlay_sprites`, and the single biggest remaining piece
  of C64 VIC-II sprite accuracy.

**Recommended shape when this is picked up:** treat the sprite sequencer as its
own mini-rewrite with the same increment discipline — (1) oracle in place ✅;
(2) port the draw-stage shift-register sequencer in *shadow* (assert it
reproduces `overlay_sprites` for `spritedma`'s parity rows before switching the
renderer over); (3) switch the renderer to the sequencer, holding `spritedma`
≥99.78 % per-row; (4) add the MC/MCBASE fetch chain feeding it (crunch); (5)
close `spritefetchbug` / `sb_sprite_fetch` per-row against the oracle. This is
larger than the VC/VCBASE rewrite was and deserves its own plan section or file.

### Increment 6 — NTSC 6567 (optional, post-PAL)

Encode the 65-cycle 6567R8 (and 64-cycle R56A) tables; extend the oracle and
counter chain. PAL is canonical for the engineering bar; NTSC follows.

## Non-goals

Same as the architecture review: no new chip crates, no chip-boundary refactor,
no cartridge/REU/8580 work (those are other buckets of the 100% plan).
