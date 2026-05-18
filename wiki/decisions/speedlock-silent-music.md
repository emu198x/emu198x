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

8. **Mechanism pinned: Speedlock's anti-tamper decoy state.** At the
   audio-capture waypoint Rainbow Islands has `PC=$029B` (48K BASIC
   ROM `KEY_SCAN`), `I=$00 IM=2 IFF1=0 IFF2=0` (interrupts disabled
   — no IRQs fire so no music driver can ever run), `SP=$7FFE` parked
   in screen RAM. The bytes at `$7FFE` upward decode to ASCII
   "`OCEAN SOFTWARE LIMITED \x80PAUL OW…`" — the loader's embedded
   copyright + Paul Owens credit. Stack contents aren't return
   addresses; they're loader data the SP happens to point at. Only
   the top word ($8309) looks like a real return. This is the
   Speedlock decoy: tamper check failed → `DI / LD SP, $7FFE /
   JP $029B` traps the CPU in `KEY_SCAN` forever, pretending to
   wait for tape, hiding which check actually failed from a
   debugger. Chase H.Q. at the same waypoint has `PC=$F8AC` in user
   code, `I=$BC IFF1=1`, `SP=$A210` with a clean game-loop stack —
   running normally. So the silence isn't a music-driver bug at
   all — it's that the music driver never gets a chance to install
   itself or run, because the game's runtime is stuck in Speedlock's
   decoy trap. The title screen we see is the loader's pre-render
   frozen on screen RAM.

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
| 2026-05-15 | **Mechanism pinned.** Same-session deeper dive added a memory + Z80-state + stack-walk dump to `run_spectrum_128k_entry` (env-gated via `EMU198X_DUMP_MEM=START:END:PATH`). Findings on Rainbow Islands at the audio-capture waypoint: `PC=$029B` (48K BASIC ROM `KEY_SCAN` at $028E), `I=$00 IM=2 IFF1=0 IFF2=0` (interrupts disabled — no IRQs can fire, so even if a music driver were installed it would never be called), `SP=$7FFE`, and the "stack" at $7FFE contains the bytes `09 83 4F 43 45 41 4E 20 53 4F 46 54 57 41 52 45 20 4C 49 4D 49 54 45 44 80 50 41 55 4C 20 4F 57 …` = `\x09\x83 OCEAN SOFTWARE LIMITED \x80PAUL OW…` — the Speedlock loader's embedded copyright + Paul Owens credit. Initially read as anti-tamper decoy state; in fact this is the loader sitting in its post-block-1 key-wait + retry loop after Speedlock turbo blocks failed to decode (next entry corrects the original "decoy" reading). |
| 2026-05-18 | **Music driver init runs exactly once, update never fires.** With the `load_snapshot` 128K fix landed (commit `e71dd1e`), loaded SkoolKit's known-good Rainbow Islands `.z80` snapshot directly into our emulator via `--headless --script` and ran it forward for 200 simulated seconds. Game IS running: at 200s the high-score table renders ("BEST 5 / DOC / AEB / BAV / GJF / EGO / 100000 / ..."). But AY writes across the full 200s: **exactly 16**, all in one initial sweep of registers 0-15 (mostly $00, with r7=$FF mixer-disable and r14=$FF port-A). Then **nothing** — no music driver update writes for the next 200s. Two candidate explanations: (a) the music driver's update routine has an early-exit conditional that always trips in our emulator (some flag byte at an address whose value we have wrong), or (b) IRQs are never firing after the snapshot's IFF1=0 state and the high-score animation progresses via polling-only code paths. Discriminator: trace `Z80::regs.iff1` over the 200s run. If it stays 0 throughout, (b); if it flips to 1 occasionally but the music driver still skips its writes, (a). The discriminator hasn't been written yet — natural next investigator move. |
| 2026-05-17 | **Watchpoint trace + SkoolKit register dump correct the previous "checksum divergence" reading.** New test at `crates/runtime-sinclair-zx-spectrum/tests/rainbow_islands_speedlock_watchpoint.rs` follows `speedlock7_tape_ram_dump::find_feb3_write_in_green_beret`'s two-phase shape: coarse-scan 100-frame chunks to locate the change window, restart and single-T-state step in the narrow window logging each write to $74A4-$74A5 with the last 64 PCs. **It revealed $74A4 is repeatedly overwritten throughout the load** (8+ distinct values logged: $676B, $0D84, $571A, $280E, $1A1B, $131D, $261D, $921C, $1D14, $081E, $101C, $E31F, $0100, $3B60), so it's not a "computed checksum result". Parsing SkoolKit's snapshot register state nailed the real interpretation: SkoolKit has **SP=$74A6 and BC=$1F97** — the bytes at $74A4-$74A5 are the value PUSH-ed by the code at $70E3 (`PUSH BC`) just before the POP at $70EA. So **$74A4-$74A5 is just the current value of register BC visible through stack memory**. Our BC=$1EE7 vs SkoolKit BC=$1F97 — difference $00B0 = 176 iterations of an inner loop (~14,400 T-states = a few ms). Our tape transport reports "stopped" about 176 loop iterations *later* than SkoolKit's. Both BC values will decrement to 0 in both emulators before the loop exits, making the in-memory divergence completely transient and irrelevant to Speedlock's protection. **The SkoolKit-divergence theory was a false lead.** Our Z80 produces byte-identical RAM to SkoolKit; we're 49,150 of 49,152 bytes match exactly, and the 2 "differing" bytes are a captured-at-a-different-moment register value. The actual silent-music root cause is elsewhere — most likely the audio capture window doesn't extend past whenever the music driver actually starts (which may be triggered by attract-mode timing, a key press, or post-load IRQ re-enablement we haven't traced). |
| 2026-05-17 | **SkoolKit `tap2sna.py` byte-diff: 2 bytes of divergence across 49,152** (interpretation now corrected — see next entry). Installed SkoolKit (pip), ran `tap2sna.py -c machine=128 ... rainbow-islands.tzx out.z80` to get a ground-truth post-load 128K snapshot. Extracted its RAM (Bank 5 at $4000-$7FFF, Bank 2 at $8000-$BFFF, Bank 1 at $C000-$FFFF per its `$7FFD=$11`) and diff'd against our end-of-tape RAM dump (new env var `EMU198X_DUMP_RAM_EOT=PATH` added to `run_spectrum_128k_entry`, dumps full 64K immediately after `wait_for_tape_stop` returns). Result: **only 2 bytes differ** out of 49,152. End-of-tape Z80 state nearly matches too: ours `PC=$70E5 I=$84 IM=2 IFF=0 7FFD=$11`, SkoolKit `PC=$70EB I=$84 IM=2 IFF=0 7FFD=$11` — same I / IM / IFF / paging, PC off by 6 bytes (one loop iteration's worth). The two divergent bytes are at `$74A4-$74A5` — a 16-bit little-endian word: **ours `$1EE7`, SkoolKit `$1F97`, XOR diff `$0170`**. They sit inside what looks like an 8-entry pointer table at `$74A0..$74AF`: `6B 84 9E 77 [E7 1E or 97 1F] A8 77 2A 77 16 74 F6 10 01 FD`. The code around PC ($70E0..$70F0) is byte-identical: `LD HL, $0000 / loop { PUSH BC / LD A,(DE) / INC DE / LD C,A / LD B,0 / ADD HL,BC / POP BC / DEC BC / LD A,B / OR C / JR NZ }` — a CRC-style byte-sum accumulator over a buffer at DE. So the 2 divergent bytes are almost certainly **the accumulated checksum result**. A single R-register divergence anywhere in the loader's decryption pass would integrate into exactly this signature: identical RAM elsewhere, one wrong word where the divergence was accumulated. This precisely matches the Muckypaws Speedlock-87 prediction ("no feedback to the decoding routine" → single divergence = single bad accumulated word). Speedlock's protection then sees wrong checksum → silently sets the kill flag → the loader's post-load runtime falls into the BASIC-ROM-return state we observed earlier ($029B). **Next investigator move: instrument a memory watchpoint on `$74A4-$74A5` to identify the exact instruction that writes them, then step back through the calculation chain to find which Z80 instruction's R-register increment (or other side-effect) differs from real hardware.** The `crates/runtime-sinclair-zx-spectrum/tests/speedlock7_tape_ram_dump.rs` harness has a `find_*_write_in_*` shape that the Green Beret thread used to pin the `$feb3` write; reuse that here. |
| 2026-05-17 | **Root cause for Rainbow Islands: TAP-format tape with Speedlock turbo blocks.** Disassembled $8200..$8400 from Bank 2 via `z80dasm`. The code at `$8302..$8313` is the loader's key-wait + tape-retry loop: `DI / XOR A / OUT (#FE), A / CALL $028E ; KEY_SCAN / INC E / JR Z, $8306 ; retry if no key / LD A, $29 / EI / HALT / DEC A / JR NZ ; 41-frame delay / JP $A37A`. So $028E is `KEY_SCAN` (not `LD_BYTES`, which is at $0556 — that mistake was the previous entry's "decoy" reading). The loader needs a keypress to advance and is also re-calling LD_BYTES from elsewhere on the stack ($70FC) waiting for more tape data. The Rainbow Islands TAP file has **21 blocks: 1 standard data block followed by 18 blocks with flag `$98`** — Speedlock turbo-encoded blocks. The TAP format can't preserve non-standard pulse timing or pauses, so our standard-timing playback corrupts the Speedlock data; the loader's checksum fails on every retry and it stays in the key-wait + retry loop forever. Swapping the manifest path to the TZX version (`Reference/sinclair/spectrum/Games/[TZX]/...`) unblocks the loader: Rainbow Islands now advances past the loader stub all the way to its proper `RAINBOW ISLANDS / THE STORY OF BUBBLE BOBBLE 2 / TAITO / © TAITO 1987 / © GRAFTGOLD 1989 / CREDIT 0` attract screen. Audio still nearly silent at the default 2-second window (2 unique sample values, range -9862..-9781) and at a +12-second window (same 2 values, different distribution) — the title attract loop apparently doesn't include music, or interrupts are still disabled at the moment we capture. So the silence has split: the TAP→TZX migration fixes the loader-stall half of the bug; whatever holds IFF1 low at the title screen is a residual issue. Chase H.Q. doesn't hit either because its TAP has 9 standard-flag blocks throughout (no Speedlock turbo), so the TAP format faithfully represents what real hardware would play. **Action for the next session: bulk-migrate Rainbow Islands and the four sibling-affected entries (Bubble Bobble, Out Run, RoboCop, Operation Wolf) to their TZX equivalents in the manifest, re-capture hashes, then re-investigate residual silence per-title.** Bubble Bobble probe with TZX also hits a known controls-select screen waiting for a 1/2/3/4 keypress before its title music starts — a separate complication needing per-entry autoload coverage. |
