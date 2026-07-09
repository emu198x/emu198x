> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.

---
title: "plan: Acorn BBC Micro Model B to 100% — video accuracy, disk preservation, system audio fidelity"
type: plan
date: 2026-06-09
system: docs/systems/acorn/bbc-micro.md
basis: code-grounded survey of the machine/runtime/emu crates, the motorola-6845 + ti-sn76489 + mos-via-6522 chip crates, and the shared 6502/6845/SN76489 chip findings, with live test runs, 2026-06-09
---

# Acorn BBC Micro Model B — road to 100%

What it would take to bring the BBC Micro Model B to feature- and accuracy-complete,
grounded in a code-level survey of the actual crates and tests. The CPU is shared
and at ceiling; the chip-level debt (6845, SN76489) is covered by the shared-chip
findings and not re-derived here. This plan is the BBC-specific forward view:
machine wiring, the video path, storage/media breadth, and the system-doc drift.

## Executive summary

**The BBC Micro is a different shape again: it boots, but its video path routes
around its own CRTC, and it has no storage at all.** The headline is that the
machine *renders MODE 7 (teletext) properly* — that is the one screen path with a
real model (`render_teletext_scanline`, a from-scratch SAA5050) — while **MODE 0–6
(every bitmap mode) is geometry-derived, not CRTC-driven**, and **there is no disk,
no tape, no serial, no Econet** in the machine at all. So the long pole is twofold:
a **real 6845-driven video address path** for bitmap modes, and a **storage stack
from zero** (no 8271/WD1770 FDC, no DFS, no `.SSD/.DSD` format) for the disk-loaded
library that defines the platform.

The CPU is shared and already at the ceiling (`mos-6502`, NMOS variant, Tom Harte
2.56M PASS per the shared finding) — so there is **no CPU work** on the road to
100%, with one verification caveat: the BBC drives both VIAs' interrupts heavily,
and the IRQ/NMI edge-case ROM-level cross-check for non-NES 6502 systems is flagged
needs-verification in the shared finding.

What is genuinely done: boot to the BASIC banner in MODE 7 (the SAA5050 model + the
keyboard→VIA-PA7 fix landed it, 2026-06-04), the 1 MHz/2 MHz bus-contention clock
model (`access_master_ticks`, contradicting the stale "flat 2 MHz" doc note — see
Done section), the μPD7002 analogue-joystick ADC with EOC→VIA-CB1 interrupt, the
IC32-latch→SN76489 PSG write path, the sideways-ROM bank register, and the
operational-parity surfaces (Capture + Script + MCP).

A learner cannot yet ship a typical BBC game: most shipped on disk, and the disk
stack is absent; bitmap-mode games will mis-render under any non-default screen
geometry or hardware scroll; and the noise channel is detuned (wrong LFSR width).

**Totals (focused work):**

| Tier | Scope | Estimate |
|------|-------|----------|
| A — "Curriculum 100%" | BBC SN76489 LFSR fix, real 6845-driven bitmap video (MODE 0–6 + hardware scroll + screen-wrap), interactive-prompt confirmation, system-doc + profile drift | **~5–8 weeks** |
| B — Storage stack (the platform-defining breadth) | FDC (8271 or WD1770) + DFS sideways ROM + `.SSD/.DSD` format + load/save path | **~5–8 weeks** |
| C — Video fidelity finish | SAA5050 niceties (rounding/double-height/flash), Video ULA flash + cursor, half-cycle 2→1 MHz resync penalty, CRTC mid-frame re-latch | **~3–5 weeks** |
| D — Preservation breadth | cassette (UEF/CSW), RS-423 serial, Econet, Tube co-processor, User-VIA Centronics, speech, snapshot completion | **~7–11 weeks** |

**True 100% of everything ≈ 20–32 weeks.** It is **not** front-loaded onto cheap
wins: Tiers A and B are both genuinely hard (a video-addressing rewrite and a
storage stack from nothing). The cheap, high-leverage wins are the SN76489 LFSR fix
(a one-system audio defect) and the doc/profile drift; the launch-relevant slice
(Tier A) is most of the headline cost.

Effort key: **S** = hours · **M** = a few days · **L** = 1–2 weeks · **XL** = multi-week.

## Tier A — "Curriculum 100%" (the launch-relevant slice)

| Item | Effort | Notes |
|------|--------|-------|
| **BBC SN76489 LFSR width fix** | **S** | Shared-chip defect, single-system impact: `ti-sn76489` hardcodes the 16-bit SN76489A LFSR (seed `0x8000`, taps 0/3, `lib.rs:87,209`); the BBC's discrete SN76489 needs a 15-bit LFSR with taps 0/1, so BBC noise is audibly detuned. `Sn76489::new` takes only `clock_hz`. The fix is a variant selector on the chip plus the BBC consumer (`machine-acorn-bbc-micro/src/lib.rs:359`) selecting it. The period-N=0 bug is a separate shared-chip issue filed against the chip, not here. |
| **Real 6845-driven bitmap video (MODE 0–6)** | **L–XL** | The single biggest BBC accuracy gap. `render_scanline` (`machine-acorn-bbc-micro/src/lib.rs:587–635`) re-derives the display address from geometry (`ma = crtc_start + char_row * chars_per_line + col`, hardcoded `ra = line % 8`, `char_row = line / 8`) and reads the column count from the Video ULA (40/80), **ignoring CRTC R1/R6/R9/R12-R13 and the CRTC's own MA counter and display-enable**. The CRTC `tick()` is called only to make VSYNC for VIA CA1 (`lib.rs:440`). Drive the visible address from the 6845's real MA/RA chain so non-default geometries, R9 row heights, and the MODE-specific layouts are correct. |
| **BBC screen-address wrap + hardware scroll** | **M–L** | On real hardware the Video ULA wraps the 6845 MA into the active screen window (the address-translation used for hardware scrolling); R12/R13 changes mid-frame scroll the display. Neither the chip nor the consumer models it — `render_scanline` just masks `ma & 0x3FFF` and reads RAM linearly (`lib.rs:601–607`). Rides on the MODE 0–6 rewrite above; many games scroll this way. |
| **Confirm interactive `>` prompt / keyboard round-trip to BASIC** | **M** | The system doc's open question (`docs/systems/acorn/bbc-micro.md:23,33`): the MODE 7 banner renders, but a typed `>` prompt feeding BASIC is unconfirmed. Both boot tests are `#[ignore]` (need out-of-repo ROMs). Stand up a keyboard-driven `PRINT`-executes test (like the Electron/Atom ones) gated on the ROMs to close it. **needs-runtime-verification** with MOS + BASIC II ROMs present. |
| **System-doc + profile drift fixes** | **S** | Free, see Done section. The profile summary claims an "Intel 8271" that does not exist; the doc still implies "flat 2 MHz" when contention is modelled. |

## Tier B — Storage stack (platform-defining breadth)

The BBC has **no storage at all today**: no FDC crate (`machine-acorn-bbc-micro/Cargo.toml`
deps are only `mos-6502`, `mos-via-6522`, `motorola-6845`, `ti-sn76489`), no DFS
sideways ROM wired, no disk-image format crate (none under `crates/format-*acorn*`
or `*bbc*`), and `load_media` is a no-op (`runtime.rs:190`) with `media_slots: vec![]`
(`profiles.rs:57`). The vast majority of the BBC library shipped on 5.25" disk.

| Item | Effort | Notes |
|------|--------|-------|
| **FDC — Intel 8271 (or WD1770)** | **L–XL** | The Model B shipped the Intel 8271; later boards used the WD1770 (a `western-digital-wd1770` crate already exists in the workspace for other systems and could seed the WD path). Pick one (8271 is the canonical Model B), wire it into SHEILA, expose its command/status/data registers. The `the-advanced-disk-user-guide-for-the-bbc-micro` reference in the library documents the register interface. |
| **DFS sideways ROM + `.SSD/.DSD` format** | **L** | Acorn DFS lives in a sideways ROM slot (the machine already pages 16 banks via `$FE30`); add a `format-acorn-bbc-micro-ssd` crate for the single-/double-sided DFS image (`.SSD/.DSD`) and wire DFS reads/writes through the FDC. Without this, disk-only games and the curriculum's save/load do not run. |
| **Media load/save path** | **M** | Implement `load_media` and `media_slots` (both empty today) so the runtime can mount a disk image, and add a save/write-back decision in the mould of the C64 disk-save work. |

## Tier C — Video fidelity finish

| Item | Effort | Notes |
|------|--------|-------|
| **SAA5050 niceties** | **M** | Character rounding (diagonal smoothing), double-height, and flash are unmodelled (`docs/.../bbc-micro.md:57`; the renderer at `render_teletext_scanline` is crisp single-height). Double-height rows currently show at single height. |
| **Video ULA flash + cursor + per-pixel scroll** | **M** | The Video ULA model (`machine-acorn-bbc-micro/src/lib.rs:92–146`) handles palette, bpp, fast-clock and teletext-select, but no flash colour, no hardware cursor in bitmap modes, no horizontal fine-scroll offset. Pairs with the Tier-A bitmap rewrite. |
| **Half-cycle 2→1 MHz clock-resync penalty** | **S–M** | `access_master_ticks` (`lib.rs:489`) models the 1 MHz-bus contention at access-class granularity but, by its own comment (`lib.rs:488`), omits the half-cycle resync penalty when the CPU crosses the 2→1 MHz boundary mid-access. |
| **CRTC mid-frame re-latch + cursor raster/blink** | **S–M** | Once the Tier-A rewrite routes video through the 6845, the shared 6845 gaps (cursor raster R10/R11, interlace R8, sync-width-zero) become BBC-visible; the BBC-relevant subset is re-verified here. The chip-level fixes are filed against `motorola-6845`, not this system. |

## Tier D — Preservation breadth (back-loaded)

| Item | Effort | Notes |
|------|--------|-------|
| **Cassette (UEF / CSW)** | **M–L** | No tape path exists (`grep` for `cassette/tape` in the crate finds only the contention comment). The BBC's pre-disk and budget library is on cassette; needs a tape model + format crate + the serial/cassette read path. |
| **RS-423 serial + ACIA (6850)** | **M** | The doc lists RS-423 (`bbc-micro.md:72`) and the 6850 ACIA sits at the slow-SHEILA bus, but no ACIA is wired (the `mem_read`/`mem_write` SHEILA decode has no `$FE08–$FE1F` ACIA path). |
| **Econet LAN** | **L** | The doc flags Econet as a standout "always was networked" feature (`bbc-micro.md:71`). The ADLC (68B54) + Econet station model is a substantial preservation/curriculum item, not launch-blocking. |
| **Tube co-processor** | **XL** | Second-processor parasite interface; large, niche, preservation-grade. |
| **User VIA Centronics + user port** | **M** | The User VIA at `$FE60` is wired as a bare 6522 (`lib.rs:540,574`) but nothing consumes its ports — no Centronics printer, no user-port peripherals. |
| **Speech (TMS5220)** | **M** | The PB6/PB7 speech lines are reserved in the fire-button merge comment (`lib.rs:518`) but no speech chip exists. |
| **Snapshot completion** | **M** | The snapshot is effectively non-functional: `snapshot.rs` serialises only `time`, `model_id`, `mos_bytes` and `sideways_roms`, and `restore` rebuilds a **fresh** machine (`runtime.rs:139,148`) — RAM, CPU, CRTC, VIAs, PSG and latch state are all lost. Full snapshot needs the machine to expose and round-trip its live state (shared family pattern). |

## Done as part of this plan (free, ~half a day)

System-doc and profile drift, corrected against the code:

- **The clock model is no longer "flat 2 MHz."** `outstanding-work.md:828–830` says "Donor
  and this port both run CPU at flat 2 MHz" and lists CRTC bus contention as an open A-item —
  but `access_master_ticks` (`machine-acorn-bbc-micro/src/lib.rs:489`) **does** model the
  1 MHz/2 MHz contention (FRED/JIM/slow-SHEILA cost two master ticks, everything else one),
  with a passing test (`one_mhz_bus_accesses_cost_two_ticks_rest_one`). The system doc
  (`bbc-micro.md:49`) already reflects this; the `outstanding-work.md` entry is stale.
- **The profile claims hardware that does not exist.** `profiles.rs:50` summary says
  "…+ Intel 8271, 16 KB MOS ROM…" but there is no 8271 crate, no FDC dependency, and
  `media_slots: vec![]` (`profiles.rs:57`). Either remove the 8271 claim or gate it behind
  the Tier-B FDC work.
- **No `knowledge/chips/motorola-6845.md` and no `knowledge/systems/` BBC entry exist**
  (confirmed by `ls`), and the system doc's own "Validated against" section records **no**
  BBC-specific cross-check (`bbc-micro.md:42`). The reference library has strong primary
  sources — `bbc-micro-advanced-user-guide`, `the-advanced-disk-user-guide-for-the-bbc-micro`,
  `bbc-micro-reference` — that the distillation layer should cite.

## Recommended sequence (highest leverage first)

1. **BBC SN76489 LFSR fix** (S) — a one-system audio defect, cheapest correctness win; whole
   noise channel is detuned today.
2. **System-doc + profile drift** (S) — free; stop the 8271 claim and the stale flat-2 MHz note
   from misleading the next reader.
3. **Confirm interactive `>` prompt** (M) — quantify the doc's open boot question before
   building on top of it; un-ignore a keyboard round-trip test gated on the ROMs.
4. **Real 6845-driven bitmap video (MODE 0–6)** (L–XL) — the core-accuracy long pole; unblocks
   every bitmap-mode game. Build the address path from the CRTC's MA/RA chain.
5. **Screen-address wrap + hardware scroll** (M–L) — rides on (4); many games scroll this way.
6. **FDC (8271) → DFS ROM + `.SSD/.DSD` → load/save** (L–XL + L + M) — the storage stack; the
   platform-defining breadth that makes the disk library runnable.
7. **SAA5050 niceties + Video ULA flash/cursor + resync penalty** (M + M + S–M) — video finish.
8. **Cassette (UEF) → RS-423/ACIA → User-VIA Centronics → snapshot completion** (M–L + M + M + M)
   — preservation mid-tail.
9. **Econet (L) → Tube (XL) → speech (M)** — the completionist long tail.

## Key files

- CPU (shared, at ceiling): `crates/mos-6502/src/{lib,cycle,tick}.rs` (NMOS variant; no BBC-specific work — IRQ/NMI ROM-level cross-check for IRQ-heavy 6502 systems is a shared needs-verification item).
- Machine wiring + memory map + SHEILA decode: `crates/machine-acorn-bbc-micro/src/lib.rs` (`mem_read`/`mem_write` `:530–585`, `access_master_ticks` `:489`, `tick_cpu_cycle` `:446`).
- Video (the long pole): `crates/machine-acorn-bbc-micro/src/lib.rs` — `render_scanline` `:587–635` (geometry-derived bitmap addressing to replace), `render_teletext_scanline` `:645` (the working SAA5050), `VideoUla` `:92–146`.
- CRTC: `crates/motorola-6845/src/lib.rs` (`tick`, `start_address`, `read_data`; chip-level gaps filed against the crate).
- PSG: `crates/ti-sn76489/src/lib.rs` (LFSR `:87,209`; BBC consumer constructs at `machine-acorn-bbc-micro/src/lib.rs:359`).
- VIAs: `crates/mos-via-6522/src/lib.rs` (System VIA `$FE40`, User VIA `$FE60`); ADC `Upd7002` inline at `machine-acorn-bbc-micro/src/lib.rs:189–271`.
- Runtime + media + snapshot: `crates/runtime-acorn-bbc-micro/src/{runtime.rs,profiles.rs,snapshot.rs,input.rs}` (`load_media` no-op `runtime.rs:190`, `media_slots` empty `profiles.rs:50,57`, snapshot loses live state `snapshot.rs` + `runtime.rs:139`).
- Tests: `crates/machine-acorn-bbc-micro/tests/bios_boot.rs` (2 `#[ignore]`, need ROMs), 13 in-crate lib tests + 8 in `motorola-6845`.
- Reference (uncited): `reference/by-system/bbc-micro/{bbc-micro-advanced-user-guide,the-advanced-disk-user-guide-for-the-bbc-micro,bbc-micro-reference}.md`; MiSTer cores in `emulators/bbc-micro/`.
