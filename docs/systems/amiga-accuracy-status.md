# Amiga Accuracy Status (A500/OCS Baseline)

This document tracks the current Emu198x Amiga baseline after Phase 1/2 work.
It is meant to separate "good enough to move into Phase 3" from "still known
accuracy gaps".

## Baseline Confirmed Working

- `machine-amiga` boots Kickstart 1.3 to the insert-disk screen.
- `machine-amiga` KS1.3 framebuffer regression assertions are in place
  (color set, anchor pixels, bounding box, color-count ranges).
- `amiga-runner` can run windowed and headless, capture screenshots, and report
  boot-to-insert-screen timing (`--bench-insert-screen`).

## CI-Covered Amiga Baseline (No ROM Required)

The GitHub Actions baseline workflow runs:

- 68000 core library tests (`motorola-68000`)
- CIA, Agnus, Denise, Paula, floppy-drive unit tests
- `machine-amiga` integration regressions:
  - `audio_paula`
  - `blitter_timing`
  - `disk_dma`
  - `sprite_dma`
  - `sprite_rendering`
- `amiga-runner` build/test smoke checks

The KS1.3 boot screenshot test remains local/manual because it requires a
proprietary Kickstart ROM.

## Major Accuracy Areas Improved (Phase 2+)

- Blitter line-mode octant decode and memory stepping fixed (Kickstart insert
  screen now renders correctly).
- Agnus owns a machine-facing CCK bus-plan API and drives arbitration decisions
  used by Paula, CPU waits, blitter progress, disk, and sprite DMA slot timing.
- Blitter timing now progresses on Agnus-granted queued DMA ops (area + line),
  with timing/IRQ/nasty-mode regressions.
- Paula audio has DMA/direct playback, IRQ timing improvements, period clamp,
  `ADKCON` modulation basics, and contention-aware DMA return timing.
- Disk DMA is Agnus-slot timed with simplified `DSKSYNC`, `DSKDATR`, `DSKBYTR`,
  `WORDSYNC`, wrapping read stream, and basic write-DMA / programmed-write paths.
- Denise sprite path has timed DMA fetch integration, basic rendering, attached
  pairs, `BPLCON2` priority, `CLXDAT` collision latching/filtering, and a
  stateful comparator/shifter model.

## Known Accuracy Gaps (Intentional / Outstanding)

### CPU (68000 family)

- `motorola-68010` / `motorola-68020` are thin wrappers today; many later-model
  semantics are not implemented yet (`MOVEC` path is only scaffolded).
- 68000 execution is functional and heavily tested, but not yet a complete
  cycle-accurate bus-timing model for every instruction/prefetch edge case.

### Agnus / Bus Arbitration

- Agnus bus-plan is now the source of truth for current consumers, but some
  grants remain simplified and not fully hardware-slot exact under all modes.
- Blitter execution timing now follows queued ops, but internal blitter micro-op
  behavior is still an approximation versus a fully cycle-accurate hardware model.

### Paula Audio

- Audio output is good enough for baseline software, but analog filtering/DAC
  character is not modeled.
- Some Paula edge cases remain approximate (mid-cycle register latching details,
  all interrupt latencies, full `ADKCON` behavior under extreme modulation cases).

### Paula Disk / Floppy Path

- Current disk read/write path is still a simplified MFM-word stream bridge, not
  a full bit-level PLL/decoder/encoder pipeline.
- `DSKBYTR` timing has been improved, but it is not a full bitcell-accurate disk
  serial implementation.
- Write path currently captures words in the drive model; full media mutation and
  exact write encoding semantics are not implemented.

### Denise / Sprites / Collisions

- Sprite rendering and `CLXDAT` are substantially improved, but some mid-line
  register-write timing details are still approximate.
- Additional hardware edge cases (especially exact serial load timing and all
  collision timing nuances) may still diverge from real hardware.

### Runner / Integration

- `amiga-runner` audio output is basic (host device selection/resampling/fallbacks
  are limited).
- No full desktop UX yet (debug overlays, runtime device switching, save states,
  etc.) — this is expected at this stage.

## Manual Regression Commands (Local)

```sh
# KS1.3 boot screenshot regression (requires ROM)
AMIGA_KS13_ROM=roms/kick13.rom \
  cargo test -p machine-amiga --test boot_kickstart test_boot_kick13 -- --ignored

# Headless timing benchmark to insert-disk screen
cargo run --release -p amiga-runner -- \
  --rom roms/kick13.rom \
  --headless \
  --frames 300 \
  --bench-insert-screen \
  --mute
```
