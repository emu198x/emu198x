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

**What the proper fix needs.** Co-ordinated shift of all four pipeline timing points:
1. `fetch_start: 4` and `fetch_end: 260` ✓
2. `IDLE_TABLE` and `MEM_TABLE` shifted as above ✓
3. **Also shift the latch-to-register transfer point** from `p & 0x07 == 4` to `p & 0x07 == 0`, so the previous fetch's latched byte is in `data_reg` 4 pixels before the new fetch overwrites the latch.
4. **Verify `screen_start: 12` stays correct** — it might need adjustment too depending on where the rendered pixels actually start.

Plus: the 128K config will need its own coordinated shift if we want it to match FUSE's 14364 (currently we're at 14368, +4).

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
