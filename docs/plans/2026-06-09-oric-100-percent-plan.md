> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.

---
title: "plan: Oric-1 / Atmos to 100% — AY-via-VIA correctness, ULA rendering depth, media + timing breadth"
type: plan
date: 2026-06-09
system: docs/systems/oric/oric.md
basis: code-grounded survey of the machine/runtime/AY/VIA crates + live test runs, cross-checked against MAME tangerine/oric.cpp, Oricutron, and reference/by-system/oric/oric-reference.md, 2026-06-09
---

# Oric-1 / Atmos — road to 100%

What it would take to bring the Oric to feature- and accuracy-complete, grounded in
a code-level survey of the actual crates and tests, cross-checked against MAME's
`tangerine/oric.cpp` (the authoritative model here), Oricutron, and the in-repo
reference. The system doc / status rows had drifted and are corrected as part of
this work.

## Executive summary

**The Oric is a fourth distinct shape: a thin, single-file machine that boots and
types but whose accuracy depth was never built.** Unlike the C64 (plays its library,
hides a VIC-II long pole) or the NES (finished core, breadth + two bugs), the Oric
is one 782-line machine crate (`machine-oric-atmos/src/lib.rs`) wiring a
ceiling-grade 6502, a complete 6522 VIA, and a shared AY core into an **inlined,
hand-rolled ULA**. It cold-boots to `ORIC EXTENDED BASIC V1.1` and the keyboard
types — verified live (gated `bios_boot.rs`, the only ROM-dependent test). But three
things are missing or wrong underneath that surface, and **no Oric-specific media of
any kind loads** — `load_media` rejects every slot (`runtime.rs:156-162`,
`media_slots: Vec::new()` in `profiles.rs:71`).

The long pole is **not** one hard rewrite. It is the **sum of three medium efforts**:

1. **An AY-via-VIA correctness bug.** MAME drives the AY from CA2/CB2 with
   `(CA2=1,CB2=0)→read` and `(CA2=0,CB2=1)→write` (`oric.cpp:325-334`). The machine's
   decode (`lib.rs:271-286`) names `bdir=CA2`, `bc1=CB2` and does
   `(CA2=1,CB2=0)→write_data`, `(CA2=0,CB2=1)→read` — **the read and write cases are
   swapped relative to MAME.** It is masked today because the keyboard scan reads the
   matrix directly off the VIA's own ORA (`scan_keyboard`, `lib.rs:296-307`) rather
   than through the AY port-A read path MAME uses (`m_psg_a`, `oric.cpp:321`), so boot
   and typing don't exercise the broken path. Sound register *writes* may still land
   correctly or not depending on which (CA2,CB2) phase the KERNAL drives — this
   NEEDS RUNTIME VERIFICATION against a known Oric sound program, but the decode table
   does not match the reference and is the first thing to settle.

2. **The ULA renders only a fraction of its documented behaviour.** The per-scanline
   renderer (`render_scanline`, `lib.rs:395-494`) is genuinely per-line — the status
   doc's "runs end-of-frame" claim is stale doc-drift — and handles ink/paper serial
   attributes, TEXT vs HIRES split, and inverse video. But it implements **none** of:
   double-height (attrs 10/11/14/15), flash (12-15), the alternate Teletext character
   set (`$B800`/`$BC00`, attrs 9/11/13/15), or the 50/60 Hz per-row sync attributes
   (24-31). `apply_serial_attribute` (`lib.rs:488-494`) only matches ink (0-7) and
   paper (16-23) and silently drops every other control code. Whole genres of Oric
   software — Teletext-style chunky graphics, double-height titles, flashing text —
   render wrong.

3. **No media path at all.** The donor carried a working Oric `.tap` parser
   (`Emu198x-Oldest/crates/machine-oric-atmos/src/lib.rs:104` `parse_oric_tap`, the
   `$16` sync / `$24` marker / type+autorun+addrs+name format) that was never ported.
   There is no TAP format crate, no `MediaKind` slot, no Microdisc. A learner cannot
   load a single piece of real Oric software today; only ROM boot + typed BASIC works.

The CPU is at the ceiling (shared `mos-6502`, NMOS variant, Tom Harte
2,560,000/2,560,000 — see the shared-chip finding) and the Oric does not even use NMI
(`oric-reference.md:866`), so there is **no CPU work** on the road to 100%. The
shared AY core carries four chip-level defects (envelope 2× rate, noise 2× rate,
alternating shapes never reverse, hold-shape final level) — tracked at the chip level,
not re-filed here; they hit every AY consumer equally.

**Totals (focused work):**

| Tier | Scope | Estimate |
|------|-------|----------|
| A — "Curriculum 100%" | AY-via-VIA decode correctness, TAP load (port the donor parser), double-height + flash + alt-charset rendering, doc/status fixes | **~3–4 weeks** |
| B — Cycle-exact core | ULA cycle-stealing (CPU/bus contention) + the 50/60 Hz per-row sync attribute model + per-line attribute state to the silicon model | **~3–5 weeks** |
| C — Audio fidelity | rides the shared AY envelope/noise/shape fixes; Oric-specific verification of the AY-via-VIA path under a real sound program once decode is fixed | **~3–5 days** (Oric share) |
| D — Preservation breadth | Microdisc (WD1793 + Sedoric ROM overlay), `.dsk` format, tape SAVE, Oric-1 v1.0 ROM profile, Telestrat | **~6–10 weeks** |

**True 100% of everything ≈ 12–19 weeks.** The launch-relevant + "feels complete"
slice (Tier A + the audible part of C) is **~3–4 weeks** and buys correct sound
wiring, real software loading from tape, and a ULA that renders the documented modes.
Tier B (cycle-stealing + 50/60 Hz) and Tier D (Microdisc / Telestrat) are the
back-loaded depth and preservation long tail.

Effort key: **S** = hours · **M** = a few days · **L** = 1–2 weeks · **XL** = multi-week.

## Tier A — Curriculum 100% (do first)

| Item | Effort | Notes |
|------|--------|-------|
| **AY-via-VIA decode correctness** | **S–M** | `process_ay_bus` (`lib.rs:267-287`) swaps read vs write relative to MAME's `update_psg` (`oric.cpp:325-334`): MAME `(CA2=1,CB2=0)→read`, `(CA2=0,CB2=1)→data_w`; the machine does the opposite. Re-derive the `(CA2,CB2)→operation` table from MAME, add a unit test per phase, and route the keyboard scan through the AY port-A read path (`m_psg_a`) the way MAME does so the read path is actually exercised. **Bug-first per hard rules.** NEEDS RUNTIME VERIFICATION with a real Oric sound program. |
| **TAP cassette load** | **M** | Port the donor `parse_oric_tap` (`Emu198x-Oldest/.../lib.rs:104`): `$16` sync / `$24` marker / type + autorun + end/start addrs + null-terminated name + payload. Add a `format-oric-tap` crate, a `MediaKind`/`media_slots` tape slot in `profiles.rs`, and a `load_media` arm in `runtime.rs:156`. Donor does direct memory-injection (fast-load); acceptable for Tier A, with the real bit-banged path deferred to Tier B. The single highest-leverage gap — no real software loads today. |
| **Double-height + flash + alternate charset rendering** | **M** | `apply_serial_attribute` (`lib.rs:488-494`) drops attrs 8-15. Add: double-height (10/11/14/15 — scan the same glyph row across two display lines, per `oric-reference.md:334-337`), flash (12-15 — frame-counter modulated ink/paper swap), and the alternate Teletext set (9/11/13/15 selecting the `$B800`/`$BC00` charset base, `oric-reference.md:363-366`). The renderer is already per-line so the state-machine hook points exist. |
| **Doc + status drift fixes** | **S** | (1) `docs/status/outstanding-work.md:185-187` says display "runs end-of-frame" — it is per-scanline (`run_frame`, `lib.rs:192-200`). (2) `reference/by-system/oric/oric-reference.md:397` says AY BDIR=PB7/BC1=PB6 (port B) — MAME proves it is **CA2/CB2**; the reference is wrong and the machine header (`lib.rs:49`) is right. (3) The machine's own `(BDIR,BC1)` naming in `process_ay_bus` is inverted vs the silicon — fix alongside item 1. |

Tier A together gives correct sound control wiring, real tape software loading, and a
ULA that renders the documented colour/charset/height modes — the bulk of the "feels
complete" value.

## Tier B — Cycle-exact core finish

| Item | Effort | Notes |
|------|--------|-------|
| **ULA cycle-stealing / CPU contention** | **L** | The Oric ULA gates the CPU off the bus on the dot cycles it reads screen + charset RAM, more aggressively in HIRES (`oric-reference.md:872-876`). The machine ticks the CPU a flat 64 cycles/line (`CYCLES_PER_LINE`, `lib.rs:96`) with no contention. Timing-sensitive software (cycle-counted raster effects) runs at the wrong speed. Build against Oricutron's contention model. |
| **50/60 Hz per-row sync attributes** | **M** | Attrs 24-31 switch the vertical sync divider per row (`oric-reference.md:269-275, 314-321`). Unmodelled — `apply_serial_attribute` drops them. The visible artefact (partial roll / deliberate rolling-bar effects) must be rendered, not collapsed to a clean frame. |
| **Per-line attribute state to the silicon model** | **M** | Promote the ad-hoc ink/paper locals into the full per-row latch (ink, paper, charset, double-height, flash, 50/60 Hz) that resets at hsync, per `oric-reference.md:877-881`. Folds the Tier-A rendering additions and the 50/60 Hz item into one coherent state machine. |
| **Real bit-banged tape (CB1/CB2) + tape SAVE** | **M** | Replace the Tier-A direct-injection TAP load with the real VIA CB1-read / CB2-write bit-banged path gated by the PB3 motor relay (`oric-reference.md:906-911`), and add the SAVE write-back. Needed for protected tapes and for accuracy; rides the disk/tape write-back decision pattern. |

## Tier C — Audio fidelity (Oric share of shared-AY work)

| Item | Effort | Notes |
|------|--------|-------|
| **Shared AY envelope / noise / shape fixes** | **(shared)** | The four `gi-ay-3-8910` defects (envelope 2× rate, noise 2× rate, alternating shapes 10/14 never reverse, continue+hold shapes 11/13 hold at the wrong level) are tracked at the chip level and fixed once for all five AY consumers. Not re-filed here. |
| **Oric AY-via-VIA path verification** | **S** | Once the Tier-A decode fix lands, verify a known Oric sound program (PING/ZAP/SHOOT/EXPLODE, or a `PLAY`/`MUSIC` tune) produces the right registers reaching the AY. Confirms the machine-layer wiring, distinct from the chip-core fixes. |

## Tier D — Preservation breadth (back-loaded)

| Item | Effort | Notes |
|------|--------|-------|
| **Oric-1 v1.0 ROM profile** | **S** | The `Model::Oric1` profile exists (`profiles.rs`) but is wired identically to the Atmos aside from RAM size. v1.0 has relocated system variables and a different screen base (`oric-reference.md:765-781`); some v1.0-specific software and its famous bugs need the real v1.0 ROM image + the variant's memory-variable layout. |
| **Microdisc 3" floppy** | **L–XL** | WD1793 controller + the Sedoric/Stratsed DOS ROM that overlays the internal `$C000-$FFFF` ROM via the expansion-bus ROM-disable signal (`oric-reference.md:538-558`). The dominant French software-distribution medium. Needs a `.dsk` format crate + the ROM-overlay catch. |
| **Atmos RAM-under-ROM banking** | **M** | 64 KB is allocated and writes already land in RAM under ROM (`mem_write`, `lib.rs:257-262`; `writes_go_to_ram_under_rom_on_atmos` test), but exposing the RAM at `$C000-$FFFF` for reads (the expansion ROM-disable) is unmodelled (`outstanding-work.md:177-181`). Needed by Microdisc and advanced software. |
| **Telestrat** | **XL** | Bank-switched 64 KB, a second 6522 VIA, a 6551 ACIA, the WD1772, three cartridge ROM slots, and the Stratoric Atmos-compatibility ROM (`oric-reference.md:562-586`). A separate, larger undertaking; France-only, ~6000 units. Completionist tail. |

## Done as part of this plan (free, ~half a day)

Doc + status drift corrected. `outstanding-work.md:185-187` claimed display "runs
end-of-frame" — the code renders per-scanline as the beam scans (`run_frame`,
`lib.rs:192-200`; the `render_scanline_reads_its_own_character_row` test proves it).
The reference doc's AY pin map (`oric-reference.md:397`, "PB7=BDIR, PB6=BC1") is
wrong — MAME `oric.cpp:325-334` and the machine both use **CA2/CB2** — and the
reference should be corrected at the primary-library level. The machine's own
`process_ay_bus` `(BDIR,BC1)` naming is additionally inverted vs the silicon read/
write decode (the Tier-A bug). Test inventory recorded: machine 11 unit tests pass +
1 ignored ROM-gated boot test (`bios_boot.rs`, boots live to BASIC); VIA 14 pass;
AY-3-8912 3 pass; runtime 4 pass — all green, none failing.

## Recommended sequence (highest leverage first)

1. **AY-via-VIA decode correctness** (S–M) — bug first, per hard rules; re-derive the
   `(CA2,CB2)` table from MAME, test each phase, route the keyboard read through the
   AY port-A path, and runtime-verify against a real sound program.
2. **TAP load** (M) — port the donor parser + add the media slot; the one gap that
   stops all real software running. Highest breadth leverage per week.
3. **Double-height + flash + alt charset** (M) — the rendering modes the ULA doc
   describes and current software uses.
4. **Doc + status fixes** (S) — eradicate the drift surfaced above.
5. **Shared AY fixes land** (chip-level) **+ Oric sound verification** (S) — the
   audible win, once decode is correct.
6. **ULA cycle-stealing** (L) **+ 50/60 Hz attributes** (M) **+ per-line latch** (M)
   — the cycle-exact core; fold into one state machine.
7. **Real bit-banged tape + SAVE** (M) — accuracy + write-back.
8. **Oric-1 v1.0 profile** (S) **→ RAM-under-ROM banking** (M) **→ Microdisc** (L–XL)
   **→ Telestrat** (XL) — the preservation long tail.

## Key files

- CPU (at ceiling): shared `crates/mos-6502/` (NMOS variant; no Oric-specific work; Oric does not use NMI).
- Machine (the whole system — CPU/VIA/AY wiring + inlined ULA): `crates/machine-oric-atmos/src/lib.rs` — AY decode `process_ay_bus` (`:267-287`, the inverted table), keyboard `scan_keyboard` (`:296-307`), IJK joystick `update_ijk_joystick` (`:323-340`), renderer `render_scanline`/`render_text_scanline`/`render_bitmap_scanline`/`apply_serial_attribute` (`:395-494`), memory map `mem_read`/`mem_write` (`:218-264`), frame loop `run_frame` (`:184-203`).
- VIA (complete; T1/T2/SR/CA-CB all modelled): `crates/mos-via-6522/src/lib.rs` — `tick` (`:196`), `process_ay_bus` consumers `peek`/`ora`/`orb`/`port_b_drive_state` (`:401,506,511,541`).
- AY (shared core, chip-level defects tracked separately): `crates/gi-ay-3-8912/src/lib.rs` (facade) over `crates/gi-ay-3-8910/src/lib.rs`.
- Runtime (media gap): `crates/runtime-oric-atmos/src/runtime.rs:156-162` (`load_media` rejects all), `crates/runtime-oric-atmos/src/profiles.rs:71` (`media_slots: Vec::new()`), `input.rs` (keyboard + IJK mapping).
- Tests: `crates/machine-oric-atmos/tests/bios_boot.rs` (ignored, ROM-gated, boots live); 11 inline machine tests + VIA 14 + AY 3 + runtime 4, all passing.
- Donor (TAP parser to port): `Emu198x-Oldest/crates/machine-oric-atmos/src/lib.rs:104` (`parse_oric_tap`), `:697` (`load_tap`).
- Reference: `reference/by-system/oric/oric-reference.md` (correct except the AY pin-map at `:397`); MAME `emulators/multi-system/mame/src/mame/tangerine/oric.cpp` (authoritative AY/keyboard wiring); Oricutron `emulators/oric/oricutron/` (contention + IJK joystick source); MAME `oric_tap.cpp` / `oric_dsk.cpp` (formats).
