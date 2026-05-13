# Speedlock-tape loading incomplete

**Status: RESOLVED 2026-05-13.** Root cause was a one-line bug in our TZX parser: `append_data_spans` read the *lower* N bits of a 0x14/0x11 block's last byte when `bits_in_last_byte = N < 8`, but the TZX spec stores partial last bytes left-justified (upper N bits). Speedlock-7's check-pattern block is a single byte `$E8` with `bits_in_last_byte = 6` — the loader expects the top six bits (`1 1 1 0 1 0`) to decode into `L = $3A`; we were delivering the bottom six bits (`1 0 1 0 0 0`) which built `L = $28`, missing the anti-tamper compare at `$fd6c` and firing the wipe. Fix: `for bit in (8-bits..8).rev()` instead of `for bit in (0..bits).rev()`. Op Wolf SpeedLock 7 now loads past the wipe-fire window; PC stays in the byte-decoder ($fcdd-$fced) from frame 1800 through 4000 instead of getting stuck in the `INC IY ; JR -8` wipe sled.

The rest of this document is preserved as the investigation history. Skip to **Resolution** at the bottom for the full closing summary.

---

**Original status (pre-fix):** Known limitation as of 2026-05-12. The TZX → TapeSpan pipeline parses Speedlock-7-protected tapes cleanly and the tape plays through to end-of-spans, but the loader doesn't reach a post-load attract state. The Speedlock-tape cluster (Op Wolf, RoboCop, Where Time Stood Still, Bad Dudes vs Dragon Ninja — same titles as the +3 Speedlock-6 cluster, different protection mechanism) is not yet a usable catalogue entry source.

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

### Decrypted-loader disassembly (2026-05-12 late afternoon)

The 48K-tape RAM-dump harness now exists as `crates/runtime-sinclair-zx-spectrum/tests/speedlock7_tape_ram_dump.rs`. Two tests: a frame-ladder probe that walks 100/300/600/.../9600 frames sampling `$F48E` at each step, and a focused capture that runs to frame 1400 and writes the post-decryption RAM to `/tmp/speedlock7-decrypted-f48e.bin`.

Timing of the loader's life cycle in our emulator:

- **Frame 1200**: BASIC's auto-run has not yet fired. CPU sits in ROM's `LD-EDGE-1` at `$05ED-$05F9` loading the BASIC program block.
- **Frame 1300**: Auto-run has fired. Bootstrap (`DI ; clear attrs ; OUT $FE,0 ; LDIR 2650 → $F48E ; RET`) has executed. 2 643 bytes appear at `$F48E`. PC is at `$F9FA` (deep in the loader).
- **Frame 1400-1700**: PC is at `$FCE4` — the `IN A,($FE)` site inside the loader's byte-decoder. 3 `IN A,($FE)` instructions are now visible across `$F48E..+0x0A5A`. Loader is in pilot / byte detection.
- **Frame 1800 onward**: anti-tamper wipe has fired. PROG (sysvar at `$5C53`) is corrupted to 0. PC is at `$FBD6` in an `INC IY ; JR -8` two-instruction loop. The loader region is mostly zeroed back out; only 7 bytes (`36 11 00 FD 23 18 F8` at `$FBD0`) remain — the wipe's halt sled.

### Loader byte-decoder anatomy

The pulse-decode loop at `$FCD5..$FCF5` is **virtually identical to the 48K BASIC ROM's `LD-EDGE-2` / `LD-EDGE-1`** at `$05E3..$0604`. The body:

```
$fcd5: CD D9 FC       CALL $FCD9         ; LD-EDGE-2: detect first edge
$fcd8: D0             RET NC             ; first edge timeout
$fcd9: 3E 16          LD A, $16          ; LD-EDGE-1: inner-delay seed (22)
$fcdb: 3D             DEC A
$fcdc: 20 FD          JR NZ, $fcdb       ; spin for 22 * 16 = 352 T-states
$fcde: A7             AND A              ; clear carry
$fcdf: 04             INC B              ; pulse counter
$fce0: C8             RET Z              ; B overflowed to 0: timeout, CY=0
$fce1: 3E 7F          LD A, $7F
$fce3: DB FE          IN A, ($FE)        ; sample EAR
$fce5: 1F             RRA
$fce6: A9             XOR C              ; compare against previous EAR (bit 5)
$fce7: E6 20          AND $20
$fce9: 28 F4          JR Z, $fcdf        ; no edge → loop
$fceb: 79             LD A, C
$fcec: 2F             CPL
$fced: 4F             LD C, A            ; update EAR-state mirror
$fcee: E6 02          AND $02            ; (vs ROM's AND $07)
$fcf0: F6 08          OR $08
$fcf2: D3 FE          OUT ($FE), A       ; border flash
$fcf4: 37             SCF
$fcf5: C9             RET
```

Loop body cost (between `INC B` and `JR Z $fcdf`): `4 + 5 + 7 + 11 + 4 + 4 + 7 + 12 = 54` T-states per iteration, exactly matching the standard ROM.

The pilot detector at `$FCFE..$FD20` initialises `B = $9C (156)` and loops calling `$FCD5`, requiring 32 consecutive pulses whose final `B > $C2 (194)`. With Speedlock-7's pilot pulse = 2 165 T-states:

```
2165 / 54 ≈ 40.09 iterations  →  final B = $9C + 40 = $C4 (196)
```

Compared against `$C2`: `196 > 194` by **2**. **Very tight margin.** If our pulse delivery is even ~100 T-states short of nominal, iterations drop to 38 and `B = $C2` exactly — the pilot is rejected and the pilot-count restart fires.

### Pulse-edge timing is NOT the bug

We tested the hypothesis directly. Three unit tests in `crates/common-sinclair-zx-spectrum/src/tape.rs::tests`:

1. **`pulse_span_holds_level_for_exact_tstates_then_toggles`** — drives 2 × 5-T-state pulses through `TapePlayer::advance_tstates(1)` 10 times and asserts the level toggles on T=5 and T=10 exactly, not T=4/6 or T=9/11.
2. **`bulk_advance_lands_toggle_at_exact_tstate`** — bulk `advance_tstates(99)` then `advance_tstates(1)` over a `Pulse(100)` span, asserts toggle lands exactly at T=100. Validates the bulk-advance shortcut in `advance_tstates`, not just the 1-at-a-time path.
3. **`speedlock7_pilot_pulses_produce_edges_at_exact_offsets`** — drives 32 back-to-back `Pulse(2165)` spans (Speedlock-7 pilot widths) through 2165 × 32 = 69 280 single-T-state advances, asserts every edge lands on a multiple of 2165 T-states and that there are exactly 32 edges.

**All three pass.** Our edge timing is byte-perfect against the T-state grid.

### Reframing

The Speedlock-7 failure is therefore NOT pulse-edge timing. The loader's byte-decoder (which is essentially the standard ROM's `LD-EDGE-1/2`) would correctly count `B = $C4 = 196` for a 2 165-T-state pilot pulse with our delivery, satisfying the `> $C2 = 194` threshold by the same 2-iteration margin real hardware has.

The 300-frame stall at `PC = $FCE4` (frames 1400-1700) followed by the anti-tamper wipe at frame 1800 must therefore come from somewhere *other* than pulse decoding. Candidates, none yet falsified:

1. **R-register anti-debug.** The decryptor's first instructions (`LD A, $47 ; LD R, A`) set R to a specific value. Later code probably reads R back and checks it against an expected post-execution value derived from the precise instruction trace. Tom Harte verifies our Z80's R register against the published behaviour for every instruction, but real hardware's R-bit-7 latching across `LD A, R` reads is subtle; subtle bugs there are a classic anti-debug surface. Worth re-reviewing our `LD A, R` implementation against FUSE's against real hardware.
2. **Floating-bus / port-FF reads.** The 48K's "floating bus" returns the byte the ULA is currently fetching from screen RAM, but only for unattached ports (not $FE). Speedlock could `IN A, ($FF)` (or any other unattached port) and use the screen-RAM byte as a check value. Our 48K's IO read for unattached ports returns `$FF` rather than the floating-bus byte. If Speedlock checks this, it fails.
3. **ULA contention timing.** Memory access at $4000-$7FFF is contended on the 48K. The loader at $F48E is uncontended, but if it reads from contended addresses during its check loop, the variable T-state cost matters. We model contention but tight loops are still where subtle off-by-fractional-T-state errors surface.
4. **HALT-state quirks.** The loader probably issues `EI ; HALT` somewhere to sync to vblank. If our HALT doesn't enter the interrupt service routine on the right cycle, anti-tamper could detect.

### The wipe trigger — and L = $28 instead of $3A

Tracing the loader's register state densely between frames 1700-1800 (when the wipe fires) pinned the exact trigger. The wipe is the `INC IY ; JR -8` loop at `$FBD0`, called from a `JP NZ $FBCB` at `$fd6c`. The conditional check immediately before:

```
$fd5f: 3E BC          LD A, $BC
$fd61: B8             CP B          ; B = pulse count from latest CALL $FCD5
$fd62: CB 15          RL L          ; shift CY (CP B result) into L
$fd64: 06 9E          LD B, $9E     ; re-seed B for next pulse
$fd66: D2 4F FD       JP NC, $fd4f  ; bit was 0 (old L bit 7 was 0) → read next
$fd69: 3E 3A          LD A, $3A
$fd6b: BD             CP L          ; L must equal $3A
$fd6c: C2 CB FB       JP NZ, $fbcb  ; ←—— THE WIPE TRIGGER
```

This isn't bit-decoding a data byte: it's a **rolling pattern match**. The loader shifts the pilot-vs-data discriminator into L on every pulse and looks for the moment L = `$3A`. The `$3A` (binary `0011 1010`) is the expected pilot-to-sync-to-data transition pattern.

When the wipe fires in our emulator (frame ~1760), `L = $28` (binary `0010 1000`). Two bits differ from `$3A` at positions 1 and 4.

### Why our chip produces $28 instead of $3A

The bit-discriminator is `LD A, $BC ; CP B`. Bit = 1 iff `B > $BC = 188`.

`B` is incremented during LD-EDGE-2 inside `$FCD5`. Math for our delivered pulses with the 54-T-state-per-iter loop and 354-T-state initial delay:

| Pulse type | Width  | Iterations | Final B from initial $9E | Bit |
|---|---|---|---|---|
| Speedlock-7 PILOT | 2 165 T | ~33 | $BF | **1** (> $BC) |
| Speedlock-7 SYNC1/2 | 714 T | ~6 | $A4 | 0 |
| Speedlock-7 ZERO | 583 T | ~4 | $A2 | 0 |
| Speedlock-7 ONE | 1 166 T | ~15 | $AD | 0 |

So our chip would shift in 1s during pilot, 0s during sync and data. The transition is monotonic: many 1s, then a flip to 0s. The pattern `$3A = 0011 1010` requires bits to **alternate** — three pilot pulses, then a data pulse, then three pilots, then alternating — which is **not what our delivery produces**.

This suggests either:
- The pulse widths Speedlock writes for pilot vs sync vs data are not the simple regular sequence the TZX block headers describe — there's per-pulse variation we're not preserving, OR
- The pilot-vs-data discriminator isn't measuring the pulse-width threshold we think — it's measuring something subtler (maybe the EAR level transition direction, or the inter-edge gap modulo a phase reference), OR
- The TZX dump's recorded pulse widths drift from the original master in a way Speedlock measures and we don't reproduce

### What I've learned trying to reverse-engineer the byte-decoder

The byte-decoder is more sophisticated than a uniform-threshold pulse counter. The structure I see:

1. **7-iter pre-check loop** at `$fd2d-$fd3e`: `LD E, $07 ; LD HL, $FEB4 ; LD B, (HL) ; INC HL ; CALL $FCD5 ; LD A, (HL) ; INC HL ; CP B ; JR NC, $fd0a (restart)`. Reads 7 pulse pairs, each with its own initial-B seed and threshold pair from a 14-byte table at `$FEB4`. The table values are `E9 EA D2 E2 B6 E0 B6 E0 B6 E0 D2 E2 EC ED`. These are high enough that *plain pilot pulses* would fail the check (with B around `$E1` vs threshold `$E9` for the first pair), yet our trace shows the loader does get past pilot detect into this phase. Either the pre-check pulse pairs aren't the same as the pilot pulses, or the seed values change how iters accumulate.

2. **Bit-shift loop** at `$fd5f-$fd6c`: `LD A, $BC ; CP B ; RL L ; ...` with sub-routine calls (`$FCDB` directly, bypassing the default 354-T inner delay) that pre-set A with various values (`$09, $0E, $13, $16`). The custom inner-delays are 144, 224, 304, 352 T respectively — that's the per-bit-position sensitivity calibration. Each bit position uses a different effective threshold by varying the delay-before-counting.

3. **Result check** at `$fd6b`: `LD A, $3A ; CP L ; JP NZ $fbcb`. After 8 shifts (or scans?), L must equal `$3A`.

### What we actually observed

Running Op Wolf in our emulator and densely sampling between frames 1700-1800:

- Loader gets past pilot detect (E=0 at frame 1710, immediately after pulses start).
- L starts at `$C3` (residual from earlier), eventually becomes `$28` by frame 1760.
- The wipe (`PC=$FBD6` in the `INC IY ; JR -8` loop) is running at frame 1760+.

So the loader successfully decodes the sync/pre-check, then the 8-bit shift produces L=`$28` (binary `0010 1000`) — but the loader expects `$3A` (binary `0011 1010`). Three bit positions differ.

### Why static analysis can't fully resolve this without FUSE

The per-bit-position inner-delay calibration combined with the threshold table encodes an *expected per-position pulse-width signature*. The loader is essentially verifying "is the data block I'm reading the one I wrote?" using a position-sensitive pulse-width hash. To know what pulse widths produce `$3A`, we'd need to either:

- Walk the algorithm with all 4 delay values × all 8 positions for every reasonable input pulse width and figure out the inverse, OR
- Capture FUSE's per-bit B values for the same TZX and compare.

### FUSE-free trace via single-T-state stepping (2026-05-12)

We never needed FUSE for the proximate observation. The trace test `trace_speedlock7_byte_decoder_b_values` in `speedlock7_tape_ram_dump.rs` runs Op Wolf to frame 1700, then drops to single-T-state stepping (`session.machine_mut().machine_mut().advance_tstates(1)` per loop, polling PC after each), and records every PC transition in `$fd5f..$fd6f` until the wipe zone (`$FBC0..$FBE0`) is hit.

Recorded for Op Wolf SpeedLock 7 (Hit Squad TZX), frame 1700 onward:

| Iter | Cumulative T (from frame 1700) | B at `$fd5f` | L before RL | L after RL (at `$fd65`) | Bit |
|---|---|---|---|---|---|
| 0 | +3 637 347 | $C1 | $04 | $09 | 1 (B > $BC) |
| 1 | +3 638 802 | $AC | $09 | $12 | 0 |
| 2 | +3 641 659 | $C3 | $12 | $25 | 1 |
| 3 | +3 643 100 | $AC | $25 | $4A | 0 |
| 4 | +3 644 490 | $AB | $4A | $94 | 0 |
| 5 | +3 645 946 | $AC | $94 | $28 | 0 |
| → wipe | +3 645 991 | (PC = `$fd6c`, `JP NZ $fbcb` taken) | | | |

Bit sequence shifted into L (oldest → newest): `1 0 1 0 0 0`. After 6 iterations L = $28. The wipe fires immediately at `$fd6c` because L ≠ $3A — the loader didn't even wait for 8 bits; the bit-shift loop has its own early-exit at `JP NC $fd4f` (taken when B < $BC) that loops back for more pulses, and the `CP L != $3A` check fires whenever a bit is *successfully* shifted with CY=1 (i.e. B > $BC).

Inter-call gaps (time from one `$fd5f` hit to the next):

| Iter | Gap (T-states) | Approx pulse width |
|---|---|---|
| 0→1 | 1 455 | shorter pulse |
| 1→2 | 2 857 | ≈ 2 × shorter |
| 2→3 | 1 441 | shorter |
| 3→4 | 1 390 | shorter |
| 4→5 | 1 456 | shorter |

The 2 857 T gap stands out as roughly 2 × the others — the classic ZERO/ONE FSK ratio. Mapping iters with high B (=$C1, $C3) to wide pulses and iters with low B (=$AC, $AB) to narrow pulses gives:

- Wide pulse → bit = 1 (B > $BC)
- Narrow pulse → bit = 0 (B < $BC)

So our chip is decoding the pulse train as `1 0 1 0 0 0` — but the loader expects whatever sequence produces L = $3A (binary 0011 1010, oldest bit first = 0 0 1 1 1 0 1 0 over 8 iters).

### What the trace tells us

Two competing interpretations remain:

1. **Our pulse stream is structurally wrong.** The TZX → TapePlayer pipeline doesn't preserve some piece of per-pulse information Speedlock encodes. The pulse widths at the input to the loader are not the widths Speedlock wrote.
2. **Our pulse stream is right; the loader's per-bit-position delay/threshold logic is what makes the test position-sensitive.** Same physical pulse, different B value depending on which `LD A, $09/$0E/$13/$16` was used to seed `$FCDB`.

Option 2 is testable: capture the actual `A` register on entry to `$FCDB` per iteration and compare against the 4-value rotation `$09/$0E/$13/$16`. If our trace shows the loader is sweeping delays but our chip is producing iter counts inconsistent with the delay × pulse-width math, we have a CPU-side timing issue. If iter counts match the math but the bit sequence is still wrong, the pulse stream is at fault.

Either way, **we no longer need FUSE for the diagnosis** — only for cross-validation if option 2's analysis is inconclusive.

### Implications for fixing the gap

Three new findings narrow it further:
1. The TZX file's encoded data is structurally fine — our parser is reading the right pulse widths.
2. Pulse-edge timing in our `TapePlayer` is exact (proven by unit tests).
3. The loader's byte-decoder uses per-bit-position custom timing that's sensitive to *something* our chip + pulse delivery doesn't match. Likely candidates: T-state accounting through the `IN A,($FE)` cycle (we may be off by a few T-states due to contention model details), or the loader being sensitive to the exact phase of when pulse edges fall within the M3 cycle of the IN instruction.

Next-investigation candidates (in suspicion order, post-2026-05-12 trace):
1. **Capture `A` (delay seed) per `$FCDB` entry.** Extend the trace test to also record `A` at the moment of CALL `$FCDB`. The 4-value rotation `$09/$0E/$13/$16` should be visible; if not, the pre-check phase isn't behaving as the static disassembly suggests. If yes, we can compute the expected B for each (delay, pulse-width) pair and compare against observed B — telling us whether the divergence is in the CPU's T-state accounting or the pulse stream.
2. **Sample pulse widths directly through `TapePlayer::current_span`.** At the trace test's stop point (frame 1700 → wipe), iterate `current_span()` over the active stream and record (width, level) for the spans being consumed during iters 0..5. Compare against TZX-declared widths. If the spans differ from TZX-declared, the parser is dropping or merging spans somewhere.
3. **48K IN/OUT contention timing audit.** `IN A,($FE)` is a contended I/O cycle on the 48K — 4 (M1) + 3 (M2) + 4 (M3 with up to 4 added wait T-states). Our model needs verifying T-state-by-T-state against the FUSE reference. A few-T-state offset on every IN multiplies by ~30 iters per pulse → could shift B by 30+ T-states worth of iters, plausibly enough to flip a per-bit-position threshold check.
4. **R-register check / floating-bus check** — lower-suspicion candidates from earlier, kept for completeness.

The RAM-dump harness (`crates/runtime-sinclair-zx-spectrum/tests/speedlock7_tape_ram_dump.rs`), decrypted-loader disassembly notes (in this doc), exact wipe-trigger location, threshold-table values, and edge-timing unit tests are all in place. The next person to pick this up has the full investigation context.

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

## Resolution (2026-05-13)

The bug was in `crates/format-sinclair-zx-spectrum-tzx/src/lib.rs::append_data_spans`. For a 0x11/0x14 block's *last* byte with `bits_in_last_byte = N`, the parser was iterating bits `(0..N).rev()` — reading the lower N bits of the byte. The TZX spec stores partial last bytes left-justified: the N significant bits live in the upper N bits, with the lower 8-N bits zero. Correct iteration is `(8-N..8).rev()`.

Speedlock-7 exposes this in the most surgical way possible. The `0x14` "check-pattern" block carries one byte (`$E8` for Op Wolf, paired with `bits_in_last_byte = 6`) and is consumed by the loader's bit-shift verifier at `$fd5f`. With the bug, the verifier saw bits `1 0 1 0 0 0` (lower six of `$E8`) and built `L = $28`; the `CP L != $3A` test fired and triggered the `INC IY ; JR -8` anti-tamper wipe at `$fbd0`. With the fix it sees bits `1 1 1 0 1 0` (upper six) and builds `L = $3A`, falling through to the normal data-load path.

The trace harness at `crates/runtime-sinclair-zx-spectrum/tests/speedlock7_tape_ram_dump.rs` made the diagnosis possible without FUSE: single-T-state stepping plus hooks at `$FCD5`, `$FCDB`, `$fd12`, `$fd37`, and `$fd5f` reconstructed the bit sequence, the per-iteration delay seeds, and the span widths the loader was actually consuming. Dumping the TZX block-by-block then pinpointed block 6 as a one-byte 0x14 with `bits_last = 6`, which made the parser-side bug obvious.

Why this stayed hidden: every previously-loading TZX used either `bits_in_last_byte = 8` (the parser's correct path) or relied only on full bytes. Speedlock-7 was the first protection in our catalogue to use the partial-last-byte field for its anti-tamper signature, so the bug was effectively a Speedlock-tape-only failure mode. Other Speedlock generations (2, 5) plausibly share the same construction; their wedges should now also clear.

Regression test: `pure_data_partial_last_byte_uses_upper_bits` in the TZX parser unit tests pins the correct bit ordering for byte `$E8` with `bits_last = 6`. End-to-end coverage: `opwolf_loads_past_speedlock_wipe` in the runtime test crate runs Op Wolf to frame 4000 and asserts PC never enters the wipe sled.

Catalogue entries for the Speedlock-tape cluster (Op Wolf, RoboCop, Where Time Stood Still, Bad Dudes vs Dragon Ninja, Head over Heels, Bubble Bobble) can now be authored.
