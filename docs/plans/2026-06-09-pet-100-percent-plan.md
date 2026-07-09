> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.

---
title: "plan: Commodore PET to 100% — peripherals, media, CRTC accuracy, preservation breadth"
type: plan
date: 2026-06-09
system: docs/systems/commodore/pet.md
basis: code-grounded survey of machine-commodore-pet, runtime-commodore-pet, the shared mos-6502 / mos-pia-6520 / mos-via-6522 / motorola-6845 crates, their tests, the status docs, and reference/by-system/pet/pet-reference.md — 2026-06-09
---

# Commodore PET — road to 100%

What it would take to bring the Commodore PET (the 8032/4032-class CRTC machine
modelled here) to feature- and accuracy-complete, grounded in a code-level survey
of the actual crates and tests. The PET is the seventeenth donor extraction
(`docs/status/outstanding-work.md:529`) and the youngest of the Commodore cores.

## Executive summary

**The PET is a "boots, types, and stops there" core — the hard part (cold boot to
`READY.` with a working keyboard) is done; everything that makes it *useful* is
missing.** This is the fourth distinct shape in the fleet:

- The **Spectrum** had a finished core and cheap breadth.
- The **C64** plays its library and hides a VIC-II core-accuracy long pole.
- The **NES** is finished core + cheap breadth + two bugs.
- The **PET** has a *correct CPU* and a *just-barely-complete machine shell*: it
  cold-starts to the canonical `### COMMODORE BASIC ###` / `31743 BYTES FREE` /
  `READY.` banner and types a BASIC line through a ground-truthed keyboard matrix
  (`machine-commodore-pet/tests/keyboard_type.rs`, `outstanding-work.md:550-573`),
  but it has **no program loading, no storage, no sound, no second PIA, and no
  IEEE-488 bus**. The long pole here is *breadth*, not depth — there is no demoscene
  long tail to chase, but almost the entire I/O surface is unbuilt.

The CPU is already at the ceiling. Per the established 6502 shared-chip finding,
`mos-6502` (`M6502::new()`, the NMOS variant the PET uses at
`machine-commodore-pet/src/lib.rs:115`) passes Tom Harte 2,560,000/2,560,000,
Klaus Dormann functional, and 36/36 hermetic unit tests. **There is no CPU work on
the road to 100%** — the only 6502 items are accuracy-debt/verification-coverage
items tracked fleet-wide, not PET blockers.

What's actually built (verified by reading the code, not the docs):

- **CPU + reset:** `cpu.reset()` runs at construction (`lib.rs:115-116`); the boot
  reaches `READY.` Memory map at `lib.rs:237-289` is plausible and matches the
  reference (`reference/by-system/pet/pet-reference.md:235`).
- **Video:** the `motorola-6845` CRTC (really a 6545 on this machine — see below)
  is wired and drives a green-on-black framebuffer (`lib.rs:174-221`). CB1 vertical
  retrace raises the 60 Hz IRQ (`lib.rs:160-165`) that runs the keyboard scan.
- **Keyboard:** the 10×8 graphics-keyboard matrix is ground-truthed against the
  real editor ROM (`machine-commodore-pet/src/input.rs:1-11`, `keyboard.rs`).
- **One PIA + one VIA** are wired (`lib.rs:251-289`).

What's missing (the body of this plan): the **second PIA at `$E820`** and the
**IEEE-488 bus** it carries, **`.prg`/`.tap` program loading** (today the only way
to get code in is to type it), **CB2 piezo sound**, the **datassette**, **CRTC
accuracy** (cursor blink, 80-column 2 MHz clock, the shared `motorola-6845`
overflow bug), **snapshot**, and the **native verifier window**.

Two confirmed correctness defects sit in shared chips the PET drives: the
`motorola-6845` `h_counter` overflow when R0=255 (established 6845 finding), and the
CRTC cursor renders as a solid non-blinking full-cell block (established 6845
finding; observable on the PET editor cursor per `lib.rs:196` + `lib.rs:214`).

**Totals (focused work):**

| Tier | Scope | Estimate |
|------|-------|----------|
| A — "Curriculum 100%" | `.prg` autoload, `.tap`/datassette LOAD, CB2 piezo sound, fix CRTC cursor block-render | **~2–3 weeks** |
| B — System completeness | second PIA at `$E820`, IEEE-488 bus + IEEE drive (disk LOAD/SAVE), tape SAVE, snapshot | **~4–6 weeks** |
| C — CRTC / timing accuracy | 80-column 2 MHz CRTC clock, cursor blink (R10/R11), CRTC overflow-bug fix, mid-frame register reprogramming | **~2–3 weeks** |
| D — Preservation breadth | model variants (2001 discrete-TTL video, BASIC 1/4, business keyboard, SuperPET 6809), native window | **~4–6 weeks** |

**True 100% of everything ≈ 12–18 weeks.** It is **front-loaded in value**: Tier A
(get programs in, make sound) is small and buys most of the "feels usable" jump,
because a PET with no way to load a program is a typewriter. Tier B (IEEE-488 + a
real drive) is the largest single chunk and the genuine system-completeness work.

Effort key: **S** = hours · **M** = a few days · **L** = 1–2 weeks · **XL** = multi-week.

## Tier A — Curriculum 100% (highest leverage; ~2–3 weeks)

| Item | Effort | Notes |
|------|--------|-------|
| **`.prg` program load** | **M** | There is no media path at all today: `media_slots: vec![]` (`runtime-commodore-pet/src/profiles.rs:76`), `load_media` is a no-op (`runtime.rs:256-258`), and no `format-commodore-pet-*` crate exists. The C64 already has `format-commodore-c64-prg`; a PET `.prg` autoload (parse the 2-byte load address, splat into RAM, fix BASIC pointers / `RUN`) is the single highest-leverage item — without it the only way to run code is to type it. Flagged "A — `.prg` / `.tap` load not implemented" at `outstanding-work.md:579`. |
| **CB2 piezo sound** | **M** | The PET's only sound is the VIA CB2 line toggling the piezo speaker (`reference/.../pet-reference.md:53` §12; `lib.rs:21` doc-comment names it). The `mos-via-6522` crate already models CB2 output, shift-register pulse modes, and `cb2_out`/`cb2_pulse_low` (`mos-via-6522/src/lib.rs:84-190,233-271`), but the machine never reads CB2 to drive audio: the runtime pushes an **empty** audio buffer every frame (`runtime.rs:293-298`, `samples: &[]`). Wire CB2 → a 1-bit speaker sampler → the audio packet. Whole genres of PET type-in games (the 1981 CURSOR programs in `reference/by-system/pet/`) use CB2 beeps. |
| **CRTC cursor renders as a solid block (defect)** | **S** | Established 6845 finding: `cursor_active` is a pure MA==cursor compare (`motorola-6845/src/lib.rs:186-187`) ignoring R10/R11 raster + blink bits. The PET inverts the whole cell on it (`lib.rs:196` + `lib.rs:214` `fg = bit == 0`), so the editor cursor is a permanently-on full-cell block instead of a blinking rastered block. The fix lives in the shared crate; do NOT re-file the chip-level issue, but the PET is the observable consumer and should be the verification target. |

## Tier B — System completeness (the largest chunk; ~4–6 weeks)

| Item | Effort | Notes |
|------|--------|-------|
| **Second PIA at `$E820`** | **M** | The real PET has **two** 6520 PIAs: #1 at `$E810` (keyboard) and **#2 at `$E820`** carrying IEEE-488 data + handshake (`reference/.../pet-reference.md:235` `$E820–$E823`, §10). The machine wires only PIA #1; there is **no `$E820` handler at all** in `mem_read`/`mem_write` (`lib.rs:237-289` — `$E800-$EFFF` falls through to `0xFF` / no-op). The `mos-pia-6520` crate is reusable as-is (14/14 tests pass). This is the gateway to IEEE-488. |
| **IEEE-488 (GPIB) bus + IEEE disk drive** | **L–XL** | The PET's mass storage is IEEE-488, not the C64's serial IEC, so the existing `common-commodore-iec` / `machine-commodore-1541` cannot be reused directly. Building a CBM IEEE-488 drive (2040/4040/8050-class) with its own DOS ROM and a `.d80`/`.d82` (or at minimum a virtual-device "JiffyDOS-style" LOAD/SAVE intercept) is the real system-completeness work. Flagged "A — Cassette / IEEE-488 unwired. VIA exists but the external lines aren't connected" (`outstanding-work.md:575`). NEEDS RUNTIME VERIFICATION of which KERNAL IEEE routines must be satisfied. |
| **Datassette LOAD + tape SAVE** | **M–L** | Cassette is wired to PIA CA1 (sense), the VIA, and the motor relay (`reference/.../pet-reference.md:53` §13). Today nothing connects the tape lines (`outstanding-work.md:575`). A `.tap` LOAD path (pulse-fed bit stream) plus the write-back SAVE path, riding the same disk-save-write-back decision the C64 used (`knowledge/decisions/disk-save-write-back.md`). |
| **Snapshot / save-state** | **M** | `runtime.rs:304-310` delegates to `snapshot::encode/decode`, but `command()` returns `UnsupportedOperation` for everything (`runtime.rs:312-316`) and the feature is flagged "A — Snapshot deferred (shared family pattern)" (`outstanding-work.md:578`). The 6845 already serialises its full state (`motorola-6845/src/lib.rs:295-346`), so the chip side is ready; the machine-level encode of RAM/video-RAM/CPU/PIA/VIA is the work. |

## Tier C — CRTC / timing accuracy (~2–3 weeks)

| Item | Effort | Notes |
|------|--------|-------|
| **80-column CRTC clocked at 2 MHz** | **M** | The machine ticks the CRTC at CPU rate (1 MHz) in all modes, but real 80-column hardware clocks the CRTC at 2 MHz — self-admitted as a "donor v1 simplification" at `lib.rs:30-33` and `outstanding-work.md:546-548`. Mid-frame timing and the 80-column display geometry are therefore approximate. The 80-col register set is hardcoded at `lib.rs:101-105`. |
| **Cursor blink (R10/R11)** | **S–M** | Pairs with the Tier-A solid-block fix: once the cursor uses R10 start-raster / blink bits + R11 end-raster, the PET editor cursor blinks correctly. Established 6845 finding (`motorola-6845/src/lib.rs:186-187`). |
| **CRTC counter-overflow defect (R0=255)** | **S** | Established 6845 finding: `h_counter += 1` on a `u8` with R0=255 panics in debug / silently wraps in release (`motorola-6845/src/lib.rs:203-204`), with the same unguarded `+=` on `v_counter`/`v_adjust`/`hsync_counter`/`vsync_counter`. The PET boot ROM programs R0=49 (40-col) / 99 (80-col) (`lib.rs:101-105`), so the boot path does not trigger it today — but any PET program that reprograms the CRTC could. Fix is in the shared crate; do NOT re-file. NEEDS RUNTIME VERIFICATION that no PET title programs R0=255. |
| **Mid-frame CRTC reprogramming** | **M** | The reference notes PET ROMs "almost never reprogram" the CRTC (`reference/.../pet-reference.md:289`), so this is low-priority, but raster-split/scroll demos that change R12/R13 mid-frame are not reflected (the machine reads `memory_address()` masked to the 2 KB video window at `lib.rs:187`). |

## Tier D — Preservation breadth (back-loaded; ~4–6 weeks)

| Item | Effort | Notes |
|------|--------|-------|
| **Model variants** | **L** | Only the CRTC 4032/8032 class is modelled (`runtime-commodore-pet/src/profiles.rs:9-14` — `Pet40Col`/`Pet80Col`). The original **2001** used discrete-TTL video (no CRTC), the chiclet keyboard, and BASIC 1; the **business keyboard** differs from the modelled graphics keyboard (`reference/.../pet-reference.md:53` §9); BASIC 1/2/3/4 differ (`§15`). Each is a profile + ROM set + (for the 2001) a different video path. |
| **SuperPET (6809 second CPU)** | **XL** | The SuperPET adds a Motorola 6809 (`reference/.../pet-reference.md:source` lists `commodore-superpet`). The fleet already has `motorola-6809` (used by Dragon), so the CPU exists, but dual-CPU bus arbitration is genuinely-someday preservation work. |
| **Native verifier window** | **S** | Capture + script + MCP parity landed (`outstanding-work.md:580`); the native `wgpu` interactive window is the remaining surface — shared fleet item, tracked for the PET for completeness. |

## Done as part of this plan (free, ~half a day)

System-doc reconciliation. The status docs are largely accurate for the PET (they
were written at the 2026-06-01 extraction and 2026-06-04 boot), but two framing
corrections are worth capturing: (1) the machine models a **6545** as a
`motorola-6845` — fine, since the reference itself calls the 6545 "a second-source
variant of the Motorola 6845" (`reference/.../pet-reference.md:284`), but the
`Cargo.toml`/source should note the substitution so it isn't mistaken for a gap;
(2) `outstanding-work.md:575` bundles "Cassette / IEEE-488 unwired" as one line —
they are two separate subsystems (PIA #2 + tape) and split cleanly into the issues
below.

## Recommended sequence (highest leverage first)

1. **`.prg` program load** (M) — the one Tier-A gap that turns a typewriter into a
   computer you can run software on. Highest leverage per week, mirrors
   `format-commodore-c64-prg`.
2. **CB2 piezo sound** (M) — the cheapest audible win; the VIA already does the
   hard part, the runtime just pushes silence.
3. **CRTC cursor block-render fix + blink** (S + S–M) — small, visible correctness;
   the editor cursor is wrong on every boot.
4. **Second PIA at `$E820`** (M) — the prerequisite for everything storage-related.
5. **IEEE-488 bus + IEEE drive** (L–XL) — the real system-completeness long pole;
   PET storage is IEEE, not IEC.
6. **Datassette LOAD + tape SAVE** (M–L) — the period-authentic loading path.
7. **Snapshot** (M) — the chip side is ready; finish the machine encode.
8. **80-col 2 MHz CRTC + mid-frame reprogramming** (M + M) — timing accuracy.
9. **Model variants → SuperPET → native window** (L / XL / S) — the preservation
   tail.

## Key files

- CPU (already at ceiling): `crates/mos-6502/src/{lib,cycle,tick}.rs` (NMOS `M6502::new()` at `crates/machine-commodore-pet/src/lib.rs:115`).
- Machine: `crates/machine-commodore-pet/src/lib.rs` (`tick` `:153`, `tick_display` `:174`, `mem_read`/`mem_write` `:237`/`:279`, CRTC setup `:99-109`, CB1 retrace IRQ `:160-165`, missing `$E820` handler in the `:237-289` match).
- Keyboard (ground-truthed): `crates/machine-commodore-pet/src/{input.rs,keyboard.rs}`.
- Runtime: `crates/runtime-commodore-pet/src/{runtime.rs,profiles.rs,input.rs,snapshot.rs}` (no-op `load_media` `runtime.rs:256`, empty audio `runtime.rs:293-298`, `media_slots: vec![]` `profiles.rs:76`).
- Shared chips: `crates/mos-pia-6520/src/lib.rs` (reuse for PIA #2), `crates/mos-via-6522/src/lib.rs` (CB2/shift-register already present `:84-271`), `crates/motorola-6845/src/lib.rs` (cursor `:186-187`, overflow `:203-204`, ma latch `:182-183`).
- Tests: `crates/machine-commodore-pet/tests/{rom_boot.rs,keyboard_type.rs}` (both `#[ignore]` pending the ROM set), 7 inline machine unit tests (`lib.rs:392-464`).
- Reference: `reference/by-system/pet/pet-reference.md` (memory map `:235`, PIAs `:459` §10, VIA `§11`, CB2 sound `§12`, datassette `§13`, IEEE-488 `§14`, 6545 `:282-293`).

