---
title: "plan: Nintendo Entertainment System to 100% — bugs, library breadth, cycle-exact finish, expansion audio"
type: plan
date: 2026-06-08
system: docs/systems/nintendo/nes.md
basis: four code-grounded assessments (CPU/DMA/interrupts, PPU, APU, mappers/formats/peripherals) with live test runs, 2026-06-08
---

# Nintendo Entertainment System — road to 100%

What it would take to bring the NES to feature- and accuracy-complete, grounded in
a code-level survey **with live test-suite runs** (Tom Harte 2.56 M, nestest 8991,
the full 155-ROM blargg sweep, every PPU/APU torture suite). The system doc was
accurate and needed only light correction; this plan is the forward view.

## Executive summary

**The NES is the most finished core in the fleet — and the work that remains is
almost entirely breadth, plus two real bugs.** This is a third distinct shape:

- The **Spectrum** had a finished core and cheap, front-loaded breadth.
- The **C64** plays its library but hides a hard core-accuracy long pole (the
  VIC-II rewrite).
- The **NES** has *both* a finished core **and** the breadth basics are cheap —
  but it carries a couple of genuine correctness bugs that, per the project's
  "bugs before features" rule, come first.

The three chips are essentially done:

- **CPU (2A03):** at the ceiling. Tom Harte 2,560,000/2,560,000, nestest 8991/8991,
  every `instr_test`/`instr_timing`/`cpu_interrupts_v2` suite green. Decimal
  correctly disabled; the full illegal + unstable opcode set modelled. **No work.**
- **PPU (2C02):** dot-exact. Passes blargg_ppu 18/18, ppu_onscreen 22/22,
  vbl_nmi_timing 7/7, ppu_open_bus — sprite-0, the sprite-overflow hardware
  diagonal bug, the $2002 vblank-read NMI-suppression race, odd-frame skip, full
  loopy v/t/x scroll. **NTSC is at the ceiling.**
- **APU:** at the ceiling. Passes the entire blargg APU suite (apu_test 8/8,
  blargg_apu 11/11, apu_reset 6/6, apu_mixer 4/4) with the exact nonlinear mixer.
  **No core work** — only the shared DMA-interleave item below.

So "100% NES" is **breadth + two bugs + a one-week cycle-exact cleanup**, not a
core rewrite.

**Totals (focused work):**

| Tier | Scope | Estimate |
|------|-------|----------|
| 0 — Bugs (first, per hard rules) | MMC5 `$4020–$5FFF` read routing; battery `.sav` persistence | **~3–5 days** |
| 1 — Library coverage (cheap, high yield) | MMC2/4, GxROM, PAL selectable, NES 2.0 completion, Zapper + Four Score | **~2–3 weeks** |
| 2 — Cycle-exact core finish | the 3 timing ROMs (DMA interleave, cpu_timing_test6, cpu_test5 01-implied) + reset write-ignore window + niche PPU polish | **~1.5–2 weeks** |
| 3 — Expansion audio + preservation | VRC6, Sunsoft 5B, VRC4 IRQ, Namco 163, VRC7 FM, FDS, UNIF, niche controllers | **~6–9 weeks** |

**True 100% of everything ≈ 11–16 weeks** — the *least* of the three core
platforms, because the core is already done. But it is **back-loaded**: Tier 3
(expansion-audio chips + FDS) is the bulk and is mostly JP/niche/preservation.
The **launch-relevant + "feels complete" slice (Tiers 0+1+2) is ~5–7 weeks** and
buys >90% library coverage, PAL, a light gun, 4-player, and a cycle-exact core —
the cheapest "feels finished" of any platform so far.

Effort key: **S** = hours · **M** = a few days · **L** = 1–2 weeks · **XL** = multi-week.

## Tier 0 — Bugs (do first)

| Item | Effort | Notes |
|------|--------|-------|
| **MMC5 register-read routing** | **S** | The machine returns open-bus for `$4020–$5FFF` and never calls `mapper.cpu_read` there (`machine-nintendo-nes/src/lib.rs:528`, flagged in-code). MMC5's IRQ-status, multiplier and ExRAM *reads* are dead; writes work. Unblocks an already-written mapper (SMB3-class engines, Castlevania III US). Full MMC5 validation after the fix is +M. |
| **Battery `.sav` persistence** | **S–M** | `has_battery` is parsed and PRG-RAM is battery-backed in the mappers, but nothing loads/flushes a `.sav`. Add load-on-insert + flush-on-exit/eject. Every battery RPG is unplayable-as-intended without it. |

## Tier 1 — Library coverage (cheap, high yield)

| Item | Effort | Notes |
|------|--------|-------|
| **MMC2 (9) + MMC4 (10)** | **S** each | Not implemented (the trait already supports the CHR latch they need). Punch-Out!!, Fire Emblem, Famicom Wars. |
| **GxROM (66)** | **S** | Missing. Mid-tail Western carts (Dragon Power, combo carts). |
| **PAL/Dendy selectable** | **S–M** | Chips are half-built (APU has PAL tables, PPU takes 311 lines) but the machine hardwires NTSC + a 3:1 divider; PAL needs 3.2:1, 70-line vblank, no odd-frame skip. Plumb a region arg through `Nes::new` → APU/PPU, add a `Model::NesPal` profile + CLI flag, read the iNES region byte. The whole PAL library runs at wrong speed today. |
| **NES 2.0 header completion** | **S–M** | Parser folds the 12-bit mapper + extended sizes but ignores submapper, the region/TV byte, and RAM-size bytes. Completing it feeds the PAL selector and disambiguates mapper variants. |
| **Zapper light gun** | **M** | Light-sensor + trigger on `$4017` bits 3–4; needs PPU pixel-brightness sampling. Duck Hunt-class titles. |
| **Four Score (4-player)** | **S–M** | The 8+bit serial signature shift on the controller ports. |

Tier 0+1 together lift playable coverage past ~90% and add PAL, a light gun, and
4-player — the bulk of the "feels complete" value.

## Tier 2 — Cycle-exact core finish

All three failing core ROMs need a **cycle-by-cycle Mesen2/FCEUX reference trace**
to localise the divergence — the existing handoff
(`docs/handoffs/2026-05-30-nes-official-cpu-test5-investigation.md`) flags this and
warns against guessing.

| Item | Effort | Notes |
|------|--------|-------|
| **`sprdma_and_dmc_dma` interleave** | **M** | The headline DMA frontier. The exact clock count when DMC DMA collides with OAM DMA is off in specific alignment cases (the 513-vs-514 OAMDMA odd-cycle penalty + the DMC mid-OAM stall count). Machine-layer (`lib.rs:393–457`), not the APU. |
| **`cpu_timing_test6`** | **M** | Frame-relative instruction timing; since `instr_timing`/`branch_timing` pass, this is a CPU↔PPU phase-alignment or page-cross-dummy edge. Likely shares a root with the next item. |
| **`cpu_test5` 01-implied** | **M** | 10/11 sub-tests pass; one implied/NOP opcode side-effect fails the stricter CRC (standalone `instr_test` 01-implied passes, so it's a flag/open-bus subtlety). |
| **Reset write-ignore window** | **S** | The ~2-frame post-reset PPUCTRL/MASK/SCROLL/ADDR lockout (~29 658 cycles) is unmodelled. No in-repo ROM exercises it; a few reset-sensitive titles. |
| **Per-cycle sprite pattern fetch + OAMADDR quirks** | **S / M** | Sprite patterns are fetched batched at dot 257 rather than spread 257–320 (A12 is already driven correctly per-dot, so MMC3 timing is unaffected — cosmetic). OAMADDR sprite-eval-offset + `$2002`-read-near-vblank glitches are niche; oam_read/oam_stress already pass. |

## Tier 3 — Expansion audio + preservation breadth (back-loaded)

The APU exposes a single additive `expansion_audio` hook, so each chip is
self-contained per-mapper work with **no APU-core changes**. Only MMC5 audio
exists today.

| Item | Effort | Notes |
|------|--------|-------|
| **VRC4 IRQ + banking** | **M** | Cycle-counter IRQ + banking; several Konami JP titles. |
| **VRC6 audio** (2 pulse + saw) | **M** | New mapper + 3 channels. Castlevania III JP (Akumajou Densetsu). The highest-value first expansion-audio target. |
| **Sunsoft 5B** (69) | **M** | YM2149 (AY-3-8910) 3-square audio + IRQ — reuse an AY core if the fleet has one. Gimmick!. |
| **Namco 163** (19) | **L** | Wavetable, up to 8 time-multiplexed channels + IRQ + expansion audio. |
| **VRC7 FM** (85) | **XL** | Full Yamaha OPLL 6-channel FM synthesis. Lagrange Point. The heaviest single item. |
| **FDS — Famicom Disk System** | **XL** | `.fds` disk format + disk-drive state machine + FDS BIOS + the FDS wavetable/mod audio channel. JP-heavy preservation; no `MediaKind::Disk` for NES today. |
| **UNIF parsing** | **M** | Mostly homebrew/pirate; preservation, not "runs the library." |
| **Power Pad, Arkanoid paddle, R.O.B.** | **S** each | Niche. |

## Done as part of this plan (free, ~half a day)

System-doc touch-up (the doc was accurate — only two stale items): the `cpu_test5`
"2/20 CRC probe" figure was stale (it's **10/11**); `cpu_timing_test6` was called
"protocol not modelled" when it runs and reports a real `$F0` fail. Added the three
newly-surfaced items the doc lacked: the **MMC5 read-routing bug**, absent
**`.sav` persistence**, and **PAL unwired**.

## Recommended sequence (highest leverage first)

1. **MMC5 read routing** (S) + **`.sav` persistence** (S–M) — bugs first; both are
   correctness defects with real game impact.
2. **MMC2/4 + GxROM** (S×3) — marquee Western titles for almost no effort.
3. **PAL selectable + NES 2.0 region byte** (S–M) — a whole library at the right
   speed; the chips already do the hard part.
4. **Zapper + Four Score** (M + S–M) — light gun + 4-player round out peripherals.
5. **The 3 cycle-exact timing ROMs** (M×3, trace-driven) — capture the Mesen2
   reference traces first, then close DMA interleave + the two CPU-timing cases.
6. **Reset window + PPU polish** (S/S/M) — completeness.
7. **VRC6 → Sunsoft 5B → VRC4** (M×3) — the affordable expansion-audio wins.
8. **Namco 163 (L) → VRC7 FM (XL) → FDS (XL) → UNIF (M)** — the preservation tail.

## Key files

- CPU (at ceiling): `crates/mos-6502/src/{lib,cycle,tick}.rs`, `crates/mos-6502/tests/single_step_tests.rs`, `crates/machine-nintendo-nes/tests/{nestest,cpu_test5_probe}.rs`.
- PPU (NTSC at ceiling; PAL wiring): `crates/ricoh-ppu-2c02/src/lib.rs`, `crates/machine-nintendo-nes/src/lib.rs:209` + `:270` (NTSC hardwire / divider), `crates/runtime-nintendo-nes/src/profiles.rs:54`.
- APU (at ceiling; expansion hook): `crates/ricoh-apu-2a03/src/lib.rs` (DMC `:716–863`, mixer `:1636`, `expansion_audio` `:995`).
- DMA (the one core frontier): `crates/machine-nintendo-nes/src/lib.rs:393–457`.
- Mappers + bug + formats: `crates/format-nintendo-nes-ines/src/format.rs:144` (dispatch), `src/mappers/` (per-mapper), MMC5 read-routing gap at `crates/machine-nintendo-nes/src/lib.rs:528`.
- Input/peripherals: `crates/runtime-nintendo-nes/src/input.rs:34`, `crates/machine-nintendo-nes/src/lib.rs:491–517`.
- Tests: `crates/machine-nintendo-nes/tests/{nes_sweep,blargg_ppu,ppu_onscreen,nestest}.rs`.
- Reference: Mesen2, fceux, nestopia (`emulators/nes/`); handoff `docs/handoffs/2026-05-30-nes-official-cpu-test5-investigation.md`.
