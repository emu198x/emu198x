# Speedlock anti-tamper silences 128K AY music

**Status:** Open. Diagnosed 2026-05-15. Affects Rainbow Islands, Bubble Bobble,
Out Run, RoboCop, Operation Wolf (all 128K versions, all loaded from their
Ocean / U.S. Gold (48K-128K) combo tapes). Sibling of [`speedlock-tape-incomplete.md`](speedlock-tape-incomplete.md): same loader family, different failure mode (the tape *finishes loading*, the title screen *renders*, but the AY music driver is suppressed).

## Summary

Five 128K catalogue entries reach their title screen with the visible frame
hash matching expectations, but produce zero AY chip output across the
catalogue's audio capture window. The same five titles, when loaded from a
cracked / re-rip distribution that strips Speedlock, **do** produce AY music.
The bug is in our tape-load path triggering Speedlock's runtime anti-tamper
indicator, which the music driver checks once per frame and uses to skip its
AY register writes.

The visible symptom is silence, not a wedge. The game's main loop runs (HUD
updates, attract demo progresses, gameplay reaches in-game on Operation Wolf
by +120 s) — only audio is suppressed.

## Reproducer

`Rainbow Islands - The Story of Bubble Bobble 2 (1990)(Ocean)(48K-128K).zip`,
the manifest's `rainbow-islands` entry:

```bash
target/release/catalogue capture --entry rainbow-islands \
    --save-audio /tmp/rainbow.wav \
    --save-screenshot /tmp/rainbow.png
```

**Expected:** Tim Follin's AY arrangement of the title theme playing as the
rainbow logo, Bub & Bob, Taito / Graftgold / Ocean credits render.

**Actual:** The screen renders identically to expectations
(`frame_hash` matches `xxh64:622164f5fb580f1d`). The WAV is uniformly
`-16384` across the entire 2-second window — beeper-low rail, AY contributing
zero.

The same script run against the Pentacle / Matasoft cracked 128K-only build
of the same game (`Rainbow Islands - The Story of Bubble Bobble 2 (1990)(Ocean)(128K)[cr Pentacle - Matasoft][t].zip`) produces **953 AY write events** in the same capture window with the same machine state.

## What we've established

1. **It is not the AY mix.** The 128K-class and Amstrad-class layer cores
   correctly mix `Ay3_8912::end_frame` output into `audio_frame` as of commit
   `2fc1abb` (2026-05-15). RoboCop's credits screen reproduces the original
   silence hash byte-for-byte through the new mix path, proving zero
   contribution from the AY when the chip is genuinely idle.

2. **It is not the AY chip's port-A wiring.** As of commit `4313a94` we model
   the Sinclair 128K port A pull (`0xBF`, CTS pin tied low). Verified against
   FUSE at `peripherals/sound/ay.c:ay_registerport_read`. Loaders that probe
   R14 with the canonical "write `0xFF`, read back `0xBF`" detection now see
   the same value FUSE delivers. Chase H.Q.'s music driver flow is unchanged
   by the fix; Rainbow / Bubble / Out Run / RoboCop / Op Wolf are still silent.

3. **It is not the HALT instruction.** As of commit `81a4697` HALT correctly
   blocks until IRQ. Chase H.Q.'s title-screen rendering shifted with the fix
   (boot frame hash changed, music timing more accurate); Rainbow Islands'
   boot frame hash did **not** shift — confirming Rainbow's affected code
   path doesn't depend on HALT timing.

4. **The music driver runs but suppresses its writes.** AY-write-event traces
   on Rainbow Islands show exactly 2 events ever (`r14=FF, r7=FF` — the
   initial detection writes) followed by an unending poll of
   `read r7, (read r14, read r14, read r7) × 7` — 22 reads per iteration,
   repeated for the full capture window. Chase H.Q. enters the **same 22-read
   pattern** but exits after exactly one iteration, then begins writing
   channel registers `r11=00, r10=0E, r9=00, …`. The poll itself is identical;
   the exit decision differs. So the driver code path is shared and the
   decision to suppress writes is taken between iterations.

5. **Bubble Bobble and Out Run never even enter the poll.** Zero AY events
   in their full capture window. They use a non-AY 128K detection path —
   probably RAM-at-`$C000` write/read or stack-pointer location — that's
   also failing, and they conclude "this is 48K" without ever touching the
   chip. Same observable symptom (silence) but a separate detection path.

6. **The cracked Pentacle version of Rainbow Islands plays music in our
   emulator.** 953 AY events / 2-second window. Same emulator, same memory
   layout, same Z80 + ULA + AY + autoload path. The only thing that differs
   between the working cracked tape and the silent original tape is the
   loader and the anti-tamper code that survives to runtime. This is the
   single strongest piece of evidence that the bug lies in how our emulator
   interacts with Speedlock's anti-tamper, not in AY emulation, not in HALT,
   not in IRQ delivery, and not in the music driver itself.

7. **Loader sibling.** This is the same Speedlock-7 family that's been
   investigated in [`speedlock-tape-incomplete.md`](speedlock-tape-incomplete.md) — Green Beret still wedges
   with an all-black screen via path 3 of three independent anti-tamper
   triggers (`$ff00: LD A,($feb3) ; OR A ; CALL NZ, $fbcb`). The silent-music
   five may be hitting the same anti-tamper machinery in a different way:
   instead of the wipe firing during load (Green Beret-style), the runtime
   indicator silently disables music post-load.

## Specific hypotheses to test

- **H1: Speedlock's post-load anti-tamper writes the same indicator (`$feb3`
  or sibling) that the music driver polls per frame.** The Op Wolf vs Green
  Beret split established that `$feb3` mutates between frame 2285 and 6040
  on Green Beret but stays `$00` on Op Wolf. Rainbow Islands / Bubble Bobble /
  Out Run / RoboCop are post-load runtime, not load-time. Test: dump the
  `$fe00..$ff00` page from a known-passing entry (Chase H.Q. or
  chase-hq-plus3) and from Rainbow Islands at the same frame. The byte
  Rainbow has set that Chase H.Q. doesn't is a strong candidate for the
  indicator the music driver consults.

- **H2: The protection uses an EAR / tape-edge timing check after load
  completes.** The Speedlock-tape decoder docs in the wiki note that we
  differ from real hardware in partial-byte handling (was fixed for layer 1
  in commit `80ec856`) and likely elsewhere. A protection that reads the
  EAR line and expects specific pulse counts during a "post-load checksum"
  pass would still see drift. Test: trace EAR-line reads (`IN A, ($FE)`
  with bit 6 sampled) in the first 100 ms after `wait_for_tape_stop`
  returns. Real Spectrum reads should return `0` for those after-tape
  reads; an emulator returning anything else would trigger the protection.

- **H3: IM 2 vector chain corruption.** If our IM 2 implementation differs
  from real hardware on some edge case — IFF state, vector lookup address,
  data-bus value during interrupt acknowledge — the music IRQ vector
  could land on protection-flagged code. Test: dump the IM 2 vector
  table in RAM after load, compare against FUSE's RAM at the same point.

- **H4: ROM contents differ.** The 128K editor ROM and 48K BASIC ROM
  contain code that the loaders fingerprint. If our ROMs aren't byte-perfect
  copies of the real Sinclair 128K firmware, Speedlock's CRC over a
  specific ROM region fires. Test: `sha256sum` our installed 128K ROMs
  against published canonical hashes.

- **H5: `$7FFD` lock-bit semantics.** Real 128K hardware locks `$7FFD`
  writes when bit 5 is set. We model this. But the *exact moment* the
  lock takes effect (same-instruction vs next-instruction) may differ.
  Test: write a focused integration test of `OUT (#FD), A` with `A=$20`
  (lock-only), then `OUT (#FD), A` with `A=$10` (ROM swap), and verify
  the second write is ignored. Compare against FUSE on the same sequence.

## What I'd find most useful

The cleanest next experiment is the **side-by-side memory diff against FUSE**
at the catalogue boot waypoint. Load Rainbow Islands in FUSE, wait the same
number of frames the catalogue waits, dump `$0000..$FFFF` (all four 16K
windows). Run the same script against our emulator, dump the same. `diff -u`
the two. The first byte that differs is either where the protection has
written its kill-flag or where our emulator's runtime state has diverged
from real hardware. That tells us whether to chase H1 (kill-flag location)
or one of the other hypotheses.

If the memory contents are byte-identical at the waypoint and the divergence
is in CPU register state, it's H3 (IM 2 / IRQ-vector difference). If the
divergence is in `$7FFD` value, it's H5.

A second useful experiment: **load the Pentacle cracked version through the
same autoload path, then diff its `$fe00..$ff00` page against the original
loaded version**. The crack works by removing or neutering the protection's
runtime arm, so the bytes the cracked version has in `$feXX` that the
original doesn't are exactly the "music-suppressed" indicators.

## Files referenced

- Catalogue runner: `crates/emu198x-catalogue/src/lib.rs::run_spectrum_128k_entry`
- Manifest entries:
  `crates/emu198x-catalogue/manifest/spectrum.toml` — `rainbow-islands`, `bubble-bobble`, `out-run`, `robocop`, `operation-wolf` and their `*-plus2`, `*-plus2a`, `*-plus2b`, `*-plus3` mirrors.
- Test tapes: `~/Projects/Emu198x-Unclean/Reference/sinclair/spectrum/Games/[TAP]/Rainbow Islands - The Story of Bubble Bobble 2 (1990)(Ocean)(48K-128K).zip` (silent) and `~/Projects/Emu198x-Unclean/Reference/sinclair/spectrum/Games/[TAP]/Rainbow Islands - The Story of Bubble Bobble 2 (1990)(Ocean)(128K)[cr Pentacle - Matasoft][t].zip` (musical).
- Related decision: [`speedlock-tape-incomplete.md`](speedlock-tape-incomplete.md) — the Green Beret thread that established the three-layer anti-tamper machinery.
- FUSE reference: `~/Projects/Emu198x-Unclean/fuse-emulator-fuse/peripherals/sound/ay.c` and `machines/spec128.c`.

## Log

| Date | Event |
|---|---|
| 2026-05-15 | Brief written. AY mix (`2fc1abb`) + HALT fix (`81a4697`) + AY port-A pull (`4313a94`) landed in the same session. Five 128K titles confirmed silent against original tapes, musical against cracked tape. |
