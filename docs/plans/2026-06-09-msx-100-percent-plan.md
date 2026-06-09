---
title: "plan: MSX1 to 100% — system timing, media breadth, peripherals, and the shared-chip long poles"
type: plan
date: 2026-06-09
system: docs/systems/microsoft/msx.md
basis: code-grounded survey of machine-msx/runtime-msx/emu198x-msx + intel-8255 + shared-chip findings (TMS9918, AY-3-8910/8912, Z80), cross-checked against reference/by-system/msx, 2026-06-09
---

# MSX1 — road to 100%

What it would take to bring the MSX1 to feature- and accuracy-complete, grounded in
a code-level survey of the actual crates and tests — not doc prose. The MSX boots
the real Microsoft BIOS to the BASIC prompt today and runs at least one commercial
MegaROM (Gradius/Nemesis), so the spine is healthy; the gaps are a system-timing
defect, media breadth (cassette + disk), peripheral polish, and the shared-chip
accuracy debt the MSX inherits but does not own.

## Executive summary

**The MSX1 is a "wide-but-shallow standard" shape, and its long poles are mostly
not its own.** Unlike the C64 (one hard core rewrite) or the NES (a finished core
plus breadth), the MSX is a *standard* assembled from chips that already back six
other systems in the fleet. The machine wiring is competent and small
(`machine-msx/src/lib.rs`, 749 lines incl. tests): slot system, MegaROM mappers,
keyboard matrix, joystick-through-PSG, the correct 3:2 VDP phase clock, and a clean
save-state envelope. All in-crate tests pass — `machine-msx` unit tests and
`runtime-msx` 11/11 (verified 2026-06-09). The two interesting tests
(`bios_boot.rs`, `debug_target.rs`) are `#[ignore]`-gated on a copyrighted 32 KB
BIOS that is not shipped in-tree, so **boot is verified only by hand, never in CI**
— the single most important verification gap.

What is done: BIOS boot to BASIC (hand-verified 2026-06-01,
`outstanding-work.md:1127`), the four MegaROM mappers (Plain/Konami/KonamiSCC/
ASCII8/ASCII16, `lib.rs:108-212`), slot select via PPI port A (`lib.rs:402-427`),
joystick-through-PSG-port-A (`lib.rs:294-331`, `lib.rs:434-443`), the keyboard
matrix (`lib.rs:444-454`), and a bootstrap-only snapshot (`snapshot.rs`).

The long poles:

1. **System-specific: the M1 wait-state is missing.** The reference is explicit —
   the MSX inserts **one wait state in every M1 (opcode-fetch) cycle**, ~21% slower
   than a bare Z80A (`reference/by-system/msx/msx-reference.md:140`). The Z80 core
   exposes a `wait` pin (`crates/zilog-z80/src/z80.rs:82-86,462-464`) but
   `machine-msx` never asserts it. Every program runs ~21% too fast; PSG software
   envelopes, cassette decode, and racing-the-beam effects drift. This is the
   clearest MSX-owned correctness defect.

2. **System-specific: no cassette and no disk.** The reference documents both (CAS
   FSK tape, `formats.md:244`; FAT12 DSK 720K, `formats.md:336` + `msx-reference.md:736`),
   and the cassette is bit-banged through PPI port C bits 4-5
   (`msx-reference.md:584-585`) — but `machine-msx` wires neither. No
   `format-microsoft-msx-*` crate exists; `load_media` accepts only cartridges
   (`runtime.rs:276-296`). A huge slice of the MSX library is disk- and tape-only.

3. **Shared-chip debt the MSX inherits (do NOT re-derive — see fleet chip plans):**
   the TMS9918 sprite-collision/backdrop/mid-line defects, the AY-3-8910 envelope/
   noise octave-doubling and broken alternating-envelope shapes, and the Z80 IM0
   gap. These are filed against the chips, not here; this plan only notes their
   MSX-visible impact and the verification owed.

**Totals (focused work):**

| Tier | Scope | Estimate |
|------|-------|----------|
| A — "Curriculum 100%" | M1 wait-state, BIOS-boot CI gate (C-BIOS), cart mapper auto-detect, doc fixes | **~1.5–2 weeks** |
| B — System media + peripherals | cassette (CAS load + bit-bang), disk (FAT12 DSK + FDC + disk-BIOS cart), PPI port-C click/cassette/CAPS wiring, full live snapshot | **~5–8 weeks** |
| C — Inherited chip accuracy (verify + lean on chip fixes) | confirm TMS9918 + AY defects on real MSX software once chip fixes land; M1-wait audio re-verification | **~1–1.5 weeks of MSX-side work** (chip fixes costed in chip plans) |
| D — Preservation breadth | rarer mappers (R-Type/Kon-wired-logic, MegaRAM, Panasonic FM, ASCII16+SRAM), subslot expansion, second-disk-drive, printer | **~4–6 weeks** |

**True 100% of everything ≈ 12–18 weeks of MSX-specific work**, on top of the
fleet-shared chip fixes. It is **front-loaded onto cheap wins** (the wait-state and
the CI gate are small and high-value) but the *bulk* is Tier B media breadth — the
MSX's identity as a disk/tape machine is the long pole, not a core rewrite.

The launch-relevant slice (Tier A + a cassette loader) is a small fraction; the
disk stack and the inherited chip accuracy are the completionist tail.

Effort key: **S** = hours · **M** = a few days · **L** = 1–2 weeks · **XL** = multi-week.

## Tier A — Curriculum 100% (cheap, high-value; do first)

| Item | Effort | Notes |
|------|--------|-------|
| **M1 wait-state** | **M** | Assert the Z80 `wait` pin during M1 fetch so the MSX runs at true speed (~21% slower than bare Z80A). The pin exists (`zilog-z80/src/z80.rs:82-86,462-464`); `machine-msx` must drive it per the M1 phase and re-budget `tstates_per_frame` if the budget is wall-clock-derived. Source: `reference/by-system/msx/msx-reference.md:140`. **This is the one confirmed MSX-owned correctness defect** — `tick_tstate` (`lib.rs:354-375`) runs the CPU with no contention. |
| **BIOS-boot CI gate via C-BIOS** | **S–M** | `bios_boot.rs` is `#[ignore]`-gated on a copyrighted BIOS that is not shipped, so boot is **never exercised in CI** (`bios_boot.rs:46`). The free GPL **C-BIOS** is already named as an accepted source (`bios_boot.rs:34`, `outstanding-work.md:1125`). Vendor C-BIOS into the asset layer and un-ignore the smoke so boot is a tracked, regression-guarded gate, not a hand-check. Highest verification leverage per hour. |
| **Cartridge mapper auto-detection** | **S–M** | `load_media` hardwires `MapperType::Plain` for every cart (`runtime.rs:279-284`); only the CLI `--mapper` flag selects others (`script.rs:114-122`). The reference documents the "AB" header + size/bank-write heuristics (`formats.md:`ROM Header + Mapper Detection). Add header/size-based detection so dropping a Konami/ASCII MegaROM through the normal media path Just Works. Without it, every >32 KB cart loaded via `load_media` mis-maps. |
| **System-doc + outstanding-work touch-up** | **S** | Record the M1-wait gap, the C-BIOS CI gate, and the absent cassette/disk in the status docs; `outstanding-work.md:1111-1158` lists joystick as "no input surface yet" but the joystick **is** wired (`lib.rs:294-331`, tested `lib.rs:682-706`) and drives Gradius (`drivability-assessment.md:275`) — correct that drift. |

## Tier B — System media + peripherals (the bulk)

| Item | Effort | Notes |
|------|--------|-------|
| **Cassette — CAS load + PPI bit-bang** | **L** | The MSX reads/writes tape by bit-banging PPI port C (CASON bit 4 motor, CASW bit 5 write) and reading the cassette input through PSG port A bit 6 (`msx-reference.md:584-585,642`). Today `io_write` ignores those bits and `io_read` never presents the cassette line (`lib.rs:444-454,470-489`). Add a CAS reader (FSK, `formats.md:244`), a tape transport paced by the motor bit, and the read-line feed. Whole genres of MSX software are tape-only. |
| **Disk — FAT12 DSK + FDC + disk-BIOS cartridge** | **XL** | A floppy interface is a cartridge-plus-ROM expansion: an FDC (WD2793 or µPD765) plus a disk-BIOS ROM that hooks `PHYDIO`/`FORMAT` (`msx-reference.md:736-752`). The fleet already has `nec-upd765a` and `western-digital-wd1770` chip crates and a `format-amstrad-dsk` to learn from — but no MSX disk-BIOS cart, no `format-microsoft-msx-dsk`, and no `MediaKind::Disk` slot in the MSX profile (`profiles.rs:80-95` only declares two cartridge slots). The 720K 3.5" FAT12 image is what virtually all surviving MSX disk software uses (`msx-reference.md:752`). The single largest breadth item. |
| **PPI port C — click / CAPS LED / cassette outputs** | **S–M** | The 1-bit keyboard click and CAPS-lamp drive through PPI port C (module doc `lib.rs:60`) but `machine-msx` only consumes port C bits 0-3 for the keyboard row (`lib.rs:444-454`, `intel-8255/src/lib.rs:78-82`). Surface the click as audio and the CAPS state for the host; rides the cassette work. |
| **Full live snapshot** | **M** | Snapshot is bootstrap-only — it persists BIOS/cart bytes + time, **not** live CPU/VDP/PSG/PPI/RAM (`snapshot.rs:1-7` self-admits "replay from a known starting point"). A real save-state needs `machine-msx` to grow an `MsxSnapshot` (the file flags this as the follow-up). Shared deferral pattern across the TMS9918 family (`outstanding-work.md:1148`). |

## Tier C — Inherited chip accuracy (verify on MSX; fixes live in chip plans)

These defects are **already established at the chip level** — they are filed against
`ti-tms9918`, `gi-ay-3-8910`/`gi-ay-3-8912`, and `zilog-z80`, **not** re-filed here.
This tier is only the MSX-side verification owed once those fixes land, plus the
re-check the M1-wait change forces.

| Item | Effort | Notes |
|------|--------|-------|
| **TMS9918 defects on real MSX software** | **S–M** | Sprite-collision-ignores-transparent-sprites and mid-frame backdrop one-frame-late hit MSX games that use VDP collision and raster border splits. The MSX consumes `ti-tms9918` unchanged (`lib.rs:81,275`); when the chip fix lands, re-verify against a collision-driven MSX title. Render model is per-dot and byte-identical to the old batch model (`ti-tms9918/src/lib.rs:6-10,325-360`), so no MSX-side render rework. |
| **AY-3-8910/8912 envelope + noise octave-doubling** | **S** | The MSX ticks the PSG at CPU÷2 = 1.789773 MHz correctly (`lib.rs:95,365-369`), so the envelope-runs-2x, noise-runs-2x, and broken alternating-envelope-shape defects are pure chip bugs that hit MSX music as written. Re-verify MSX PSG tunes once the `gi-ay-3-8910` fixes land — and again after the M1-wait change, since wait states shift the audio cadence. |
| **Z80 IM0 latency** | **S** | The MSX BIOS uses IM 1 (`lib.rs:391-395` returns `0xFF` → RST 38h), so the IM0-collapsed-into-IM1 gap is latent. No MSX software is known to depend on IM0. Verification-only; flag, do not fix here. |

## Tier D — Preservation breadth (the completionist tail)

| Item | Effort | Notes |
|------|--------|-------|
| **Rarer cartridge mappers** | **M–L** | The five wired mappers (`lib.rs:108-122`) cover the bulk, but the long tail — R-Type/Konami-wired-logic variants, ASCII16+SRAM (battery saves), Panasonic, MegaRAM (Brazilian, `msx-reference.md:700`) — needs per-mapper work plus battery-RAM persistence (which no MSX path has today). |
| **Subslot expansion** | **M** | Writes to `$FFFF` (slot 3 in page 3) expand each primary slot into 4 subslots; recognised-but-disabled (`lib.rs:45-48`). MSX1 mostly doesn't need it, but some expanded MSX1 machines and the disk-BIOS-in-a-subslot layout do. |
| **Memory-mapper RAM (>64 KB)** | **M** | Ports `$FC`–`$FF` set 16 KB segment numbers; BIOS detects size by write-read-back (`msx-reference.md:699`). MSX1-relevant for larger-RAM machines; precondition for some disk/MSX-DOS setups. |
| **Printer + second floppy + RTC** | **S–M** each | Printer via PPI port C, second disk drive, and the clock-IC are completeness items on the very edge of MSX1 scope. |

## Done as part of this plan (free, ~half a day)

Status-doc drift corrected. `outstanding-work.md:1143-1147` lists "Joystick … no
joystick input surface on the machine yet" — but the joystick **is** fully wired
(`set_joystick`/`joystick_byte` `lib.rs:294-331`, read through PSG port A
`lib.rs:434-443`, unit-tested `lib.rs:682-706`) and drives Gradius in the
drivability sweep (`drivability-assessment.md:275`). The doc understates the
machine. The plan also records three items the status docs lack entirely: the
**M1 wait-state gap**, the **C-BIOS CI gate** (boot is currently hand-verified
only), and the **absent cassette/disk media stack**.

## Recommended sequence (highest leverage first)

1. **BIOS-boot CI gate via C-BIOS** (S–M) — make boot a tracked gate before
   touching anything else; everything downstream needs a regression guard.
2. **M1 wait-state** (M) — the one confirmed MSX-owned correctness defect; fixes
   real-time speed and unblocks honest audio/timing verification.
3. **Cartridge mapper auto-detect** (S–M) — every >32 KB cart through `load_media`
   mis-maps today; cheap, high library yield.
4. **Doc fixes** (S) — eradicate the joystick understatement; record the three new
   items.
5. **Cassette — CAS load + bit-bang** (L) — the cheaper of the two media stacks and
   a large slice of the tape-only library.
6. **TMS9918 + AY re-verification on MSX** (S–M) — once the chip fixes land (their
   plans), confirm on real MSX software; re-check audio after the wait-state change.
7. **Disk — FAT12 DSK + FDC + disk-BIOS cart** (XL) — the single largest breadth
   item; the MSX's disk identity.
8. **PPI port-C click/CAPS + full live snapshot** (S–M + M) — peripheral polish.
9. **Rarer mappers + battery RAM, subslot, memory-mapper RAM, printer/RTC** — the
   preservation completionist tail.

## Key files

- Machine wiring: `crates/machine-msx/src/lib.rs` — clock model `:84-99,354-375`,
  slot resolution `:402-427`, MegaROM mappers `:108-212`, joystick `:294-331,434-443`,
  keyboard `:444-454`, I/O map `:429-489`, IM1 IntAck `:391-395`.
- M1 wait-state target: `crates/zilog-z80/src/z80.rs:82-86,462-464` (the `wait`
  pin exists, unused by MSX), asserted from `machine-msx/src/lib.rs:354-375`.
- PPI: `crates/intel-8255/src/lib.rs` (Mode 0; only port-C bits 0-3 consumed `:78-82`).
- Runtime: `crates/runtime-msx/src/runtime.rs` — mapper hardwire `:279-284`,
  rebuild `:204-227`; profiles `crates/runtime-msx/src/profiles.rs:80-95` (two
  cartridge slots, no disk slot); snapshot `crates/runtime-msx/src/snapshot.rs`
  (bootstrap-only).
- Binary: `crates/emu198x-msx/src/{main,script}.rs` (headless; `--mapper` flag
  `script.rs:114-122`).
- Tests (all pass; two `#[ignore]`): `crates/machine-msx/tests/bios_boot.rs`
  (C-BIOS-gated `:46`), `crates/runtime-msx/tests/debug_target.rs` (BIOS-gated `:22`).
- Shared chips (defects filed in their own plans): `crates/ti-tms9918/src/lib.rs`,
  `crates/gi-ay-3-8910/src/lib.rs`, `crates/gi-ay-3-8912/src/lib.rs`,
  `crates/zilog-z80/src/z80.rs`.
- Reference: `reference/by-system/msx/msx-reference.md` (M1 wait `:140`, cassette
  `:584-585,642,704`, disk `:736-752`, subslot/mapper-RAM `:699-700`),
  `reference/by-system/msx/formats.md` (CAS `:244`, DSK `:336`, ROM header/mapper).
- No `knowledge/systems/msx*` distillation and no `knowledge/chips/` TMS9918 doc
  exist yet (the latter flagged in the TMS9918 chip findings).
