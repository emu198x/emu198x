# ULA first-display-fetch T-state offset (open investigation)

**Status:** Open 2026-05-18, **substantially resolved same day in framing** after Chris Smith's *The ZX Spectrum ULA: How to design a microcomputer* was ingested and Chapters 18 + 12 distilled. The fix path and reason for the apparent offset are both now understood; implementation work remains.

**Updates 2026-05-19 (Chapters 9/16/19/21/23 distillations):**
- **Chapter 21 gives the verbatim 14336 derivation:** "the interrupt occurs exactly 64 scan lines before the first pixel of a frame is displayed by the television, which is 64 × 224 CPU clock cycles or 14336 T-states." Chapter 21 is the primary source; Chapter 11 cited it.
- **14335 is a Z80-die-batch dependency, NOT board-issue.** Smith documents the 42 ns /INT-to-clock-rise lag on a 6C001E-7 ULA; the "intolerance" is on the Z80 side (specific dies with stricter setup-time requirements). Emulators should use 14336 unless explicitly modelling a "warm Z80" corner case.
- **Chapter 19 confirms `floating_bus()` semantics:** `IN A,($FF)` returns whichever byte was most-recently latched into DataLatch or AttrLatch from the *current* fetch slot — NOT the byte being shifted to screen. The Seam 1 fix must preserve this: `bus_data` should track the pending-latch value, not the shifter output.
- **Chapter 23 is SILENT on the floating-bus sample point.** The architecture review previously framed Chapter 23 as the canonical reference for floating-bus semantics. That was wrong. Chapter 23 covers Test Modes and 5 documented silicon errors but doesn't derive `IN A,($FF)` sample-point semantics. Float48K (14338) remains the only empirical authority.

**Distilled references:**
- [`~/Projects/Emu198x-Reference/_organised/by-system/zx-spectrum/zx-spectrum-ula-chapter-10-internal-clocks.md`](../../../Emu198x-Reference/_organised/by-system/zx-spectrum/zx-spectrum-ula-chapter-10-internal-clocks.md) — C-counter / V-counter derivations from 14 MHz crystal. Critically: INT is a pure consumer of `(scan, pixel)`, not a producer — counters are never reset by INT.
- [`~/Projects/Emu198x-Reference/_organised/by-system/zx-spectrum/zx-spectrum-ula-chapter-11-video-synchronisation.md`](../../../Emu198x-Reference/_organised/by-system/zx-spectrum/zx-spectrum-ula-chapter-11-video-synchronisation.md) — **the canonical INT-to-first-fetch number is 14336 T-states**, derived in Chapter 21 p. 227 as "exactly 64 scan lines before the first pixel of a frame is displayed, which is 64 × 224 CPU clock cycles or 14336 T-states." 14335 is documented as the "late timing" alternative for early-issue boards with intolerant Z80s.
- [`~/Projects/Emu198x-Reference/_organised/by-system/zx-spectrum/zx-spectrum-ula-chapter-12-generating-the-display.md`](../../../Emu198x-Reference/_organised/by-system/zx-spectrum/zx-spectrum-ula-chapter-12-generating-the-display.md) — two-stage shifter pipeline, `DataLatch` and `SLoad` derivations, and the `VidEN = /Border delayed by one character-cell` finding that resolves the 14336/14338/14340 puzzle.
- [`~/Projects/Emu198x-Reference/_organised/by-system/zx-spectrum/zx-spectrum-ula-chapter-13-video-memory-access.md`](../../../Emu198x-Reference/_organised/by-system/zx-spectrum/zx-spectrum-ula-chapter-13-video-memory-access.md) — the 8-phase cycle is **continuously fetching** (two RAS-CAS fetch pairs per character cell), not "4 fetch + 4 idle" as the Chapter 18 distillation originally claimed.
- [`~/Projects/Emu198x-Reference/_organised/by-system/zx-spectrum/zx-spectrum-ula-chapter-14-video-control-clocks.md`](../../../Emu198x-Reference/_organised/by-system/zx-spectrum/zx-spectrum-ula-chapter-14-video-control-clocks.md) — complete signal derivations: CLK7, Border, VidEN, VidC3, DataLatch, AttrLatch, SLoad, AOLatch, Flash Clock. AOLatch is **not gated by VidEN** — silicon basis for 8-pixel border-write granularity.
- [`~/Projects/Emu198x-Reference/_organised/by-system/zx-spectrum/zx-spectrum-ula-chapter-18-cpu-clock-and-contention.md`](../../../Emu198x-Reference/_organised/by-system/zx-spectrum/zx-spectrum-ula-chapter-18-cpu-clock-and-contention.md) — contention mechanism, `CLKWAIT = (C3 OR C2) AND /Border AND A14 AND /A15 AND /MREQT23`, half-C0-cycle phase offset between ULA and Z80 clock domains.

## The three-event resolution (Chapter 12)

Our 4-T-state apparent offset is not a single bug. It is a confusion between three legitimate silicon-level taps on the same fetch event:

| T-state | Event | Sampled by |
|---|---|---|
| **14336** | First VRAM fetch — `DataLatch` fires on scan 0 | Community consensus (Patrik Rak) |
| **14338** | Fetched byte appears on the ULA data bus | Float48K `IN A,($FF)` probe |
| **14340** | First `SLoad` fires; pixel emission begins | Our current model |

The 4-T-state spacing between first `DataLatch` and first `SLoad` is silicon-correct: `SLoad` is gated on `/VidEN`, and `VidEN` is `/Border` delayed by one character cell (8 CLK7 cycles = 4 Z80 T-states). Our current model is not wrong about visible-pixel timing. What is wrong is that our `floating_bus()` exposes the byte at the same T-state as visible pixel emission, when it should expose it from the `DataLatch` point onwards.

Investigation started after Woody's `Float48K.tap` test (added in commit f9c5da1) failed to find its match T-state. Our `ferranti-ula-6c001e` appears to fetch the first display byte 4 T-states later than real Sinclair 48K hardware.

## The numbers

| Source | First display-byte fetch | Notes |
|---|---:|---|
| Community consensus (Patrik Rak, WoS forum 17551) | **14336** | T-states from INT assert |
| Woody `Float48K.tap` probe | 14338 | 2 T-states later than fetch — sample point of `IN A,($FF)` mid-instruction |
| `ulatest3` (Jan Bartholomew) | 14339 | 1 T-state later than `Float48K` — different sample convention |
| **Our `ferranti-ula-6c001e`** | **14340** | scan-0 pixel-8, derived below |

Our model is **4 T-states late** relative to the community first-fetch reference, and **2 T-states late** vs Float48K's expected match.

## Derivation of our 14340

From `crates/common-sinclair-zx-spectrum/src/ula_engine.rs`:

```rust
pub const CONFIG_48K: UlaConfig = UlaConfig {
    pixels_per_line: 448,        // 2 pixel-clocks per T-state, so 224 T-states/line
    lines_per_frame: 312,
    fetch_start: 8,              // pixel position where video fetch begins
    int_scan: 248,
    int_start_pixel: 1,
    ...
};
```

INT asserts at scan 248 pixel 1. Display lines are scans 0–191 (the per-tick rendering only sets `self.video = self.scan < 192`). After INT the frame continues: 248 → 311 (64 lines), wraps to scan 0, which is the first display line.

- From INT (scan 248) to scan 0: 64 lines × 224 T-states = **14336 T-states** ✓ (matches community)
- First VRAM read happens at pixel 8 within scan 0 (MEM_TABLE phase 8): pixel 8 = 4 T-states into the line
- Total: 14336 + 4 = **14340 T-states**

Community says 14336. We say 14340. Delta: **+4 T-states (8 pixel-clocks)**.

## Why this matches the Float48K observation

`Float48K.tap` builds an ML probe that does `(delay NOPs); IN A,($FF); RET` and iterates `delay` looking for a non-`0xFF` byte. The match `delay` value translates to a sample T-state. On real hardware the match happens at the iteration where the IN's sample point aligns with the active fetch window — published value 14338.

For us, the active fetch window starts 4 T-states later (14340 instead of 14336). Float48K's iteration uses 4-T-state granular delays (one NOP = 4 T-states); the probe should still find a match within our shifted window, but with a different `n` value. The current `float_bus.rs` PNG shows the probe at T-state offsets 14474..14495, well past where it should have matched — suggesting the issue is *not* just a translation in T-state space, or the test's matching criterion involves more than one byte and the offset breaks it.

Need to read the probe's ML to know exactly what byte it's matching against, before being certain whether a single `fetch_start` adjustment is sufficient.

## Fix design (cross-referenced against FUSE, 2026-05-18)

FUSE's `fuse/spectrum.c` (function `spectrum_unattached_port`) has the canonical 8-T-state phase table:

```c
switch( tstates_through_line % 8 ) {
    case 2: case 4: return [screen byte];   // bitmap fetches
    case 3: case 5: return [attr byte];     // attribute fetches
    case 0: case 1: case 6: case 7: return 0xff;  // idle
}
```

And the comment: *"the first byte being returned at 14338 (48K) and 14364 (128K)"*. That's the authoritative number — first byte on bus at T=14338, not T=14336 as I'd initially assumed for "first fetch". The 14336 figure is for the first VRAM access; the byte appears on the floating bus 2 T-states later.

Each FUSE T-state = 2 of our pixel-clocks. The phase mapping should therefore be:

| FUSE phase | Our pixels | Action | Current IDLE / MEM | Target IDLE / MEM |
|---:|---:|---|---|---|
| 0 | 0–1 | idle | true / true | true / true |
| 1 | 2–3 | idle | true / true | true / true |
| 2 | 4–5 | bitmap fetch | true / true | **false / read** |
| 3 | 6–7 | attr fetch | true / true | **false / read** |
| 4 | 8–9 | bitmap fetch | false / read | false / read |
| 5 | 10–11 | attr fetch | false / read | false / read |
| 6 | 12–13 | idle | false / hold | **true / hold** |
| 7 | 14–15 | idle | false / hold | **true / hold** |

So both `IDLE_TABLE` and `MEM_TABLE` need to shift left by 4 entries (= -2 T-states = -4 pixel-clocks). And `fetch_start` needs to move from `8` to `4` so the first scan-0 cycle hits the new active window.

## Empirical experiment 2026-05-18 — failed but informative

Applied four-thing change: `fetch_start: 4`, `fetch_end: 260`, `screen_start: 8`, and the shifted tables above. Result on Float48K:

- PR-ALL print count jumped from ~1013 → ~1495 (test is hitting more values).
- Framebuffer rendering broke: numbers appear as `4477  2255` instead of `14477  255` — leading character pushed off the left edge, doubled "2" suggests data_reg lagging by one full cycle.
- Reverting just `screen_start` to `12` did not fix the rendering — confirming the rendering breakage came from the table shift, not the screen-start shift.

**Why the rendering broke.** The latch-to-register transfer happens at `p & 0x07 == 4` (pixels 4, 12, 20, …), which under the OLD timing was 4 pixels BEFORE the next fetch at phase 8. Under the SHIFTED timing the transfer at pixel 4 happens AT THE SAME TIME as the new fetch at phase 4, so `data_reg` gets last cycle's bitmap byte instead of the byte intended for this cycle — pipeline depth jumps from 4 pixels to a full 16-pixel cycle. The result is screen content shifted right by one cell.

**What the proper fix needs — initial spec (insufficient, see deeper finding below).** Co-ordinated shift of all four pipeline timing points:
1. `fetch_start: 4` and `fetch_end: 260` ✓
2. `IDLE_TABLE` and `MEM_TABLE` shifted as above ✓
3. **Also shift the latch-to-register transfer point** from `p & 0x07 == 4` to `p & 0x07 == 0`, so the previous fetch's latched byte is in `data_reg` 4 pixels before the new fetch overwrites the latch.
4. **Verify `screen_start: 12` stays correct** — it might need adjustment too depending on where the rendered pixels actually start.

Plus: the 128K config will need its own coordinated shift if we want it to match FUSE's 14364 (currently we're at 14368, +4).

### Deeper finding — the single-latch model can't support FUSE's fetch pattern

Walking through the per-pixel timing once more after the experiment, the four-point spec above turns out to be insufficient: the rendering model itself doesn't fit FUSE-style fetches with a single bitmap latch. The detail:

In `tick_rendering()` the latch transfer is one trigger (`if (p & 0x07) == 4`), which fires twice per 16-pixel cycle — at pixel 4 and pixel 12 of each cycle. With OLD timing this works because:

- **OLD fetches** at pixels 8 and 12 within cycle (4 pixels apart, but with the second fetch happening AT the second transfer point).
- Pixel 4 transfer copies the latch (which holds the previous cycle's pixel-12 byte), giving `data_reg = previous cycle's byte 1`.
- Pixel 8 fetch writes `latch = byte 0 of this cycle`.
- Pixel 12 transfer copies `latch` (now byte 0) into `data_reg`. **The transfer fires BEFORE the fetch at the same pixel** (code order), so the new byte enters `data_reg` cleanly before the pixel-12 fetch overwrites the latch with byte 1.
- Pixel 12 fetch then writes `latch = byte 1 of this cycle`.
- Pixel 20 transfer (= pixel 4 of next cycle) copies `latch` (byte 1) into `data_reg`.

Net: each byte spends ~4 pixels in the latch before being transferred to `data_reg` and rendered for 8 pixels. Uniform 4-pixel pipeline, symmetric pipeline for both bytes per cycle. This is why the OLD model works.

For **NEW (FUSE-aligned) timing**, fetches are at pixels 4 and 8 within cycle. The two fetches are *still* 4 pixels apart, but they happen in the *first* half of the cycle instead of straddling the middle. The single-transfer-trigger pattern can't service both:

- A transfer trigger between the fetches (at pixel 5–7) would have to be a *third* trigger point — currently we have two (pixels 4 and 12 within cycle, both matching `p & 0x07 == 4`).
- The pixel-12 transfer in OLD timing was the "natural" point because it was both *after* fetch 0 and *before* fetch 1, with code-order preserving the byte. In NEW timing both fetches happen *before* pixel 12, so pixel 12 transfer captures only the second fetch — byte 1 ends up in `data_reg` where byte 0 should be.
- Adding a transfer at pixel 8 (just before that fetch) helps capture byte 0 — but then byte 1 (fetched at pixel 8) needs a transfer point between pixel 8 and the start of its render at pixel 20. Pixel 12 transfer fits, but it captures byte 1 from latch — which means byte 1 sits in `data_reg` from pixel 12 onward, overwriting byte 0 before byte 0 finishes rendering.

The fundamental issue: with FUSE-style timing, the gap between fetches (pixel 4 → 8 = 4 pixels) is *shorter than* the byte-render duration (8 pixels). A single `data_latch` holding "the byte currently being shifted" can't represent two bytes in flight at once.

### What a proper fix actually requires

One of:

1. **Two-deep latch.** Add `data_latch_pending` alongside `data_latch` and `data_reg`. Fetch into `data_latch_pending`. Promote `data_latch_pending → data_latch` at one trigger, then `data_latch → data_reg` at another. Two transfer triggers, one for each "pipeline stage shift". Mirror for `attr_latch_pending`. This preserves the existing rendering loop but pushes the pipeline one stage deeper.
2. **Per-fetch FIFO.** Replace the single bitmap latch with a small ring buffer that captures every bitmap fetch in order; the rendering loop pops a byte every 8 pixels at `screen_start + N*8` boundaries. Decouples fetch timing from render timing entirely. More invasive but more flexible — also handles 128K's different timing without further structural changes.
3. **Cycle-restructure with eager render.** Skip the latch entirely: render directly from a per-pixel fetched-byte buffer. Even bigger refactor; probably overkill.

Option 1 is the smallest change and most likely correct — it matches what real silicon almost certainly does (the ULA's shifter is fed from a register that's loaded from the fetch latch, two pipeline stages). The Verilog at https://opencores.org/projects/zx_ula (Chris Smith's HDL translation) should confirm.

### 128K is a separate co-ordinated change

The 128K config (`CONFIG_128K`) currently has `fetch_start: 8` too, and its first-fetch lands at T=14368 vs FUSE's 14364. The same -4 pixel shift applies. Tables are shared with 48K so they'd move together; only the 128K `UlaConfig` constants need their own update. **Do not change 128K until 48K is fixed and validated** — easier to debug one variant at a time.

### Pre-flight checklist before the next attempt

Before writing the fix:

1. Read `https://opencores.org/projects/zx_ula` to confirm whether real silicon has a 2-stage shift register feeding the rendering, and what the transfer triggers look like at the HDL level.
2. Decide between Option 1 (two-deep latch) and Option 2 (FIFO).
3. Write a small `cargo test` that exercises just the engine's fetch+transfer+render with a known memory pattern, asserting which byte appears at each framebuffer pixel — gives a fast-iteration check independent of Float48K. The existing `image_generation_test` in SpecIde's `ULATest.cc` shows roughly what this should look like.
4. Apply the structural change to the engine.
5. Update `CONFIG_48K` constants and the `IDLE_TABLE`/`MEM_TABLE` shifts in one commit.
6. Run Float48K with `EMU198X_FLOAT48K_STRICT=1` — must print 14338.
7. Run the existing 48K test suite + boot test — no regressions.
8. Once 48K is green, repeat constants update for 128K, validate against `Float128k.tap` (expected 14364).

This is bigger than a one-character experiment; it touches the rendering pipeline timing in several coupled places and needs a design pass before another attempt.

## What Chapter 18 should answer (still useful)

If you're transcribing Chapter 18 § "CPU Clock and Contention", the bits that resolve this:

1. **The 8-T-state ULA cycle phase table.** What does the ULA do at each of the 8 T-state phases within its repeating cycle: which phase is "idle / floating bus returns 0xFF", which fetches display byte, which fetches attribute byte, which holds the previous byte on the bus?
2. **The exact T-state of the first display-byte fetch on scan 0.** Specifically the offset from INT assert (or VSYNC start, whichever the book uses as origin) to the moment the ULA latches the first display byte.
3. **The pipeline diagram between fetch and render.** How many pixel-clocks does a fetched byte spend in the shifter before its first pixel comes out the analogue side?
4. **Whether `IN A,($FF)` sees the *currently fetching* byte or the *previously latched* byte.** The 14336 (fetch) vs 14338 (Float48K) vs 14339 (ulatest3) range comes from differences in what these tests are actually measuring.

Any one of these four — preferably as the *table* or *timing diagram* the book likely contains — is enough to choose between the fix candidates with confidence.

## What's currently in CI

Float48K is currently asserted as load-chain-passes only. The strict T-state assertion is gated behind `EMU198X_FLOAT48K_STRICT=1`. The two FUSE INIR/INDR allowlist cases are unaffected (different bug class).

## See also

- [`spectrum-test-oracle-priority.md`](spectrum-test-oracle-priority.md) — why we prioritise Spectrum-validated oracles for this kind of bug.
- [`no-rom-trap-load.md`](no-rom-trap-load.md) — why we run Float48K through the actual tape pipeline (which is what surfaced this).
- `../../crates/common-sinclair-zx-spectrum/src/ula_engine.rs` — the code to change once Chapter 18 lands.
- `../../crates/machine-sinclair-zx-spectrum-48k/tests/float_bus.rs` — the test that triggers it.
- `../../../Emu198x-Reference/_organised/by-system/zx-spectrum/ula-snow-effect.md` — the silicon-level snow-effect reference, transcribed from pages 246–248 of the same book.
- `../../../Emu198x-Reference/_organised/known-unknowns.md` — the open-question entry pointing here.
