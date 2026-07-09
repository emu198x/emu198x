# Screenshot oracle policy — design spec

> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.


**Date:** 2026-07-08
**Status:** Design approved (brainstormed in the 198x umbrella session);
implementation belongs to the Emu198x session.
**Deliverables:** one decision record, one process doc, provenance
sidecars for existing goldens, and a first wave of tier-2 visual test
suites.

## Problem

Golden-frame tests exist for a handful of systems (Spectrum family,
Timex, Pentagon, Dragon, Amiga, TIA), but a golden PNG hides what it
proves. A frame captured from FS-UAE proves *agreement with FS-UAE*,
not accuracy. With ~44 machine crates and more coming, we need:

1. A definition of what "accurate" means for video output.
2. A scalable way to source reference images across the roster.
3. Honest labelling of what each existing and future golden proves.

## The definition of "accurate"

Two moves, both extending existing project patterns:

**Accuracy claims stop at the digital framebuffer** (`CapturedFrame`).

- *Pixel geometry* — which pixels the video chip emits — is a digital
  fact, provable pixel-exact.
- *Palette* is a cited per-system choice, not a digital fact. The
  VIC-II emits luma/chroma; "the C64 palette" is published research
  (Pepto / Colodore), chosen and documented with provenance.
- *Display-chain effects* (composite artefacts, PAL blending, CRT
  bloom) are rendering features, explicitly outside accuracy scope.
- Comparisons happen post-conversion in RGBA space, so indexed and
  ARGB cores test identically (per the framebuffer-pixel-format
  decision: per-chip choice, invisible downstream).

**Closest to the actual silicon wins** — the video analogue of the
CPU oracle-priority decisions. Every reference image carries a
provenance tier; "accurate" means matching the highest-tier oracle
available for that system.

## The provenance ladder

1. **Real-hardware capture** — digital RGB capture of a real machine.
   Community-sourced; we own no capture path.
2. **Hardware-validated test-ROM reference images** — images the
   test-ROM author verified against real silicon (dmg-acid2,
   mealybug-tearoom per-revision captures, VICE testprogs'
   real-hardware screenshots).
3. **Reference-emulator capture** — FS-UAE, VICE, Mesen2, FUSE etc.,
   with emulator + version + config recorded. Proves *agreement*, not
   accuracy; the sidecar says so plainly.
4. **Self-minted golden** — our own output, frozen. Proves *stability
   only*. Legitimate and cheap; can never adjudicate a correctness
   dispute.

**Tier 0 (qualitative, outside the numbered ladder):** CRT photos of
Steve's own machines — admissible for adjudicating palette family,
geometry, and artefacts by eye; never pixel-exact.

**Adjudication rule:** when tiers disagree, higher tier wins.
Disagreements with a lower tier get recorded (the FUSE
accepted-disagreement table is the precedent). Per-system
oracle-priority decisions may refine the ladder locally, exactly as
`spectrum-test-oracle-priority.md` does for CPUs.

## Chip-keyed sourcing

Reference material is collected **per video chip, not per machine** —
the same leverage move as Asm198x's CPU-keyed ISA specs. Roughly a
dozen chips sit behind the 44 machines; one TMS9918 reference set
lights up MSX, ColecoVision, SG-1000, Sord M5, Einstein, SVI-328…

## Deliverable 1 — decision record

`knowledge/decisions/screenshot-oracle-policy.md` (naming mirrors
`test-rom-policy.md` and the `*-test-oracle-priority.md` family).
Contains: the ladder, the framebuffer boundary, the chip-keyed
sourcing principle, the adjudication rule, licensing stance (defers
entirely to `test-rom-policy.md` tiers — no new licensing policy),
and drift triggers:

- *"The golden passes so we're accurate"* — tier 3/4 goldens cannot
  prove accuracy; check the sidecar tier before claiming it.
- *"Just crop/scale the capture to fit"* — forbidden; reconfigure the
  source instead (per the existing FS-UAE capture doc).
- *"Bundle these reference PNGs, they're tiny"* — licensing follows
  `test-rom-policy.md` unchanged; reference images live under the same
  env-var roots as the ROMs they ship with.
- *"Replace the golden, the new output looks right"* — a tier-2+
  mismatch is presumed our bug; a tier-3 mismatch opens an
  adjudication question, not an automatic golden replacement.

## Deliverable 2 — provenance sidecars

Every golden PNG gets `<stem>.provenance.toml` beside it:

```toml
tier = 3                    # 1-4 per the ladder; 0 = qualitative
source = "FS-UAE 3.1.66"    # or repo+commit, or capture credit
config = "A500, KS 1.3, 512k chip"  # what produced the frame
capture = "frame 250 from reset"    # determinism pin (+ input script ref)
verified = "2026-07-08"
notes = ""                  # known deltas, palette cited, etc.
```

Human-written documentation; the comparison helpers do **not** parse
it. A lint that every golden has a sidecar can come later if drift
appears. No test changes in this step.

**Retrofit pass:** one sweep over existing golden directories
(`machine-sinclair-zx-spectrum-{16k,48k,128k,…}`, `machine-timex-*`,
`machine-pentagon-128`, `runtime-sinclair-zx-spectrum`,
`runtime-dragon`, `runtime-commodore-amiga`, `atari-tia`) writing
honest sidecars — most will be tier 3 (FS-UAE per
`knowledge/processes/golden-image-capture.md`) or tier 4
(self-minted). Zero regression risk; this only makes existing claims
explicit. Where the original source is unrecoverable, say so in
`notes` and label tier 4.

## Deliverable 3 — chip-keyed source map

`knowledge/processes/screenshot-sources.md` — per video chip: known
tier-2 corpora, scriptable reference emulators with capture recipes
(extending the FS-UAE doc's pattern), and gaps. Initial entries:

| Chip | Tier-2 corpora | Tier-3 capture route |
|---|---|---|
| Game Boy PPU | dmg-acid2, cgb-acid2, mealybug-tearoom (MIT, per-revision hardware captures) | SameBoy |
| MOS VIC-II | VICE testprogs (incl. real-hardware reference screenshots) | headless VICE (`-exitscreenshot`) |
| Ferranti ULA family | — (Rak/FUSE suites are CRC-based, not visual) | FUSE / ZEsarUX capture; RZX replays |
| TI TMS9918 | TMS9918 test suites (survey needed) | openMSX / blueMSX / ares |
| Ricoh PPU-2C02 | 240p test suite; PPU tests (most Blargg self-check) | Mesen2 (Lua screenshot) |
| Atari TIA | — (survey needed) | Stella |
| Sega VDP | SMS VDP test ROMs | Mesen2 / ares |
| Amiga OCS/ECS/AGA | — | FS-UAE (existing capture doc) |

Survey/verify each row during implementation — the table above is the
starting hypothesis, not settled fact.

## Deliverable 4 — first tier-2 wave

Wire the image-shipping suites into the harness:

- **Game Boy:** dmg-acid2 (+ mealybug subset) — load ROM *and*
  reference PNG from the existing `EMU198X_GB_*` env-var roots,
  skip-if-missing, consistent with `test-rom-policy.md`
  (referenced-not-bundled).
- **C64:** a VICE testprogs subset with real-hardware reference
  screenshots, same env-var pattern (`EMU198X_C64_TESTPROGS_ROOT`).

Reference images may need normalisation to our capture geometry
(viewport, scale) — normalise via documented reproducible steps in
the sidecar, never by cropping goldens ad hoc.

## Commercial + demoscene content

Captures of commercial software follow the media itself: env-var
gated, stored locally, never in the public repo (same posture as the
Manic Miner TZX tests). Demoscene productions with free-redistribution
licences may be repo-eligible case by case, but default to the same
env-var pattern for uniformity. Determinism pinned via the existing
scripted-input + frame-count machinery.

## Error handling

Mismatch behaviour stays as-is (`.actual.png` + magenta `.diff.png`,
git-ignored). The policy adds interpretation: tier-2+ mismatch →
presumed our bug; tier-3 mismatch → adjudication question; tier-4
mismatch → intentional-change check, replace golden if so.

## Testing the design

The retrofit pass plus one tier-2 suite (dmg-acid2) wired end-to-end
proves the env-var + reference-image path. Existing golden tests must
stay green throughout (sidecars are inert).

## Implementation order

1. Decision record (`knowledge/decisions/screenshot-oracle-policy.md`).
2. Sidecar retrofit over existing goldens.
3. Source-map process doc (`knowledge/processes/screenshot-sources.md`).
4. dmg-acid2 wave (Game Boy), then VICE testprogs wave (C64).
5. Subsequent chips opportunistically, as each system gets accuracy
   work — no upfront collection sweep.
