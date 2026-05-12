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

## What we know about Speedlock-7's loader architecture (2026-05-12)

Static analysis of the Op Wolf Speedlock 7 TZX bytes — extracted directly from the TZX without running the emulator — established the staging sequence the loader uses:

### TZX block structure

- **Block 0**: archive info (metadata, ignored).
- **Block 1**: standard-speed `0x10`, 19 bytes, `flag = 0x00` — BASIC header for a `PROGRAM` named `WOLF48K`, claimed length 2743 bytes, autorun line 0, `var_offset = 2743` (no variable area).
- **Block 2**: standard-speed `0x10`, 2745 bytes, `flag = 0xff` — the 2743-byte BASIC program payload (flag + 2743 + checksum).
- **Block 3** onwards: grouped pure-tone + pulse-sequence + pure-data + turbo blocks containing the encrypted game data.

### BASIC stub anatomy

The 2743-byte BASIC program loads to `PROG` (system variable at `$5C53`). Layout:

| Offset | Bytes | Decoded |
|---|---|---|
| 0..58 | 59 bytes | BASIC line 0: `INK 0 : PAPER 0 : RANDOMIZE USR (PEEK 23635 + 256 * PEEK 23636 + [stated 10 / actually 59])`. The `RANDOMIZE USR` argument evaluates to `PROG + 59` because Spectrum BASIC stores the digit string `"10"` separately from the floating-point representation, which is `0x3B` (= 59 decimal). Classic deception trick — listing shows `+10`, executes `+59`. |
| 59..91 | 33 bytes | Bootstrap machine code: `DI ; LD HL,$5800 ; LD DE,$5801 ; LD BC,$03FF ; LD (HL),L ; LDIR ; XOR A ; OUT ($FE),A ; LD HL,($5C53) ; LD DE,$005C ; ADD HL,DE ; LD BC,$0A5A ; LD DE,$F48E ; PUSH DE ; LDIR ; RET`. Disables IRQs, fills attribute area with black, sets border black via `OUT ($FE),0`, copies 0x0A5A (2650) bytes from `PROG + 0x5C` to `$F48E`, pushes `$F48E` on the stack, `RET`s into it. |
| 92..2741 | 2650 bytes | The actual loader payload, copied verbatim to `$F48E`. |

### "All-black" symptom decoded

The captured failure-state screenshot (uniform black with no border colour) isn't a wedge — it's the *intended* visual state after the bootstrap runs. The 33-byte bootstrap deliberately blacks out attributes (paper / ink → 0) and border (`OUT ($FE), 0`) **before** the loader starts streaming. So the "screen looks dead" symptom appears no later than the moment the BASIC autorun completes, which is *before* any data-stream pulse decoding begins. This is what we observed when the catalogue's `wait_for_tape_stop` returned and the runner captured the frame — the loader had got past the BASIC stub but was either still streaming or had already failed; either way the visible state is identical because the bootstrap pre-wipes the screen.

### Encrypted loader at `$F48E`

The 2650 bytes copied to `$F48E` are not directly executable. Static disassembly:

- First few instructions: `LD A, $47 ; LD R, A` — set the Z80 refresh register `R` to `$47`. Classic Speedlock anti-debug; subsequent code checks `R` to detect single-step debuggers and breakpoint-driven execution where `R` is updated differently.
- Then a small decryption / unscrambling pass operates on the bytes immediately following.
- **Zero `IN A, ($FE)` instructions appear in the encrypted bytes** (zero `DB FE` opcodes, zero `ED 78` with `BC = 0xnnFE`). The byte-decoder lives *inside* the decrypted code, not in the visible layer.

This means the EAR-polling routine we'd need to study to know what pulse-width thresholds the loader expects only exists at runtime, after the decryptor has run. Static analysis can't reach it.

### Implications for fixing the gap

Closing Speedlock-7 cleanly requires running our emulator until the decryptor finishes, dumping the RAM at `$F48E..+0x0A5A`, then disassembling that. That work is straightforward but requires new harness code (a 48K-tape equivalent of `plus3_disk_trace.rs` with a memory-dump trigger). Roughly half a day of focused work; deferred to a future session.

## What would unblock the fix (operational)

1. **Build the 48K-tape memory-dump harness.** Adapt `plus3_disk_trace.rs` to run a 48K runtime with a TZX, sample memory at `$F48E` at multiple frame counts (e.g. 100, 500, 1000, 5000), and emit hex dumps to stdout. Comparing dumps shows the decryption progressing; once the byte-decoder is visible, disassemble it.
2. **Side-by-side trace against FUSE on the same TZX.** Capture every `IN A,(0xFE)` execution, the surrounding T-state position, and the EAR bit FUSE returned at that exact moment. Compare against our run. The first divergence narrows hypothesis (1) / (2) / (3) to one.
3. **Direct timing test.** Write a unit test that drives the TapePlayer through a known turbo block (`pilot=2165, sync1=714, sync2=714, zero=583, one=1166`) and samples `current_tape_level()` at every T-state, asserting the level transitions land at the expected T-state offsets. If we're off-by-one anywhere, the test surfaces it before any title-level diagnosis.
4. **Audit the CPU's `IN A,(0xFE)` path on 48K.** Confirm the EAR bit (`0x40`) reads from `TapePlayer::ear_level()` and that the read happens at the same T-state phase real hardware would.

## What this is *not*

This is not a stub or a candidate for ROM trapping (see `no-rom-trap-load.md`). Speedlock-tape loading on real hardware is a CPU-bound bit-banging routine; the right fix is to make our pulse stream + edge timing precise enough that the routine succeeds, not to short-circuit the routine.

## Generation coverage

A 2026-05-12 phase-1 sweep of three Speedlock-tape generations all hit the same wedge state:

- **Speedlock 2** (Head over Heels, Hit Squad 1990 re-release) — red border + black screen.
- **Speedlock 5** (Bubble Bobble, Hit Squad 1992 re-release) — pure-black screen with hash `99bf46ee0b35abc0` (same all-black state Turrican / Tetris land on).
- **Speedlock 7** (Op Wolf, etc.) — blue border + black screen.

Different visible *symptoms* per version but all fail before reaching a playable state. Confirms the issue is the Speedlock-family cycle-counted byte decoder, not version-specific behaviour. A fix should unlock all three generations at once.

## Catalogue scope today

- **Microsphere fast / Bleepload / Alkatraz coverage:** three entries (`back-to-skool-tape-microsphere`, `back-to-the-future-tape-bleepload`, `ace-of-aces-tape-alkatraz`) all load end-to-end via TZX turbo blocks. Validates the pulse-stream pipeline is correct for non-Speedlock custom loaders.
- **Speedlock-tape coverage:** none. Hit Squad re-release TZXs of Op Wolf / RoboCop / WTSS / Dragon Ninja / Head over Heels / Bubble Bobble all remain in the reference library; entries can be authored once the timing-precision work above lands.
- **Speedlock-disk coverage:** four entries via the marginal-encoding model (see `marginal-encoding-model.md`). Different mechanism, different bug, separately closed.

## Related rules and decisions

- RULES.md rule 20 — "No stub implementations. Every chip does what the silicon does."
- RULES.md rule 21 — "Accuracy is foundational, not retrofitted."
- `wiki/decisions/no-rom-trap-load.md` — the contrast: cycle-accurate playback stays, ROM-trap shortcuts are rejected.
- `wiki/decisions/marginal-encoding-model.md` — the +3 disk Speedlock cousin that *is* closed.
