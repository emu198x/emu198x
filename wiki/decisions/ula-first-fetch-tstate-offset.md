# ULA first-display-fetch T-state offset (open investigation)

**Status:** Open 2026-05-18. Investigation started after Woody's `Float48K.tap` test (added in commit f9c5da1) failed to find its match T-state. Our `ferranti-ula-6c001e` appears to fetch the first display byte 4 T-states later than real Sinclair 48K hardware. Fix is blocked on silicon-level confirmation from Chapter 18 of Chris Smith's *The ZX Spectrum ULA: How to design a microcomputer*.

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

## Fix candidates

In rough order of likelihood:

1. **`fetch_start: 4`** instead of `8`. Earliest fix. Aligns first VRAM read with T=14336 exactly. The 4-pixel pipeline delay between fetch and render (currently `screen_start - fetch_start = 12 - 8 = 4`) would have to be preserved — would also need `screen_start: 8`. Need to verify this doesn't shift the rendered display 4 pixels to the left vs the border.
2. **`fetch_start: 0`** with no prefetch. Treats fetch and render as simultaneous. The `screen_start: 12` comment claims a pipeline delay exists; need silicon-level confirmation either way.
3. **`int_scan: 248` is wrong**. If INT actually asserts at scan 247 (or pixel 113 of scan 247), the "T=0 from INT" baseline shifts and the 14336 figure aligns naturally. Less likely — community sources agree INT is at the start of VSYNC at scan 248-equivalent.
4. **`int_start_pixel: 1` is wrong**. If INT asserts at pixel 0 (not 1), the T=0 baseline shifts by half a T-state. Less likely — would only fix a half-T-state discrepancy, not a 4-T-state one.
5. **The bug is elsewhere entirely**. Possibilities: `compute_data_addr` returns wrong row; the IDLE_TABLE phase boundary doesn't match silicon (we have idle[0..7] / active[8..15] — should it be idle[0..3] / active[4..15]?); the floating bus latches the byte at a different sub-phase than we model.

## What Chapter 18 should answer

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
