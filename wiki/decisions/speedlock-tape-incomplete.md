# Speedlock-tape loading incomplete

**Status:** Known limitation as of 2026-05-12. The TZX → TapeSpan pipeline parses Speedlock-7-protected tapes cleanly and the tape plays through to end-of-spans, but the loader doesn't reach a post-load attract state. The Speedlock-tape cluster (Op Wolf, RoboCop, Where Time Stood Still, Bad Dudes vs Dragon Ninja — same titles as the +3 Speedlock-6 cluster, different protection mechanism) is not yet a usable catalogue entry source.

## Observation

Pointing a catalogue entry at `Operation Wolf (Hit Squad, The)[SpeedLock 7].tzx` and running the standard 48K autoload flow:

- BASIC `LOAD ""` reads the first standard-speed (`0x10`) block — the BASIC stub — successfully.
- The stub `RANDOMIZE USR`s into the Speedlock loader code in upper RAM.
- The loader expects to drive the tape directly, polling EAR via `IN A,(0xFE)` with cycle-counted timing across `0x12` pure-tone + `0x13` pulse-sequence + `0x14` pure-data turbo blocks.
- Captured boot frame after `wait_frames = 6000` is a uniform blue border + black screen — the post-CLEAR state of the BASIC LOAD command, before the loader has produced any visible output.

The same TZX loads end-to-end in FUSE and Spectaculator. Our pulse-stream playback delivers the right *number* of pulses at the right *positions*, so the issue isn't structural; it's timing precision.

## What worked along the way

A real, separate bug fell out of the diagnosis and is fixed:

- **`pause=0` semantics in data-bearing TZX blocks (`0x10`/`0x11`/`0x14`/`0x15`)** were wrong. We emitted `TapeSpan::Stop` for `pause=0`, which the TZX spec defines as "no pause, continue immediately to the next block." The bug stopped tape playback after the first turbo block of any Speedlock-7 tape, which is why the loader never advanced past the BASIC stub no matter how many wait_frames the catalogue ran. Fix: `parse_pause` (the standalone `0x20` block) explicitly emits `Stop` for its own `pause=0` case; `append_pause_spans` (the data-block helper, shared between the TAP and TZX paths) now emits nothing for `pause=0` and the existing tests are updated to reflect the corrected semantics.

After the pause-fix the tape plays through all ~1M spans (~70 s wall, ~24 k frames simulated) and tape stops naturally at end-of-stream. Op Wolf still doesn't reach a post-load state in that window, which is what isolates the remaining issue to timing precision rather than pipeline correctness.

## Hypothesis: cycle-counted edge polling

Speedlock-tape's loader is the classic 80s technique of bit-banged `IN A,(0xFE)` with `LD B, n / DJNZ` countdown rings sized to count the gap between EAR edges. A "0" bit is decoded if the countdown ends with a specific B-register value; "1" if a different value; "sync lost" otherwise. The protection element rides on the *exact* timing: the loader was written against a particular T-state budget on a particular machine, and copies that altered the pulse train's timing fail the sync check.

Our pulse playback computes T-state countdowns as `u32` and ticks them down each Z80 instruction's worth of cycles. That should be sample-perfect for ROM-speed loading and works for Alkatraz (Ace of Aces loads end-to-end to its input-select menu via the same pulse-stream pipeline). Speedlock-7 is presumably tighter — possibly:

1. **CPU clock granularity** — we tick the ULA every half-cycle and the CPU on its native phases. If there's a one-half-cycle skew between when EAR transitions vs when the CPU samples `0xFE`, Speedlock's countdown could miss by one. Alkatraz's loader is more forgiving of single-half-cycle skew.
2. **Edge timing within a span** — `TapeSpan::Pulse(duration)` says "hold the current level for `duration` T-states, then toggle". The toggle happens at the *end* of the span; the CPU might be sampling at the *start* of the next span before the toggle propagates to `current_tape_level()`. A one-T-state lag in EAR observation would break Speedlock's countdown.
3. **Floating bus / non-ULA ports** — `IN A,(0xFE)` reads EAR + (on 48K) the keyboard rows. If our keyboard scan or floating-bus value bleeds into the EAR bit, Speedlock could see spurious edges.

None of these have been falsified yet.

## What would unblock the fix

1. **Side-by-side trace against FUSE on the same TZX.** Capture every `IN A,(0xFE)` execution, the surrounding T-state position, and the EAR bit FUSE returned at that exact moment. Compare against our run. The first divergence narrows hypothesis (1) / (2) / (3) to one.
2. **Direct timing test.** Write a unit test that drives the TapePlayer through a known turbo block (`pilot=2165, sync1=714, sync2=714, zero=583, one=1166`) and samples `current_tape_level()` at every T-state, asserting the level transitions land at the expected T-state offsets. If we're off-by-one anywhere, the test surfaces it before any title-level diagnosis.
3. **Audit the CPU's `IN A,(0xFE)` path on 48K.** Confirm the EAR bit (`0x40`) reads from `TapePlayer::ear_level()` and that the read happens at the same T-state phase real hardware would.

## What this is *not*

This is not a stub or a candidate for ROM trapping (see `no-rom-trap-load.md`). Speedlock-tape loading on real hardware is a CPU-bound bit-banging routine; the right fix is to make our pulse stream + edge timing precise enough that the routine succeeds, not to short-circuit the routine.

## Catalogue scope today

- **Alkatraz coverage:** `ace-of-aces-tape-alkatraz` (48K). Loads end-to-end via TZX turbo blocks.
- **Speedlock-tape coverage:** none. Hit Squad re-release TZXs of Op Wolf / RoboCop / WTSS / Dragon Ninja remain in the reference library; entries can be authored once the timing-precision work above lands.
- **Speedlock-disk coverage:** four entries via the marginal-encoding model (see `marginal-encoding-model.md`). Different mechanism, different bug, separately closed.

## Related rules and decisions

- RULES.md rule 20 — "No stub implementations. Every chip does what the silicon does."
- RULES.md rule 21 — "Accuracy is foundational, not retrofitted."
- `wiki/decisions/no-rom-trap-load.md` — the contrast: cycle-accurate playback stays, ROM-trap shortcuts are rejected.
- `wiki/decisions/marginal-encoding-model.md` — the +3 disk Speedlock cousin that *is* closed.
