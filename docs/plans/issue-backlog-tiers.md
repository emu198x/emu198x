# Issue backlog — tiered groupings (2026-06-23)

> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.


A leverage-ordered map of the **382 open issues** into groupings worth tackling
as a unit, so the backlog is approached by *theme* (one chip fix that helps many
systems; one systemic pattern repeated across the fleet) rather than one-issue-
at-a-time.

**How to use.** Pick a grouping, not a single issue. Tier 1 is highest
leverage-to-effort; work down. Issue numbers are a snapshot — close/verify before
starting (several "not implemented" issues turn out done; see
[`../status/ui-boot-verification-2026-06-22.md`](../status/ui-boot-verification-2026-06-22.md)
for the kind of drift that hides completed work). The two **classification axes**
are the only labels: type (`enhancement`/`accuracy`/`bug`/`idea`) and
`system:<name>`; multi-system labels are the leverage signal.

Snapshot counts: 236 `enhancement`, 198 `accuracy`, 105 `bug`, 33 `idea`.

---

## Tier 1 — shared-chip bug clusters (best leverage:effort — start here)

One fix in a chip crate corrects many systems at once. These are *documented*
defects, not open-ended work.

### AY-3-8910 audio — 4 bugs, ~5 systems each
Affects MSX, Oric, SVI-328, Aquarius, Einstein (and Spectrum-128 / Amstrad,
which also use the part).
- **#152** envelope generator runs 2× too fast (missing /2 step divider)
- **#153** noise generator runs 2× too fast (missing /2 LFSR prescale)
- **#154** alternating envelope shapes 10/14 never reverse direction
- **#155** Continue+Hold envelope shapes 11/13 hold at the wrong final level
- **#156** add audio-timing + envelope-shape test coverage · **#157** reconcile volume DAC table

The two `/2`-rate bugs (#152/#153) are clearly-wrong and small — the single best
starting point in the whole tracker.

### TMS9918 video — 4 bugs, 6 systems each
Affects ColecoVision, MTX, MSX, SG-1000, SVI-328, Einstein.
- **#134** sprite coincidence flag ignores transparent (colour-0) sprites
- **#135** mid-frame backdrop (VR7) change only affects the next frame's border
- **#136** sprite attributes evaluated once per line (ignores mid-line writes)
- **#137** Graphics II colour-table masking for partial-table VR3 values
- **#138** write the missing `knowledge/chips` distillation (a chip backing 7 systems)

(The SMS `sega-vdp` variant of this family — #139–#146 — is the same lineage; see Tier 3.)

### Z80 CPU — one bug, 12 systems
- **#121** IM0 interrupt mode collapsed into IM1 (forced RST 38h) — Coleco, Jupiter
  Ace, Aquarius, MTX, MSX, Master System, SG-1000, ZX80, ZX81, Sord M5, SVI-328, Einstein
- **#122** re-confirm ZEXALL/ZEXDOC + Patrik Rak z80test green · **#123** document the 5 FUSE disagreements

---

## Tier 2 — systemic cross-fleet themes (high count; rationalise one fix)

The same defect recurs verbatim across the fleet — candidates for a *shared*
mechanism rather than N bespoke fixes.

### Save-state is bootstrap-only on ~18 systems
"snapshot re-derives from construction inputs / cold-boots / loses live state" —
the largest single cluster. A shared "capture live machine state" snapshot
pattern could close most of it.
**#443 #435 #420 #393 #381 #374 #354 #310 #308 #299 #291 #280 #272 #262 #255 #246 #230 #222 #203**

### No media path on ~15 systems
Can't load real software (tape/disk). This is what makes the secondary fleet
actually usable.
**#394 #386 #371 #366 #351 #347 #338 #313 #303 #298 #292 #281 #263 #253 #240 #364**

---

## Tier 3 — per-system accuracy concentrations (pair each with its oracle)

Where issues pile up on one system, a focused push closes many — but each wants a
validation oracle first.

- **Master System / sega-vdp (32)** — VDP bug run **#139–#146**, **#199–#220** (same VDP lineage as Tier 1).
- **Atari 800XL (28)** — GTIA/ANTIC/POKEY: **#178–#192** (no oracle yet).
- **Atari 7800 MARIA (23)** — **#194–#198**, **#426–#431** (incl. the MARIA oracle, #431).
- **C64 (22)** — VIC-II / SID / CIA: **#14 #16 #17 #19 #20 #21**.

### Oracle / harness gaps (the validation infrastructure)
Build these to unblock the accuracy work above:
**#15** (C64 VIC-II vs VICE), **#18** (C64 Lorenz), **#297** (ZX81 ULA), **#295**
(ZX80 display), **#362** (VIC-20 VIC), **#431** (MARIA), **#332** (DragonDOS),
**#317/#319** (GB Mealybug/blargg), **#238** (MSX C-BIOS CI gate).

### Other shared-chip clusters (smaller reach)
- **6845 CRTC** (BBC + PET): **#162–#168**
- **MC6847 VDG** (Atom + Dragon): **#158–#161**
- **POKEY** (Atari 800XL + 5200): **#188–#192**
- **6502** (verification across ~10 systems, core already proven via Tom Harte/Dormann): **#124–#128**

---

## Tier 4 — new cores (strategic, deferrable)

Whole new machines — the bulk of the 33 `idea`s:
**#496** Mega Drive · **#497** SNES · **#498** Atari ST · **#499** Amstrad CPC ·
**#500** Sharp X68000 · **#501** Apple II · **#502** SAM Coupé · **#503** Atari Lynx.

---

## Recommended order

1. **Tier 1** — small, concrete, high-confidence; a handful of chip-crate fixes
   visibly improves ~12 systems. Begin with the AY `/2` bugs (#152/#153).
2. Then choose between **the save-state rationalisation** (Tier 2 — broad
   correctness, one unified mechanism) and **a per-system push** (Tier 3 — e.g.
   SMS or 800XL), depending on appetite for breadth vs depth.
3. Build the **Tier 3 oracles** alongside whichever accuracy push you pick — they
   are how you know the fixes are right (assert rendered output, not chip state).
