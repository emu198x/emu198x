# 1571 drive — build spec (C64-mode D71 LOAD first)

> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.


**Status:** planning / awaiting decision confirmation (2026-07-06).
**Issue:** #69 (covers both 1571 and 1581; the 1581 is done — PRs #747/#750/#751,
plus the D81 catalogue entry #753). This spec is the 1571 half.

Read alongside the 1581 spec (`2026-07-03-1581-drive-build-spec.md`) — the 1571
mirrors that effort's slicing, and reuses the same `common-commodore-iec`,
`western-digital-wd1770`, and `mos-cia-6526` crates.

## Goal (first milestone)

`LOAD"*",8,1` from a **D71** working on the C64 — GCR, double-sided — exactly
mirroring the 1581 milestone. This is the C64-mode path; MFM / CP-M / C128
fast-burst serial are **deferred** (they only matter for C128-mode software we
don't run yet).

## What a 1571 is

In **C64 mode the 1571 is a double-sided 1541**: same 6502 + two 6522 VIAs +
the same GCR read/write serialiser and head mechanics. On top of the 1541 it
adds, for C128/CP-M use:

- a **WD1770** MFM controller (we already have `western-digital-wd1770`),
- a **6526 CIA** for fast/burst serial (`mos-cia-6526`, as VICE's `cia1571d`),
- **glue** for mode (GCR ↔ MFM) and **side** select — VICE's `glue1571.c` is
  tiny: the only 1571-specific glue is `glue1571_side_set`, which picks which
  physical side the head reads. All the double-sided GCR logic is the shared
  drive core with a side parameter.

Our own roadmap (`2026-06-08-c64-100-percent-plan.md`) already frames the 1571
as "a drive-core in the 1541 mould."

## Reference material (surveyed, per "survey emulators before new system")

- **VICE** `emulators/c64/vice-3.10/src/drive/iec/`: `glue1571.c` (side-set),
  `cia1571d.c` (fast-serial CIA), `wd1770.c` (MFM), `fdd.c` (shared rotation).
  Shared drive core in `emulators/c64/vice-3.10/src/drive/`.
- **Our bases:** `machine-commodore-1541` (3075-line GCR core: 6502 + 2×VIA6522
  + GCR serialiser + density/head — the C64-mode heart), `machine-commodore-1581`
  (the WD1770 + CIA + IEC-glue shape I just fixed), `format-commodore-c64-d64`
  (the 1541 uses it — `parse_directory`/`read_sector`/`sectors_in_track` — to
  build its GCR/rotation layer).
- **ROM:** 1571 DOS `310654-05`, 32 KB, mapping `$8000-$FFFF`. Staged +
  verified: `~/.emu198x/roms/commodore-c64/1571.rom` extracted from TOSEC
  `1571 310654-05 (1985)(Commodore)[!]` — **byte-identical to VICE's bundled
  `dos1571-310654-05.bin`** (sha256 `1fd73459…fac81d45`).
- **Media:** real D71 games in TOSEC `Commodore/C128/Games/[D71]` (for a later
  catalogue entry).

## Memory map (standard 1571 — CONFIRM by tracing the ROM's own accesses)

Per the 1581/Sord-M5 lesson, donor/textbook maps can be wrong; verify each
region by tracing what the boot ROM reads/writes (VICE binary monitor
via `tools/vice-monitor.py`).

| Region | Device | Notes |
|--------|--------|-------|
| `$0000-$07FF` | RAM (2 KB) | zero-page + stack + buffers |
| `$1800-$180F` | VIA1 (6522) | IEC serial (ATN/CLK/DATA), like the 1541 |
| `$1C00-$1C0F` | VIA2 (6522) | head/motor/GCR byte-ready, like the 1541 |
| `$2000-$2007` | WD1770 | MFM FDC — **defer** (C128/CP-M) |
| `$4000-$400F` | CIA (6526) | fast/burst serial — **defer** (C128) |
| `$8000-$FFFF` | ROM (32 KB) | 1571 DOS 310654-05 |

For the C64-mode milestone only VIA1 + VIA2 + RAM + ROM are on the critical
path. The WD1770 and CIA must exist enough that the ROM's power-on init doesn't
hang on them (the 1581 stalled exactly this way), but need not do real work yet.

## D71 format

Double-sided D64: **70 tracks**, `349_696` bytes (`+1366` bytes with error
info = `351_062`). The per-track sector counts are the 35-track 1541 pattern
(21/19/18/17 zones) **repeated twice** — track 36 = 21 sectors (like track 1),
… track 70 = 17. Directory stays on track 18; a second-side BAM lives on track
53. File chains may cross the side boundary.

## Slices (mirroring the 1581 build)

1. **`format-commodore-c64-d71` crate** — mirror `format-commodore-c64-d64`
   with the 70-track geometry (duplicated sector counts, 349_696/351_062 sizes,
   track-18 directory, track-53 side-2 BAM). Decision-independent; on the LOAD
   path (the drive core reads sectors through it). ~500 lines, self-contained,
   unit-tested against a real D71's directory.

2. **`machine-commodore-1571` crate** — standalone, mirroring the proven 1541
   GCR/VIA core + **double-sided** (side-select bit → which head/side the GCR
   rotation reads) + a WD1770 and CIA present-but-idle so the ROM boots. Build
   to the same shape as `machine-commodore-1581` (CPU + peripherals + IEC glue +
   snapshot). Validate against the real ROM: boots to its idle loop (trace with
   `tools/vice-monitor.py` vs our drive), then `LOAD"*",8,1` reaches `LOADING`.

3. **Runtime integration** — the 1571 is a device-8 GCR drive (see decision 2).
   Load the 1571 DOS ROM instead of the 1541's; it becomes device 8, coexisting
   with the 1581 on device 9. Firmware key `commodore-1571-dos-rom`; autoload
   reuses the device-8 `LOAD"*",8,1` path. Re-verify no 1541/1581 regression.

4. **Catalogue** — a `pal-1571` variant + a D71 entry (mirror the D81 catalogue
   work), once LOAD works.

## Open decisions (recommended defaults in **bold**; confirm before slice 2)

1. **First milestone scope** — **C64-mode D71 LOAD** (defer MFM/CP-M/burst).
2. **Device assignment** — **1571 replaces the 1541 on device 8** when its ROM
   is loaded (real-hardware model; coexists with the 1581 on device 9).
3. **1541 code relationship** — **standalone `machine-commodore-1571` crate**
   mirroring the proven 1541 logic (like the 1581 was its own crate); leave the
   working 1541 untouched, factor a shared GCR core out later only if it pays
   off.

Slice 1 (D71 format crate) and the ROM staging are decision-independent and can
proceed now; slice 2 (the drive core) is the large, decision-dependent
investment and waits on confirmation.
