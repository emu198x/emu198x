# Atari 2600 TIA — per-clock HMOVE port (#406)

**Status:** Phase 0 (design). In progress.

Port Stella's per-color-clock object-counter model to close #406 — the
HBLANK-aligned HMOVE model with no 8-clock beam injection. Decided 2026-06-18
to do the full per-clock rewrite for edge-case fidelity (see "Scope finding").

## Scope finding (read before assuming this fixes a visible bug)

Our **normal (clean, in-HBLANK) HMOVE is already correct.** Verified
`decode_hmove` + `apply_motion` against Stella's `(value>>4) ^ 0x08` decode for
every HM value: `$70`→7 left, `$80`→8 right, `$F0`→1 right, `$00`→0, etc., all
matching the canonical 2600 table. So this rewrite does **not** change normal
HMOVE positioning. It buys **edge-case timing fidelity only**:

- Late HMOVE (strobed in the visible region → movement clocks masked/merged).
- The starfield effect (ball/missile width modulation when a regular clock
  lands mid-movement).
- Exact comb length and multiple HMOVEs per line.
- TIA-revision quirks (inverted phase clock, short-late-HMOVE, late-RESPx).

The Pole Position speedometer-band left-edge sliver (logged on #406) is **not
confirmed** to be an HMOVE case — its scanline shows un-blanked content at
columns 0–4, implying no HMOVE comb on that line, so it may be a RESPx/wrap
issue this port won't touch. Re-check in Phase 4.

## Current model (what we have)

Position-formula. Each object stores a position (0–159); `compose_pixel(x)`
draws it where `(x − pos) mod 160 < width`. `RESPx` sets the position via
`resx_reset_position` (beam column + `RESX_PIPELINE_DELAY`); `HMOVE` calls
`apply_hmove` → `apply_motion(pos, decode_hmove(value))` **instantly**; a fixed
8-pixel comb blanks `x < 8` when `hmove_pending`. The TIA already advances one
color clock per `tick()` (the clocking spine exists).

## Stella's model (the target)

Per-color-clock circuit sim. Each object owns a free-running counter
(`myCounter`, 0–159) clocked once per color clock; position is emergent (when
the counter hits the object's decode point). HMOVE **injects extra clock
pulses** into those counters during an extended (8-clock) HBLANK:

- `HMOVE` ($2A, delayed 6 clocks): `movementClock = 0`, `movementInProgress =
  true`, extend HBLANK + paint the 8px comb (first HMOVE of the line only), set
  every object `isMoving = true`.
- HM register decode (delayed 2 clocks): `hmmClocks = (value >> 4) ^ 0x08`
  (0–15). Net motion = `hmmClocks − 8` pixels left.
- `tickMovement` (every 4th color clock, while `movementInProgress`): for each
  object call `movementTick(movementClock, hctr, inHblank)`; stop the engine
  when all objects' `isMoving` clear; `++movementClock`.
- `movementTick(clock, hclock, hblank)`: if `clock == hmmClocks` → `isMoving =
  false`; else (subject to short-late-HMOVE) if `hblank` inject one extra
  `tick()` (the same counter-advance as a normal color clock); track
  `invertedPhaseClock = !hblank`.
- Extended HBLANK ends at `hctr == 75` (vs normal 67); `clearHmoveComb` fills
  the first 8 pixels with the HBLANK colour.
- Late HMOVE falls out for free (the `if (hblank)` gate + inverted-phase merge);
  starfield is the regular-tick-during-movement width modulation on ball/missile.

Render-counter offsets to match: Player/Missile `-5`, Ball `-4`. Constants:
`H_CLOCKS 228`, `H_PIXEL 160`, `H_BLANK_CLOCKS 68`, comb/extension 8, movement
divisor 4, `Delay::hmove 6`, `Delay::hmp/hmm/hmbl/hmclr 2`.

## Phase plan (one small PR each, Stella-verified)

1. **Per-object counters (output-equivalent).** Replace the position-formula
   render with free-running counters reset by RESPx, rendered at the decode
   point. Lock the current rendering in with TIA-level pixel tests first, then
   keep them green — this phase must not change any output. The riskiest phase
   (touches all object rendering); de-risk with the regression tests.
2. **Movement engine.** Replace `apply_motion` with the extra-clock injection +
   extended-HBLANK/comb. Normal HMOVE output stays identical (regression tests);
   late-HMOVE/comb now match Stella (new tests + pixel diff vs a Stella snapshot).
3. **Starfield + multi-HMOVE/line + revision quirks** (inverted phase clock,
   short-late-HMOVE, late-RESPx) — default the quirks off (baseline behaviour).
4. **Verify the Pole Position sliver** vs Stella; close #406 or re-scope the
   residual.

## Verification

Stella 7.0 is built at `emulators/atari/stella/stella` for pixel-exact
comparison (GUI/display-bound — drive it manually for snapshots). Each phase
adds deterministic TIA-level rendering tests (set registers, tick, assert
pixels) — no external ROMs, following the existing `atari-tia` test style.

Reference: this doc distils a full read of Stella's `tia/TIA.cxx`,
`tia/Player.hxx`, `tia/Missile.hxx`, `tia/Ball.hxx` (vendored at
`emulators/atari/stella/src/emucore/`).
