> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.

---
title: "plan: Atari 7800 ProSystem to 100% — MARIA graphics-mode completeness, TIA audio, cartridge + peripheral breadth"
type: plan
date: 2026-06-09
system: docs/systems/atari/atari-7800.md
basis: code-grounded survey of machine-atari-7800, atari-maria, atari-pokey, runtime-atari-7800, the shared mos-6502/atari-tia/atari-pokey chip assessments, and live test runs (machine 19/19, MARIA 15/15, POKEY 14/14, runtime 2/2), 2026-06-09
---

# Atari 7800 ProSystem — road to 100%

What it would take to bring the Atari 7800 to feature- and accuracy-complete,
grounded in a code-level survey of the actual crates and a live test run (not doc
prose). The machine **boots, renders, and is drivable today** — the recent
black-screen fix and MARIA rewrite were real. The remaining work is a video long
pole plus two large silent gaps.

## Executive summary

**The 7800 is a fourth distinct shape from the launch cores.** It is *past* the
"does it boot at all" stage — the MARIA CTRL-bit black-screen bug is fixed
(`machine-atari-7800/src/lib.rs`, confirmed in `docs/status/outstanding-work.md:361`),
Asteroids/Centipede render and play, the joystick + fire buttons are bit-exact vs
MAME (`drivability-assessment.md:277`), and the CPU underneath is the externally
verified `mos-6502` core at ceiling (Tom Harte 2.56M/2.56M — see shared 6502
finding). So the basics are in hand.

What's left splits three ways, and unlike the C64 the long pole is **video, not
the CPU**:

- **The long pole is MARIA graphics-mode completeness.** Real MARIA selects among
  six modes (160A/160B/320A/320B/320C/320D) from `CTRL.RM` combined with the
  per-entry WM bit. The crate **never reads `CTRL.RM`** (shared MARIA finding;
  self-admitted at `atari-maria/src/lib.rs:71` "160B/320B/C/D variants exist but
  are not yet implemented") — so only 160A and 320A render. Games using the common
  160B high-colour mode, or 320B/C/D, get wrong colours and pixel packing. Plus
  **Kangaroo (transparency-off) mode is unimplemented** — pixel value 0 stays
  transparent where hardware draws it opaque. These are the two headline video
  gaps that stop the library rendering *correctly* (not just *at all*).

- **Audio is fully silent.** The 7800 uses the TIA for sound in native mode; the
  `machine-atari-7800/src/tia_audio.rs` "TIA" is a register file plus controller
  buttons — six audio registers (`AUDC/AUDF/AUDV`) are stored and never
  synthesised (`tia_audio.rs:31-41`). The runtime pushes an **empty** audio buffer
  every frame (`runtime.rs:181-187`, "TIA audio not yet exposed"). No game makes a
  sound. Note: the machine wires a bespoke `TiaAudio` stub, **not** the shared
  `atari-tia` crate — so the 2600's (also stubbed) TIA audio and this are two
  separate silent paths.

- **Breadth is thin.** Cartridge support is Flat (16/32/48 KB) + SuperGame only
  (`cartridge.rs:24-69`); the A78 header is detected and stripped but its
  mapper-type bytes are **not parsed** (`cartridge.rs:71-82`), so SuperGame-RAM,
  POKEY-on-cart, Activision, Absolute, and bankset schemes are unsupported. The
  BIOS overlay is absent (acceptable — games boot from their own vector). Input is
  P0-only (`input.rs:11,63`; no second controller, no keypad). Snapshot is a
  **cold** envelope that re-boots rather than restoring live state
  (`snapshot.rs:11-16` stores only time + model + cart bytes).

That makes the 7800's "feels finished" slice **MARIA modes + TIA audio** — both
audible/visible to a learner on the first game they write. The CPU needs **no
work** (shared 6502 finding: at ceiling, NMOS variant, no 65C02 needed).

**Totals (focused work):**

| Tier | Scope | Estimate |
|------|-------|----------|
| A — "Curriculum 100%" | MARIA 160B/320B/C/D modes + Kangaroo, TIA audio synthesis, DMA cycle-cost accuracy, doc fixes | **~5–8 weeks** |
| B — Cycle-exact MARIA core | DMA per-mode startup/shutdown cost model, DLI/NMI cross-line edge timing, mid-frame BACKGRND border | **~3–5 weeks** |
| C — Cartridge + peripheral breadth | A78 header mapper parse, SuperGame-RAM / POKEY-on-cart / Absolute / Activision schemes, BIOS overlay, second controller + keypad | **~3–5 weeks** |
| D — Preservation / completeness | live-state snapshot, high-score cart (HSC), light-gun / paddle peripherals, region-palette audit | **~2–4 weeks** |

**True 100% of everything ≈ 13–22 weeks.** It is **front-loaded onto the
visible/audible wins** (Tier A): the two things a learner notices first (correct
MARIA colours and any sound at all) are also the highest-value. Tier B (cycle-exact
MARIA) is the genuinely hard depth work and is mostly invisible to non-demoscene
software. The launch-relevant slice is Tier A alone (~5–8 weeks).

Effort key: **S** = hours · **M** = a few days · **L** = 1–2 weeks · **XL** = multi-week.

## Tier A — "Curriculum 100%" (the visible/audible wins)

| Item | Effort | Notes |
|------|--------|-------|
| **MARIA `CTRL.RM` graphics-mode matrix (160B/320B/C/D)** | **L** | The headline video gap. `render_line`/`blit_byte` never read `ctrl & 0x03` (shared MARIA finding; self-admitted `atari-maria/src/lib.rs:71`). Implement the full `casex(RM)` × WM-bit mode select per MiSTer `line_ram.sv:74-162`: 160B (4-colour-from-2-bytes high colour, the common one), 320B/C/D pixel packing. Without it, high-colour games render wrong colours/packing. Build with a per-mode pixel-decode unit test. |
| **MARIA Kangaroo (transparency-off) mode** | **M** | `CTRL` bit 2 is masked and documented (`atari-maria/src/lib.rs:115`) but never applied; `blit_byte` always treats pixel 0 as transparent (`atari-maria/src/lib.rs:683,691`). Under Kangaroo, value 0 is opaque background. Games relying on it show holes. |
| **TIA audio synthesis** | **L** | The whole audible gap. `tia_audio.rs:31-41` stores `AUDC/AUDF/AUDV` and synthesises nothing; `runtime.rs:182-187` pushes an empty buffer. Build the two-channel TIA polynomial/pure-tone generator (the same model the 2600 TIA needs — shared `atari-tia` audio finding §1 cites tia-reference.md:314-345) and wire a 48 kHz output path into the runtime. Decide whether to grow the shared `atari-tia` crate's audio and consume it here, or keep the bespoke `TiaAudio` — currently two separate silent stubs (raise the dedup decision before building). |
| **MARIA DMA cycle-cost accuracy** | **M** | `dma_cycles` is a flat +N per byte/header with a `MAX_DMA_CYCLES_PER_LINE=512` cap (shared MARIA finding §3). The machine feeds it back as `dma_budget` to throttle the CPU (`machine-atari-7800/src/lib.rs:158,182`), so timing-sensitive code drifts. Add real per-mode DMA startup (fixed 5-cycle), shutdown, and header-vs-graphics costs. Riding the same MiSTer DMA.sv reference as Tier B. |
| **Doc + test-count fixes** | **S** | `outstanding-work.md:341` says "18/18 tests" — actual is **19/19** (verified live). The 7800 has no `knowledge/systems/` doc and MARIA has no `knowledge/chips/` doc; the bit-map fix and rewrite live only in source comments. Capture MARIA + the 7800 system into the knowledge layer. |

## Tier B — Cycle-exact MARIA core (the hard depth)

| Item | Effort | Notes |
|------|--------|-------|
| **DLI / NMI cross-line edge timing** | **M** | The machine does `self.cpu.nmi = self.maria.take_dli()` once per scanline (`machine-atari-7800/src/lib.rs:185`), overwriting the NMI line each line rather than modelling the edge the 6502 samples (shared MARIA finding §4). MiSTer fires DLI at the end of the zone's DMA (DMA.sv:117,308,385). Possible off-by-one-line / missed-NMI under back-to-back single-line zones. **needs-runtime-verification** against a DLI-timing test or reference trace. |
| **MARIA oracle / visual-diff harness** | **M** | Build a per-line comparator against a reference 7800 emulator (a7800/MAME, ProSystem) so the mode + DMA work is provable, not vibes — sibling of the C64 VIC-II oracle item. Build alongside Tier A, not after. |
| **Mid-frame BACKGRND / border accuracy** | **S–M** | `fill_border()` paints the whole framebuffer at frame start from current BACKGRND; mid-frame changes land on the *next* frame (shared MARIA finding §6, self-noted `atari-maria/src/lib.rs:776-779`). Acceptable GTIA-parity v1; not cycle-accurate. |
| **DMA-abort-at-end-of-line semantics** | **S** | Real MARIA aborts the display-list walk at line end; the crate caps at 512 cycles instead (`atari-maria/src/lib.rs` MAX_DMA_CYCLES_PER_LINE). Replace the cap with the true abort once the per-mode cost model lands. |

## Tier C — Cartridge + peripheral breadth

| Item | Effort | Notes |
|------|--------|-------|
| **A78 header mapper-type parse** | **S–M** | `strip_a78_header` (`cartridge.rs:71-82`) only checks the 4-byte magic and slices 128 bytes — it **ignores** the header's mapper/feature bytes. Parse them to drive scheme + RAM + POKEY selection instead of inferring banking from size alone. Tier-C prerequisite. |
| **SuperGame-RAM + POKEY-on-cart + Absolute/Activision schemes** | **M–L** | Only Flat + plain SuperGame exist (`cartridge.rs:24-27`). Add SuperGame with on-cart RAM (Ballblazer/Commando class), POKEY-on-cart (wire the existing `atari-pokey` crate, which is **not** instantiated by the 7800 today — confirmed in the shared POKEY finding), and the Absolute (F18 Hornet) + Activision (Double Dragon/Rampage) bank schemes. POKEY-on-cart also inherits the two confirmed POKEY defects (distortion table, 16-bit linked mode) — see shared finding. |
| **BIOS overlay** | **M** | The `$8000-$FFFF` BIOS overlay (INPTCTRL bit 2 disables it) is absent (`outstanding-work.md:388`). Authenticity, not a blocker — games boot from their own reset vector. Needs the encrypted-header signature check for full fidelity. |
| **Second controller (P1) + keypad** | **S–M** | Input is P0-only (`input.rs:11,63` "P1 port not yet exposed"; `drivability-assessment.md:119` "7800 consume no input" was pre-fix, P0 now wired). Add the second RIOT/TIA controller port and the keypad/console-key matrix. |

## Tier D — Preservation / completeness

| Item | Effort | Notes |
|------|--------|-------|
| **Live-state snapshot** | **M** | Snapshot is a cold envelope: `Atari7800RuntimeSnapshotV1` stores only `time` + `model_id` + `cart_bytes` and rebuilds the machine from scratch (`snapshot.rs:11-16,30-43`). It re-boots, it does not restore CPU/RAM/MARIA/RIOT state. Add real state serialisation across the chips (shared family deferral, `outstanding-work.md:394`). |
| **High-Score Cartridge (HSC)** | **S–M** | The 7800 HSC ($1000-$17FF NVRAM + BIOS) is unsupported; preservation/authenticity for score-saving titles. |
| **Light-gun + paddle peripherals** | **M** | XG-1 light gun and paddle controllers unsupported; niche library tail. |
| **Region-palette + lines-per-frame audit** | **S** | NTSC=263 / PAL=313 lines and the shared Stella palette are wired (`lib.rs:80-85`, `palette.rs`); confirm PAL colour fidelity and PAL frame timing against a reference once audio/video land. **needs-runtime-verification.** |

## Done as part of this plan (free, ~half a day)

Doc-drift fixes. `docs/status/outstanding-work.md:341` says the machine has
"18/18 tests" — the live run is **19/19** (plus MARIA 15/15, POKEY 14/14, runtime
2/2; `cart_boot` is `#[ignore]`, needs a ROM). The system summary in
`profiles.rs:55-57` calls it "BIOS-less in v1" and `support_tier:
SupportTier::Boots` — accurate, but `current-system-usability.md:79` still reads
"BIOS-driven boot pending", which the 2026-06-04 fix superseded (games boot from
their own vector, no BIOS needed). Correct that row. Capture a MARIA chip doc and a
7800 system doc into `knowledge/` — neither exists today, so the CTRL-bit fix and
the mode-matrix gap live only in source comments.

## Recommended sequence (highest leverage first)

1. **MARIA `CTRL.RM` mode matrix (160B first)** (L) — the single highest-leverage
   item; the common 160B high-colour mode is what makes real games look right.
2. **TIA audio synthesis** (L) — the other thing a learner notices instantly;
   currently total silence. Settle the shared-`atari-tia`-vs-bespoke decision first.
3. **MARIA Kangaroo mode** (M) — small, removes visible holes in games that use it.
4. **MARIA oracle/visual-diff harness** (M) — build the comparator *before* the
   DMA-cost and edge-timing depth work.
5. **MARIA DMA cycle-cost + DLI/NMI edge timing** (M + M) — the cycle-exact core;
   capture a reference trace first (per the "ask, don't guess" rule).
6. **A78 header parse → SuperGame-RAM / POKEY-on-cart / Absolute / Activision** (S–M
   → M–L) — the cartridge breadth that runs more of the library.
7. **Second controller + keypad** (S–M), **BIOS overlay** (M) — peripheral + authenticity.
8. **Live snapshot** (M), **HSC / light-gun / paddle** (S–M/M) — preservation tail.

## Key files

- CPU (at ceiling — no work): `crates/mos-6502/src/{lib,cycle,tick}.rs` (NMOS `M6502::new()`); see shared 6502 finding.
- MARIA (the long pole): `crates/atari-maria/src/lib.rs` (`render_line`, `blit_byte` at `:683,691`, the `RM`-unread mode select self-admitted at `:71`, Kangaroo mask at `:115`, DMA cost + `MAX_DMA_CYCLES_PER_LINE`, `fill_border` at `:776-779`), `crates/atari-maria/src/palette.rs`.
- Machine wiring: `crates/machine-atari-7800/src/lib.rs` (memory map `:188-264`, DMA-budget CPU throttle `:156-185`, per-line NMI overwrite `:185`).
- TIA audio (silent stub): `crates/machine-atari-7800/src/tia_audio.rs` (`:31-41` register-only writes), runtime empty buffer `crates/runtime-atari-7800/src/runtime.rs:181-187`.
- Cartridge: `crates/machine-atari-7800/src/cartridge.rs` (Flat + SuperGame only `:24-69`, A78 header detect-only `:71-82`).
- Input (P0-only): `crates/runtime-atari-7800/src/input.rs:11,63`.
- Snapshot (cold envelope): `crates/runtime-atari-7800/src/snapshot.rs:11-16,30-43`.
- POKEY (for cart audio): `crates/atari-pokey/src/lib.rs` — not yet instantiated by the 7800; carries two confirmed defects (shared POKEY finding).
- Tests: `crates/machine-atari-7800/tests/cart_boot.rs` (gated; `EMU198X_ATARI_7800_CART`), inline tests in machine (19), MARIA (15), POKEY (14), runtime (2).
- Status: `docs/status/outstanding-work.md:330-395`, `docs/status/current-system-usability.md:79`, `docs/status/drivability-assessment.md:277-301`.
- Reference: a7800/MAME (`maria.cpp`, `tia_r`), 7800 MiSTer core (`DMA.sv`, `line_ram.sv`); 7800-specific prose reference is thin under `198x/reference/by-system/atari-8bit/`.

