> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.

---
title: "plan: Dragon 32 to 100% — a near-finished core, an artifact-colour long pole, and a preservation tail"
type: plan
date: 2026-06-09
system: docs/systems/dragon/dragon-32.md
basis: code-grounded survey of every Dragon crate + live test runs (69 machine, 28 runtime, 10 golden_basic, chip crates all green), cross-checked against established 6809/MC6847 chip findings and reference/by-system/dragon-32, 2026-06-09
---

# Dragon 32 — road to 100%

What it would take to bring the Dragon 32 (and the already-wired Dragon 64) to
feature- and accuracy-complete, grounded in a code-level survey of the actual
crates and tests. The on-disk docs had drifted badly **understating** the system
(the knowledge doc still lists Dragon 64 mode, cartridge audio, and disk support
as "not done yet" — all three are implemented and tested); this plan corrects that
and gives the forward view.

## Executive summary

**The Dragon is one of the most finished cores in the fleet, and its shape is the
Spectrum's, not the C64's: a done core with a modest breadth tail — plus one
genuine accuracy long pole that lives in a shared chip.** This is the fourth
distinct shape across the platforms:

- The **CPU (MC6809)** is at the ceiling: a microstep, bus-cycle-accurate core
  with the full documented ISA, all 14 addressing modes, every interrupt, and
  87/87 tests green (established chip finding). **No CPU work** on the road to 100%.
- The **board** is real and complete, not a harness: two MC6821 PIAs, the MC6883
  SAM (with the slow/fast/address-dependent CPU-rate model actually consumed —
  `SamCycleTiming`, lib.rs:2786-2825), 32 KiB RAM + mirrored BASIC ROM, keyboard
  matrix, analogue joystick, and a faithful **XRoar-derived 6-source audio mux**
  (DAC / tape / cartridge / single-bit, with measured gain/offset tables,
  lib.rs:2341-2380).
- **Storage works in both directions:** the DragonDOS WD2797 controller does real
  read-sector, write-sector, write-track/format, with DRQ/INTRQ/NMI and an index
  pulse (lib.rs:493-770). VDK images round-trip. The outstanding-work doc's claim
  that "write paths need filling in" is **stale** — they are implemented and the
  69 machine tests pass.
- **Dragon 64 is already wired:** three hardware models (Dragon32, Dragon64Compat,
  Dragon64Mode), the ROM-select PIA path, 64K SAM paging, and a golden test that
  proves `EXEC 48000` enters 64K mode (golden_basic.rs:71).

So "100% Dragon" is **not a core rewrite**. What remains:

1. **The long pole is shared, not Dragon-specific:** the MC6847 has no RG6
   **artifact-colour** model (established chip finding). Dragon software leans on
   RG6 artifacting for 4-colour-from-2-colour effects, so this is the single
   biggest *visible-output* gap — but it is filed at the chip level, not here.
2. **One real Dragon-specific stub:** the Dragon 64 **6551 ACIA** is a hardcoded
   status byte with dropped writes (lib.rs:4329-4334). The only board device that
   is a stub rather than a model.
3. **Preservation breadth:** disk is **VDK-only** (no DMK/JVC raw-track, no
   protections); cassette is LOAD-only (no CSAVE writeback); cartridge covers
   plain ROM + Games Master banking but not the wider banked/active cart space.

**Totals (focused work):**

| Tier | Scope | Estimate |
|------|-------|----------|
| A — "Curriculum 100%" | doc-drift correction (free), real-ROM + VDK software smoke harness, ACIA promoted from stub to a usable status model | **~1–2 weeks** |
| B — Accuracy depth | MC6847 artifact-colour + beam/sub-line composition (**chip-level, filed elsewhere**); Dragon-side: verify VDG mode-change sampling and SAM/VDG address timing against real software | **~1 week Dragon-side** (the chip work is the long pole, tracked on the MC6847) |
| C — Audio/timing fidelity | DragonDOS controller timing validated against real DragonDOS ROM; joystick pot-read timing; audio-mux provenance check | **~1–2 weeks** |
| D — Preservation breadth | full 6551 ACIA, cassette SAVE, DMK/JVC disk + protections, wider cartridge banking, Dragon 64 serial peripherals | **~3–5 weeks** |

**True 100% of everything ≈ 6–10 weeks of Dragon-specific work**, plus the
shared MC6847 artifact/beam rewrite (the genuine long pole, but counted against
the chip, where the Atom also benefits). Like the Spectrum, the Dragon's
remaining work is **front-loadable and cheap** — the hard core is already done.
The launch-irrelevant note applies: the Dragon is explicitly **out of October
scope** (knowledge/systems/dragon-32.md), so this is the post-October engineering
bar, not a deadline.

Effort key: **S** = hours · **M** = a few days · **L** = 1–2 weeks · **XL** = multi-week.

## Tier A — "Curriculum 100%" (cheap, high yield)

| Item | Effort | Notes |
|------|--------|-------|
| **Real-ROM + VDK software smoke harness** | **M** | The golden_basic tests gate real-software behaviour behind ROM availability ("when available", golden_basic.rs:196-355). DragonDOS read+write works in unit tests but has never been driven by a real DragonDOS ROM over a VDK software sweep. Build the smoke harness (mirroring the existing CAS/PAK XRoar-comparison smokes) so controller behaviour is proven against real software, not just synthetic register pokes. |
| **ACIA: stub → usable status model** | **S–M** | `acia_read` returns only a fixed transmit-empty byte and drops writes (lib.rs:4329-4334, 2081-2087). Promote to a minimal 6551 status/control/data model so Dragon 64 serial software at least reads coherent status. Full ACIA is Tier D; this is the "doesn't read garbage" floor. |
| **Doc-drift correction** | **S (free)** | See "Done as part of this plan". |

## Tier B — Accuracy depth (the long pole is shared)

| Item | Effort | Notes |
|------|--------|-------|
| **MC6847 RG6 artifact colours** | **chip-level (XL), filed on the MC6847** | The densest 2-colour graphics mode is rendered strict fg/bg with no colourburst-suppression artifact model. Dragon software depends on RG6 artifacting. This is the single biggest visible-output gap but it is a **chip concern** shared with the Atom — not re-filed here. The Dragon-side dependency: once the chip models artifacts, re-verify the Dragon's CSS pipeline (lib.rs:2398-2401) feeds the artifact path correctly. |
| **Beam/sub-line VDG composition** | **chip-level, Dragon verification S–M** | The machine already prefetches per-byte and runs a two-byte CSS pipeline (`BeamVideo`, lib.rs:2383-2403) — **more beam-accurate than the knowledge doc admits**. Verify mid-scanline A/G, GM, and INT/EXT mode changes reproduce against XRoar on demo software; tighten the Dragon's per-line mode sampling if the chip gains sub-line granularity. |
| **SAM/VDG display-address timing** | **S** | `SamCycleTiming` models the slow/fast/address-dependent rate and the VDG fetch-clock split (lib.rs:2876-2883). Cross-check the long/short-cycle boundaries and the display-base latch against real software that switches CPU rate mid-frame. |

## Tier C — Controller & input timing fidelity

| Item | Effort | Notes |
|------|--------|-------|
| **DragonDOS controller timing vs real ROM** | **M** | The WD2797 byte-pacing, index period, and command-complete delays (`DRAGON_DOS_BYTE_CYCLES` etc.) are plausible but unvalidated against the real DragonDOS ROM's expectations across a software sweep. Ride the Tier-A smoke harness to confirm DRQ/INTRQ timing carries real loaders. |
| **Joystick pot-read timing** | **S** | `joystick_threshold_from_dac` quantises the DAC to `(v & 0xFC) | 0x02` for the comparator (lib.rs:133). Verify against real Dragon pot-read timing across the full axis range. |
| **Audio-mux provenance** | **S** | The gain/offset tables (lib.rs:84-125) are XRoar-derived; cite the XRoar source/version in-repo so the level model is provable, not vibes. |

## Tier D — Preservation breadth (the tail)

| Item | Effort | Notes |
|------|--------|-------|
| **Full 6551 ACIA** | **M** | Real RX/TX, control/command registers, baud divisor, IRQ — beyond the Tier-A status floor. Dragon 64 serial peripherals (modems, printers). |
| **Cassette SAVE (CSAVE/CSAVEM)** | **M** | The tape slot is `InMemoryOnly` and the cassette model is LOAD-side (`line_level`). Add a write/encode path + CAS writer + writable mount, mirroring the C64 disk-save-write-back decision. |
| **DMK / JVC disk + protections** | **L** | Disk is VDK-only (format-dragon-disk). DMK is the raw-track format copy-protected originals need; JVC is the common CoCo-side container. Each is a parser + the controller's existing read/write path. |
| **Wider cartridge banking** | **M** | Plain ROM + Games Master 16K banking exist (lib.rs:772-810); the broader Dragon/CoCo banked-cart and any register-driven carts are the long tail. |
| **Dragon 64 serial peripherals** | **M** | Rides the full ACIA: the actual devices Dragon 64 owners attached. |

## Done as part of this plan (free, ~half a day)

Knowledge-doc drift eradicated. `knowledge/systems/dragon-32.md` "Notably not done
yet" lists four items that are **all done and tested**: **Dragon 64 mode**
(three hardware models + `EXEC 48000` golden test, golden_basic.rs:71),
**cartridge audio** (`DragonAudioSource::Cart` in the mux, lib.rs:2367 +
`set_cartridge_sound_level`), **disk support** (full DragonDOS WD2797 read+write,
lib.rs:493-770), and the "line-accurate per scanline" renderer claim (the
`BeamVideo` path actually prefetches per-byte with a CSS pipeline). The
outstanding-work doc's "DragonDOS write paths need filling in" (outstanding-work.md:138)
is likewise stale — writes are implemented. The status/usability rows should be
re-anchored to "near-complete core, artifact-colour + ACIA the remaining gaps".

## Recommended sequence (highest leverage first)

1. **Doc-drift correction** (S, free) — stop the docs lying about what's done.
2. **Real-ROM + VDK software smoke harness** (M) — proves the DragonDOS
   read+write path against real software; the highest-confidence-per-week item and
   the prerequisite for Tier C validation.
3. **ACIA status floor** (S–M) — the one Dragon-specific stub; cheap to make
   coherent.
4. **MC6847 artifact colours** (chip-level, tracked on the MC6847) — the genuine
   long pole and the biggest visible-output win; the Dragon's CSS-pipeline
   re-verification (S) rides it.
5. **Controller-timing + joystick + audio-provenance validation** (M + S + S) —
   close the timing/fidelity nuances once real software is driving them.
6. **Cassette SAVE** (M), then **full ACIA** (M) — preservation mid-tail.
7. **DMK/JVC + protections** (L), **wider cartridge banking** (M) — the
   completionist tail.

## Key files

- CPU (at ceiling): `crates/motorola-6809/src/lib.rs` (87 tests; established chip finding — no work).
- Machine wiring / memory map / devices: `crates/machine-dragon-32/src/lib.rs` (`read_bus`/`write` :2017-2138, `decode_pia`/`decode_acia`/`decode_dragon_dos`/`decode_device_write` :4307-4341, `DeviceRegion` :1466-1480).
- ACIA stub (the one Dragon-specific stub): `crates/machine-dragon-32/src/lib.rs:4329-4334` (`acia_read`) + write drop at `:2081-2087`.
- DragonDOS WD2797 (real read+write, doc says otherwise): `crates/machine-dragon-32/src/lib.rs:493-770`.
- SAM rate/display model: `crates/motorola-sam-6883/src/lib.rs`; consumed at `crates/machine-dragon-32/src/lib.rs:2786-2825` (`SamCycleTiming`) + `:2876-2905` (VDG fetch/address).
- VDG (chip-level artifact/beam gap): `crates/motorola-vdg-6847/src/lib.rs`; Dragon beam path `crates/machine-dragon-32/src/lib.rs:2383-2403` (`BeamVideo`, CSS pipeline).
- Audio mux (XRoar-derived): `crates/machine-dragon-32/src/lib.rs:84-125` (tables), `:2341-2380` (mux).
- Cartridge + GMC banking: `crates/machine-dragon-32/src/lib.rs:772-810`; format `crates/format-dragon-pak/src/lib.rs`.
- Dragon 64 modes: `DragonHardwareModel` `crates/machine-dragon-32/src/lib.rs:74-82`; golden `crates/runtime-dragon/tests/golden_basic.rs:71`.
- Media formats: `crates/format-dragon-{cas,disk,bin,pak}/src/lib.rs` (CAS / VDK / DragonDOS .BIN / PAK-snapshot+cart).
- Runtime + profiles: `crates/runtime-dragon/src/{runtime.rs,profiles.rs}` (Dragon32Pal + Dragon64Pal).
- Tests: `crates/runtime-dragon/tests/golden_basic.rs` (real-ROM gated "when available"), `crates/machine-dragon-32/src/lib.rs` test module (69 tests).
- Reference: `reference/by-system/dragon-32/{dragon32-reference.md,insidethedragon.md}`; XRoar (`emulators/dragon-coco/`).
- Docs to re-anchor: `knowledge/systems/dragon-32.md`, `docs/status/outstanding-work.md:132-142`, `docs/status/current-system-usability.md:51`.
