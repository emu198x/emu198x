---
title: "plan: Commodore VIC-20 to 100% — VIC-I video/audio depth, media loading, system breadth"
type: plan
date: 2026-06-09
system: docs/systems/commodore/vic-20.md
basis: code-grounded survey of the VIC-20 crates + chip crates (mos-vic-i, mos-via-6522) with live test runs, cross-checked against reference/by-system/commodore-vic20/, 2026-06-09
---

# Commodore VIC-20 — road to 100%

What it would take to bring the VIC-20 to feature- and accuracy-complete,
grounded in a code-level survey of the actual crates and tests (not doc prose).
Every claim below cites a file I read. Where the code contradicts the docs, it is
called out.

## Executive summary

**The VIC-20 is a "boots, but shallow" core: the system wiring is solid and the
long pole is the VIC-I chip itself.** Unlike the C64 (whose long pole is a
cycle-exact VIC-II rewrite to pass demoscene tricks) or the NES (done core, breadth
tail), the VIC-20's headline is that its single most important chip — the MOS
6560/6561 VIC-I, which does *both* video and audio — is implemented as a
**text-mode-only renderer with audio entirely stubbed**
(`crates/mos-vic-i/src/lib.rs:11` "audio is stubbed"; `:115-154` renders only the
hi-res text grid). The reference catalogues multicolour cells, 8×16 characters,
programmable columns/rows, a movable screen origin, reverse mode, and a live raster
register (`reference/by-system/commodore-vic20/vic20-reference.md:265-373`) — none
of which the chip implements.

What *is* done is genuinely done and at-or-near ceiling for its scope:

- **CPU (6502):** at the ceiling — shared `mos-6502` NMOS core, externally verified
  (see shared-chip findings). The machine resets it correctly
  (`crates/machine-commodore-vic-20/src/lib.rs:107-108`). **No CPU work.**
- **System wiring:** the memory map is correct VIC-20 (BASIC at `$C000-$DFFF`,
  KERNAL at `$E000-$FFFF`, BLK5 cartridge at `$A000-$BFFF` —
  `crates/machine-commodore-vic-20/src/lib.rs:253-266`), explicitly *unlike* the
  C64; two 6522 VIAs are wired (VIA #1 → NMI/RESTORE, VIA #2 → keyboard + 60 Hz
  IRQ, `:199-204`); the keyboard matrix, single DE-9 joystick (the awkward split
  across both VIAs), and RAM expansion (low + high blocks) all work and are tested
  (`:448-520`). It **boots to BASIC `READY`** per the gated smoke test
  (`tests/rom_boot.rs:51-66`).
- **VIA 6522:** mature and heavily tested (14/14 unit tests, full timer/SR/IRQ
  model — `crates/mos-via-6522/src/lib.rs:789-1046`); shared with the 1541.

So "100% VIC-20" is **a VIC-I video-mode + audio build-out (the long pole), plus
media loading (cartridges, PRG/TAP/D64) and the remaining system breadth.** The
machine plays nothing today beyond what you type at the BASIC prompt — there is no
cartridge path (`load_media` is a no-op,
`crates/runtime-commodore-vic-20/src/runtime.rs:303-307`; BLK5 always reads open
bus, `machine-commodore-vic-20/src/lib.rs:256`) and no format crate exists for the
platform.

**Totals (focused work):**

| Tier | Scope | Estimate |
|------|-------|----------|
| A — "Curriculum 100%" | VIC-I register-driven video (screen/colour base, columns/rows, origin, reverse, multicolour, 8×16), VIC-I audio (3 tone + noise + volume), cartridge (.crt/BLK loading), doc fixes | **~5–8 weeks** |
| B — Cycle-exact / faithful video | live raster register ($9004) + per-cycle VIC fetch timing, exact PAL/NTSC geometry & borders, mid-frame register changes, a VIC oracle harness | **~3–5 weeks** |
| C — Audio fidelity | faithful 6560/6561 oscillator pitch tables, noise LFSR, DC/mix behaviour to a reference, host audio routing | **~2–3 weeks** |
| D — Preservation breadth | PRG/TAP/T64/D64 + 1541 reuse, cassette LOAD/SAVE, IEC peripherals, RAM-expansion auto-config from cart/PRG, snapshot completeness | **~4–6 weeks** |

**True 100% of everything ≈ 14–22 weeks.** Like the C64 it is **not** front-loaded
onto cheap wins — Tier A (the VIC-I video/audio build-out) is the long pole and is
the bulk of the launch-relevant value, because without it the screen can only show
the default text grid and the machine is silent. The cheap part of the breadth
(PRG loading) is *partly already there* at the runtime layer (`autoload_prg`,
`runtime.rs:177-208`).

Effort key: **S** = hours · **M** = a few days · **L** = 1–2 weeks · **XL** = multi-week.

## Bucket 1 — VIC-I video accuracy (the long pole; L–XL)

The current renderer (`crates/mos-vic-i/src/lib.rs:115-154`) hardwires the display:
22 columns × 23 rows, screen RAM read through the machine's `addr & 0x0FFF` mirror
of main RAM (`machine-commodore-vic-20/src/lib.rs:168-181`), colour RAM hardwired
to `$9400`, a fixed border thickness, and only the plain hi-res text path. The
reference shows the registers that drive nearly all of this
(`reference/by-system/commodore-vic20/vic20-reference.md:265-373`).

| Item | Effort | Notes |
|------|--------|-------|
| **Register-driven screen + colour-RAM base** | **M** | `$9005` bits 4-7 + `$9002` bit 7 select the screen-matrix address; `$9002` bit 7 also selects colour-RAM base ($9400 vs $9600). Today `screen_base` is computed with hand-tuned shifts (`mos-vic-i/src/lib.rs:122-123`) but the machine then **ignores it** and reads through a fixed `$1000-$1FFF` mirror (`machine/src/lib.rs:170-173`); colour RAM is hardwired to `$9400` (`:174`). Programs that relocate the screen render garbage. |
| **Programmable columns / rows / char height** | **M** | `$9002` bits 0-6 = columns, `$9003` bits 4-6 = rows, `$9003` bit 0 = 8×8 vs 8×16. All hardwired to 22/23/8×8 (`mos-vic-i/src/lib.rs:115-119,125`). |
| **Screen origin ($9000/$9001)** | **S–M** | Horizontal origin (`$9000` bits 0-6) and vertical origin (`$9001`, in 2-line units) move the active region; demos scroll the whole display this way (`vic20-reference.md:261`). Currently the active region is fixed at `visible_y_start = 28` (`mos-vic-i/src/lib.rs:102`). |
| **Reverse mode ($900F bit 3)** | **S** | When bit 3 is 0 all characters display inverted (`vic20-reference.md:353`). Not modelled — `render` always treats bit=1 as foreground (`mos-vic-i/src/lib.rs:144-145`). |
| **Multicolour character mode** | **M** | Colour-RAM bit 3 set → bitmap bits read in pairs selecting bg / border / char / aux colour (`vic20-reference.md:355-368`). Not modelled at all. A large fraction of VIC-20 games are multicolour. |
| **Background / aux colour wiring** | **S** | Background is `$900F` bits 4-7 (handled, `mos-vic-i/src/lib.rs:136`); aux colour `$900E` bits 4-7 (for multicolour) is unread. |

## Bucket 2 — VIC-I timing / faithful frame (B; ~3–5 weeks)

| Item | Effort | Notes |
|------|--------|-------|
| **Live raster register ($9004 + $9003 bit 7)** | **M** | `$9004` should return the live TV raster ÷ 2; the VIC has no raster IRQ so beam-racing code polls it (`vic20-reference.md:138,275`). Today `read` returns the stored register byte (`mos-vic-i/src/lib.rs:68-71`) — reading `$9004` gives whatever was last written, not the beam position. |
| **Per-cycle VIC fetch + exact geometry** | **L** | The tick model advances `pixel_x`/`scanline` and renders a whole character column's 8 pixels in one tick (`mos-vic-i/src/lib.rs:87-154`) rather than streaming per dot. Exact PAL (312×71) vs NTSC (261×65) borders and active placement need grounding against VICE; the current border thickness is an admitted approximation (`mos-vic-i/src/lib.rs:17-25`). |
| **Mid-frame register changes** | **S–M** | Border colour is latched once per frame (`mos-vic-i/src/lib.rs:108-113` — "Mid-frame border-colour changes affect the next frame — v1 simplification"); raster splits need per-line evaluation. |
| **VIC-I oracle harness** | **M** | A per-frame/per-line comparator against VICE so the video build-out is provable. Build alongside the rewrite, not after. There are **zero** tests in `mos-vic-i` today (`cargo test -p mos-vic-i` → 0 tests). |

## Bucket 3 — VIC-I audio (C; ~2–3 weeks)

The VIC chip's audio is **entirely stubbed** — the runtime emits empty audio
packets every frame (`runtime.rs:342-348` "VIC audio not yet routed") and the chip
has no oscillator state (`mos-vic-i/src/lib.rs` has only `regs`, no audio fields).

| Item | Effort | Notes |
|------|--------|-------|
| **3 tone generators ($900A-$900C)** | **M** | Three square-wave voices at different octaves, each a 7-bit frequency register (`vic20-reference.md:265-286` register map). The core of VIC-20 sound. |
| **Noise generator ($900D) + master volume ($900E bits 0-3)** | **M** | LFSR noise channel + 4-bit master volume gating the mix (`vic20-reference.md:285`). |
| **Host audio routing** | **S** | Wire the VIC's sample output into the runtime's `AudioPacket` instead of the empty slice (`runtime.rs:343-348`). |
| **Pitch-table / mix fidelity to a reference** | **S–M** | Ground the oscillator divider math + mixing against VICE so it sounds right, not just present. |

## Bucket 4 — Media + preservation breadth (D; ~4–6 weeks)

| Item | Effort | Notes |
|------|--------|-------|
| **Cartridge (.crt / raw BLK) loading** | **M** | The single biggest "runs real software" gap: BLK5 (`$A000-$BFFF`) always reads open bus (`machine/src/lib.rs:256`) and `load_media` is a no-op (`runtime.rs:303-307`). Need a format crate (none exists — no `format-commodore-vic-20-*` in the workspace) parsing the standard VIC-20 cart layout (BLK1/2/3/5 + autostart `$A000` `CBM` signature, `vic20-reference.md:142,170`) and a machine API to map cart ROM into the right blocks. **Tier-A priority** — without it cart-only games don't run at all. |
| **PRG load completion** | **S** | `autoload_prg` exists at the runtime (`runtime.rs:177-208`) and the CLI exposes `--prg`/`--prg-sys` (`script.rs:128-129,224-233`), but `load_media` ignores a real `MediaSet`. Wire `.prg` through the standard media path + auto-pick RAM expansion to match the load address (the `$1201`/+8K coupling the code already documents at `runtime.rs:170-172`). |
| **TAP / T64 cassette** | **M** | No tape path; `media_slots` declares only a Cartridge slot (`profiles.rs:70-76`). Datasette LOAD (and later SAVE) via the C64's TAP/T64 format crates (`format-commodore-c64-{tap,t64}` exist) adapted for VIC-20 KERNAL timing. |
| **D64 + 1541 reuse** | **M–L** | Reuse the real `machine-commodore-1541` over IEC for disk software; needs the VIC-20's serial IEC lines wired to the VIA (the C64 already drives a 1541). |
| **Cassette / IEC actually wired** | **M** | The machine notes cassette + IEC bits live on the VIA ports (`machine/src/lib.rs:65-79,189-192`) but nothing drives them. |
| **Snapshot completeness** | **S–M** | Snapshot exists (`runtime.rs:354-360`, `snapshot.rs`); confirm VIA + VIC + RAM-expansion state all round-trip once audio/video state grows. NEEDS RUNTIME VERIFICATION. |

## Done as part of this plan (free, ~half a day)

Doc-drift corrected:

1. **Stale module doc in the machine crate.** `machine-commodore-vic-20/src/lib.rs:22`
   still says "8 KB BASIC at `$A000`" in the header comment — directly
   contradicting the actual map (`:257` BASIC at `$C000-$DFFF`), the reference
   (`vic20-reference.md:90,171`), and the in-file comment at `:253-255` that
   *explicitly* corrects the C64-mirror bug. The header should say BASIC at `$C000`.
2. **outstanding-work.md overstates the `.prg` gap.** It lists "A — `.prg` / `.tap`
   load not implemented" (`docs/status/outstanding-work.md:631`), but PRG autoload
   is implemented at the runtime + CLI (`runtime.rs:177-208`, `script.rs:224-233`).
   The accurate statement is: PRG injects via a side channel; `load_media` and the
   standard media path are not wired, and cart/tape have no path at all.
3. **Audio gap is understated as one line.** It is the whole VIC-I audio subsystem
   (3 tone + noise + volume), not a single item — see Bucket 3.

## Recommended sequence (highest leverage first)

1. **Cartridge (.crt/BLK) loading** (M) — the one Tier-A gap that stops real games
   running at all; highest leverage per week. Build the format crate + BLK mapping
   + autostart detection.
2. **Register-driven screen/colour base + columns/rows/origin + reverse** (M+M+S)
   — make the existing text renderer obey the VIC registers so relocated-screen and
   custom-geometry programs render correctly.
3. **Multicolour character mode** (M) — unlocks the large multicolour-games slice.
4. **VIC-I audio: 3 tone + noise + volume + host routing** (M+M+S) — the machine is
   silent today; this is the single biggest "feels alive" win.
5. **PRG via standard media path + RAM-expansion auto-select** (S) — cheap, the
   hard part is already written.
6. **VIC-I oracle harness** (M) — build the comparator *before* the timing work.
7. **Live raster register + per-cycle fetch + exact geometry/borders** (M+L) — the
   beam-racing / faithful-frame core-accuracy items.
8. **TAP/T64 cassette + D64/1541 reuse + IEC wiring** (M+M-L+M) — preservation
   breadth.
9. **Mid-frame register changes, audio pitch/mix fidelity, snapshot completeness**
   (S–M each) — completionist polish.

## Key files

- CPU (at ceiling): shared `crates/mos-6502/` (see shared-chip findings); reset wired at `crates/machine-commodore-vic-20/src/lib.rs:107-108`.
- VIC-I (the long pole — video + audio): `crates/mos-vic-i/src/lib.rs` (`tick` `:81-157`, `read`/`write` `:67-77`, palette `:198-215`; the hardwired geometry at `:102,115-125`, no audio, no tests).
- Machine wiring: `crates/machine-commodore-vic-20/src/lib.rs` (memory map `:223-292`, VIC callbacks/screen-RAM mirror `:162-181`, VIA → IRQ/NMI `:199-204`, joystick split `:138-148`, BLK5 open bus `:256`, stale header `:22`).
- VIA 6522 (mature, shared): `crates/mos-via-6522/src/lib.rs` (timers `:196-228`, SR `:246-322`, 14 tests `:789-1046`).
- Runtime: `crates/runtime-commodore-vic-20/src/runtime.rs` (`load_media` no-op `:303-307`, `autoload_prg` `:177-208`, empty audio `:342-348`), `src/profiles.rs` (cartridge-only media slot `:70-76`), `src/input.rs`.
- CLI / scripting: `crates/emu198x-commodore-vic-20/src/script.rs` (`--prg`/`--prg-sys` `:128-129,224-233`).
- Tests: `crates/machine-commodore-vic-20/tests/{rom_boot,keyboard_type,joystick_probe}.rs` (all `#[ignore]`, ROM-gated); 11 machine unit tests + 14 VIA + 3 runtime pass; **0** `mos-vic-i` tests.
- Reference: `reference/by-system/commodore-vic20/vic20-reference.md` (register map `:265-373`, memory map `:90,162-194`), `vic-20-programmers-reference-guide-1st-edition-6th-printing.md`; `emulators/vic20/VIC20_MiSTer/` + `emulators/c64/vice-3.10/` (VICE covers VIC-20).
- Status: `docs/status/outstanding-work.md:582-634`, `docs/status/current-system-usability.md:76`, `docs/status/drivability-assessment.md:351-356`.

