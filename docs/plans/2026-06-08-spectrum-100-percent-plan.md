> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.

---
title: "plan: ZX Spectrum line to 100% — accuracy, storage, clones, peripherals"
type: plan
date: 2026-06-08
system: docs/systems/sinclair/zx-spectrum/index.md
basis: four code-grounded assessments (CPU/timing, storage, peripherals, variants), 2026-06-08
---

# ZX Spectrum — road to 100%

What it would take to bring the **whole Spectrum line** (13 variants) to feature-
and accuracy-complete, grounded in a code-level survey of the actual crates and
tests (not doc prose — the docs had drifted; corrected in the system doc as part
of this work).

## Executive summary

The **hard part is done.** The mainstream Sinclair/Amstrad models (48K, 16K, +,
128K, +2, +2A, +2B, +3) are effectively 100% on **CPU, timing, video, audio**:
Z80 Tom Harte 100%, Patrik Rak `z80test` 6/6 *zero-allowlist*, ZEXDOC/ZEXALL, the
project's reference contention model (the +2A/+3 40078 is separately modelled,
contrary to old docs). "100%" is therefore not about the core — it is **four
buckets**: storage write + formats, the clones, the peripheral catalogue, and a
few accuracy edges.

Almost everything below is **porting a proven pattern**, not inventing one — the
`western-digital-wd1770` write+dirty+flush model, the C64 `disk-save-write-back`
writable-mount/flush decision, the `Peripheral` trait, and the Beta TR-DOS
ROM-paging precedent. That de-risks the estimate.

**Totals (focused work):**

| Tier | Scope | Estimate |
|------|-------|----------|
| A — "Curriculum 100%" | mainstream line genuinely complete: disk write, key formats, cheap peripherals, accuracy edges, doc fixes | **~6–8 weeks** |
| B — Full-family first-class | + Scorpion screen, Timex extended video, Pentagon TRD | **+3–4 weeks** |
| C — Completionist peripherals | + Multiface, Interface 1 + Microdrive | **+3–4 weeks** |
| D — Net | + Spectranet (the cross-platform netplay target) | **+3–4 weeks** |

**True 100% of everything ≈ 15–20 weeks**, heavily front-loaded — the launch-
relevant bulk (Tier A) lands first.

Effort key: **S** = hours · **M** = a few days · **L** = 1–2 weeks · **XL** = multi-week.

## Bucket 1 — Accuracy edges (the core is ~98%)

| Item | Effort | Notes |
|------|--------|-------|
| ULA smoke → byte-equal vs Spectron | M | The one item that could expose a hidden timing bug. Needs a downscale-and-crop comparator (Spectron renders 1224×968). Contention logic itself is settled (passes `z80test`). |
| +2A/+3 video/INT config split + verify | S | Contention pattern already separate (`DELAY_TABLE_PLUS2A`); only `CONFIG_PLUS2A = CONFIG_128K` aliases the video geometry. Verify vs primary 40078 timing; likely a no-op. |
| Snow effect | M | Genuinely absent (the 128K ULA address-corruption quirk). Touches the ULA fetch-address path. Niche — few titles depend on it. |
| 5 FUSE block-I/O flag bits (`INIR`/`OTIR`/`INDR`/`OTDR`) | accept | Undocumented X/Y bits at the final repeat; real silicon varies. Effectively unclosable, zero practical impact. Leave allowlisted. |

## Bucket 2 — Storage (the largest gap; ~4–6 weeks)

Cross-cutting prerequisite: thread the **writable-mount flag + flush** through
`load_media` and add `save_disk` tools — the model is already decided and
implemented for the C64 (`knowledge/decisions/disk-save-write-back.md`). **Carry
its gotcha**: report WRPROT from the disk's protect state and make an *empty
drive read NOT-protected*, or you reproduce the phantom-disk-change bug the C64
path already hit and fixed.

| Item | Effort | Notes |
|------|--------|-------|
| **+3 µPD765A WRITE** — `WriteData` + `FormatTrack` execution, EDSK *writer*, writable mount/flush | **L** | The biggest single item. Buffer plumbing (`exec_buf`) exists; nothing fills it for a write or flushes to the `DiskImage`. `FormatTrack` (rebuild a track's sectors) + EDSK serialization are the new/fiddly parts. Also set ST3 bit 6 (WRPROT), currently never reported. |
| **Beta WD1793 WRITE** + TRD write-back | M | Near-direct port of the `western-digital-wd1770` write model; TRD is a flat `Vec<u8>`, so flush is a byte copy. Smaller than +3 (no FormatTrack-rebuild). |
| **Pentagon/Scorpion TRD LOAD wiring** + `.trd` parser | M | Controller + `insert_disk` exist and are wired, but no `MediaKind::Disk` route calls them and there's no `.trd` format crate. Most clone software is TRD — blocks real clone usage. |
| **SZX** snapshot parser | M | The modern de-facto snapshot standard; extension is allowlisted but has no parser behind it. Chunked; per-chunk state already has homes. |
| **CSW** / **PZX** tape, **SCL** Beta | S–M each | CSW v1 is RLE of pulse lengths feeding the existing player; v2 adds zlib. PZX block-structured (like TZX). SCL expands into the existing TRD layout. |
| **128K-family tape auto-LOAD** | S | `autoload_basic_tape` is coupled to the 48K K-cursor editor; the 128K family boots to a menu. A 128-menu detector already exists — branch autoload on variant. Cheapest high-leverage win: unblocks autoload across the entire 128K family. |

## Bucket 3 — Clones (~2–3 weeks)

| Item | Effort | Notes |
|------|--------|-------|
| **Scorpion ZS-256 screen** | M | 3 coupled memory-map bugs vs FUSE (`memory.rs`): `$1FFD` page-select bit (uses bit 0, FUSE uses bit 4), ROM-select logic, ROM 3 / Beta overlay. Coupled — applying one alone regressed boot. Also confirm which ROM distribution the local files match. |
| **Timex extended SCLD video** (modes 1–7) + TS2068/TC2068 boot-to-menu + NTSC timing | **L** | The Timex headline feature is entirely unrendered (`timex-scld` does mode 0 only). TS2068/TC2068 don't reach the boot menu (golden honestly locks a stripe state). TS2068 `frame_timing()` returns PAL despite being NTSC. |
| **128K-family BASIC-authoring helpers** | M | `autoload_basic_tape`/`load_basic_program` are 48K-only — coupled to the 48K editor model, not just the slot. Generalising means per-family menu nav + the 128K editor's keyword model. (Overlaps the 128K auto-LOAD item in Bucket 2.) |

## Bucket 4 — Peripherals (~1 week cheap set + multi-week tentpoles)

The `Peripheral` trait makes the *port* side of every one a paved road; the
recurring real cost is the **memory-bus ROM/RAM intercept** (IF1/IF2/Multiface).
If 3+ of these get built, generalise that intercept hook first.

| Item | Effort | Tier |
|------|--------|------|
| Kempston mouse | S | clone the joystick crate's shape |
| ZX Printer (`$FB`) | S | `tick` hook already exists for bit-serial timing |
| ULAplus (`$BF3B`/`$FF3B`) | S–M | two ports (select + data); the cost is threading a 64-entry palette override into the renderer |
| Interface 2 (16K cart ROM + 2nd joystick) | M | reuses the Beta ROM-paging pattern; raw 16K `.rom`, no new format |
| Multiface 128/+3 | M–L | NMI freeze + bank-over-RAM intercept + paging-register shadow; snapshot save already exists |
| **Interface 1 + Microdrive** | **XL** | the iconic tentpole: shadow-ROM paging + microdrive loop model + **new MDR format crate** + RS-232 + ZX Net. 1.5–3 weeks. |

Ranks 1–4 ≈ ~1 week and close most of the *visible* peripheral gap; #5 ≈ ~1 week;
#6 is the multi-week tentpole.

## Bucket 5 — Net (separable; for the netplay vision)

| Item | Effort | Notes |
|------|--------|-------|
| **Spectranet** (Ethernet, W5100 + paged flash/RAM) | XL | Fully greenfield, but the *right* target for `project_rachel_cross_platform_netplay`: a real TCP/IP peripheral the emulator bridges to host sockets. |
| Interface 1 ZX Net | M atop IF1 | period 2-wire LAN; only useful between emulated Spectrums — low value for cross-platform netplay. |

## Done as part of this plan (free, ~1 day)

Doc-drift eradicated in the system doc + status doc: snow was claimed implemented
(it isn't), +3 contention was claimed not-modelled (it is — `amstrad-ula-40077`),
the CRT shader was claimed not-done (it is — CRT-Lottes in `emu198x-native-video`),
the 105-test table was stale (~636 real tests). The FUSE block-I/O label
`CPDR`→`INDR` was corrected in `docs/status/outstanding-work.md`.

## Recommended sequence (highest leverage first)

1. **128K-family tape auto-LOAD** (S) — unblocks the bulk of the software library
   across the entire 128K family for hours of work.
2. **Beta WD1793 write + Pentagon TRD load wiring + `.trd` parser** (M+M+M) —
   makes the Russian clones genuinely usable; rides the WD1770 write model.
3. **Cheap peripherals** — Kempston mouse, ZX Printer, ULAplus, Interface 2 (~1 wk).
4. **+3 disk WRITE** (L) — the mainstream-line tentpole; FDC write + EDSK writer +
   writable mount/flush.
5. **SZX + CSW** formats (M + S–M) — the formats users most expect.
6. **ULA smoke strictness** (M) — the one accuracy item that could find a real bug.
7. **Scorpion screen** (M) then **Timex extended video** (L) — finish the clones.
8. **Multiface** (M–L), then **Interface 1 + Microdrive** (XL) — completionist.
9. **Spectranet** (XL) — pursue for the netplay goal, not period completeness.

## Key files

- Core/timing: `crates/zilog-z80/tests/z80_fuse.rs`, `crates/common-sinclair-zx-spectrum/src/ula_engine.rs`, `crates/amstrad-ula-40077/src/lib.rs`, `crates/machine-sinclair-zx-spectrum-{48k,128k}/tests/{float_bus,tape_smoke}.rs`.
- Storage: `crates/nec-upd765a/src/lib.rs`, `crates/beta-disk-interface/src/lib.rs`, `crates/western-digital-wd1770/src/lib.rs` (write-model template), `crates/format-amstrad-dsk/src/lib.rs`, `crates/runtime-sinclair-zx-spectrum/src/{variants.rs,autoload.rs}`, `knowledge/decisions/disk-save-write-back.md`.
- Clones: `crates/machine-scorpion-zs256/src/memory.rs`, `crates/timex-scld/src/lib.rs`, `crates/machine-timex-ts2068/tests/golden.rs`.
- Peripherals: `crates/common-sinclair-zx-spectrum/src/peripheral.rs`, `crates/machine-pentagon-128/src/lib.rs` (ROM-paging reference wiring).
