> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.

---
title: "plan: ColecoVision to 100% — megacart breadth, live snapshot, shared-chip accuracy, peripherals"
type: plan
date: 2026-06-09
system: docs/systems/coleco/colecovision.md
basis: code-grounded survey of machine-coleco-colecovision, runtime-coleco-colecovision, emu198x-colecovision + the three shared-chip assessments (Z80 at-ceiling, TMS9918 partial, SN76489 partial), with live test runs, 2026-06-09
---

# ColecoVision — road to 100%

What it would take to bring the ColecoVision to feature- and accuracy-complete,
grounded in a code-level survey of the actual crates and tests. The machine
wiring is a clean, recent (2026-06-01) donor extraction; the chip cores are
shared and assessed separately. This plan covers the **system-specific** work and
points at the shared-chip work that backs it.

## Executive summary

**The ColecoVision is the simplest system shape in the fleet, and it is almost
all done — what remains is one breadth gap, one depth gap, and the shared-chip
accuracy work it inherits.** The machine is a thin, correct wiring of three
chips: a Z80 that is genuinely at the accuracy ceiling, plus a TMS9918A VDP and
an SN76489 PSG that are competent-but-partial shared cores. There is no
system-specific CPU work, no exotic bus, no disk subsystem — the CV is a
cartridge console with 1 KB of RAM and a BIOS.

The machine boots the real 1982 BIOS to its title screen (verified by the gated
`bios_boot.rs` smoke), runs the 3:2 VDP-to-CPU phase clock correctly
(`machine-coleco-colecovision/src/lib.rs:55-66`, tested at `lib.rs:488-514`),
routes the full I/O map, multiplexes the keypad/joystick controllers, and drives
the Z80 IRQ from the VDP VBlank. All 8 in-crate machine tests pass; all 2 runtime
tests pass.

**The long pole is not in the machine crate — it is the shared TMS9918A
accuracy backlog** (sprite collision defect, mid-frame backdrop, per-line sprite
re-evaluation, no primary-source distillation), which the CV inherits along with
six other systems. After that, the two genuinely system-specific gaps are
**megacart bankswitching** (the one breadth gap that stops larger carts running
at all) and a **live snapshot** (today's snapshot is bootstrap-only — it
re-boots from BIOS+cart on restore, losing all live state).

The CPU is at the ceiling — Tom Harte 1,604,000/1,604,000, FUSE 1351/1356 exact
— so unlike most systems there is **no CPU work** on the road to 100%, only the
latent IM0 enhancement that no shipped CV depends on (the BIOS uses IM 1).

**Totals (focused work):**

| Tier | Scope | Estimate |
|------|-------|----------|
| A — "Curriculum 100%" | megacart bankswitching, live snapshot, region CLI/region-from-image verification, doc-drift fixes | **~1.5–2.5 weeks** |
| B — Shared-chip accuracy (inherited) | TMS9918A sprite-collision fix + mid-frame backdrop + per-line sprite re-eval; SN76489 N=0 clamp; chip distillation docs | **~2–4 weeks (shared across 7 systems)** |
| C — Peripherals breadth | Super Action Controller (spinner/roller + extra keypad), Roller Controller, paddle/pot seam | **~1.5–2.5 weeks** |
| D — Preservation breadth | Expansion Module #1 (Atari 2600 adapter) + #2 (Turbo driving), cartridge-image format validation, additional megacart mappers | **~3–5 weeks** |

**True 100% of everything ≈ 8–14 weeks**, but most of Tier B is shared work that
benefits seven systems at once, and the launch-relevant slice (Tier A + the
audible/visible parts of B) is a much smaller ~3–4 weeks. The CV is **front-loaded
onto cheap wins**: the machine itself is nearly complete, and the expensive items
(Tier B chip accuracy, Tier D expansion modules) are either shared or
preservation-tail.

Effort key: **S** = hours · **M** = a few days · **L** = 1–2 weeks · **XL** = multi-week.

## Tier A — Curriculum 100% (system-specific, do first)

| Item | Effort | Notes |
|------|--------|-------|
| **Megacart bankswitching** | **M** | The cartridge is mapped **flat** — `mem_read` slices `cart_rom[addr-0x8000]` for `$8000-$FFFF` with no bank register (`machine-coleco-colecovision/src/lib.rs:317-324`), and ROM writes are silently dropped (`mem_write` at `lib.rs:328-332` only accepts `$6000-$7FFF`). Carts larger than 32 KB cannot run at all. ColecoVision megacarts page a 16 KB window at `$C000-$FFFF` via reads to `$FFC0-$FFFF` (the bank select is address-decoded, fixed first 16 KB at `$8000-$BFFF`). Add a cart-size-driven mapper: ≤32 KB stays flat; >32 KB uses the megacart window. The one Tier-A breadth gap that stops real (larger homebrew + a few original) carts running. |
| **Live snapshot** | **M** | The snapshot is **bootstrap-only**: `CvRuntimeSnapshotV1` stores only `time`, `model_id`, `bios_bytes`, `cart_bytes` (`runtime-coleco-colecovision/src/snapshot.rs:14-21`), and `decode` calls `rebuild_after_restore()` → `rebuild_machine()` which constructs a **fresh** `ColecoVision` (`runtime.rs:140-158`). Restore re-boots from BIOS+cart and loses all live state (CPU registers, 1 KB RAM, 16 KB VRAM, VDP registers/beam, PSG channels, controller state). Self-admitted at `snapshot.rs:4-5` ("A full live snapshot lands once machine-coleco-colecovision grows a deep snapshot") and `outstanding-work.md` "A — Snapshot story". Add `save_state`/`load_state` on `ColecoVision` (the chip crates already have save/load state per the shared-chip findings) and serialise machine RAM + controller + clock fields. Rewind/save-state UX is broken until this lands. |
| **Region selection verification** | **S** | NTSC/PAL is selectable via two profiles (`profiles.rs:10-15`, `Model::CvNtsc`/`CvPal`) and the machine honours the region in frame budget and VDP region (`lib.rs:197-205`). Verify the headless binary actually exposes region choice on the CLI and that nothing hardwires NTSC; the `emu198x-colecovision` main wiring was not confirmed to surface `--region`/`--pal`. **needs-runtime-verification.** |
| **Doc-drift fix** | **S** | `outstanding-work.md` item "A — Initial-port clock ratios" is **stale**: it claims the VDP runs "3 dots per CPU cycle … the actual ratio is 1.5 dots per CPU cycle, not 3" and says "real-time speed is off." The code already fixed this — `lib.rs:55-66` implements the 3:2 phase clock (3 dots per 2 T-states), tested at `lib.rs:488-514` (`vdp_runs_at_three_dots_per_two_tstates`, `one_frame_of_tstates_is_exactly_one_vdp_frame`). The module doc at `lib.rs:42-49` documents the fix. Strike the stale item. (The "scanline-batched render" item is **also partly superseded** — the VDP now ticks per-dot per the shared-chip findings; re-scope it to the genuine per-line-sprite-eval gap below.) |

## Tier B — Shared-chip accuracy (inherited; see shared-chip findings)

These items are **not** re-derived here — they come from the established
TMS9918A, SN76489, and Z80 assessments. They are listed because the CV is a
consumer and they are the dominant accuracy spend, but the fix lands in the chip
crate and benefits all consumers. **Do not file CV-specific chip issues for these.**

| Item | Effort | Notes |
|------|--------|-------|
| **TMS9918A sprite-collision fix** (priority chip defect) | inherited | Coincidence flag ignores transparent (colour-0) sprites, contradicting its own comment (`ti-tms9918/src/lib.rs:782-809`). Affects collision-based CV game logic. The clearest confirmed chip defect. |
| **TMS9918A mid-frame backdrop (VR7)** | inherited | Border backdrop painted once per frame (`ti-tms9918/src/lib.rs:465-468`); raster-bar/split border effects render one frame late. |
| **TMS9918A per-line sprite re-evaluation** | inherited | Sprites evaluated once at dot 0 (`ti-tms9918/src/lib.rs:338-340`); mid-line VR1 / sprite-table writes not reflected. This subsumes the old `outstanding-work.md` "scanline-batched render" CV note. |
| **SN76489 N=0 period clamp** | inherited | `tick()` reloads tone counter with period 0 instead of clamping to 1 (`ti-sn76489/src/lib.rs:189-194`), doubling frequency; the CV hardwires `Sn76489::new(3_579_545)` (`machine-coleco-colecovision/src/lib.rs:209`) and is affected. |
| **TMS9918A primary-source distillation** | inherited | No `knowledge/chips/` doc and no `reference/by-topic/` distillation for the TMS9918 backing 7 systems (only a thin segaretro web mirror). CV accuracy work is unanchored until this exists. |

## Tier C — Peripherals breadth

| Item | Effort | Notes |
|------|--------|-------|
| **Super Action Controller** | **M** | The CV's flagship controller: the standard joystick + keypad plus a thumb-wheel **spinner** and four action buttons. Only the base joystick + 12-key keypad + two fire buttons are modelled (`CvController` at `machine-coleco-colecovision/src/lib.rs:118-165`, input mapping at `runtime-coleco-colecovision/src/input.rs`). The spinner uses a quadrature count read through the controller port; no pot/spinner seam exists. Sports/action titles want it. |
| **Roller Controller (trackball)** | **M** | Centipede/Slither-class titles. A trackball quadrature counter on the controller ports; shares the spinner-read mechanism with the Super Action Controller. |
| **Paddle / pot seam** | **S–M** | `drivability-assessment.md:116,241` flags ColecoVision as **absent** for paddle/analogue ("no pot seam modelled yet"). The driving-wheel and spinner inputs need an analogue read path the machine does not have today. |

## Tier D — Preservation breadth

| Item | Effort | Notes |
|------|--------|-------|
| **Expansion Module #1 (Atari 2600 adapter)** | **L–XL** | Plugs into the CV expansion connector to run Atari 2600 carts (a full 2600 personality — TIA + RIOT + 6507). The expansion region `$2000-$5FFF` currently returns `0xFF` (`machine-coleco-colecovision/src/lib.rs:315`). A 2600 core exists in the fleet (atari-2600), so this is integration, not a new core — but it is the heaviest single CV item. Pure preservation. |
| **Expansion Module #2 (Turbo driving module)** | **M** | Steering wheel + pedal for Turbo. Needs the analogue/pot seam from Tier C plus the module's port decode. |
| **Cartridge image validation** | **S** | Cartridges are loaded as raw bytes with no header/format check (`insert_cartridge` at `runtime.rs:93-97`, `load_media` at `runtime.rs:189-206`). ColecoVision carts carry an 8-byte header at the cart base (magic `$55 $AA` or `$AA $55` selecting whether the BIOS shows its logo) — validate and surface a bad-image error rather than silently running garbage. Low risk; improves the load path. |
| **Additional megacart mappers** | **S–M** | Once the common `$FFC0` megacart mapper (Tier A) lands, the handful of variant homebrew mappers (e.g. linear 64K, SGM-adjacent) round out the long tail. The **Super Game Module** (extra 24 KB RAM at `$2000-$7FFF` + an AY-3-8910) is a larger preservation item if homebrew coverage is wanted — flagged as **idea**, not committed. |

## Done as part of this plan (free, ~half a day)

Doc-drift correction in `docs/status/outstanding-work.md` § ColecoVision: the
"Initial-port clock ratios" item is stale (the 3:2 phase clock is implemented and
tested — `lib.rs:55-66`, `lib.rs:488-514`); the "scanline-batched render" item is
superseded by the per-dot VDP tick and should be re-pointed at the genuine
per-line sprite-evaluation gap (a shared-chip item). The SN76489 module-doc
pointer to "outstanding-work.md § ColecoVision for the accuracy backlog"
(`ti-sn76489/src/lib.rs:6-8`) points at a section that enumerates only VDP/clock
work and no PSG gaps — note the mismatch.

## Recommended sequence (highest leverage first)

1. **Megacart bankswitching** (M) — the one Tier-A breadth gap that stops larger
   carts running at all. Highest leverage per week.
2. **Doc-drift fix + cartridge validation** (S + S) — cheap correctness and a
   cleaner load path; clears stale status before deeper work.
3. **Live snapshot** (M) — fixes save-state/rewind, which is silently broken.
4. **TMS9918A sprite-collision fix** (inherited, chip-level) — the priority
   shared-chip defect; CV collision-based games benefit. Land in the chip crate.
5. **SN76489 N=0 clamp + TMS9918A mid-frame backdrop + per-line sprite eval**
   (inherited) — the rest of the audible/visible chip accuracy.
6. **Super Action Controller + Roller Controller + pot seam** (M + M + S–M) —
   the peripheral set that the CV library leans on.
7. **Expansion Module #2 + image validation** (M + S) — preservation mid-tail.
8. **Expansion Module #1 (2600 adapter)** (L–XL) and **Super Game Module**
   (idea) — the completionist long tail.

## Key files

- CPU (already at ceiling): `crates/zilog-z80/` (Tom Harte 1,604,000/1,604,000, FUSE 1351/1356 exact; latent IM0 at `crates/zilog-z80/src/z80.rs:977-981`).
- Machine wiring: `crates/machine-coleco-colecovision/src/lib.rs` — memory map (`mem_read` `:305-326`, `mem_write` `:328-332`, **flat cart at `:317-324`**), I/O map (`io_read` `:334-358`, `io_write` `:360-370`), 3:2 phase clock (`:55-66`, `tick_cpu_cycle` `:236-259`), IntAck IM1 (`:295-300`), controllers (`CvController` `:118-165`).
- Runtime: `crates/runtime-coleco-colecovision/src/runtime.rs` (`rebuild_machine` `:140-158`, `insert_cartridge` `:93-97`, `load_media` `:189-206`), `snapshot.rs` (**bootstrap-only** `:14-21,36-51`), `profiles.rs` (NTSC/PAL models `:10-50`), `input.rs` (base controller mapping).
- Headless binary: `crates/emu198x-colecovision/src/{main.rs,script.rs,mcp.rs,mcp_tools.rs}` (region-flag exposure unverified).
- Tests: `crates/machine-coleco-colecovision/src/lib.rs:458-563` (8 unit tests, all pass), `crates/machine-coleco-colecovision/tests/bios_boot.rs` (gated `#[ignore]`, needs real BIOS), `crates/runtime-coleco-colecovision/src/profiles.rs:93-117` (2 tests).
- Shared chips: `crates/ti-tms9918/src/lib.rs`, `crates/ti-sn76489/src/lib.rs` (see shared-chip findings — fixes land here, benefit 7 / 6 systems).
- Reference: `reference/by-topic/psg-sn76489/psg-sn76489-reference.md` (SN76489 primary). **No TMS9918 primary distillation** — only `reference/assets/web-mirrors/segaretro.org/TMS9918.html` and smspower mirrors; no `knowledge/chips/` TMS9918 doc.

