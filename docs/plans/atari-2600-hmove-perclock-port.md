# Atari 2600 TIA — per-clock HMOVE port (#406)

**Status:** Phase 4 done — #406 closed. The per-clock HMOVE port is complete.

Phase 4 verified the Pole Position sliver against the established Stella
reference: it **persists** after the per-clock port, so it is *not* the HMOVE
comb #406 fixed. Root-caused (see #581): the sliver is the HUD's missiles + ball,
mis-positioned at the far left by (1) `resx_reset_position` clamping HBLANK RESx
strobes to column 0, and (2) the score kernel's end-of-line HMOVE accumulation.
Tracked as a separate residual in #581 (needs a Stella cycle-exact reference to
fix); #406 stays correctly closed.

Phase 3 is merged: the starfield width modulation (#578) — the Cosmic Ark
twinkle, where a regular clock during movement modulates a moving ball/missile's
effective width — and multi-HMOVE-per-line plus the three TIA-revision quirks
(#579: inverted phase clock, short-late HMOVE, late RESPx) behind `set_hmove_quirks`
flags that default off (baseline unchanged). Each quirk is shown to change output
only when enabled. Outstanding cross-checks below.

Phases 1 and 2 are merged. Phase 1 (#571 ball, #572 players, #573 missiles) put
every movable object on the per-clock counter model, output-equivalent. Phase 2
made the counters free-run (#575) and replaced the instant HMOVE offset with
Stella's movement engine (#576): HMOVE injects extra counter ticks during an
extended HBLANK (the 8px comb), giving the `hmmClocks − 8` net motion as an
*emergent, persistent* result. Normal in-HBLANK HMOVE stays byte-for-byte
identical (lock-in + oracle green); late HMOVE now produces hardware-correct
partial motion. The old position-formula renderers and `apply_motion` survive
only as test-only reference specs.

Two items deferred out of Phase 2: exact late-HMOVE pixels still want a manual
Stella 7.0 cross-check (GUI-driven), and collisions in the 8px comb region on an
HMOVE line are not yet latched.

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

1. **Per-object counters (output-equivalent).** ✅ Done (#571 ball, #572
   players, #573 missiles). Replaced the position-formula render with a per-clock
   decode/render-counter pipeline per object, seeded from the canonical position
   at the start of each visible line (and re-seeded on a visible-region
   RESPx/HMOVE/NUSIZ strobe). Each object's pipeline reproduces its old
   position-formula renderer byte-for-byte, proven by an exhaustive
   position×config property test plus the phase-1a lock-in tests and the frame
   oracle. The decode is pos-derived (counter `(off−4−trip) mod 160` for players)
   so positioning matches the old model exactly, including double/quad size.
   Implementation note: the counter advances only during the *visible* region
   (160/line), which keeps its phase stable against the 228-clock line; the
   per-line re-seed keeps `pos_*` canonical for now (Phase 2 makes it free-run so
   the movement engine can inject HBLANK ticks).
2. **Movement engine.** ✅ Done (#575 free-run foundation, #576 engine). Replaced
   `apply_motion` with the per-clock injection + extended-HBLANK/comb. Two steps:
   #575 made the counters free-run (counters canonical, `pos_*` a synced shadow
   re-derived at each line start, no per-line re-seed); #576 added the engine —
   per-object `hmmClocks` + `is_moving`, `tick_movement` every 4th clock injecting
   one `advance_*` tick per still-moving object during HBLANK until
   `clock == hmmClocks`, plus the 8-clock HBLANK extension (comb + the `−8`). Net
   motion `hmmClocks − 8` is emergent and persists across lines; normal HMOVE
   output is byte-for-byte identical (lock-in + oracle), late HMOVE is partially
   masked as on hardware. Remaining: pixel-diff late HMOVE vs a Stella snapshot
   (GUI-driven), and latch collisions in the comb region.
3. **Starfield + multi-HMOVE/line + revision quirks.** ✅ Done (#578 starfield,
   #579 multi-HMOVE + quirks). Ported Stella's ball/missile effective-width
   modulation (a regular clock during movement keys the width off the movement
   phase — `delta mod 4` for the ball, `(hclock+1) mod 4` for the missile), the
   inverted-phase-clock / short-late-HMOVE / late-RESPx quirks behind
   `set_hmove_quirks` (default off), and locked in multi-HMOVE-per-line. Each
   quirk is shown to change output only when enabled; baseline byte-for-byte
   unchanged. Remaining: pixel-diff the starfield/quirks vs a Stella snapshot
   (GUI-driven), and wire a TIA-revision selector on the machine to
   `set_hmove_quirks` (needs a real game to validate against).
4. **Verify the Pole Position sliver** vs Stella. ✅ Done. Extracted the NTSC
   parent (md5 `a4ff39d513b993159911efe01ac12eba`) from the merged a2600 softlist
   zip, drove the emulator to the race screen via a console-RESET input script,
   and instrumented the TIA to capture the per-object draw mask + register-write
   trace. **The sliver persists** and is *not* an HMOVE-comb artifact: it is the
   HUD's missiles + ball, RESM/RESBL-strobed in HBLANK and moved by an end-of-line
   HMOVE-accumulation kernel, landing ~2 columns too far left (our
   `resx_reset_position` clamps HBLANK strobes to col 0; Stella's `resxCounter`
   ≈ col 2). Re-scoped to #581 — needs a Stella cycle-exact reference to fix
   safely. #406 (the HMOVE model) is complete and stays closed.

### Outstanding cross-checks (carried from phases 2–3)

These all need driving the Stella 7.0 GUI for a pixel reference, which this
session could not do:
- Late-HMOVE exact pixels (phase 2), the starfield, and the three quirks
  (phase 3) — the tests pin current behaviour as regression baselines.
- Collisions in the 8px comb region on an HMOVE line are not latched.
- No machine wiring selects a TIA revision / calls `set_hmove_quirks` yet.

## Verification

Stella 7.0 is built at `emulators/atari/stella/stella` for pixel-exact
comparison (GUI/display-bound — drive it manually for snapshots). Each phase
adds deterministic TIA-level rendering tests (set registers, tick, assert
pixels) — no external ROMs, following the existing `atari-tia` test style.

Reference: this doc distils a full read of Stella's `tia/TIA.cxx`,
`tia/Player.hxx`, `tia/Missile.hxx`, `tia/Ball.hxx` (vendored at
`emulators/atari/stella/src/emucore/`).
